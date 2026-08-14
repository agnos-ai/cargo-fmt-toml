# cargo-fmt-toml

[![Crates.io](https://img.shields.io/crates/v/cargo-fmt-toml.svg)](https://crates.io/crates/cargo-fmt-toml)
[![Documentation](https://docs.rs/cargo-fmt-toml/badge.svg)](https://docs.rs/cargo-fmt-toml)
[![CI](https://github.com/legra-ai/cargo-fmt-toml/actions/workflows/ci.yml/badge.svg)](https://github.com/legra-ai/cargo-fmt-toml/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Downloads](https://img.shields.io/crates/d/cargo-fmt-toml.svg)](https://crates.io/crates/cargo-fmt-toml)

Cargo subcommand that **opinionatedly** formats and normalizes `Cargo.toml`
**layout**: top-level table order, `[package]` field order, sorted dependency
**keys**, and nested-table collapse.

## This crate vs [`cargo-propagate-features`](https://github.com/legra-ai/cargo-propagate-features)

| Expect from **cargo-fmt-toml** | Expect from **cargo-propagate-features** |
| --- | --- |
| **Structural** changes only: where sections and keys appear, alphabetical dependency **names**, inline-table collapse. | **Semantic** changes to `[features]`: when crate A has feature `X` and depends on B with the same `X`, A’s feature list gains `B/X` (and similar wiring) so enabling `X` on A turns on the matching features on deps. |
| Stable, review-friendly diffs for manifest **shape**; does **not** change versions, add/remove dependencies, or rewrite specs to `workspace = true`. | Focuses on **feature graphs** across workspace/path dependencies, not on enforcing a canonical TOML layout. |
| Run when you want every member `Cargo.toml` to follow the same ordering rules. | Run when you want feature flags to **propagate** correctly across crates with shared feature names. |

The two tools complement each other: format first (or last) for consistent layout; use propagate-features when you need automatic `dep/feature` entries in `[features]`.

Only manifests for **workspace members** are considered, and only if they lie
under the **canonical workspace root** you pass (default: current directory).
If that directory is inside a **git** work tree, manifests must also stay
under `git rev-parse --show-toplevel` (so crates.io checkouts under
`~/.cargo/registry` are never touched).

## Opinionated layout

The formatter is meant to be a **policy enforcer**, not a minimal pretty-printer.

- **Top-level tables** — A fixed order for standard Cargo roots (`package`,
  target definitions, dependency tables, `features`, `target`, `workspace`,
  `patch`, `replace`, `profile`, `lints`, `badges`, `cargo-features`). Any
  **other** top-level key (extensions, tooling) is sorted **lexicographically**
  after that list so output is deterministic.
- **`[package]` fields** — A fixed order for common keys (`name`, `version`,
  `description`, … through `metadata`). Remaining keys sort **lexicographically**
  at the end of `[package]`.
- **Dependencies** — Keys in `dependencies`, `dev-dependencies`,
  `build-dependencies`, and target-specific dependency tables are sorted
  alphabetically (crate names only; values are not rewritten).
- **TOML shape** — Nested tables in `[package]` and dependency sections may be
  collapsed to inline tables where the formatter applies that transform.

The exact lists live in `src/main.rs` as `TOP_LEVEL_SECTION_ORDER` and
`PACKAGE_FIELD_ORDER`; change them there when the policy evolves.

## Installation

### Using cargo-binstall (recommended)

Pre-built binaries from GitHub Releases (see `[package.metadata.binstall]` in
`Cargo.toml`):

```bash
cargo install cargo-binstall
cargo binstall cargo-fmt-toml
```

### Using cargo install (crates.io)

```bash
cargo install cargo-fmt-toml
```

If the crate is not yet on crates.io, install from git:

```bash
cargo install --git https://github.com/legra-ai/cargo-fmt-toml
```

### Network / offline Cargo config

If the project you run `cargo install` from sets `net.offline = true` in
`.cargo/config.toml`, either run the install from a directory without that
setting or override for one invocation, for example:

```bash
CARGO_NET_OFFLINE=false cargo install cargo-fmt-toml
```

## Features

1. **Canonical layout** — Opinionated top-level and `[package]` ordering;
   unknown keys sorted for stable diffs
2. **Sorted dependency tables** — Alphabetical **keys** in dependency sections
   (no version or `workspace = true` rewriting)
3. **`[package]` field normalization** — Consistent key order and inline-table
   collapse where applied
4. **Member-only, repo-scoped** — Formats workspace member manifests under the
   chosen root and git work tree (see introduction above)

Workspace dependency policy (centralizing versions, `workspace = true` refactors)
and **feature propagation** are out of scope here; see the table in
[This crate vs cargo-propagate-features](#this-crate-vs-cargo-propagate-features)
and [`cargo-propagate-features`](https://github.com/legra-ai/cargo-propagate-features).

## Usage

```bash
# Format all Cargo.toml files in the workspace
cargo fmt-toml

# Preview changes without modifying files
cargo fmt-toml --dry-run

# Check if files need formatting (returns non-zero if changes
# needed)
cargo fmt-toml --check

# Explicit workspace root
cargo fmt-toml --workspace-path /path/to/repo
```

```bash
cargo fmt-toml --help
```

## Package section example

`[package]` keys are ordered per `PACKAGE_FIELD_ORDER` in `src/main.rs`
(identity and metadata first, then `metadata` and any other keys sorted at the
end). The formatter does not insert `workspace = true`; below is a typical
**hand-written** member-crate style:

```toml
[package]
name = "crate-name"
version = { workspace = true }
description = "Brief description"
authors = { workspace = true }
edition = { workspace = true }
rust-version = { workspace = true }
license = { workspace = true }
readme = { workspace = true }
```

## Dependency Sorting

All dependency sections are sorted alphabetically:

- `[dependencies]`
- `[dev-dependencies]`
- `[build-dependencies]`
- `[target.'cfg(...)'.dependencies]`

## Integration

After `cargo install cargo-fmt-toml` (or `cargo binstall`):

```makefile
.PHONY: fmt-toml
fmt-toml:
	cargo fmt-toml

.PHONY: check-fmt-toml
check-fmt-toml:
	cargo fmt-toml --check
```

## License

Copyright © 2026 DataRoad Inc, Delaware, USA, trading as Legra.

Licensed under either the [MIT license](LICENSE-MIT) or the
[Apache License, Version 2.0](LICENSE-APACHE), at your option.
