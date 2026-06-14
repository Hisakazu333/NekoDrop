# Local Bridge Protocol Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Define the first stable JSON model for CCS/OpenNeko local bridge requests and bundle events.

**Architecture:** Add protocol-only types in `nekolink-protocol` so the bridge contract is not tied to Tauri command DTOs or NekoDrop internal structs. This PR only defines request/event payloads and validation; it does not start a localhost server, does not implement bridge auth, and does not import bundles.

**Tech Stack:** Rust, serde, existing `ProtocolError`, existing `BundleType` and path/target validators in `crates/nekolink-protocol/src/lib.rs`.

---

### Task 1: Bridge Request Types

**Files:**
- Modify: `crates/nekolink-protocol/src/lib.rs`

- [x] **Step 1: Add failing request tests**

Add tests near the existing protocol tests:

```rust
#[test]
fn local_bridge_send_bundle_request_uses_stable_json_shape() {
    let request = LocalBridgeRequest::SendBundle(LocalBridgeSendBundleRequest {
        request_id: "bridge-request-1".to_string(),
        target_device_id: Some("device-b".to_string()),
        bundle_root: "bundle".to_string(),
        bundle_type: BundleType::Skill,
        require_trusted_device: true,
    });

    request.validate().unwrap();

    let json = serde_json::to_value(&request).unwrap();
    assert_eq!(json["kind"], "bundle.send");
    assert_eq!(json["payload"]["request_id"], "bridge-request-1");
    assert_eq!(json["payload"]["target_device_id"], "device-b");
    assert_eq!(json["payload"]["bundle_root"], "bundle");
    assert_eq!(json["payload"]["bundle_type"], "skill");
    assert_eq!(json["payload"]["require_trusted_device"], true);
    assert_eq!(
        serde_json::from_value::<LocalBridgeRequest>(json).unwrap(),
        request
    );
}

#[test]
fn local_bridge_rejects_unsafe_bundle_roots() {
    let request = LocalBridgeRequest::SendBundle(LocalBridgeSendBundleRequest {
        request_id: "bridge-request-1".to_string(),
        target_device_id: None,
        bundle_root: "../bundle".to_string(),
        bundle_type: BundleType::Skill,
        require_trusted_device: true,
    });

    let error = request.validate().unwrap_err();

    assert!(error.message.contains("bundle_root"));
}
```

Run:

```bash
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekolink-protocol local_bridge_send_bundle_request_uses_stable_json_shape
```

Expected: FAIL because local bridge types do not exist.

- [x] **Step 2: Add request model**

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload")]
pub enum LocalBridgeRequest {
    #[serde(rename = "devices.list")]
    ListDevices(LocalBridgeListDevicesRequest),
    #[serde(rename = "bundle.send")]
    SendBundle(LocalBridgeSendBundleRequest),
    #[serde(rename = "bundle.import")]
    ImportBundle(LocalBridgeImportBundleRequest),
    #[serde(rename = "transfer.status")]
    TransferStatus(LocalBridgeTransferStatusRequest),
}
```

Request structs:

```rust
pub struct LocalBridgeListDevicesRequest {
    pub request_id: String,
    pub trusted_only: bool,
}

pub struct LocalBridgeSendBundleRequest {
    pub request_id: String,
    pub target_device_id: Option<String>,
    pub bundle_root: String,
    pub bundle_type: BundleType,
    pub require_trusted_device: bool,
}

pub struct LocalBridgeImportBundleRequest {
    pub request_id: String,
    pub staged_bundle_id: String,
    pub expected_bundle_type: Option<BundleType>,
}

pub struct LocalBridgeTransferStatusRequest {
    pub request_id: String,
    pub transfer_id: Option<String>,
}
```

Validation:
- every `request_id` must be non-empty
- optional ids must be non-empty when present
- `bundle_root` must be a safe relative logical path and must not be absolute or contain `..`
- `staged_bundle_id` must be non-empty and must not contain `/`, `\`, `..`, `:`, NUL, or leading/trailing whitespace

### Task 2: Bridge Event Types

**Files:**
- Modify: `crates/nekolink-protocol/src/lib.rs`

- [x] **Step 1: Add failing event tests**

Add:

```rust
#[test]
fn local_bridge_bundle_received_event_uses_stable_json_shape() {
    let event = LocalBridgeEvent::BundleReceived(LocalBridgeBundleReceivedEvent {
        event_id: "bridge-event-1".to_string(),
        transfer_id: "transfer-1".to_string(),
        bundle_id: "bundle_1234567890".to_string(),
        bundle_type: BundleType::Skill,
        display_name: "voice_transcribe".to_string(),
        source_app: "OpenNeko".to_string(),
        file_count: 2,
        total_bytes: 28,
        import_allowed: true,
    });

    event.validate().unwrap();

    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["kind"], "bundle.received");
    assert_eq!(json["payload"]["event_id"], "bridge-event-1");
    assert_eq!(json["payload"]["bundle_type"], "skill");
    assert_eq!(
        serde_json::from_value::<LocalBridgeEvent>(json).unwrap(),
        event
    );
}
```

Run:

```bash
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekolink-protocol local_bridge_bundle_received_event_uses_stable_json_shape
```

Expected: FAIL because event types do not exist.

- [x] **Step 2: Add event model**

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload")]
pub enum LocalBridgeEvent {
    #[serde(rename = "bundle.received")]
    BundleReceived(LocalBridgeBundleReceivedEvent),
    #[serde(rename = "transfer.updated")]
    TransferUpdated(LocalBridgeTransferUpdatedEvent),
}
```

Event structs:

```rust
pub struct LocalBridgeBundleReceivedEvent {
    pub event_id: String,
    pub transfer_id: String,
    pub bundle_id: String,
    pub bundle_type: BundleType,
    pub display_name: String,
    pub source_app: String,
    pub file_count: usize,
    pub total_bytes: u64,
    pub import_allowed: bool,
}

pub struct LocalBridgeTransferUpdatedEvent {
    pub event_id: String,
    pub transfer_id: String,
    pub phase: LocalBridgeTransferPhase,
    pub bytes_transferred: u64,
    pub total_bytes: u64,
}
```

`LocalBridgeTransferPhase` uses snake_case labels: `queued`, `sending`, `receiving`, `completed`, `failed`, `cancelled`.

Validation:
- ids, names, and source app must be non-empty
- `file_count > 0` for received bundle event
- `total_bytes` must be at least `bytes_transferred` for transfer update

### Task 3: Docs And Verification

**Files:**
- Modify: `docs/STATUS.md`
- Modify: `docs/BUNDLE_SPEC.md`

- [x] **Step 1: Update status**

Update the bundle/local bridge rows so they say the local bridge protocol model exists, while runtime bridge server, auth, and import action remain pending.

- [x] **Step 2: Update bundle spec**

In the local bridge section, mention that the first contract uses `LocalBridgeRequest` and `LocalBridgeEvent` JSON envelopes in `nekolink-protocol`.

- [x] **Step 3: Verify**

```bash
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekolink-protocol local_bridge_
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekolink-protocol
rustup run stable cargo fmt --all -- --check
git diff --check
```

- [ ] **Step 4: Commit and PR**

```bash
git add crates/nekolink-protocol/src/lib.rs docs/STATUS.md docs/BUNDLE_SPEC.md docs/superpowers/plans/2026-06-14-local-bridge-protocol.md
git commit -m "feat: add local bridge protocol model"
git push -u origin bridge/local-bundle-protocol
```
