//! Cargo subcommand to format and normalize `Cargo.toml` files (layout and
//! TOML shape), not workspace dependency policy.
//!
//! This tool is an **opinionated layout enforcer**: canonical top-level table
//! order, `[package]` field order, sorted dependency **keys**, and collapsing
//! nested tables to inline form where applicable—so manifests are predictable
//! in review and diff.
//!
//! It does **not** move versions into `[workspace.dependencies]`, rewrite
//! crates to `workspace = true`, or edit `[features]` to propagate flags to
//! dependencies. For that, use
//! [`cargo-propagate-features`](https://github.com/dataroadinc/cargo-propagate-features),
//! which wires `dep/feature` entries so enabling a feature on one crate enables
//! the same-named features on its deps—while **this** crate only normalizes
//! manifest layout (section order, key order, TOML shape) without changing
//! dependency semantics.
//!
//! This tool applies:
//! 1. Alphabetically sorted dependency **table keys** (not version rewriting)
//! 2. Consistent `[package]` field order (`PACKAGE_FIELD_ORDER`)
//! 3. Consistent top-level table order (`TOP_LEVEL_SECTION_ORDER`)
//! 4. Nested-table → inline collapse in `[package]` and dependency tables

use std::collections::BTreeMap;
use std::path::{
    Path,
    PathBuf,
};

use anyhow::{
    Context,
    Result,
};
use async_fs_io::{
    canonicalize,
    read_string_bounded,
    symlink_metadata,
    write_bytes,
};
use cargo_plugin_utils::ProgressLogger;
use clap::Parser;
use toml_edit::{
    DocumentMut,
    InlineTable,
    Item,
    Table,
    Value,
};

/// Preferred order of top-level logical tables in `Cargo.toml`.
///
/// Keys that exist are emitted in this sequence. Any other top-level key is
/// emitted **after** these, sorted **lexicographically** (deterministic diffs;
/// covers Cargo extensions and tooling-specific roots).
const TOP_LEVEL_SECTION_ORDER: &[&str] = &[
    "package",
    "lib",
    "bin",
    "test",
    "bench",
    "example",
    "dependencies",
    "dev-dependencies",
    "build-dependencies",
    "features",
    "target",
    "workspace",
    "patch",
    "replace",
    "profile",
    "lints",
    "badges",
    "cargo-features",
];

/// Preferred inline key order inside `[package]`.
///
/// Keys that exist are emitted in this sequence. Any other key in `[package]`
/// follows **lexicographically** after these.
const PACKAGE_FIELD_ORDER: &[&str] = &[
    "name",
    "version",
    "description",
    "authors",
    "edition",
    "rust-version",
    "license",
    "license-file",
    "readme",
    "documentation",
    "homepage",
    "repository",
    "publish",
    "keywords",
    "categories",
    "default-run",
    "autolib",
    "autobins",
    "autoexamples",
    "autotests",
    "autobenches",
    "include",
    "exclude",
    "build",
    "links",
    "metadata",
];

/// Cargo runs this binary as `cargo-fmt-toml fmt-toml …` (first argument is
/// always the `fmt-toml` subcommand name). Model that with a top-level
/// subcommand enum. Use `name = "cargo"` + `bin_name = "cargo-fmt-toml"` so
/// `--version` matches other Cargo plugins; use `override_usage` on the
/// `fmt-toml` variant for help text.
#[derive(Parser, Debug)]
#[command(
    name = "cargo",
    bin_name = "cargo-fmt-toml",
    version = env!("CARGO_PKG_VERSION"),
    propagate_version = true,
    about = "Opinionated Cargo.toml layout formatter (sections, keys, TOML shape)",
    after_help = "Cargo runs this program as: cargo-fmt-toml fmt-toml …"
)]
enum CargoFmtTomlCli {
    /// Format every workspace member manifest under the workspace (and git)
    /// root
    #[command(name = "fmt-toml", override_usage = "cargo fmt-toml [OPTIONS]")]
    FmtToml(FmtArgs),
}

#[derive(Parser, Debug)]
struct FmtArgs {
    /// Show what would be changed without modifying files
    #[arg(long)]
    dry_run: bool,

    /// Check if files need formatting (exit code 1 if changes needed)
    #[arg(long)]
    check: bool,

    /// Path to workspace root
    #[arg(long, default_value = ".")]
    workspace_path: PathBuf,

    /// Suppress output when there are no changes
    #[arg(long)]
    quiet: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    match CargoFmtTomlCli::parse() {
        CargoFmtTomlCli::FmtToml(args) => fmt_toml(args).await,
    }
}

async fn try_git_worktree_root(start: &Path) -> Option<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(start)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let root = String::from_utf8(output.stdout).ok()?;
    let root = root.trim();
    if root.is_empty() {
        return None;
    }
    canonicalize(root).await.ok()
}

/// `Cargo.toml` paths for workspace members only (`cargo metadata --no-deps`),
/// restricted to the canonical workspace directory and (when inside a git work
/// tree) to that repository root. Refuses crates.io checkout paths.
async fn workspace_member_manifest_paths(workspace_root: &Path) -> Result<Vec<PathBuf>> {
    let workspace_root = canonicalize(workspace_root)
        .await
        .with_context(|| format!("could not canonicalize workspace {:?}", workspace_root))?;

    let repo_root = try_git_worktree_root(&workspace_root).await;
    if let Some(ref rr) = repo_root
        && !workspace_root.starts_with(rr)
    {
        anyhow::bail!(
            "workspace path {} is outside the git repository root {}",
            workspace_root.display(),
            rr.display()
        );
    }

    let manifest_path = workspace_root.join("Cargo.toml");
    if !symlink_metadata(&manifest_path)
        .await
        .with_context(|| format!("could not inspect manifest {:?}", manifest_path))?
        .is_file()
    {
        anyhow::bail!(
            "not a Cargo workspace root (missing {}): use --workspace-path",
            manifest_path.display()
        );
    }

    let metadata = cargo_metadata::MetadataCommand::new()
        .manifest_path(&manifest_path)
        .no_deps()
        .exec()
        .context("Failed to get cargo metadata (workspace packages only)")?;

    let mut paths: Vec<PathBuf> = Vec::new();
    for pkg in metadata.packages {
        let path = pkg.manifest_path.as_std_path();
        let canonical = canonicalize(path)
            .await
            .with_context(|| format!("could not canonicalize manifest path {}", path.display()))?;

        if !canonical.starts_with(&workspace_root) {
            anyhow::bail!(
                "workspace member manifest {} is outside workspace root {}",
                canonical.display(),
                workspace_root.display()
            );
        }

        if let Some(ref rr) = repo_root
            && !canonical.starts_with(rr)
        {
            anyhow::bail!(
                "refusing to format manifest {} (outside git repository {})",
                canonical.display(),
                rr.display()
            );
        }

        let lossy = canonical.to_string_lossy();
        if lossy.contains(".cargo/registry") || lossy.contains(".cargo\\registry") {
            anyhow::bail!(
                "refusing to format crates.io checkout manifest {}",
                canonical.display()
            );
        }

        paths.push(canonical);
    }

    paths.sort();
    paths.dedup();
    Ok(paths)
}

async fn fmt_toml(args: FmtArgs) -> Result<()> {
    let mut logger = ProgressLogger::new(args.quiet);

    let crate_manifests = workspace_member_manifest_paths(&args.workspace_path).await?;

    // Phase 1: Format all manifests and collect results.
    // No files are written yet — if any manifest fails to format,
    // no files will be modified on disk (atomic behavior).
    let mut results: Vec<(PathBuf, String, usize)> = Vec::new();

    logger.set_progress(crate_manifests.len() as u64);
    logger.set_message("🔍 Formatting Cargo.toml files");

    for manifest_path in &crate_manifests {
        logger.inc();
        let (output, changes) = format_manifest(manifest_path, &mut logger).await?;
        if changes > 0 {
            results.push((manifest_path.clone(), output, changes));
        }
    }
    logger.finish();

    let total_changes: usize = results.iter().map(|(_, _, c)| c).sum();
    let files_changed = results.len();

    // Phase 2: Write all formatted files to disk.
    if !args.dry_run && !args.check {
        for (path, output, changes) in &results {
            write_bytes(path, output.as_bytes())
                .await
                .context(format!("Failed to write {:?}", path))?;
            logger.println(&format!("\n📦 {}", path.display()));
            logger.println(&format!("   💾 Formatted with {} changes", changes));
        }
    } else {
        for (path, _, changes) in &results {
            logger.println(&format!("\n📦 {}", path.display()));
            logger.println(&format!("   Would format with {} changes", changes));
        }
    }

    // In quiet mode, show nothing. Otherwise show summary.
    if !args.quiet {
        if total_changes > 0 {
            logger.println("✨ Complete!");
            if args.dry_run || args.check {
                logger.println(&format!("   {} files need formatting", files_changed));
                logger.println(&format!("   {} total changes needed", total_changes));
                if args.check {
                    std::process::exit(1);
                } else {
                    logger.println("   Run without --dry-run to apply changes");
                }
            } else {
                logger.println(&format!("   Formatted {} files", files_changed));
                logger.println(&format!("   Made {} changes", total_changes));
            }
        } else {
            logger.println("✨ All files are properly formatted");
        }
    } else if args.check && total_changes > 0 {
        // In quiet + check mode, still exit with error code
        std::process::exit(1);
    }

    Ok(())
}

/// Format a single manifest and return the formatted output string
/// along with the number of changes made. Does NOT write to disk.
async fn format_manifest(
    manifest_path: &Path,
    logger: &mut ProgressLogger,
) -> Result<(String, usize)> {
    let content = read_string_bounded(manifest_path, 16 * 1024 * 1024)
        .await
        .context(format!("Failed to read {:?}", manifest_path))?;

    let mut doc = content
        .parse::<DocumentMut>()
        .context(format!("Failed to parse {:?}", manifest_path))?;

    let mut changes = 0;

    // 1. Collapse nested tables into inline entries where appropriate
    changes += collapse_nested_tables(&mut doc, logger)?;

    // 2. Reorder sections in the document
    changes += reorder_sections(&mut doc, logger)?;

    // 3. Format [package] section
    changes += format_package_section(&mut doc, logger)?;

    // 4. Sort all dependency sections
    changes += sort_dependencies(&mut doc, "dependencies", logger)?;
    changes += sort_dependencies(&mut doc, "dev-dependencies", logger)?;
    changes += sort_dependencies(&mut doc, "build-dependencies", logger)?;

    // 5. Sort target-specific dependencies
    if let Some(target_table) = doc.get_mut("target").and_then(|t| t.as_table_mut()) {
        for (_target_name, target_config) in target_table.iter_mut() {
            if target_config.get("dependencies").is_some()
                && let Some(deps_table) = target_config
                    .get_mut("dependencies")
                    .and_then(|d| d.as_table_mut())
            {
                let collapsed = collapse_table_entries(deps_table);
                if collapsed > 0 {
                    deps_table.set_implicit(false);
                    changes += collapsed;
                }
                changes += sort_table_in_place(deps_table, logger)?;
            }
        }
    }

    let output = doc.to_string();

    if changes > 0 {
        // Validate the output is valid TOML before returning.
        // This prevents corrupting the file when an internal
        // transformation produces invalid content.
        output.parse::<DocumentMut>().context(format!(
            "Internal error: formatted output for {:?} is not valid TOML. \
             File was NOT modified. Please report this as a bug.",
            manifest_path
        ))?;
    }

    Ok((output, changes))
}

fn collapse_nested_tables(doc: &mut DocumentMut, logger: &mut ProgressLogger) -> Result<usize> {
    let mut changes = 0;

    if let Some(package) = doc.get_mut("package").and_then(|p| p.as_table_mut()) {
        let collapsed = collapse_table_entries(package);
        if collapsed > 0 {
            changes += collapsed;
        }
    }

    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(deps) = doc.get_mut(section).and_then(|d| d.as_table_mut()) {
            let collapsed = collapse_table_entries(deps);
            if collapsed > 0 {
                deps.set_implicit(false);
                changes += collapsed;
            }
        }
    }

    if let Some(target_table) = doc.get_mut("target").and_then(|t| t.as_table_mut()) {
        for (_target_name, target_config) in target_table.iter_mut() {
            if let Some(deps_table) = target_config
                .get_mut("dependencies")
                .and_then(|d| d.as_table_mut())
            {
                let collapsed = collapse_table_entries(deps_table);
                if collapsed > 0 {
                    deps_table.set_implicit(false);
                    changes += collapsed;
                }
            }
        }
    }

    if changes > 0 {
        logger.println("   ✓ Collapsed nested tables into inline entries");
    }

    Ok(changes)
}

fn collapse_table_entries(table: &mut Table) -> usize {
    let keys: Vec<String> = table.iter().map(|(k, _)| k.to_string()).collect();
    let mut replacements: Vec<(String, InlineTable)> = Vec::new();

    for key in &keys {
        let Some(Item::Table(inner)) = table.get(key) else {
            continue;
        };

        if inner.is_dotted() {
            continue;
        }

        let mut inline = InlineTable::new();
        let mut convertible = true;

        for (child_key, child_item) in inner.iter() {
            if let Some(value) = child_item.as_value() {
                inline.insert(child_key, value.clone());
            } else {
                convertible = false;
                break;
            }
        }

        if convertible {
            replacements.push((key.clone(), inline));
        }
    }

    let mut changes = 0;
    for (key, inline) in replacements {
        if let Some(item) = table.get_mut(&key) {
            *item = Item::Value(Value::InlineTable(inline));
            changes += 1;
        } else {
            table.insert(&key, Item::Value(Value::InlineTable(inline)));
            changes += 1;
        }
    }

    changes
}

fn reorder_sections(doc: &mut DocumentMut, logger: &mut ProgressLogger) -> Result<usize> {
    // Get current top-level keys from the document.  doc.iter()
    // correctly identifies top-level keys including dotted sections
    // like [workspace.package] grouped under "workspace".
    let current_keys: Vec<String> = doc.iter().map(|(k, _)| k.to_string()).collect();

    let mut expected_keys = Vec::new();
    for &section in TOP_LEVEL_SECTION_ORDER {
        if current_keys.contains(&section.to_string()) {
            expected_keys.push(section.to_string());
        }
    }

    let mut unknown: Vec<String> = current_keys
        .iter()
        .filter(|k| !TOP_LEVEL_SECTION_ORDER.contains(&k.as_str()))
        .cloned()
        .collect();
    unknown.sort();
    expected_keys.extend(unknown);

    // Check if reordering is needed.
    if current_keys == expected_keys {
        return Ok(0);
    }

    // Serialize each top-level key individually and reassemble in
    // the desired order.  We use toml_edit's own serialization per
    // key, which correctly handles dotted sub-sections, inline
    // tables, array-of-tables, multi-line values, and comments.
    //
    // For each key we build a temporary document containing only
    // that key, serialize it, and collect the text fragment.
    let mut section_fragments: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    // Remove all entries from the original document.
    let table = doc.as_table_mut();
    let mut entries: Vec<(toml_edit::Key, Item)> = Vec::new();
    let keys_to_remove: Vec<String> = table.iter().map(|(k, _)| k.to_string()).collect();
    for key in &keys_to_remove {
        if let Some(entry) = table.remove_entry(key) {
            entries.push(entry);
        }
    }

    // Serialize each key individually.
    for (key, item) in entries {
        let key_name = key.to_string();
        let mut tmp_doc = DocumentMut::new();
        tmp_doc.insert_formatted(&key, item);
        section_fragments.insert(key_name, tmp_doc.to_string());
    }

    // Reassemble in the desired order.
    let mut new_content = String::new();
    for key_name in &expected_keys {
        if let Some(fragment) = section_fragments.get(key_name) {
            if !new_content.is_empty() && !new_content.ends_with("\n\n") {
                // Ensure a blank line between sections.
                if !new_content.ends_with('\n') {
                    new_content.push('\n');
                }
                new_content.push('\n');
            }
            new_content.push_str(fragment.trim_start());
        }
    }

    // Ensure trailing newline.
    if !new_content.ends_with('\n') {
        new_content.push('\n');
    }

    // Parse the reordered content back into the document.
    *doc = new_content
        .parse::<DocumentMut>()
        .context("Internal error: reordered output is not valid TOML")?;

    logger.println("   ✓ Reordered sections");

    Ok(1)
}

fn format_package_section(doc: &mut DocumentMut, logger: &mut ProgressLogger) -> Result<usize> {
    let mut changes = 0;

    if let Some(package) = doc.get_mut("package").and_then(|p| p.as_table_mut()) {
        let current_keys: Vec<String> = package.iter().map(|(k, _)| k.to_string()).collect();
        let mut expected_keys = Vec::new();
        for &key in PACKAGE_FIELD_ORDER {
            if package.contains_key(key) {
                expected_keys.push(key.to_string());
            }
        }

        let mut unknown: Vec<String> = current_keys
            .iter()
            .filter(|k| !PACKAGE_FIELD_ORDER.contains(&k.as_str()))
            .cloned()
            .collect();
        unknown.sort();
        expected_keys.extend(unknown);

        if current_keys != expected_keys {
            // Need to reorder - collect all entries first
            let keys_to_collect: Vec<String> = package.iter().map(|(k, _)| k.to_string()).collect();
            let mut entries = BTreeMap::new();
            for key in keys_to_collect {
                if let Some(item) = package.remove(&key) {
                    entries.insert(key, item);
                }
            }

            // Re-insert in desired order
            for key in &expected_keys {
                if let Some(item) = entries.remove(key) {
                    package.insert(key, item);
                }
            }

            logger.println("   ✓ Reordered [package] section");
            changes += 1;
        }
    }

    Ok(changes)
}

fn sort_dependencies(
    doc: &mut DocumentMut,
    section: &str,
    logger: &mut ProgressLogger,
) -> Result<usize> {
    if let Some(deps) = doc.get_mut(section).and_then(|d| d.as_table_mut()) {
        sort_table_in_place(deps, logger)
    } else {
        Ok(0)
    }
}

fn sort_table_in_place(table: &mut Table, logger: &mut ProgressLogger) -> Result<usize> {
    let current_keys: Vec<String> = table.iter().map(|(k, _)| k.to_string()).collect();
    let mut sorted_keys = current_keys.clone();
    sorted_keys.sort();

    if current_keys != sorted_keys {
        // Need to reorder
        let mut entries = BTreeMap::new();
        for key in &current_keys {
            if let Some(item) = table.remove(key) {
                entries.insert(key.clone(), item);
            }
        }

        // Re-insert in sorted order
        for key in &sorted_keys {
            if let Some(item) = entries.remove(key) {
                table.insert(key, item);
            }
        }

        logger.println("   ✓ Sorted dependencies alphabetically");
        return Ok(1);
    }

    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_package_keys_sort_lexicographically() {
        let input = "\
[package]
name = \"t\"
version = \"0.1.0\"
zebra = \"z\"
apple = \"a\"
edition = \"2021\"
";
        let mut doc = input.parse::<DocumentMut>().expect("valid TOML");
        let mut logger = ProgressLogger::new(true);
        format_package_section(&mut doc, &mut logger).expect("format package");
        let result = doc.to_string();
        let apple = result.find("apple").expect("apple");
        let zebra = result.find("zebra").expect("zebra");
        assert!(
            apple < zebra,
            "unknown [package] keys should sort lexicographically; got:\n{result}"
        );
    }

    #[test]
    fn unknown_top_level_keys_sort_lexicographically_after_canonical() {
        let input = "\
[package]
name = \"t\"
version = \"0.1.0\"
edition = \"2021\"

[dependencies]
a = \"1\"

[zebra]
answer = 42

[apple]
x = 1

[lints]
workspace = true
";
        let result = reorder(input);
        let lints_pos = result.find("[lints]").expect("lints");
        let apple_pos = result.find("[apple]").expect("apple");
        let zebra_pos = result.find("[zebra]").expect("zebra");
        assert!(
            lints_pos < apple_pos && apple_pos < zebra_pos,
            "expected [lints] then [apple] then [zebra] (sorted unknown), got:\n{result}"
        );
    }

    #[tokio::test]
    async fn git_worktree_root_absent_without_repository() {
        let base = async_fs_io::TempDir::create(std::env::temp_dir())
            .await
            .expect("create temporary directory");
        assert!(try_git_worktree_root(base.path()).await.is_none());
        base.remove().await.expect("remove temporary directory");
    }

    #[tokio::test]
    async fn workspace_member_manifest_paths_excludes_registry_crates() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = canonicalize(root).await.expect("canonicalize crate root");
        let paths = workspace_member_manifest_paths(&root)
            .await
            .expect("metadata");
        assert_eq!(
            paths.len(),
            1,
            "this repository is a single-package workspace; got {:?}",
            paths
        );
        let manifest = &paths[0];
        assert!(
            manifest.starts_with(&root),
            "manifest outside workspace tree: {}",
            manifest.display()
        );
        let display = manifest.display().to_string();
        assert!(
            !display.contains(".cargo/registry"),
            "registry path must not appear: {display}"
        );
    }

    /// Helper that runs `reorder_sections` on the given TOML string
    /// and returns the resulting TOML string.
    fn reorder(input: &str) -> String {
        let mut doc = input.parse::<DocumentMut>().expect("valid TOML");
        let mut logger = ProgressLogger::new(true);
        reorder_sections(&mut doc, &mut logger).expect("reorder succeeded");
        doc.to_string()
    }

    #[test]
    fn workspace_dotted_sections_preserved() {
        let input = "\
[package]
name = \"test-workspace\"
version = \"0.0.0\"

[workspace]
members = [\"crate-a\"]
resolver = \"3\"

[profile]

[workspace.package]
rust-version = \"1.93.0\"
edition = \"2024\"

[workspace.dependencies]
serde = { version = \"1.0\", features = [\"derive\"] }
tokio = { version = \"1.0\" }
";
        let result = reorder(input);

        // All dotted workspace sections must be present
        assert!(
            result.contains("[workspace.package]"),
            "missing [workspace.package] in:\n{result}"
        );
        assert!(
            result.contains("[workspace.dependencies]"),
            "missing [workspace.dependencies] in:\n{result}"
        );
        assert!(
            result.contains("rust-version"),
            "missing rust-version field in:\n{result}"
        );
        assert!(
            result.contains("serde"),
            "missing serde dependency in:\n{result}"
        );
        assert!(
            result.contains("tokio"),
            "missing tokio dependency in:\n{result}"
        );
        assert!(
            result.contains("[profile]"),
            "missing [profile] in:\n{result}"
        );
    }

    #[test]
    fn lints_section_retained_when_reordering_package_and_dependencies() {
        let input = "\
[package]
name = \"test\"

[lints]
workspace = true

[dependencies]
serde = \"1.0\"
";
        let result = reorder(input);

        assert!(
            result.contains("[lints]"),
            "missing [lints] section in:\n{result}"
        );
        assert!(
            result.contains("workspace = true"),
            "missing lints content in:\n{result}"
        );
    }

    #[test]
    fn no_truncation_with_many_dotted_sections() {
        let input = "\
[package]
name = \"big-workspace\"
version = \"0.0.0\"

[workspace]
members = [\"a\", \"b\", \"c\"]
resolver = \"3\"

[profile.release]
opt-level = 3

[profile.dev]
opt-level = 0

[workspace.package]
edition = \"2024\"
license = \"MIT\"

[workspace.dependencies]
anyhow = \"1.0\"
clap = { version = \"4.0\", features = [\"derive\"] }
serde = { version = \"1.0\" }
tokio = { version = \"1.0\" }
";
        let result = reorder(input);

        // Verify nothing is lost
        assert!(
            result.contains("[workspace.package]"),
            "missing [workspace.package]:\n{result}"
        );
        assert!(
            result.contains("[workspace.dependencies]"),
            "missing [workspace.dependencies]:\n{result}"
        );
        assert!(
            result.contains("[profile.release]"),
            "missing [profile.release]:\n{result}"
        );
        assert!(
            result.contains("[profile.dev]"),
            "missing [profile.dev]:\n{result}"
        );
        assert!(result.contains("anyhow"), "missing anyhow dep:\n{result}");
        assert!(result.contains("tokio"), "missing tokio dep:\n{result}");
        assert!(
            result.contains("edition = \"2024\""),
            "missing edition field:\n{result}"
        );
    }

    #[test]
    fn lints_clippy_with_inline_priority_preserved() {
        // Reproduces the reported bug: a [lints.clippy] section with
        // entries like `disallowed_types = { level = "warn", priority = 1 }`
        // was causing "Failed to parse reordered document" errors.
        // The line-based parser must not misidentify value lines
        // containing brackets as section headers.
        let input = "\
[lints.clippy]
disallowed_types = { level = \"warn\", priority = 1 }
disallowed-names = { level = \"warn\", priority = -1 }

[package]
name = \"test-crate\"
version = \"0.1.0\"

[dependencies]
serde = \"1.0\"
";
        let result = reorder(input);

        assert!(
            result.contains("[lints.clippy]"),
            "missing [lints.clippy] in:\n{result}"
        );
        assert!(
            result.contains("priority = 1"),
            "missing priority = 1 in:\n{result}"
        );
        assert!(
            result.contains("priority = -1"),
            "missing priority = -1 in:\n{result}"
        );
        assert!(
            result.contains("[package]"),
            "missing [package] in:\n{result}"
        );
        assert!(
            result.contains("[dependencies]"),
            "missing [dependencies] in:\n{result}"
        );
    }

    #[test]
    fn multiline_arrays_not_misidentified_as_headers() {
        // Value lines starting with [ (array elements, nested arrays)
        // must not be misidentified as section headers.
        let input = "\
[package]
name = \"test\"
categories = [
    \"command-line-utilities\",
    \"development-tools\",
]

[features]
default = [\"std\"]

[dependencies]
serde = \"1.0\"
";
        let result = reorder(input);

        assert!(
            result.contains("categories"),
            "missing categories in:\n{result}"
        );
        assert!(
            result.contains("command-line-utilities"),
            "missing array element in:\n{result}"
        );
        assert!(
            result.contains("[features]"),
            "missing [features] in:\n{result}"
        );
    }

    #[test]
    fn nested_array_values_not_misidentified_as_headers() {
        // Nested arrays like [[1, 2], [3, 4]] should not be treated
        // as [[array-of-tables]] headers.
        let input = "\
[package]
name = \"test\"

[metadata]
matrix = [
    [1, 2],
    [3, 4],
]

[dependencies]
serde = \"1.0\"
";
        let result = reorder(input);

        assert!(
            result.contains("[metadata]"),
            "missing [metadata] in:\n{result}"
        );
        assert!(
            result.contains("[1, 2]"),
            "missing nested array [1, 2] in:\n{result}"
        );
        assert!(
            result.contains("[3, 4]"),
            "missing nested array [3, 4] in:\n{result}"
        );
    }

    #[test]
    fn multiline_feature_arrays_with_brackets() {
        // Feature arrays with entries in brackets on their own line
        // must not be misidentified as section headers. This
        // reproduces the reported "invalid multi-line basic string"
        // error when inline tables get expanded to multi-line.
        let input = "\
[package]
name = \"test\"
keywords = [
    \"cargo\",
    \"toml\",
]

[features]
full = [
    \"derive\",
    \"std\",
]

[dependencies]
serde = \"1.0\"
";
        let result = reorder(input);

        assert!(
            result.contains("[features]"),
            "missing [features] in:\n{result}"
        );
        assert!(
            result.contains("\"derive\""),
            "missing derive feature in:\n{result}"
        );
        assert!(
            result.contains("keywords"),
            "missing keywords in:\n{result}"
        );
    }

    /// Helper that runs the full formatting pipeline on a TOML string
    /// (collapse + reorder + format_package + sort) and returns the
    /// result.
    fn full_format(input: &str) -> String {
        let mut doc = input.parse::<DocumentMut>().expect("valid TOML");
        let mut logger = ProgressLogger::new(true);
        collapse_nested_tables(&mut doc, &mut logger).expect("collapse succeeded");
        reorder_sections(&mut doc, &mut logger).expect("reorder succeeded");
        format_package_section(&mut doc, &mut logger).expect("format_package succeeded");
        sort_dependencies(&mut doc, "dependencies", &mut logger).expect("sort deps succeeded");
        sort_dependencies(&mut doc, "dev-dependencies", &mut logger)
            .expect("sort dev-deps succeeded");
        sort_dependencies(&mut doc, "build-dependencies", &mut logger)
            .expect("sort build-deps succeeded");
        doc.to_string()
    }

    #[test]
    fn full_pipeline_workspace_lints_with_comments() {
        // Reproduces the reported bug: a workspace Cargo.toml with
        // [workspace.lints.clippy] entries containing trailing
        // comments after quoted string values was causing parse
        // errors during reordering.
        let input = "\
[package]
name = \"my-workspace\"
version = \"0.0.0\"
publish = false

[workspace]
members = [\"crate-a\", \"crate-b\"]
resolver = \"3\"

[workspace.lints.clippy]
missing_crate_level_docs = \"deny\" # require crate-level docs
disallowed_types = { level = \"warn\", priority = 1 }

[workspace.lints.rust]
missing_docs = \"warn\"
unsafe_code = \"forbid\" # never allow unsafe

[workspace.package]
rust-version = \"1.93.0\"
edition = \"2024\"
license = \"Apache-2.0\"

[workspace.dependencies]
serde = { version = \"1.0\", features = [\"derive\"] }
tokio = { version = \"1.0\", features = [\"full\"] }
anyhow = \"1.0\"

[profile.release]
opt-level = 3
";
        let result = full_format(input);

        // Verify all sections are preserved
        assert!(
            result.contains("[workspace.lints.clippy]"),
            "missing [workspace.lints.clippy] in:\n{result}"
        );
        assert!(
            result.contains("[workspace.lints.rust]"),
            "missing [workspace.lints.rust] in:\n{result}"
        );
        assert!(
            result.contains("[workspace.package]"),
            "missing [workspace.package] in:\n{result}"
        );
        assert!(
            result.contains("[workspace.dependencies]"),
            "missing [workspace.dependencies] in:\n{result}"
        );
        assert!(
            result.contains("[profile.release]"),
            "missing [profile.release] in:\n{result}"
        );
        // Verify comments are preserved
        assert!(
            result.contains("# require crate-level docs"),
            "missing trailing comment in:\n{result}"
        );
        assert!(
            result.contains("# never allow unsafe"),
            "missing trailing comment in:\n{result}"
        );
        // Verify values are preserved
        assert!(
            result.contains("missing_crate_level_docs"),
            "missing lint entry in:\n{result}"
        );
        assert!(
            result.contains("priority = 1"),
            "missing priority in:\n{result}"
        );
    }

    #[test]
    fn full_pipeline_lints_out_of_order() {
        // When [lints.clippy] appears before [package], the tool
        // must reorder correctly without corrupting values.
        let input = "\
[lints.clippy]
needless_pass_by_value = \"warn\"
missing_errors_doc = \"warn\"

[lints.rust]
unsafe_code = \"forbid\"

[package]
name = \"test-crate\"
version = \"0.1.0\"
edition = \"2024\"

[dependencies]
serde = { version = \"1.0\", features = [\"derive\"] }
tokio = \"1.0\"
anyhow = \"1.0\"
";
        let result = full_format(input);

        // [package] should come before [dependencies]
        let pkg_pos = result.find("[package]").expect("missing [package]");
        let dep_pos = result
            .find("[dependencies]")
            .expect("missing [dependencies]");
        assert!(
            pkg_pos < dep_pos,
            "[package] should come before [dependencies]"
        );
        // lints should still be present
        assert!(
            result.contains("[lints.clippy]"),
            "missing [lints.clippy] in:\n{result}"
        );
        assert!(
            result.contains("[lints.rust]"),
            "missing [lints.rust] in:\n{result}"
        );
        assert!(
            result.contains("needless_pass_by_value"),
            "missing lint entry in:\n{result}"
        );
        // dependencies should be sorted
        let anyhow_pos = result.find("anyhow").expect("missing anyhow");
        let serde_pos = result.find("serde").expect("missing serde");
        let tokio_pos = result.find("tokio").expect("missing tokio");
        assert!(
            anyhow_pos < serde_pos && serde_pos < tokio_pos,
            "dependencies should be sorted alphabetically"
        );
    }

    #[test]
    fn full_pipeline_workspace_lints_explicit_tables() {
        // Test with [workspace.lints.clippy.disallowed-names] as an
        // explicit sub-table (not inline) — this is how toml_edit
        // may serialize certain lint configurations.
        let input = "\
[workspace]
members = [\"crate-a\"]
resolver = \"3\"

[workspace.lints.clippy]
needless_pass_by_value = \"warn\"

[workspace.lints.clippy.disallowed-names]
level = \"warn\"
priority = -1

[workspace.lints.clippy.disallowed_types]
level = \"warn\"
priority = 1

[workspace.lints.rust]
missing_docs = \"warn\"

[workspace.package]
edition = \"2024\"

[package]
name = \"my-workspace\"
version = \"0.0.0\"

[dependencies]
serde = \"1.0\"
";
        let result = full_format(input);

        assert!(
            result.contains("disallowed-names"),
            "missing disallowed-names in:\n{result}"
        );
        assert!(
            result.contains("disallowed_types"),
            "missing disallowed_types in:\n{result}"
        );
        assert!(
            result.contains("priority = -1"),
            "missing priority = -1 in:\n{result}"
        );
        assert!(
            result.contains("priority = 1"),
            "missing priority = 1 in:\n{result}"
        );
        assert!(
            result.contains("[workspace.package]"),
            "missing [workspace.package] in:\n{result}"
        );
    }

    #[test]
    fn reorder_preserves_non_contiguous_dotted_sections() {
        // When [workspace] appears early and [workspace.package]
        // appears much later (separated by non-workspace sections),
        // both must be grouped together in the output.
        let input = "\
[package]
name = \"test\"
version = \"0.0.0\"

[dependencies]
serde = \"1.0\"

[workspace]
members = [\"a\"]

[features]
default = []

[workspace.package]
edition = \"2024\"

[workspace.dependencies]
anyhow = \"1.0\"
";
        let result = reorder(input);

        assert!(
            result.contains("[workspace.package]"),
            "missing [workspace.package] in:\n{result}"
        );
        assert!(
            result.contains("[workspace.dependencies]"),
            "missing [workspace.dependencies] in:\n{result}"
        );
        assert!(
            result.contains("edition = \"2024\""),
            "missing edition in:\n{result}"
        );
    }

    #[test]
    fn non_contiguous_workspace_sections_across_profile() {
        // Mimics the reported scenario: [workspace] at the top,
        // [profile] in the middle, then [workspace.package] and
        // [workspace.lints.*] and [workspace.dependencies] after.
        // The parser must group all workspace.* sections with
        // [workspace] even when [profile] separates them.
        let input = "\
[package]
name = \"my-workspace\"
version = \"0.0.0\"
publish = false

[workspace]
members = [
    \"crate-a\",
    \"crate-b\",
]
resolver = \"3\"

[profile]

[workspace.package]
rust-version = \"1.93.0\"
edition = \"2024\"
license = \"Apache-2.0\"
authors = [\"Test Author <test@example.com>\"]

[workspace.lints.clippy]
missing_errors_doc = \"warn\"
needless_pass_by_value = \"warn\"
disallowed_types = { level = \"warn\", priority = 1 }

[workspace.lints.rust]
missing_docs = \"warn\"
unsafe_code = \"forbid\"

[workspace.dependencies]
anyhow = \"1.0\"
clap = { version = \"4.0\", features = [\"derive\"] }
serde = { version = \"1.0\", features = [\"derive\"] }
tokio = { version = \"1.0\", features = [\"full\"] }
tracing = \"0.1\"
";
        let result = full_format(input);

        // All workspace sub-sections must be present
        assert!(
            result.contains("[workspace.package]"),
            "missing [workspace.package] in:\n{result}"
        );
        assert!(
            result.contains("[workspace.lints.clippy]"),
            "missing [workspace.lints.clippy] in:\n{result}"
        );
        assert!(
            result.contains("[workspace.lints.rust]"),
            "missing [workspace.lints.rust] in:\n{result}"
        );
        assert!(
            result.contains("[workspace.dependencies]"),
            "missing [workspace.dependencies] in:\n{result}"
        );
        assert!(
            result.contains("[profile]"),
            "missing [profile] in:\n{result}"
        );
        // Verify content
        assert!(
            result.contains("rust-version"),
            "missing rust-version in:\n{result}"
        );
        assert!(
            result.contains("disallowed_types"),
            "missing disallowed_types in:\n{result}"
        );
        assert!(
            result.contains("tracing"),
            "missing tracing dep in:\n{result}"
        );
    }

    #[test]
    fn real_workspace_with_profile_subsections_and_lints() {
        // Reproduces exact structure from bug report: [profile]
        // with multiple sub-profiles, followed by comment block,
        // then [workspace.lints.*] sections.
        let input = "\
########################################
# Virtual workspace root
########################################
[workspace]
members = [
    \"crate-a\",
    \"crate-b\",
]
resolver = \"3\"

[package]
name = \"my-workspace\"
version = \"0.0.0\"
edition = \"2024\"
publish = false

[build-dependencies]
rhusky = \"0.0.2\"

[workspace.package]
edition = \"2024\"
version = \"0.0.0\" # Version dynamically managed by CI
license-file = \"LICENSE\"
rust-version = \"1.93.0\"

[workspace.dependencies]
anyhow = \"1.0\"
serde = { version = \"1.0\", features = [\"derive\"] }
tokio = { version = \"1.0\", features = [\"full\"] }

[profile]

[profile.wasm-dev]
inherits = \"dev\"
opt-level = 1

[profile.release]
debug = false
strip = \"debuginfo\"

# Workspace-wide lint levels
[workspace.lints.rust]
warnings = \"deny\"     # never allow warnings to pass
missing_docs = \"deny\" # require docs on all public items

[workspace.lints.rustdoc]
missing_crate_level_docs = \"deny\" # require crate-level docs
broken_intra_doc_links = \"deny\"   # enforce valid intra-doc links
bare_urls = \"warn\"                # prefer backticks or proper links

[workspace.lints.clippy]
missing_panics_doc = \"warn\"                         # document panics
missing_errors_doc = \"warn\"                         # document errors
doc_markdown = \"warn\"                               # backticks for code
disallowed_types = { level = \"warn\", priority = 1 }

[workspace.metadata.clippy]
disallowed-types = [\"serde_json::Value\"]

########################################
# Patches for dependencies
########################################
[patch.crates-io]
# No patches currently needed
";
        let result = full_format(input);

        // All sections must survive
        assert!(
            result.contains("[workspace.lints.rust]"),
            "missing [workspace.lints.rust] in:\n{result}"
        );
        assert!(
            result.contains("[workspace.lints.rustdoc]"),
            "missing [workspace.lints.rustdoc] in:\n{result}"
        );
        assert!(
            result.contains("[workspace.lints.clippy]"),
            "missing [workspace.lints.clippy] in:\n{result}"
        );
        assert!(
            result.contains("[workspace.metadata.clippy]"),
            "missing [workspace.metadata.clippy] in:\n{result}"
        );
        assert!(
            result.contains("[patch.crates-io]"),
            "missing [patch.crates-io] in:\n{result}"
        );
        assert!(
            result.contains("# never allow warnings to pass"),
            "missing trailing comment in:\n{result}"
        );
        // Verify output is valid TOML
        let reparsed = result.parse::<DocumentMut>();
        assert!(
            reparsed.is_ok(),
            "Output is not valid TOML:\n{result}\nError: {}",
            reparsed.unwrap_err()
        );
    }

    #[tokio::test]
    async fn reorder_actual_test_file() {
        // Test with the actual file content from /tmp that triggers
        // the parse error.
        let Some(input) = async_fs_io::read_string_bounded_if_exists(
            "/tmp/cargo-fmt-toml-test-case.toml",
            16 * 1024 * 1024,
        )
        .await
        .expect("read test file") else {
            // Skip if the test file doesn't exist
            return;
        };
        let result = full_format(&input);

        // Verify the output is valid TOML
        let reparsed = result.parse::<DocumentMut>();
        assert!(
            reparsed.is_ok(),
            "Output is not valid TOML:\n{result}\nError: {}",
            reparsed.unwrap_err()
        );
    }

    #[test]
    fn full_pipeline_output_is_valid_toml() {
        // Verify the full pipeline produces valid TOML that can be
        // parsed back without errors.
        let input = "\
[package]
name = \"test-workspace\"
version = \"0.0.0\"
publish = false

[workspace]
members = [
    \"crate-a\",
    \"crate-b\",
]
resolver = \"3\"

[profile]

[workspace.package]
rust-version = \"1.93.0\"
edition = \"2024\"
license = \"Apache-2.0\"

[workspace.lints.clippy]
missing_errors_doc = \"warn\"
missing_crate_level_docs = \"deny\" # require crate-level docs
disallowed_types = { level = \"warn\", priority = 1 }

[workspace.lints.rust]
missing_docs = \"warn\"
unsafe_code = \"forbid\" # never allow unsafe

[workspace.dependencies]
serde = { version = \"1.0\", features = [\"derive\"] }
tokio = { version = \"1.0\" }
anyhow = \"1.0\"
";
        // Run the full pipeline
        let result = full_format(input);

        // Verify the output is valid TOML
        let reparsed = result.parse::<DocumentMut>();
        assert!(
            reparsed.is_ok(),
            "Output is not valid TOML:\n{result}\nError: {}",
            reparsed.unwrap_err()
        );
    }

    #[test]
    fn full_pipeline_is_idempotent() {
        // Running the formatter twice must produce the same output.
        let input = "\
[workspace]
members = [\"crate-a\"]
resolver = \"3\"

[package]
name = \"test\"
version = \"0.0.0\"

[workspace.lints.clippy]
missing_errors_doc = \"warn\"
disallowed_types = { level = \"warn\", priority = 1 }

[workspace.package]
edition = \"2024\"
rust-version = \"1.93.0\"

[dependencies]
tokio = \"1.0\"
anyhow = \"1.0\"
serde = \"1.0\"

[workspace.dependencies]
serde = { version = \"1.0\", features = [\"derive\"] }
";
        let first = full_format(input);
        let second = full_format(&first);
        assert_eq!(
            first, second,
            "Formatter is not idempotent.\nFirst:\n{first}\nSecond:\n{second}"
        );
    }

    #[test]
    fn array_of_tables_preserved() {
        // [[bin]] and [[example]] are array-of-tables headers that
        // must be preserved and reordered with their parent key.
        let input = "\
[dependencies]
serde = \"1.0\"

[[bin]]
name = \"my-tool\"
path = \"src/main.rs\"

[[bin]]
name = \"helper\"
path = \"src/helper.rs\"

[package]
name = \"test\"
version = \"0.1.0\"
";
        let result = full_format(input);

        // [package] should come before [[bin]] and [dependencies]
        let pkg_pos = result.find("[package]").expect("missing [package]");
        let bin_pos = result
            .find("[[bin]]")
            .unwrap_or_else(|| panic!("missing [[bin]] in:\n{result}"));
        let dep_pos = result
            .find("[dependencies]")
            .expect("missing [dependencies]");
        assert!(
            pkg_pos < bin_pos,
            "[package] should come before [[bin]] in:\n{result}"
        );
        assert!(
            bin_pos < dep_pos,
            "[[bin]] should come before [dependencies] in:\n{result}"
        );
        // Both [[bin]] entries must survive
        let bin_count = result.matches("[[bin]]").count();
        assert_eq!(bin_count, 2, "expected 2 [[bin]] entries, got {bin_count}");
        assert!(result.contains("my-tool"), "missing my-tool in:\n{result}");
        assert!(result.contains("helper"), "missing helper in:\n{result}");
        // Output must be valid TOML
        let reparsed = result.parse::<DocumentMut>();
        assert!(
            reparsed.is_ok(),
            "Output is not valid TOML:\n{result}\nError: {}",
            reparsed.unwrap_err()
        );
    }

    #[test]
    fn all_reorder_tests_produce_valid_toml() {
        // Verify every test scenario produces valid TOML output,
        // not just that expected strings are present.
        let inputs = [
            // workspace_dotted_sections_preserved
            "\
[package]
name = \"test-workspace\"
version = \"0.0.0\"

[workspace]
members = [\"crate-a\"]
resolver = \"3\"

[profile]

[workspace.package]
rust-version = \"1.93.0\"
edition = \"2024\"

[workspace.dependencies]
serde = { version = \"1.0\", features = [\"derive\"] }
tokio = { version = \"1.0\" }
",
            // lints_section_retained_when_reordering_package_and_dependencies
            "\
[package]
name = \"test\"

[lints]
workspace = true

[dependencies]
serde = \"1.0\"
",
            // lints_clippy_with_inline_priority_preserved
            "\
[lints.clippy]
disallowed_types = { level = \"warn\", priority = 1 }
disallowed-names = { level = \"warn\", priority = -1 }

[package]
name = \"test-crate\"
version = \"0.1.0\"

[dependencies]
serde = \"1.0\"
",
            // non_contiguous_workspace_sections_across_profile
            "\
[package]
name = \"my-workspace\"
version = \"0.0.0\"
publish = false

[workspace]
members = [
    \"crate-a\",
    \"crate-b\",
]
resolver = \"3\"

[profile]

[workspace.package]
rust-version = \"1.93.0\"
edition = \"2024\"
license = \"Apache-2.0\"
authors = [\"Test Author <test@example.com>\"]

[workspace.lints.clippy]
missing_errors_doc = \"warn\"
needless_pass_by_value = \"warn\"
disallowed_types = { level = \"warn\", priority = 1 }

[workspace.lints.rust]
missing_docs = \"warn\"
unsafe_code = \"forbid\"

[workspace.dependencies]
anyhow = \"1.0\"
clap = { version = \"4.0\", features = [\"derive\"] }
serde = { version = \"1.0\", features = [\"derive\"] }
tokio = { version = \"1.0\", features = [\"full\"] }
tracing = \"0.1\"
",
        ];

        for (idx, input) in inputs.iter().enumerate() {
            let result = full_format(input);
            let reparsed = result.parse::<DocumentMut>();
            assert!(
                reparsed.is_ok(),
                "Scenario {idx} produced invalid TOML:\n{result}\nError: {}",
                reparsed.unwrap_err()
            );
        }
    }
}
