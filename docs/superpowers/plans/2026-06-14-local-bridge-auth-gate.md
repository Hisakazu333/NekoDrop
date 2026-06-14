# Local Bridge Auth Gate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a small, testable local bridge authorization gate so read-only requests and mutating requests expose different security states before a localhost runtime exists.

**Architecture:** Keep the gate inside the existing desktop Tauri command helper. The bridge still parses `nekolink-protocol::LocalBridgeRequest`; this PR only adds response metadata that says whether the request is read-only or requires user confirmation. `bundle.send` and `bundle.import` must remain blocked as `pending_auth`.

**Tech Stack:** Rust Tauri command layer, `serde`, existing local bridge request model, existing staged bundle and device DTOs.

---

### Task 1: Response Security Metadata

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`

- [x] **Step 1: Write failing tests**

Add tests for two behaviors:

```rust
#[test]
fn local_bridge_read_only_requests_are_marked_read_only() {
    let dir = unique_bundle_temp_dir("local-bridge-read-only-security");
    let staging_root = dir.join("bundle_staging");
    let request = serde_json::json!({
        "kind": "transfer.status",
        "payload": {
            "request_id": "bridge-request-status",
            "transfer_id": null
        }
    })
    .to_string();

    let response = handle_local_bridge_request_at(&request, &[], None, &staging_root).unwrap();

    assert_eq!(response.status, "ok");
    assert_eq!(response.security_state, "read_only");
    assert!(!response.requires_user_confirmation);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_mutating_requests_require_user_confirmation() {
    let dir = unique_bundle_temp_dir("local-bridge-mutating-security");
    let staging_root = dir.join("bundle_staging");
    let request = serde_json::json!({
        "kind": "bundle.import",
        "payload": {
            "request_id": "bridge-request-import",
            "staged_bundle_id": "bundle_1234567890",
            "expected_bundle_type": "skill"
        }
    })
    .to_string();

    let response = handle_local_bridge_request_at(&request, &[], None, &staging_root).unwrap();

    assert_eq!(response.status, "pending_auth");
    assert_eq!(response.security_state, "requires_user_confirmation");
    assert!(response.requires_user_confirmation);
    assert!(response.message.contains("user confirmation"));

    fs::remove_dir_all(dir).unwrap();
}
```

Run:

```bash
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekodrop-desktop local_bridge_
```

Expected: FAIL because `security_state` and `requires_user_confirmation` do not exist.

- [x] **Step 2: Add response fields**

Extend `LocalBridgeResponseDto`:

```rust
pub struct LocalBridgeResponseDto {
    pub request_id: String,
    pub status: String,
    pub message: String,
    pub security_state: String,
    pub requires_user_confirmation: bool,
    pub devices: Vec<TrustedDeviceDto>,
    pub staged_bundles: Vec<ReceivedBundleDto>,
    pub transfer_status: Option<TransferStatusDto>,
}
```

Use stable string values:

```text
read_only
requires_user_confirmation
```

- [x] **Step 3: Add helper constructors**

Add focused constructors:

```rust
fn local_bridge_read_only_response(
    request_id: String,
    message: &str,
    devices: Vec<TrustedDeviceDto>,
    staged_bundles: Vec<ReceivedBundleDto>,
    transfer_status: Option<TransferStatusDto>,
) -> LocalBridgeResponseDto

fn local_bridge_pending_confirmation_response(request_id: String) -> LocalBridgeResponseDto
```

The pending confirmation message must be:

```text
local bridge auth runtime is not connected; user confirmation is required before this request can run
```

- [x] **Step 4: Wire helper usage**

Update `handle_local_bridge_request_at`:

- `devices.list` uses `local_bridge_read_only_response`
- `transfer.status` uses `local_bridge_read_only_response`
- `bundle.send` uses `local_bridge_pending_confirmation_response`
- `bundle.import` uses `local_bridge_pending_confirmation_response`

Run the local bridge tests again and verify they pass.

### Task 2: Docs And Verification

**Files:**
- Modify: `docs/BUNDLE_SPEC.md`
- Modify: `docs/STATUS.md`
- Modify: `docs/superpowers/plans/2026-06-14-local-bridge-auth-gate.md`

- [x] **Step 1: Update docs**

Update `docs/BUNDLE_SPEC.md` to say the desktop skeleton marks read-only bridge requests separately from requests that require user confirmation.

Update `docs/STATUS.md` to say the bridge handler now exposes a small auth gate state, while localhost runtime, persisted client authorization, and import execution remain pending.

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
git add apps/desktop/src-tauri/src/commands/mod.rs docs/BUNDLE_SPEC.md docs/STATUS.md docs/superpowers/plans/2026-06-14-local-bridge-auth-gate.md
git commit -m "feat: add local bridge auth gate"
git push -u origin bridge/local-auth-gate
```
