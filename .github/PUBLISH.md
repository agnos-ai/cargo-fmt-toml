# Publishing Guide

This project uses a **version-change detection** workflow for
automated publishing to crates.io.

## Branch Protection & Linear History

The `main` branch is protected with the following rules:

- ✅ **No direct pushes** - all changes via pull requests
- ✅ **Required approvals** - CODEOWNERS must approve
- ✅ **Required CI checks** - all tests must pass
- ✅ **Linear history** - only rebase merges allowed
- ❌ No merge commits or squash merges

This ensures a clean, linear git history.

## How Publishing Works

The CI workflow automatically detects when the version in `Cargo.toml`
changes and:

1. ✅ Runs all checks (format, clippy, build, test)
2. 📝 Generates changelog from conventional commits
3. 📌 Creates and pushes a git tag (e.g., `v0.1.0`)
4. 🎉 Creates a GitHub Release with generated changelog
5. 📦 Publishes to crates.io
6. 📦 Builds and uploads binaries for all platforms

**No manual tagging required!** Just bump the version in a PR and
merge.

## Setup (One-time)

### 1. Add crates.io Token to GitHub Secrets

1. Get your crates.io API token:

   ```bash
   # Visit https://crates.io/settings/tokens
   # Create a new token with "publish-new" and
   # "publish-update" scopes
   ```

2. Add to GitHub repository secrets:
   - Go to:
     https://github.com/agnos-ai/cargo-fmt-toml/settings/secrets/actions
   - Click "New repository secret"
   - Name: `CRATES_IO_TOKEN`
   - Value: `<your-token-here>`

## Publishing a New Version

### Step-by-Step Process (via Pull Request)

Since the `main` branch is protected, all changes must go through pull
requests:

```bash
# 1. Ensure you're on main and up to date
git checkout main
git pull origin main

# 2. Create a feature branch for the version bump
git checkout -b release/v0.1.0

# 3. Update version in Cargo.toml
vim Cargo.toml  # Change version = "0.0.1" to "0.1.0"

# 4. Commit the version bump (using conventional commit format)
git add Cargo.toml
git commit -m "chore: bump version to 0.1.0"

# 5. Push the branch to GitHub
git push origin release/v0.1.0

# 6. Create a pull request
gh pr create --title "chore: bump version to 0.1.0" \
  --body "Release v0.1.0"

# 7. Review and approve the PR (as CODEOWNER)
# 8. Merge using rebase (to maintain linear history)

# 9. Watch the magic happen! ✨
# After merging, GitHub Actions will automatically:
#   - Run all checks
#   - Generate changelog from commits
#   - Create git tag v0.1.0
#   - Create GitHub Release
#   - Publish to crates.io
#   - Build and upload binaries
```

### What Gets Published

The changelog will include all commits since the last version with
types:

- ✅ **feat**: New features
- ✅ **fix**: Bug fixes
- ✅ **docs**: Documentation changes
- ✅ **refactor**: Code refactoring
- ✅ **perf**: Performance improvements
- ✅ **build**: Build system changes
- ✅ **revert**: Reverted commits

Excluded from changelog:

- ❌ **style**: Code style changes
- ❌ **test**: Test updates
- ❌ **ci**: CI/CD changes
- ❌ **chore**: Maintenance tasks

## Conventional Commits

All commits should follow the
[Conventional Commits](https://www.conventionalcommits.org/) format:

```bash
<type>(<scope>): <subject>
```

### Examples

```bash
# Features
feat(github): add support for GitHub API authentication
feat: add dev version generation from git SHA

# Bug Fixes
fix(version): correct version parsing for pre-release versions
fix: handle missing Cargo.toml gracefully

# Documentation
docs: update README with installation instructions
docs(api): improve rustdoc comments

# Refactoring
refactor(commands): simplify version comparison logic
refactor: extract common version parsing logic

# Chores (won't appear in changelog)
chore: bump version to 0.1.0
chore: update dependencies
test: add integration tests
ci: improve workflow caching
```

### Breaking Changes

Use `!` after type and include `BREAKING CHANGE:` footer:

```bash
feat!: change default output format

BREAKING CHANGE: Default output format changed from plain text to JSON
```

## Monitoring the Release

After pushing your version bump:

1. Go to: https://github.com/agnos-ai/cargo-fmt-toml/actions
2. Watch the "CI/CD" workflow
3. The "Release" job will show:
   - Changelog generation
   - Tag creation
   - GitHub Release creation
   - crates.io publication
   - Binary builds and uploads

## Verification

After the workflow completes:

```bash
# Check the new release
open https://github.com/agnos-ai/cargo-fmt-toml/releases

# Check crates.io
open https://crates.io/crates/cargo-fmt-toml

# Check documentation
open https://docs.rs/cargo-fmt-toml

# Pull the new tag locally
git pull --tags
```

## Version Bump Types

- **Patch** (0.1.0 → 0.1.1): Bug fixes, minor improvements
- **Minor** (0.1.0 → 0.2.0): New features, backwards compatible
- **Major** (0.1.0 → 1.0.0): Breaking changes

## Troubleshooting

### "Version hasn't changed"

The workflow only runs when the version in `Cargo.toml` differs from
the latest git tag.

**Solution**: Make sure you actually bumped the version number.

### "Changelog is empty"

If no conventional commits exist since the last tag, the changelog
will be minimal.

**Solution**: Use conventional commit format for meaningful commits.

### "Tag already exists"

You can't publish the same version twice.

**Solution**: Bump to a new version number.

### "Authentication failed"

The crates.io token is missing or invalid.

**Solution**: Check that `CRATES_IO_TOKEN` secret is set correctly in
GitHub settings.

### "CI checks failed"

The release won't happen if any check fails.

**Solution**: Fix the failing checks and push again.

## Resources

- [Conventional Commits Specification](https://www.conventionalcommits.org/)
- [Cocogitto Documentation](https://github.com/cocogitto/cocogitto)
- [Contributing Guide](CONTRIBUTING.md)
