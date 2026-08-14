# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when
working with code in this repository.

## Related Projects

This crate is part of a family of Rust projects that share the same
coding standards, tooling, and workflows:

Cargo plugins:

- `cargo-fmt-toml` - Opinionated **layout** for `Cargo.toml` (section order,
  `[package]` field order, sorted dependency keys, inline collapse). Does not
  change versions, `workspace = true`, or `[features]` semantics.
- `cargo-propagate-features` - **Feature propagation**: adds `crate/feature`
  entries under `[features]` so shared feature names flow to path/workspace
  dependencies. Not a manifest pretty-printer.
- `cargo-nightly` - Nightly toolchain management
- `cargo-plugin-utils` - Shared utilities for cargo plugins
- `cargo-version-info` - Dynamic version computation

Other Rust crates:

- `dotenvage` - Environment variable management

All projects use identical configurations for rustfmt, clippy,
markdownlint, cocogitto, and git hooks. When making changes to
tooling or workflow conventions, apply them consistently across
all repositories.

## Project Overview

`cargo-fmt-toml` is a Cargo subcommand that formats and normalizes
`Cargo.toml` **layout** (section order, `[package]` field order, sorted
dependency **keys**, nested-table collapse). It does **not** change versions,
rewrite dependency specs to `workspace = true`, or propagate features.

[`cargo-propagate-features`](https://github.com/dataroadinc/cargo-propagate-features)
edits `[features]` so enabling `X` on a crate enables `X` on dependencies that
define the same feature. Use it for feature-graph correctness; use
`cargo-fmt-toml` for consistent manifest structure. README.md has a comparison
table.

It applies:

- Alphabetically sorted keys in dependency tables
- Consistent `[package]` field ordering
- Consistent top-level table ordering
- Collapsed inline table syntax where the formatter collapses nested tables
  (it does not invent `workspace = true` entries)

## Build Commands

```bash
# Build
cargo build

# Run directly (during development)
cargo run -- fmt-toml [OPTIONS]

# Run after installation
cargo fmt-toml [OPTIONS]

# Options:
#   --dry-run              Show changes without modifying files
#   --check                Exit code 1 if changes needed
#   --workspace-path PATH  Path to workspace root (default: .)
#   --quiet                Suppress output when no changes
```

## Testing and Linting

```bash
# Run tests
cargo test

# Format check (requires nightly)
cargo +nightly fmt --all -- --check

# Format code
cargo +nightly fmt --all

# Clippy (requires nightly)
cargo +nightly clippy --all-targets --all-features -- -D warnings -W missing-docs
```

## Code Style

- **Rust Edition**: 2024, MSRV 1.94.1
- **Formatting**: Uses nightly rustfmt with vertical imports grouped
  by std/external/crate
- **Clippy**: Nightly with strict settings (max 120 lines/function,
  nesting threshold 5)
- **Disallowed variable names**: foo, bar, baz, qux, i, n

## Architecture

Single-binary CLI tool (`src/main.rs`) with multi-pass algorithm:

1. **Collapse nested tables**: Convert explicit tables to inline syntax
2. **Reorder sections**: Enforce standard section ordering
3. **Format package section**: Standardize field order
4. **Sort dependencies**: Alphabetize all dependency sections

Key dependencies:

- `clap`: CLI argument parsing with derive macros
- `toml_edit`: Preserves TOML formatting during edits
- `cargo_plugin_utils`: Workspace package discovery via cargo_metadata
- `cargo-version-info`: Dynamic version computation in build.rs

## Version Management

Use `cargo version-info bump` for version management. This command
updates Cargo.toml and creates a commit, but does NOT create tags
(tags are created by CI after tests pass).

```bash
cargo version-info bump --patch   # 0.0.1 -> 0.0.2
cargo version-info bump --minor   # 0.1.0 -> 0.2.0
cargo version-info bump --major   # 1.0.0 -> 2.0.0
```

**Do NOT use `cog bump`** - it creates local tags which conflict
with CI's tag creation workflow.

**Workflow:**

1. Create PR with version bump commit
2. Merge PR to main
3. CI detects version change, creates tag, publishes release

## Git workflow

- Commits follow Angular Conventional Commits:
  `<type>(<scope>): <subject>`
- Types: feat, fix, docs, refactor, test, style, perf, build, ci,
  chore, revert
- Use lowercase for type, scope, and subject start
- Never bypass git hooks with `--no-verify`
- Never execute `git push` - user must push manually
- Prefer `git rebase` over `git merge` for linear history

## Markdown formatting

- Maximum line length: 70 characters
- Use `-` for unordered lists (not `*` or `+`)
- Use sentence case for headers (not Title Case)
- Indent nested lists with 2 spaces
- Surround lists and code blocks with blank lines

### Markdown linting

Configuration is in `.markdownlint.json`:

- Line length: 70 characters (MD013)
- Code blocks: unlimited line length

```bash
markdownlint '**/*.md' --ignore node_modules --ignore target
```
