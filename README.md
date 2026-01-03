# cargo-fmt-toml

[![Crates.io](https://img.shields.io/crates/v/cargo-fmt-toml.svg)](https://crates.io/crates/cargo-fmt-toml)
[![Documentation](https://docs.rs/cargo-fmt-toml/badge.svg)](https://docs.rs/cargo-fmt-toml)
[![CI](https://github.com/agnos-ai/cargo-fmt-toml/workflows/CI%2FCD/badge.svg)](https://github.com/agnos-ai/cargo-fmt-toml/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://github.com/agnos-ai/cargo-fmt-toml/blob/main/LICENSE)
[![Downloads](https://img.shields.io/crates/d/cargo-fmt-toml.svg)](https://crates.io/crates/cargo-fmt-toml)

Cargo subcommand to format and normalize `Cargo.toml` files according
to workspace standards.

## Features

1. **Workspace Dependencies**: Ensures all dependency versions are
   managed at workspace level
2. **Internal Dependencies**: All workspace crates use
   `{ workspace = true }` for consistency
3. **Sorted Dependencies**: All dependency sections are sorted
   alphabetically by name
4. **Package Section Format**: Enforces a consistent `[package]`
   section format

## Usage

```bash
# Format all Cargo.toml files in the workspace
cargo fmt-toml

# Preview changes without modifying files
cargo fmt-toml --dry-run

# Check if files need formatting (returns non-zero if changes
# needed)
cargo fmt-toml --check
```

## Package Section Format

The tool enforces this exact format for the `[package]` section:

```toml
[package]
name = "crate-name"
description = "Brief description"
version = { workspace = true }
edition = { workspace = true }
license-file = { workspace = true }
authors = { workspace = true }
rust-version = { workspace = true }
readme = { workspace = true }
```

## Dependency Sorting

All dependency sections are sorted alphabetically:

- `[dependencies]`
- `[dev-dependencies]`
- `[build-dependencies]`
- `[target.'cfg(...)'.dependencies]`

## Integration

Add to your Makefile:

```makefile
.PHONY: fmt-toml
fmt-toml:
    @cargo run --package cargo-fmt-toml

.PHONY: check-fmt-toml
check-fmt-toml:
    @cargo run --package cargo-fmt-toml -- --check
```
