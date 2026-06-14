# Bundle Detection Staging Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add storage-layer bundle directory detection, validation, and staging copy support.

**Architecture:** Keep bundle filesystem handling in `nekodrop-storage`, separate from normal transfer send/receive orchestration. The storage layer reads `bundle.json`, `checksums.json`, and optional `permissions.json` using `nekolink-protocol` types, validates payload files on disk, and copies valid bundles into an app-controlled staging root. Service events, desktop UI preview, local bridge, and iroh remain out of scope.

**Tech Stack:** Rust, `serde_json`, `nekolink-protocol`, existing `NekoDropError::Storage`, `cargo test -p nekodrop-storage`.

---

### Task 1: Bundle Directory Detection

**Files:**
- Create: `crates/nekodrop-storage/src/bundle.rs`
- Modify: `crates/nekodrop-storage/src/lib.rs`
- Modify: `crates/nekodrop-storage/Cargo.toml`

- [ ] **Step 1: Write failing tests**

Add tests in `bundle.rs` that create a valid bundle directory with:

```text
bundle.json
checksums.json
permissions.json
files/manifest.json
files/content.bin
```

Tests:
- `detects_valid_bundle_directory`
- `returns_none_when_bundle_json_is_missing`
- `rejects_bundle_with_mismatched_payload_checksum`
- `detects_bundle_without_permissions_as_save_only`

Run:

```bash
cargo test -p nekodrop-storage bundle_
```

Expected: FAIL because `bundle` module and detection APIs do not exist.

- [ ] **Step 2: Add dependencies and module exports**

Add to `crates/nekodrop-storage/Cargo.toml`:

```toml
nekolink-protocol = { path = "../nekolink-protocol" }
serde_json = "1"
```

Add to `crates/nekodrop-storage/src/lib.rs`:

```rust
pub mod bundle;

pub use bundle::{
    detect_bundle_directory, stage_bundle_directory, BundleImportPolicy, DetectedBundle,
    StagedBundle,
};
```

- [ ] **Step 3: Implement detection API**

Create:

```rust
pub enum BundleImportPolicy {
    ImportAllowed,
    SaveOnly,
}

pub struct DetectedBundle {
    pub root_path: PathBuf,
    pub manifest: BundleManifest,
    pub checksums: BundleChecksums,
    pub permissions: Option<BundlePermissions>,
    pub import_policy: BundleImportPolicy,
}

pub fn detect_bundle_directory(root: &Path) -> NekoDropResult<Option<DetectedBundle>>;
```

Detection rules:
- no root `bundle.json` means `Ok(None)`
- `bundle.json` and `checksums.json` must parse and validate
- `permissions.json` is optional; missing permissions means `SaveOnly`
- `contains_secrets=true` means `SaveOnly`
- unknown root entries other than `bundle.json`, `checksums.json`, `permissions.json`, and `files/` are rejected
- each manifest payload file must exist, have the expected size, and match SHA-256

### Task 2: Staging Copy

**Files:**
- Modify: `crates/nekodrop-storage/src/bundle.rs`
- Modify: `docs/STATUS.md`

- [ ] **Step 1: Write failing staging tests**

Add tests:
- `stages_valid_bundle_into_bundle_id_directory`
- `rejects_staging_when_bundle_id_would_escape_staging_root`

Run:

```bash
cargo test -p nekodrop-storage bundle_
```

Expected: FAIL until staging is implemented.

- [ ] **Step 2: Implement staging**

Create:

```rust
pub struct StagedBundle {
    pub staging_path: PathBuf,
    pub detected: DetectedBundle,
}

pub fn stage_bundle_directory(source_root: &Path, staging_root: &Path) -> NekoDropResult<StagedBundle>;
```

Staging rules:
- call `detect_bundle_directory(source_root)` first
- reject when `bundle.json` is missing
- create destination as `staging_root / manifest.bundle_id`
- reject bundle IDs containing path separators, `..`, colon, NUL, or leading/trailing whitespace
- remove an existing destination for the same bundle ID before copying
- copy only standard root files and `files/` payloads from the validated bundle
- re-run detection on the staged destination before returning

- [ ] **Step 3: Update status**

Update `docs/STATUS.md` so the `NekoLink bundle manifest` row says storage detection and staging are partially wired, while service receive event, UI preview, and local bridge remain pending.

### Task 3: Verification And PR

**Files:**
- All files above.

- [ ] **Step 1: Run focused verification**

```bash
cargo test -p nekodrop-storage bundle_
cargo test -p nekodrop-storage
cargo fmt --all -- --check
git diff --check
```

- [ ] **Step 2: Run workspace verification**

Use the repo’s normal toolchain. If local PATH points to Homebrew Rust 1.87, use explicit rustup stable rustc/rustdoc:

```bash
CARGO_TARGET_DIR=target/stable-verify RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc /Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo test --workspace
```

- [ ] **Step 3: Commit and PR**

```bash
git add crates/nekodrop-storage/Cargo.toml crates/nekodrop-storage/src/lib.rs crates/nekodrop-storage/src/bundle.rs docs/STATUS.md docs/superpowers/plans/2026-06-14-bundle-detection-staging.md
git commit -m "feat: add bundle detection and staging"
git push -u origin storage/bundle-detection-staging
```
