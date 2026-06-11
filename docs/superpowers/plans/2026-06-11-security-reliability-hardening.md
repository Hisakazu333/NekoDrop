# Security Reliability Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the first production-readiness gaps before publishing new DMG/Windows installer assets.

**Architecture:** Keep the TCP transport and current desktop UX stable while tightening protocol limits, advertised capabilities, path validation, receive policy behavior, and Tauri app configuration. The changes are intentionally small and testable; no UI redesign or transport rewrite is included.

**Tech Stack:** Rust workspace, Tauri 2 desktop config, React/Vite frontend, existing Rust unit tests.

---

## File Map

- Modify `crates/nekodrop-network/src/tcp_file.rs` for file-frame count limits and expected count validation before allocation.
- Modify `crates/nekodrop-service/src/lib.rs` to receive file frames using the offer's expected `file_count`.
- Modify `apps/desktop/src-tauri/src/device_identity.rs` to advertise only implemented capabilities.
- Modify `apps/desktop/src-tauri/src/commands/mod.rs` for corrupted/manual path validation and receive policy behavior.
- Modify `apps/desktop/src/App.tsx` only if the receive policy UI must stop exposing auto-accept.
- Modify `apps/desktop/src-tauri/tauri.conf.json` for a minimal CSP.
- Update `docs/STATUS.md` and, if needed, `README.md` to match actual capability and receive policy behavior.

## Task 1: TCP File Count Hardening

**Files:**
- Modify: `crates/nekodrop-network/src/tcp_file.rs`
- Modify: `crates/nekodrop-service/src/lib.rs`

- [ ] **Step 1: Write failing tests**

Add tests proving a malicious count is rejected before allocating and that a count mismatch against an accepted offer fails immediately.

- [ ] **Step 2: Verify RED**

Run:

```bash
RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc /Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo test -p nekodrop-network tcp_file
```

Expected: new tests fail because no maximum count / expected count check exists.

- [ ] **Step 3: Implement**

Add a constant such as `MAX_FILE_FRAME_COUNT`, validate `read_file_count`, and add a `receive_file_frames_with_expected_count` path used by the service.

- [ ] **Step 4: Verify GREEN**

Run the same command and expect all `tcp_file` tests to pass.

## Task 2: Capability Declaration Cleanup

**Files:**
- Modify: `apps/desktop/src-tauri/src/device_identity.rs`
- Update: `docs/STATUS.md`

- [ ] **Step 1: Write failing test**

Add a desktop identity test asserting that `EncryptedSession` and `DesktopAgentHost` are not advertised until implemented.

- [ ] **Step 2: Verify RED**

Run:

```bash
RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc /Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo test -p nekodrop-desktop device_identity
```

Expected: the new test fails because those capabilities are currently present.

- [ ] **Step 3: Implement**

Remove unimplemented capabilities from `desktop_capabilities()`.

- [ ] **Step 4: Verify GREEN**

Run the same command and expect all device identity tests to pass.

## Task 3: Manual Path Safety

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Write failing tests**

Add tests for manual paths containing U+FFFD replacement characters, empty/quoted paths, and Windows reserved names or ADS-like names.

- [ ] **Step 2: Verify RED**

Run:

```bash
RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc /Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo test -p nekodrop-desktop commands::tests::manual_path
```

Expected: new path validation tests fail.

- [ ] **Step 3: Implement**

Extend `normalize_user_path` with preflight validation before `exists()`, returning actionable Chinese messages for corrupted paths and Windows-unsafe path components.

- [ ] **Step 4: Verify GREEN**

Run the same command and expect all manual path tests to pass.

## Task 4: Auto-Accept Risk Reduction

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`
- Modify: `apps/desktop/src/App.tsx`
- Update: `docs/STATUS.md`

- [ ] **Step 1: Write failing test**

Change the receive policy test so `auto_accept_trusted` no longer silently accepts transfers without explicit encrypted-session support.

- [ ] **Step 2: Verify RED**

Run:

```bash
RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc /Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo test -p nekodrop-desktop receive_policy
```

Expected: the changed receive policy test fails.

- [ ] **Step 3: Implement**

Keep parsing existing config values but make the current runtime treat `auto_accept_trusted` as `always_ask` until authenticated encrypted sessions exist. Update UI label if needed so users are not misled.

- [ ] **Step 4: Verify GREEN**

Run the same command and expect receive policy tests to pass.

## Task 5: Tauri CSP Configuration

**Files:**
- Modify: `apps/desktop/src-tauri/tauri.conf.json`

- [ ] **Step 1: Add minimal CSP**

Add a CSP that allows the app shell and inline styles required by current Vite/Tauri output, while blocking remote script execution by default.

- [ ] **Step 2: Verify config/build**

Run:

```bash
npm run build
```

Expected: frontend build still succeeds.

## Final Verification

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo test --workspace` with the explicit Rust 1.96 toolchain and local TCP permission.
- [ ] Run `npm run build`.
- [ ] Run `npm audit --omit=dev`.
- [ ] Run `git diff --check`.
- [ ] Inspect `git diff --stat` and confirm `apps/desktop/src/styles.css` remains unrelated and uncommitted unless explicitly requested.
