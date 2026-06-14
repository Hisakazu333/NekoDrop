# Service Bundle Detected Event Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the service layer report a validated staged NekoLink bundle after a receive completes.

**Architecture:** Keep bundle validation and copying in `nekodrop-storage`. Add a small service DTO that summarizes staged bundle metadata and attach it to `TransferReceiveReport`. Existing receive entrypoints stay compatible and return `bundle: None`; new `*_with_bundle_staging` entrypoints opt into staging by passing an app-controlled staging root.

**Tech Stack:** Rust, existing TCP loopback service tests, `nekodrop-storage::stage_bundle_directory`, `nekolink-protocol` bundle metadata.

---

### Task 1: Service Report Shape

**Files:**
- Modify: `crates/nekodrop-service/src/lib.rs`

- [ ] **Step 1: Write failing receive test**

Add a loopback test that sends a valid bundle directory and receives it through a bundle-staging entrypoint:

```rust
#[test]
fn service_reports_staged_bundle_after_receive_completes() {
    let dir = unique_temp_dir("service-bundle-detected");
    let source_root = create_valid_bundle_source(&dir);
    let receive_dir = dir.join("receive");
    let staging_root = dir.join("staging");
    fs::create_dir_all(&receive_dir).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = Endpoint::tcp("127.0.0.1", listener.local_addr().unwrap().port());

    let receiver = thread::spawn({
        let receive_dir = receive_dir.clone();
        let staging_root = staging_root.clone();
        move || accept_transfer_with_bundle_staging(&listener, &receive_dir, &staging_root)
    });

    send_paths(&endpoint, &[source_root]).unwrap();
    let receive_report = receiver.join().unwrap().unwrap();
    let bundle = receive_report.bundle.expect("bundle should be reported");

    assert_eq!(bundle.bundle_id, "bundle_1234567890");
    assert_eq!(bundle.display_name, "voice_transcribe");
    assert_eq!(bundle.file_count, 2);
    assert_eq!(bundle.total_bytes, 28);
    assert_eq!(bundle.staging_path, staging_root.join("bundle_1234567890"));
    assert!(bundle.import_allowed);
    assert!(bundle.staging_path.join("bundle.json").is_file());

    fs::remove_dir_all(dir).unwrap();
}
```

Run:

```bash
rustup run stable cargo test -p nekodrop-service service_reports_staged_bundle_after_receive_completes
```

Expected: FAIL because `accept_transfer_with_bundle_staging` and the report field do not exist.

- [ ] **Step 2: Add service bundle summary type**

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedBundleReport {
    pub bundle_id: String,
    pub bundle_type: BundleType,
    pub display_name: String,
    pub source_app: String,
    pub file_count: usize,
    pub total_bytes: u64,
    pub staging_path: PathBuf,
    pub import_allowed: bool,
}
```

Add `pub bundle: Option<ReceivedBundleReport>` to `TransferReceiveReport`.

- [ ] **Step 3: Add opt-in receive entrypoints**

Add:

```rust
pub fn accept_transfer_with_bundle_staging(
    listener: &TcpListener,
    receive_dir: &Path,
    bundle_staging_root: &Path,
) -> NekoDropResult<TransferReceiveReport>;

pub fn accept_incoming_stream_with_encrypted_control_bundle_staging_and_cancel<D, H, P, C>(
    stream: &mut TcpStream,
    receive_dir: &Path,
    bundle_staging_root: &Path,
    receiver_identity: &DeviceIdentity,
    decide: D,
    handle_pairing: H,
    on_progress: P,
    should_cancel: C,
) -> NekoDropResult<IncomingSessionReport>;
```

Existing receive functions should call the internal helper with `None` for staging.

### Task 2: Bundle Detection After Receive

**Files:**
- Modify: `crates/nekodrop-service/src/lib.rs`
- Modify: `docs/STATUS.md`

- [ ] **Step 1: Implement staged bundle mapping**

After all expected files are received and verified, compute:

```rust
let received_root = receive_dir.join(&offer.root_name);
```

If a staging root was provided, call:

```rust
stage_bundle_directory(&received_root, bundle_staging_root)
```

Map `StagedBundle` into `ReceivedBundleReport`. If `bundle.json` is absent, return `bundle: None`. If bundle validation fails, return the storage error so callers do not treat an invalid bundle as an ordinary completed bundle.

- [ ] **Step 2: Preserve normal file transfer behavior**

Add or keep a test proving the default receive entrypoint returns `bundle: None` for ordinary folders.

Run:

```bash
rustup run stable cargo test -p nekodrop-service service_sends_selected_directory_and_receiver_writes_verified_files
```

Expected: PASS after updating assertions for the new optional field.

- [ ] **Step 3: Update status**

Update the `NekoLink bundle manifest` row in `docs/STATUS.md` to say service receive can now report staged bundles, while UI preview and local bridge import remain pending.

### Task 3: Verification And PR

**Files:**
- All files above.

- [ ] **Step 1: Run focused verification**

```bash
rustup run stable cargo test -p nekodrop-service bundle
rustup run stable cargo test -p nekodrop-service
rustup run stable cargo fmt --all -- --check
git diff --check
```

- [ ] **Step 2: Run workspace verification**

Use explicit stable toolchain if the shell resolves to an older Homebrew Rust:

```bash
CARGO_TARGET_DIR=target/stable-verify RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc /Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo test --workspace
```

- [ ] **Step 3: Commit and PR**

```bash
git add crates/nekodrop-service/src/lib.rs docs/STATUS.md docs/superpowers/plans/2026-06-14-service-bundle-detected-event.md
git commit -m "feat: report staged bundles from receive service"
git push -u origin service/bundle-detected-event
gh pr create --base main --head service/bundle-detected-event --title "Report staged bundles from receive service" --body-file <generated-body>
```
