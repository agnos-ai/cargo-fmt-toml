//! Build script that computes version using cargo-version-info library.
//!
//! This sets CARGO_PKG_VERSION to the computed version based on:
//! 1. BUILD_VERSION env var (CI workflows)
//! 2. CARGO_PKG_VERSION_OVERRIDE env var (legacy)
//! 3. GitHub API (in GitHub Actions)
//! 4. Cargo.toml version + git SHA
//! 5. Git SHA fallback: 0.0.0-dev-<short-sha>

use cargo_version_info::commands::compute_version_string;

fn main() {
    // Compute version for repository root
    let version = match compute_version_string(".") {
        Ok(v) => v,
        Err(e) => {
            println!(
                "cargo:warning=Version computation failed: {}, using fallback",
                e
            );
            "0.0.0-dev-unknown".to_string()
        }
    };

    println!("cargo:rustc-env=CARGO_PKG_VERSION={}", version);
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");
    println!("cargo:rerun-if-env-changed=BUILD_VERSION");
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION_OVERRIDE");
    println!("cargo:rerun-if-env-changed=GITHUB_ACTIONS");
    println!("cargo:rerun-if-env-changed=GITHUB_REF");
    println!("cargo:rerun-if-env-changed=GITHUB_EVENT_NAME");
    println!("cargo:rerun-if-env-changed=GITHUB_REPOSITORY");
    println!("cargo:rerun-if-env-changed=GITHUB_TOKEN");
}
