# Desktop Bundle Receive Preview Report Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose received NekoLink bundle staging metadata from the desktop backend to the frontend DTO.

**Architecture:** Reuse the service-layer bundle staging entrypoint merged in PR #26. The desktop receive loop stores staged bundles under the app config directory, keeps normal receive behavior unchanged, and includes a compact `bundle` object in `ReceiveReportDto`. This PR does not redesign the React UI and does not import bundles into CCS/OpenNeko.

**Tech Stack:** Rust Tauri commands, `nekodrop-service::ReceivedBundleReport`, app config directory helpers, existing desktop command tests.

---

### Task 1: DTO Shape

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Write failing DTO test**

Add a test that builds a `TransferReceiveReport` with `ReceivedBundleReport` and checks `receive_report_to_dto` exposes:

```rust
assert_eq!(dto.bundle.as_ref().unwrap().bundle_id, "bundle_1234567890");
assert_eq!(dto.bundle.as_ref().unwrap().bundle_type, "skill");
assert_eq!(dto.bundle.as_ref().unwrap().display_name, "voice_transcribe");
assert_eq!(dto.bundle.as_ref().unwrap().staging_path, "/tmp/bundle_1234567890");
assert!(dto.bundle.as_ref().unwrap().import_allowed);
```

Run:

```bash
rustup run stable cargo test -p nekodrop-desktop receive_report_dto_includes_bundle_preview
```

Expected: FAIL because `ReceiveReportDto` does not expose a `bundle` field yet.

- [ ] **Step 2: Add backend DTO type**

Add:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ReceivedBundleDto {
    pub bundle_id: String,
    pub bundle_type: String,
    pub display_name: String,
    pub source_app: String,
    pub file_count: usize,
    pub total_bytes: u64,
    pub staging_path: String,
    pub import_allowed: bool,
}
```

Add `pub bundle: Option<ReceivedBundleDto>` to `ReceiveReportDto`.

- [ ] **Step 3: Map service report to DTO**

Update `receive_report_to_dto` so `report.bundle.as_ref()` maps into `ReceivedBundleDto`.

### Task 2: Receive Loop Staging Root

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`
- Modify: `apps/desktop/src/types.ts`

- [ ] **Step 1: Add staging root helper**

Create:

```rust
fn bundle_staging_root() -> Result<PathBuf, String> {
    Ok(app_config_dir()?.join("bundle_staging"))
}
```

The receive loop should call `fs::create_dir_all(&staging_root)` before spawning the listener thread.

- [ ] **Step 2: Use bundle staging receive entrypoint**

Replace the receive thread call:

```rust
accept_incoming_stream_with_encrypted_control_and_cancel(...)
```

with:

```rust
accept_incoming_stream_with_encrypted_control_bundle_staging_and_cancel(
    &mut stream,
    &receive_dir_for_thread,
    &bundle_staging_root_for_thread,
    &local_identity,
    ...
)
```

Keep pairing, transfer decisions, progress, cancel behavior, history, and last receive report handling unchanged.

- [ ] **Step 3: Update TypeScript type**

Add matching `ReceivedBundleDto` and `bundle: ReceivedBundleDto | null` or optional bundle field to `ReceiveReportDto` in `apps/desktop/src/types.ts`.

### Task 3: Status And Verification

**Files:**
- Modify: `docs/STATUS.md`

- [ ] **Step 1: Update status**

Update the `NekoLink bundle manifest` row to say desktop backend DTO now exposes staged bundle metadata. Keep UI preview and local bridge import marked pending.

- [ ] **Step 2: Run focused verification**

```bash
rustup run stable cargo test -p nekodrop-desktop receive_report_dto
rustup run stable cargo test -p nekodrop-desktop
rustup run stable cargo fmt --all -- --check
npm run build
git diff --check
```

- [ ] **Step 3: Run workspace verification**

```bash
rustup run stable env CARGO_TARGET_DIR=target/stable-verify RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test --workspace
```

- [ ] **Step 4: Commit and PR**

```bash
git add apps/desktop/src-tauri/src/commands/mod.rs apps/desktop/src/types.ts docs/STATUS.md docs/superpowers/plans/2026-06-14-desktop-bundle-receive-preview-report.md
git commit -m "feat: expose received bundle preview in desktop"
git push -u origin desktop/bundle-receive-preview-report
```
