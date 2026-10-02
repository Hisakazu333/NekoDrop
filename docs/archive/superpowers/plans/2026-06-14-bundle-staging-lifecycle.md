# Bundle Staging Lifecycle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add storage-layer lifecycle operations for staged NekoLink bundles: list, delete, and prune expired staged bundles.

**Architecture:** Keep this slice inside `nekodrop-storage` so desktop UI, service code, and the future CCS/OpenNeko local bridge can share one staging implementation. The API manages only the staging directory; it does not import bundles, authorize bridge callers, or mutate app-specific OpenNeko/CCS data.

**Tech Stack:** Rust, `std::fs`, `std::time::SystemTime`, existing `DetectedBundle` / `StagedBundle` / `NekoDropError` patterns in `crates/nekodrop-storage/src/bundle.rs`.

---

### Task 1: List Staged Bundles

**Files:**
- Modify: `crates/nekodrop-storage/src/bundle.rs`
- Modify: `crates/nekodrop-storage/src/lib.rs`

- [x] **Step 1: Add failing list tests**

Add storage tests that prove:
- a missing staging root returns an empty list
- valid staged bundle directories are detected and sorted by bundle id

Run:

```bash
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekodrop-storage list_staged_bundles
```

Expected: FAIL because `list_staged_bundles` does not exist.

- [x] **Step 2: Add list implementation**

Add:

```rust
pub fn list_staged_bundles(staging_root: &Path) -> NekoDropResult<Vec<StagedBundle>>;
```

Behavior:
- return `Ok(Vec::new())` if `staging_root` does not exist
- error if `staging_root` exists but is not a directory
- skip non-directory entries
- reject unsafe entry names using existing staging id rules
- detect every bundle directory with `detect_bundle_directory`
- sort results by `manifest.bundle_id`

- [x] **Step 3: Re-export**

Export `list_staged_bundles` from `crates/nekodrop-storage/src/lib.rs`.

### Task 2: Delete Staged Bundle

**Files:**
- Modify: `crates/nekodrop-storage/src/bundle.rs`
- Modify: `crates/nekodrop-storage/src/lib.rs`

- [x] **Step 1: Add failing delete tests**

Add tests that prove:
- deleting a valid staged bundle removes only that bundle directory
- unsafe bundle ids are rejected before path joins
- deleting a missing safe id is a no-op

Run:

```bash
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekodrop-storage delete_staged_bundle
```

Expected: FAIL because `delete_staged_bundle` does not exist.

- [x] **Step 2: Add delete implementation**

Add:

```rust
pub fn delete_staged_bundle(staging_root: &Path, bundle_id: &str) -> NekoDropResult<bool>;
```

Behavior:
- validate `bundle_id` with existing staging id rules
- return `Ok(false)` if the target directory does not exist
- error if target exists but is not a directory
- remove the target directory with `fs::remove_dir_all`
- return `Ok(true)` when a directory was removed

- [x] **Step 3: Re-export**

Export `delete_staged_bundle` from `crates/nekodrop-storage/src/lib.rs`.

### Task 3: Prune Expired Staged Bundles

**Files:**
- Modify: `crates/nekodrop-storage/src/bundle.rs`
- Modify: `crates/nekodrop-storage/src/lib.rs`
- Modify: `docs/STATUS.md`

- [x] **Step 1: Add failing prune tests**

Add tests that prove:
- bundles older than a cutoff are deleted
- bundles at or newer than the cutoff are kept
- the returned ids are sorted

Run:

```bash
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekodrop-storage prune_staged_bundles
```

Expected: FAIL because `prune_staged_bundles_older_than` does not exist.

- [x] **Step 2: Add prune implementation**

Add:

```rust
pub fn prune_staged_bundles_older_than(
    staging_root: &Path,
    cutoff: SystemTime,
) -> NekoDropResult<Vec<String>>;
```

Behavior:
- use `list_staged_bundles` to validate and sort staged bundles
- compare each staging directory's filesystem modified time with `cutoff`
- delete only directories where `modified < cutoff`
- return the deleted bundle ids in sorted order

- [x] **Step 3: Update status docs**

Update `docs/STATUS.md` so the bundle row says storage can now list/delete/prune staged bundles. Keep bridge runtime, auth, UI lifecycle controls, and import execution marked pending.

### Task 4: Verification And PR

**Files:**
- Modify: `docs/superpowers/plans/2026-06-14-bundle-staging-lifecycle.md`

- [x] **Step 1: Mark completed plan checkboxes**

Update this plan's completed checkboxes before committing.

- [x] **Step 2: Verify**

Run:

```bash
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekodrop-storage
rustup run stable cargo fmt --all -- --check
git diff --check
```

- [ ] **Step 3: Commit and PR**

```bash
git add crates/nekodrop-storage/src/bundle.rs crates/nekodrop-storage/src/lib.rs docs/STATUS.md docs/superpowers/plans/2026-06-14-bundle-staging-lifecycle.md
git commit -m "feat: add bundle staging lifecycle storage"
git push -u origin storage/bundle-staging-lifecycle
```
