# Desktop Bundle Staging Actions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose staged bundle list/delete actions in the desktop backend and add a small receive UI action for deleting a saved staged bundle.

**Architecture:** Reuse `nekodrop-storage` staging lifecycle APIs from the desktop Tauri command layer. Keep the UI limited to the existing receive completion area; this PR does not implement bundle import, local bridge runtime, bridge auth, or a new bundle management page.

**Tech Stack:** Rust Tauri commands, existing `ReceivedBundleDto`, `nekodrop_storage::{list_staged_bundles, delete_staged_bundle}`, React/TypeScript desktop UI.

---

### Task 1: Desktop Staged Bundle Commands

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`
- Modify: `apps/desktop/src-tauri/src/main.rs`

- [x] **Step 1: Add failing command DTO tests**

Add tests in `apps/desktop/src-tauri/src/commands/mod.rs` that call helper functions for staged bundle list/delete using an explicit staging root:

```rust
#[test]
fn staged_bundle_dto_marks_saved_status() {
    let dir = unique_bundle_temp_dir("desktop-bundle-list");
    let staging_root = dir.join("bundle_staging");
    let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
    nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();

    let bundles = list_staged_bundle_dtos_at(&staging_root).unwrap();

    assert_eq!(bundles.len(), 1);
    assert_eq!(bundles[0].bundle_id, "bundle_1234567890");
    assert_eq!(bundles[0].staging_status, "saved");
    assert!(!bundles[0].can_import_now);

    fs::remove_dir_all(dir).unwrap();
}
```

Add a delete test:

```rust
#[test]
fn delete_staged_bundle_at_removes_saved_bundle() {
    let dir = unique_bundle_temp_dir("desktop-bundle-delete");
    let staging_root = dir.join("bundle_staging");
    let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
    nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();

    let removed = delete_staged_bundle_at(&staging_root, "bundle_1234567890").unwrap();

    assert!(removed);
    assert!(!staging_root.join("bundle_1234567890").exists());

    fs::remove_dir_all(dir).unwrap();
}
```

Run:

```bash
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekodrop-desktop staged_bundle_
```

Expected: FAIL because the helper functions and DTO fields do not exist.

- [x] **Step 2: Add DTO fields and helpers**

Extend `ReceivedBundleDto` with:

```rust
pub staging_status: String,
pub can_import_now: bool,
```

Set:
- `staging_status = "saved"`
- `can_import_now = false`

Add helper functions:

```rust
fn list_staged_bundle_dtos_at(staging_root: &Path) -> Result<Vec<ReceivedBundleDto>, String>;
fn delete_staged_bundle_at(staging_root: &Path, bundle_id: &str) -> Result<bool, String>;
```

- [x] **Step 3: Add Tauri commands**

Add:

```rust
#[tauri::command]
pub fn list_staged_bundles() -> Result<Vec<ReceivedBundleDto>, String>;

#[tauri::command]
pub fn delete_staged_bundle(bundle_id: String) -> Result<bool, String>;
```

Register both commands in `apps/desktop/src-tauri/src/main.rs`.

### Task 2: Frontend Types And Calls

**Files:**
- Modify: `apps/desktop/src/types.ts`
- Modify: `apps/desktop/src/tauri.ts`
- Modify: `apps/desktop/src/App.tsx`

- [x] **Step 1: Extend frontend DTO**

Add to `ReceivedBundleDto`:

```ts
staging_status: "saved" | "deleted" | string;
can_import_now: boolean;
```

- [x] **Step 2: Add command names**

Add command names:

```ts
| "list_staged_bundles"
| "delete_staged_bundle"
```

- [x] **Step 3: Add delete handler**

In `App.tsx`, add a handler that calls `delete_staged_bundle`, then removes `receiveReport.bundle` from the current report when it matches the deleted bundle id. Do not clear the whole receive report.

### Task 3: Compact Receive UI

**Files:**
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/styles.css`

- [x] **Step 1: Pass delete handler into receive panel**

Pass `onDeleteStagedBundle` from the app root into `ReceivePanel`.

- [x] **Step 2: Show saved state and delete action**

In the existing bundle line under "最近完成":

- show `已保存` next to the existing `可导入` / `仅保存` status
- show a small `删除暂存` button
- keep the line compact

- [x] **Step 3: Keep summary bar compact**

In the global status line, append `已保存` for a staged bundle but do not add buttons there.

### Task 4: Docs And Verification

**Files:**
- Modify: `docs/STATUS.md`
- Modify: `docs/superpowers/plans/2026-06-14-desktop-bundle-staging-actions.md`

- [x] **Step 1: Update status docs**

Update the bundle row to say desktop UI can show saved staged bundle state and delete the saved staged bundle. Keep import, local bridge runtime, and auth pending.

- [x] **Step 2: Mark completed checkboxes**

Mark completed steps in this plan before committing.

- [x] **Step 3: Verify**

Run:

```bash
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekodrop-desktop staged_bundle_
npm run build
rustup run stable cargo fmt --all -- --check
git diff --check
```

- [ ] **Step 4: Commit and PR**

```bash
git add apps/desktop/src-tauri/src/commands/mod.rs apps/desktop/src-tauri/src/main.rs apps/desktop/src/types.ts apps/desktop/src/tauri.ts apps/desktop/src/App.tsx apps/desktop/src/styles.css docs/STATUS.md docs/superpowers/plans/2026-06-14-desktop-bundle-staging-actions.md
git commit -m "feat: add desktop bundle staging actions"
git push -u origin desktop/bundle-staging-actions
```
