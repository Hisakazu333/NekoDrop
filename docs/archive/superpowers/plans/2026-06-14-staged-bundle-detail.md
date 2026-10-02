# Staged Bundle Detail Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a read-only `bundle.detail` local bridge request that returns one staged bundle summary by id.

**Architecture:** Extend `nekolink-protocol::LocalBridgeRequest` with a `bundle.detail` request that validates `staged_bundle_id`. The desktop bridge handler will reuse the existing staging list reader, filter by id, and return a read-only response containing the matching bundle in `staged_bundles`. This PR must not import bundles, send bundles, start a localhost runtime, or add authorization persistence.

**Tech Stack:** Rust protocol crate, Rust Tauri command layer, existing bundle staging DTOs, existing local bridge response auth gate.

---

### Task 1: Protocol Request Shape

**Files:**
- Modify: `crates/nekolink-protocol/src/lib.rs`

- [x] **Step 1: Write failing protocol test**

Add this test near the existing local bridge tests:

```rust
#[test]
fn local_bridge_bundle_detail_request_uses_stable_json_shape() {
    let request = LocalBridgeRequest::BundleDetail(LocalBridgeBundleDetailRequest {
        request_id: "bridge-request-detail".to_string(),
        staged_bundle_id: "bundle_1234567890".to_string(),
    });

    request.validate().unwrap();

    let json = serde_json::to_value(&request).unwrap();
    assert_eq!(json["kind"], "bundle.detail");
    assert_eq!(json["payload"]["request_id"], "bridge-request-detail");
    assert_eq!(json["payload"]["staged_bundle_id"], "bundle_1234567890");
    assert_eq!(
        serde_json::from_value::<LocalBridgeRequest>(json).unwrap(),
        request
    );
}
```

Run:

```bash
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekolink-protocol local_bridge_bundle_detail
```

Expected: FAIL because `BundleDetail` and `LocalBridgeBundleDetailRequest` do not exist.

- [x] **Step 2: Implement protocol type**

Add:

```rust
#[serde(rename = "bundle.detail")]
BundleDetail(LocalBridgeBundleDetailRequest),
```

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalBridgeBundleDetailRequest {
    pub request_id: String,
    pub staged_bundle_id: String,
}
```

Wire `LocalBridgeRequest::validate()` to call `LocalBridgeBundleDetailRequest::validate()`.

Add:

```rust
impl LocalBridgeBundleDetailRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_non_empty("request_id", &self.request_id)?;
        validate_staged_bundle_id(&self.staged_bundle_id)
    }
}
```

- [x] **Step 3: Verify protocol test passes**

Run:

```bash
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekolink-protocol local_bridge_bundle_detail
```

Expected: PASS.

### Task 2: Desktop Handler Detail Response

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`

- [x] **Step 1: Write failing desktop tests**

Add these tests near the existing local bridge tests:

```rust
#[test]
fn local_bridge_bundle_detail_returns_matching_staged_bundle() {
    let dir = unique_bundle_temp_dir("local-bridge-bundle-detail");
    let staging_root = dir.join("bundle_staging");
    let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
    nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();
    let request = serde_json::json!({
        "kind": "bundle.detail",
        "payload": {
            "request_id": "bridge-request-detail",
            "staged_bundle_id": "bundle_1234567890"
        }
    })
    .to_string();

    let response = handle_local_bridge_request_at(&request, &[], None, &staging_root).unwrap();

    assert_eq!(response.request_id, "bridge-request-detail");
    assert_eq!(response.status, "ok");
    assert_eq!(response.security_state, "read_only");
    assert_eq!(response.staged_bundles.len(), 1);
    assert_eq!(response.staged_bundles[0].bundle_id, "bundle_1234567890");
    assert!(!response.requires_user_confirmation);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_bridge_bundle_detail_returns_unsupported_for_missing_bundle() {
    let dir = unique_bundle_temp_dir("local-bridge-bundle-detail-missing");
    let staging_root = dir.join("bundle_staging");
    let request = serde_json::json!({
        "kind": "bundle.detail",
        "payload": {
            "request_id": "bridge-request-detail",
            "staged_bundle_id": "bundle_1234567890"
        }
    })
    .to_string();

    let response = handle_local_bridge_request_at(&request, &[], None, &staging_root).unwrap();

    assert_eq!(response.request_id, "bridge-request-detail");
    assert_eq!(response.status, "unsupported");
    assert_eq!(response.security_state, "read_only");
    assert!(response.staged_bundles.is_empty());
    assert!(response.message.contains("not found"));

    fs::remove_dir_all(dir).unwrap();
}
```

Run:

```bash
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekodrop-desktop local_bridge_bundle_detail
```

Expected: FAIL because the handler does not match `LocalBridgeRequest::BundleDetail`.

- [x] **Step 2: Implement detail helper**

Add:

```rust
fn find_staged_bundle_dto_at(
    staging_root: &std::path::Path,
    bundle_id: &str,
) -> Result<Option<ReceivedBundleDto>, String> {
    Ok(list_staged_bundle_dtos_at(staging_root)?
        .into_iter()
        .find(|bundle| bundle.bundle_id == bundle_id))
}
```

Add:

```rust
fn local_bridge_read_only_unsupported_response(
    request_id: String,
    message: &str,
) -> LocalBridgeResponseDto
```

It must use:

```text
status = "unsupported"
security_state = "read_only"
requires_user_confirmation = false
```

- [x] **Step 3: Wire handler match arm**

In `handle_local_bridge_request_at`, add a `LocalBridgeRequest::BundleDetail(request)` arm:

- if bundle exists, return read-only response with one item in `staged_bundles`
- if missing, return read-only unsupported response with message `staged bundle not found`

- [x] **Step 4: Verify desktop tests pass**

Run:

```bash
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekodrop-desktop local_bridge_bundle_detail
```

Expected: PASS.

### Task 3: Docs, Verification, And PR

**Files:**
- Modify: `docs/BUNDLE_SPEC.md`
- Modify: `docs/STATUS.md`
- Modify: `docs/superpowers/plans/2026-06-14-staged-bundle-detail.md`

- [x] **Step 1: Update docs**

Update `docs/BUNDLE_SPEC.md` request list to include `bundle.detail`.

Update `docs/STATUS.md` to say local bridge can query one staged bundle summary by id.

- [x] **Step 2: Verify**

Run:

```bash
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekolink-protocol local_bridge_bundle_detail
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekodrop-desktop local_bridge_
npm run build
rustup run stable cargo fmt --all -- --check
git diff --check
```

- [x] **Step 3: Commit and PR**

```bash
git add crates/nekolink-protocol/src/lib.rs apps/desktop/src-tauri/src/commands/mod.rs docs/BUNDLE_SPEC.md docs/STATUS.md docs/superpowers/plans/2026-06-14-staged-bundle-detail.md
git commit -m "feat: add local bridge staged bundle detail"
git push -u origin bridge/staged-bundle-detail
```
