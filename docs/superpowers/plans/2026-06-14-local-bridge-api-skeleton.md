# Local Bridge API Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a desktop-side local bridge request handler skeleton that can answer safe read-only requests and return explicit pending responses for mutating bundle actions.

**Architecture:** Keep this as an internal Tauri command and pure helper first. It parses `nekolink-protocol::LocalBridgeRequest`, validates it, and returns a stable JSON response shape. This PR does not start a localhost server, does not implement auth tokens, does not send bundles, and does not import staged bundles.

**Tech Stack:** Rust Tauri command layer, `serde_json`, existing `LocalBridgeRequest` protocol types, existing trusted device DTOs, staged bundle DTOs, and transfer status DTOs.

---

### Task 1: Response DTO And Read-Only Handler

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`

- [x] **Step 1: Add failing handler tests**

Add tests for a pure helper:

```rust
fn local_bridge_devices_list_returns_trusted_devices_and_staged_bundles()
```

It should build:
- one trusted device
- one staged bundle in a temp staging root
- request JSON for `devices.list`

Expected response:
- `request_id == "bridge-request-1"`
- `status == "ok"`
- one trusted device
- one staged bundle
- no side effects

Run:

```bash
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekodrop-desktop local_bridge_
```

Expected: FAIL because bridge response types and helpers do not exist.

- [x] **Step 2: Add response DTO**

Add:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct LocalBridgeResponseDto {
    pub request_id: String,
    pub status: String,
    pub message: String,
    pub devices: Vec<TrustedDeviceDto>,
    pub staged_bundles: Vec<ReceivedBundleDto>,
    pub transfer_status: Option<TransferStatusDto>,
}
```

Response status values in this PR:
- `ok`
- `pending_auth`
- `unsupported`

- [x] **Step 3: Add read-only helper**

Add:

```rust
fn handle_local_bridge_request_at(
    request_json: &str,
    trusted_devices: &[TrustedDeviceRecord],
    transfer_status: Option<&TransferStatusState>,
    staging_root: &Path,
) -> Result<LocalBridgeResponseDto, String>;
```

Support:
- `devices.list`: returns trusted devices, filtered by `trusted_only` if needed, and current staged bundles
- `transfer.status`: returns current transfer status if present

### Task 2: Explicit Pending Mutations

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`

- [x] **Step 1: Add failing pending tests**

Add tests that prove:
- `bundle.send` returns `pending_auth`
- `bundle.import` returns `pending_auth`
- the response message says auth/runtime is not connected yet

- [x] **Step 2: Implement pending responses**

For `bundle.send` and `bundle.import`, validate the protocol request and return:

```text
status: "pending_auth"
message: "local bridge auth and runtime are not connected yet"
```

Do not send files. Do not import bundles.

### Task 3: Tauri Command And Docs

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`
- Modify: `apps/desktop/src-tauri/src/main.rs`
- Modify: `apps/desktop/src/tauri.ts`
- Modify: `docs/STATUS.md`
- Modify: `docs/BUNDLE_SPEC.md`

- [x] **Step 1: Add command**

Add:

```rust
#[tauri::command]
pub fn handle_local_bridge_request(
    state: State<'_, AppState>,
    request_json: String,
) -> Result<LocalBridgeResponseDto, String>;
```

The command should read trusted devices and transfer status from `AppState`, use the desktop staging root, and call the pure helper.

- [x] **Step 2: Register command**

Register `commands::handle_local_bridge_request` in `apps/desktop/src-tauri/src/main.rs`.

- [x] **Step 3: Add frontend command name**

Add `handle_local_bridge_request` to `apps/desktop/src/tauri.ts` command union. No UI calls it yet.

- [x] **Step 4: Update docs**

Update status/spec text:
- internal desktop bridge handler skeleton exists
- localhost runtime, auth, send, import, and automatic plugin access remain pending

### Task 4: Verification And PR

**Files:**
- Modify: `docs/superpowers/plans/2026-06-14-local-bridge-api-skeleton.md`

- [x] **Step 1: Mark completed checkboxes**

Mark completed steps in this plan before committing.

- [x] **Step 2: Verify**

Run:

```bash
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekodrop-desktop local_bridge_
npm run build
rustup run stable cargo fmt --all -- --check
git diff --check
```

- [x] **Step 3: Commit and PR**

```bash
git add apps/desktop/src-tauri/src/commands/mod.rs apps/desktop/src-tauri/src/main.rs apps/desktop/src/tauri.ts docs/STATUS.md docs/BUNDLE_SPEC.md docs/superpowers/plans/2026-06-14-local-bridge-api-skeleton.md
git commit -m "feat: add local bridge api skeleton"
git push -u origin bridge/local-api-skeleton
```
