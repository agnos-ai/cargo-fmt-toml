# Maintainer Guide

This document contains information for repository maintainers.

## Production Tool Standards

⚠️ **Important**: cargo-fmt-toml is a production tool used in CI/CD
pipelines. Users depend on it for formatting Cargo.toml files across
their projects. This demands:

- **Strict review standards** - Changes must be thoroughly reviewed
- **Mandatory signed commits** - Verify authenticity of all code
  changes
- **Linear history** - Makes auditing and rollback simpler
- **Comprehensive CI** - Every commit must pass all checks
- **Zero-warning policy** - Clippy warnings can hide real issues
- **Backward compatibility** - Breaking changes require careful
  consideration

These requirements ensure reliability and maintainability for a tool
used in critical workflows.

## GitHub Repository Settings

### Enforce Commit Signing

To ensure all commits are signed, configure branch protection rules:

1. Go to repository **Settings** → **Branches**
2. Edit (or create) branch protection rule for `main`
3. Enable: **"Require signed commits"**
4. Save changes

**To verify current settings:**

```bash
# Using GitHub CLI
gh api repos/agnos-ai/cargo-fmt-toml/branches/main/protection

# Or check in the web UI:
# https://github.com/agnos-ai/cargo-fmt-toml/settings/branch_protection_rules
```

### Required Branch Protection Rules

**For a production tool, ALL of these must be enabled** on the `main`
branch:

- [x] **Require a pull request before merging** ⚠️ REQUIRED
  - Require approvals: 1 minimum
  - Dismiss stale pull request approvals when new commits are pushed
- [x] **Require status checks to pass before merging** ⚠️ REQUIRED
  - Require branches to be up to date before merging
  - Status checks: `fmt`, `clippy`, `test`
- [x] **Require conversation resolution before merging** ⚠️ REQUIRED
- [x] **Require signed commits** ⚠️ REQUIRED
- [x] **Require linear history** ⚠️ REQUIRED
- [x] **Do not allow bypassing the above settings** ⚠️ REQUIRED
- [x] **Restrict who can push to matching branches** - Maintainers
      only

**Why so strict?** This tool is used in CI/CD pipelines. A broken
release could break users' workflows. Every safeguard matters.

### Secrets Configuration

Ensure these secrets are configured:

- `CRATES_IO_TOKEN`: For automated publishing to crates.io
  - Generate at: https://crates.io/me
  - Set at: Repository Settings → Secrets → Actions

## Release Process

Releases are automated via GitHub Actions when the version in
`Cargo.toml` changes:

1. Update version in `Cargo.toml`
2. Commit and push to `main`
3. CI will automatically:
   - Create git tag
   - Generate changelog
   - Create GitHub release
   - Publish to crates.io
   - Build and upload binaries

## Review Guidelines

### Review Checklist

When reviewing PRs:

- [ ] **Code quality**: Follows Rust best practices
- [ ] **Tests**: Includes tests for new functionality
- [ ] **Documentation**: Updates README and rustdoc as needed
- [ ] **Dependencies**: Audit new dependencies for security issues
- [ ] **Commits**: Use conventional commit format
- [ ] **CI**: All checks must pass
- [ ] **Backward compatibility**: Consider impact on existing users
- [ ] **Error handling**: Proper error messages and handling

### Review Checklist for Version Management Changes

When reviewing PRs that touch version parsing, comparison, or output:

- [ ] **Semver compliance**: Verify version parsing follows SemVer
      spec
- [ ] **Edge cases**: Handle pre-release versions, build metadata
      correctly
- [ ] **Output formats**: Ensure all formats (version, tag, json,
      github-actions) work
- [ ] **Error messages**: Clear, actionable error messages for invalid
      input
- [ ] **Tests**: Comprehensive test coverage for version operations
- [ ] **Documentation**: Update rustdoc with examples and error
      conditions

### Best Practices

- All maintainer commits must be signed
- Review all PRs thoroughly - consider impact on CI/CD users
- Keep dependencies updated, especially security-sensitive ones
- Use conventional commit format for all commits
- Test changes locally before approving
- Consider backward compatibility when making changes
- Document breaking changes clearly in commit messages
