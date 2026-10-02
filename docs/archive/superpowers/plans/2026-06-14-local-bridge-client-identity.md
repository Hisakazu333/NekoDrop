# Local Bridge Client Identity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a minimal local bridge client identity model so requests can identify the local caller before runtime authorization exists.

**Architecture:** Add an optional `client` object to each existing `LocalBridgeRequest` payload. The protocol validates safe client identifiers and display names; the desktop handler exposes whether a response came from an anonymous or identified caller. This PR does not persist client authorization, issue tokens, open localhost, or allow bundle send/import to run.

**Tech Stack:** Rust protocol crate, serde JSON models, Rust Tauri command layer, existing local bridge response DTO.

---

### Task 1: Protocol Client Identity

**Files:**
- Modify: `crates/nekolink-protocol/src/lib.rs`

- [x] **Step 1: Write failing protocol tests**

Add:

```rust
#[test]
fn local_bridge_request_accepts_optional_client_identity() {
    let request = LocalBridgeRequest::ListDevices(LocalBridgeListDevicesRequest {
        request_id: "bridge-request-1".to_string(),
        client: Some(LocalBridgeClientIdentity {
            client_id: "openneko-desktop".to_string(),
            display_name: "OpenNeko Desktop".to_string(),
            app_kind: Some("openneko".to_string()),
        }),
        trusted_only: true,
    });

    request.validate().unwrap();

    let json = serde_json::to_value(&request).unwrap();
    assert_eq!(json["kind"], "devices.list");
    assert_eq!(json["payload"]["client"]["client_id"], "openneko-desktop");
    assert_eq!(json["payload"]["client"]["display_name"], "OpenNeko Desktop");
    assert_eq!(json["payload"]["client"]["app_kind"], "openneko");
    assert_eq!(
        serde_json::from_value::<LocalBridgeRequest>(json).unwrap(),
        request
    );
}

#[test]
fn local_bridge_request_rejects_unsafe_client_identity() {
    let request = LocalBridgeRequest::ListDevices(LocalBridgeListDevicesRequest {
        request_id: "bridge-request-1".to_string(),
        client: Some(LocalBridgeClientIdentity {
            client_id: "../openneko".to_string(),
            display_name: "OpenNeko Desktop".to_string(),
            app_kind: Some("openneko".to_string()),
        }),
        trusted_only: true,
    });

    let error = request.validate().unwrap_err();

    assert!(error.message.contains("client_id"));
}
```

Run:

```bash
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekolink-protocol local_bridge_request_
```

Expected: FAIL because `LocalBridgeClientIdentity` and the `client` fields do not exist.

- [x] **Step 2: Add protocol model**

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalBridgeClientIdentity {
    pub client_id: String,
    pub display_name: String,
    pub app_kind: Option<String>,
}
```

Add `pub client: Option<LocalBridgeClientIdentity>` to:

- `LocalBridgeListDevicesRequest`
- `LocalBridgeSendBundleRequest`
- `LocalBridgeBundleDetailRequest`
- `LocalBridgeImportBundleRequest`
- `LocalBridgeTransferStatusRequest`

- [x] **Step 3: Add validation**

Add:

```rust
impl LocalBridgeClientIdentity {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_bridge_client_id(&self.client_id)?;
        validate_non_empty("client display_name", &self.display_name)?;
        validate_optional_non_empty("client app_kind", self.app_kind.as_deref())
    }
}
```

Add:

```rust
fn validate_optional_bridge_client(client: Option<&LocalBridgeClientIdentity>) -> Result<(), ProtocolError>
```

Each request `validate()` must call `validate_optional_bridge_client(self.client.as_ref())`.

Add `validate_bridge_client_id` with these constraints:

- non-empty
- ASCII alphanumeric, `_`, `-`, `.`
- maximum 80 bytes

- [x] **Step 4: Update existing local bridge tests**

All existing direct Rust constructors for local bridge requests must include `client: None`.

Run:

```bash
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekolink-protocol local_bridge_
```

Expected: PASS.

### Task 2: Desktop Response Client State

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`

- [x] **Step 1: Write failing desktop tests**

Add:

```rust
#[test]
fn local_bridge_response_marks_anonymous_client() {
    let dir = unique_bundle_temp_dir("local-bridge-client-anonymous");
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

    assert_eq!(response.client_state, "anonymous");
    assert!(response.client_id.is_none());
    assert!(response.client_display_name.is_none());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_response_echoes_identified_client() {
    let dir = unique_bundle_temp_dir("local-bridge-client-identified");
    let staging_root = dir.join("bundle_staging");
    let request = serde_json::json!({
        "kind": "transfer.status",
        "payload": {
            "request_id": "bridge-request-status",
            "client": {
                "client_id": "openneko-desktop",
                "display_name": "OpenNeko Desktop",
                "app_kind": "openneko"
            },
            "transfer_id": null
        }
    })
    .to_string();

    let response = handle_local_bridge_request_at(&request, &[], None, &staging_root).unwrap();

    assert_eq!(response.client_state, "identified");
    assert_eq!(response.client_id.as_deref(), Some("openneko-desktop"));
    assert_eq!(response.client_display_name.as_deref(), Some("OpenNeko Desktop"));

    fs::remove_dir_all(dir).unwrap();
}
```

Run:

```bash
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekodrop-desktop local_bridge_response_
```

Expected: FAIL because response client fields do not exist.

- [x] **Step 2: Add response fields**

Extend `LocalBridgeResponseDto`:

```rust
pub client_state: String,
pub client_id: Option<String>,
pub client_display_name: Option<String>,
```

Use stable `client_state` values:

```text
anonymous
identified
```

- [x] **Step 3: Extract client identity from request**

Add helper:

```rust
fn local_bridge_request_client(
    request: &LocalBridgeRequest,
) -> Option<nekolink_protocol::LocalBridgeClientIdentity>
```

It should clone the optional client from any request variant.

- [x] **Step 4: Thread client identity through response constructors**

Update:

- `local_bridge_read_only_response`
- `local_bridge_read_only_unsupported_response`
- `local_bridge_pending_confirmation_response`

Each constructor should accept `client: Option<LocalBridgeClientIdentity>` and fill:

- `client_state = "identified"` when client exists
- `client_state = "anonymous"` otherwise
- `client_id`
- `client_display_name`

Run:

```bash
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekodrop-desktop local_bridge_
```

Expected: PASS.

### Task 3: Docs, Verification, And PR

**Files:**
- Modify: `docs/BUNDLE_SPEC.md`
- Modify: `docs/STATUS.md`
- Modify: `docs/superpowers/plans/2026-06-14-local-bridge-client-identity.md`

- [x] **Step 1: Update docs**

Update `docs/BUNDLE_SPEC.md` to say bridge requests may include a local `client` identity, but it is only identification until authorization is implemented.

Update `docs/STATUS.md` to say local bridge responses can mark anonymous vs identified local callers; persisted authorization remains pending.

- [x] **Step 2: Verify**

Run:

```bash
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekolink-protocol local_bridge_
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekodrop-desktop local_bridge_
npm run build
rustup run stable cargo fmt --all -- --check
git diff --check
```

- [x] **Step 3: Commit and PR**

```bash
git add crates/nekolink-protocol/src/lib.rs apps/desktop/src-tauri/src/commands/mod.rs docs/BUNDLE_SPEC.md docs/STATUS.md docs/superpowers/plans/2026-06-14-local-bridge-client-identity.md
git commit -m "feat: add local bridge client identity"
git push -u origin bridge/client-identity
```
