# Contributing

NekoDrop uses GitHub Flow. Keep `main` releasable, work in short branches, and merge through pull requests.

## Branch Rules

- `main` must always be buildable, testable, and package-ready.
- Do not commit directly to `main` once GitHub branch protection is enabled.
- Use one short-lived branch per change type.
- Do not mix unrelated work in one branch or PR.

Recommended branch names:

```text
fix/windows-path-encoding
hardening/security-reliability
feat/large-file-scan-status
ui/desktop-style-refresh
docs/release-checklist
```

Keep UI-only rewrites, security hardening, release packaging, and transfer behavior changes in separate branches.

## Pull Requests

Every PR should explain:

- what changed
- why it changed
- what was intentionally left out
- how it was verified
- whether release assets need to be rebuilt

Merge with squash commits unless the PR intentionally preserves multiple meaningful commits.

## Commit Messages

Use Conventional Commits:

```text
fix: preserve windows file picker paths
feat: show large file scan status
security: harden transfer frame validation
docs: add release checklist
chore: update packaging metadata
```

Prefer precise scopes in the message body when a change affects protocol, storage, desktop IPC, packaging, or documentation.

## Required Checks

Run these before opening or merging a PR:

```bash
cargo fmt --all -- --check
cargo test --workspace
npm run build
npm audit --omit=dev
git diff --check
```

If local Rust resolution uses the wrong toolchain, run Rust commands with the stable rustup toolchain explicitly:

```bash
RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc \
/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo test --workspace
```

Network transfer tests use local loopback TCP. In sandboxed environments they may require explicit local network permission.

## Release Rules

- Release installers must be built from a tag, not from an untagged working tree.
- Use preview tags until desktop security and large-file reliability are stable.
- Suggested tag format:

```text
v0.1.0-preview.1
v0.1.0-preview.2
```

For every release, publish:

- macOS DMG
- Windows NSIS or MSI installer when available
- SHA256 for each uploaded installer
- release notes with known limitations

Do not call a release stable while encrypted sessions, Win11 validation, and large-file reliability are still in progress.

## GitHub Settings

Enable branch protection for `main`:

- require pull requests before merging
- require status checks before merging
- disallow force pushes
- disallow direct pushes
- delete branches after merge

One-person reviews are acceptable while the project is early, but the review gate should still exist so each release change has an explicit checkpoint.
