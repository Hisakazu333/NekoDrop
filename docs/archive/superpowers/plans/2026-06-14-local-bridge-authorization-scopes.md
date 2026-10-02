# Local Bridge Authorization Scopes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a generic local bridge authorization request model so desktop clients can ask for capabilities before mutating bundle state.

**Architecture:** Keep authorization as protocol shape plus desktop pending response only. `nekolink-protocol` owns the JSON request and validation rules; the desktop command handler parses it and returns `pending_auth` without granting, persisting, sending, or importing anything.

**Tech Stack:** Rust, serde JSON models, existing Tauri command helper tests.

---

### Task 1: Protocol Model

**Files:**
- Modify: `crates/nekolink-protocol/src/lib.rs`

- [x] **Step 1: Write failing tests**

Add tests for `authorization.request`, required scopes/reason, and ttl bounds.

- [x] **Step 2: Run protocol tests and see failure**

Run:

```bash
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekolink-protocol local_bridge_authorization
```

Expected failure: missing `LocalBridgeAuthorizationRequest`, `LocalBridgePermissionScope`, and `LocalBridgeRequest::AuthorizationRequest`.

- [x] **Step 3: Add minimal model and validation**

Add:

- `LocalBridgeRequest::AuthorizationRequest`
- `LocalBridgeAuthorizationRequest`
- `LocalBridgePermissionScope`

Scopes:

- `device.read`
- `transfer.status.read`
- `bundle.read`
- `bundle.send`
- `bundle.import.request`

Validation:

- `request_id` cannot be empty
- `client` is required and must pass existing client validation
- `requested_scopes` cannot be empty
- `reason` cannot be empty
- `ttl_seconds` must be `1..=604800` when present

- [x] **Step 4: Run protocol tests and see pass**

Run the same command. Expected: the three authorization tests pass.

### Task 2: Desktop Pending Response

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`

- [x] **Step 1: Write failing desktop test**

Add a test that sends `authorization.request` and expects:

- `status == "pending_auth"`
- `security_state == "requires_user_confirmation"`
- `requires_user_confirmation == true`
- identified client metadata echoed in the response

- [x] **Step 2: Run desktop test and see failure**

Run:

```bash
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekodrop-desktop local_bridge_authorization
```

Expected failure: non-exhaustive match for `LocalBridgeRequest::AuthorizationRequest`.

- [x] **Step 3: Add handler arm**

Return a pending authorization response. Do not persist authorization, issue tokens, send bundles, import bundles, or start a localhost runtime.

- [x] **Step 4: Run desktop test and see pass**

Run the same command. Expected: desktop authorization test passes.

### Task 3: Docs

**Files:**
- Modify: `docs/BUNDLE_SPEC.md`
- Modify: `docs/STATUS.md`

- [x] **Step 1: Update bridge request list**

Add `authorization.request` and list the generic scopes.

- [x] **Step 2: Update status**

Record that authorization scope modeling exists, while localhost runtime, persistent authorization, authorization codes, and import execution remain unfinished.
