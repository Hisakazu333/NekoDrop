# Session Control Desktop Wiring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Route desktop file-transfer offer and decision control messages through a verified encrypted session while keeping the file payload stream unchanged.

**Architecture:** Keep the existing plaintext APIs for sidecar and compatibility tests. Add explicit encrypted-control service entry points that perform `session.hello`, `session.ready`, encrypted `file.offer`, and encrypted `file.accept/file.decline`, then reuse the current file frame send/receive code. Wire the Tauri desktop sender and receiver to those new entry points and advertise `encrypted_session` only after the desktop path is actually using it.

**Tech Stack:** Rust workspace, `nekolink-protocol` session primitives, `nekodrop-network` TCP JSON frame helpers, `nekodrop-service` transfer orchestration, Tauri desktop command layer.

---

## File Structure

- Modify `crates/nekodrop-service/src/lib.rs`
  - Add encrypted-control send/receive functions.
  - Add small local helpers for session identity capability, sender/responder handshake, key derivation, and control-frame counters.
  - Keep legacy plaintext transfer functions in place.
- Modify `apps/desktop/src-tauri/src/commands/mod.rs`
  - Use encrypted-control send path for desktop sends.
  - Pass local receiver identity into encrypted-control receive path.
- Modify `apps/desktop/src-tauri/src/device_identity.rs`
  - Add `Capability::EncryptedSession` to desktop capabilities after the desktop send/receive path is wired.
  - Replace the old negative assertion with a positive test.
- Test in `crates/nekodrop-service/src/lib.rs`
  - Prove encrypted-control offer/accept transfers a real file over loopback.
  - Prove encrypted-control decline happens before file payload.
  - Prove a plaintext first frame is rejected by the encrypted-control receive path.
- Test in `apps/desktop/src-tauri/src/device_identity.rs`
  - Prove desktop identity now advertises encrypted session but still does not advertise desktop agent host.

## Task 1: Service Encrypted-Control Acceptance

**Files:**
- Modify: `crates/nekodrop-service/src/lib.rs`

- [ ] **Step 1: Write the failing loopback accept test**

Add this test in the existing `#[cfg(test)] mod tests`:

```rust
#[test]
fn encrypted_control_transfer_sends_offer_and_decision_before_plain_file_payload() {
    let dir = unique_temp_dir("service-encrypted-control-loopback");
    let source_root = dir.join("source").join("drop");
    let receive_dir = dir.join("receive");
    fs::create_dir_all(&source_root).unwrap();
    fs::create_dir_all(&receive_dir).unwrap();
    fs::write(source_root.join("sample.txt"), b"encrypted control only").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = Endpoint::tcp("127.0.0.1", listener.local_addr().unwrap().port());
    let sender = test_identity("neko-device-sender", "Sender Mac");
    let receiver_identity = test_identity("neko-device-receiver", "Receiver Windows");

    let receiver = thread::spawn({
        let receive_dir = receive_dir.clone();
        let receiver_identity = receiver_identity.clone();
        move || {
            let (mut stream, _) = listener.accept().unwrap();
            accept_incoming_stream_with_encrypted_control_and_cancel(
                &mut stream,
                &receive_dir,
                &receiver_identity,
                |_| true,
                |_| panic!("pairing should not be handled on encrypted transfer path"),
                |_| {},
                || false,
            )
        }
    });

    let plan = create_transfer_plan(&[source_root]).unwrap();
    let send_report = send_plan_with_encrypted_control_and_cancel(
        &endpoint,
        plan,
        &sender,
        |_| {},
        || false,
    )
    .unwrap();
    let receive_report = match receiver.join().unwrap().unwrap() {
        IncomingSessionReport::Transfer(report) => report,
        IncomingSessionReport::Pairing(_) => panic!("expected transfer report"),
    };

    assert_eq!(send_report.sent_files.len(), 1);
    assert_eq!(receive_report.files.len(), 1);
    assert_eq!(
        fs::read_to_string(receive_dir.join("drop/sample.txt")).unwrap(),
        "encrypted control only"
    );
    assert_eq!(
        receive_report.sender_device_id.as_deref(),
        Some("neko-device-sender")
    );

    fs::remove_dir_all(dir).unwrap();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
RUSTC="$(rustup which --toolchain stable rustc)" RUSTDOC="$(rustup which --toolchain stable rustdoc)" rustup run stable cargo test -p nekodrop-service --lib encrypted_control_transfer_sends_offer_and_decision_before_plain_file_payload
```

Expected: FAIL because `accept_incoming_stream_with_encrypted_control_and_cancel`, `send_plan_with_encrypted_control_and_cancel`, and `test_identity` are not defined.

- [ ] **Step 3: Implement the encrypted session service path**

Add imports for the existing network and protocol helpers:

```rust
use nekodrop_network::{
    read_session_hello, read_session_transfer_decision, read_session_transfer_offer,
    read_verified_session_ready, write_session_hello, write_session_ready,
    write_session_transfer_decision, write_session_transfer_offer,
};
use nekolink_protocol::{
    default_session_cipher_preference, Capability, DeviceIdentity, ErrorCode, MessageKind,
    ProtocolError, SessionEphemeralKeyPair, SessionFrameKind, SessionKeyMaterial,
    SessionReadyPayload, SessionTrafficCounters, SessionHelloPayload, VerifiedSessionHandshake,
};
```

Add public service functions:

```rust
pub fn send_plan_with_encrypted_control_and_cancel<F, C>(
    endpoint: &Endpoint,
    plan: TransferSourcePlan,
    sender_identity: &DeviceIdentity,
    on_progress: F,
    mut should_cancel: C,
) -> NekoDropResult<TransferSendReport>
where
    F: FnMut(TransferProgressEvent),
    C: FnMut() -> bool,
{
    let outgoing = outgoing_frames_from_plan(&plan);
    let offer = offer_from_plan_with_sender_identity(&plan, Some(sender_identity));
    let mut stream = connect_endpoint(endpoint)?;
    let mut session = start_initiator_session(&mut stream, sender_identity)?;
    write_session_transfer_offer(
        &mut stream,
        session.session_id.clone(),
        session.next_message_id("offer"),
        &session.keys,
        session.next_send_control_header()?,
        &offer,
    )?;
    let mut on_progress = on_progress;
    on_progress(TransferProgressEvent::AwaitingApproval {
        root_name: plan.manifest.root_name.clone(),
        file_count: plan.file_count(),
        total_bytes: plan.total_bytes(),
    });
    let decision = read_session_transfer_decision(&mut stream, &session.keys)?;
    if should_cancel() {
        return Err(NekoDropError::Network("transfer cancelled".into()));
    }
    if !decision.accepted {
        return Err(NekoDropError::Network(format!(
            "receiver declined transfer: {}",
            decision
                .reason
                .unwrap_or_else(|| "no reason provided".to_string())
        )));
    }

    let sent_files = send_file_frames_with_resume_and_cancel(
        &mut stream,
        &outgoing,
        plan.total_bytes(),
        &decision.resume_files,
        |progress| on_progress(TransferProgressEvent::Sending(progress)),
        || should_cancel(),
    )?;

    Ok(TransferSendReport { plan, sent_files })
}
```

Add an encrypted receive entry point that handles session transfer offers and still rejects unsupported encrypted session first frames:

```rust
pub fn accept_incoming_stream_with_encrypted_control_and_cancel<D, H, P, C>(
    stream: &mut TcpStream,
    receive_dir: &Path,
    receiver_identity: &DeviceIdentity,
    decide: D,
    handle_pairing: H,
    on_progress: P,
    mut should_cancel: C,
) -> NekoDropResult<IncomingSessionReport>
where
    D: FnOnce(&TransferOffer) -> bool,
    H: FnOnce(&PairingRequestPayload) -> PairingDecisionPayload,
    P: FnMut(TransferProgressEvent),
    C: FnMut() -> bool,
{
    let first = read_incoming_control_frame_or_session_hello(stream)?;
    match first {
        IncomingFirstFrame::SessionHello(hello) => {
            let mut session = accept_responder_session(stream, receiver_identity, hello)?;
            let offer = read_session_transfer_offer(stream, &session.keys)?;
            accept_transfer_offer_stream_with_encrypted_decision_and_cancel(
                stream,
                receive_dir,
                offer,
                session,
                decide,
                on_progress,
                || should_cancel(),
            )
            .map(IncomingSessionReport::Transfer)
        }
        IncomingFirstFrame::Plain(frame) => accept_plain_incoming_frame_with_cancel(
            stream,
            receive_dir,
            frame,
            decide,
            handle_pairing,
            on_progress,
            || should_cancel(),
        ),
    }
}
```

Keep this implementation minimal: do not encrypt file payloads, do not add iroh transport, and do not change pairing to encrypted control in this task.

- [ ] **Step 4: Run the focused test**

Run:

```bash
RUSTC="$(rustup which --toolchain stable rustc)" RUSTDOC="$(rustup which --toolchain stable rustdoc)" rustup run stable cargo test -p nekodrop-service --lib encrypted_control_transfer_sends_offer_and_decision_before_plain_file_payload
```

Expected: PASS.

## Task 2: Service Decline and Compatibility Tests

**Files:**
- Modify: `crates/nekodrop-service/src/lib.rs`

- [ ] **Step 1: Write failing encrypted decline test**

Add:

```rust
#[test]
fn encrypted_control_receiver_declines_before_files_are_sent() {
    let dir = unique_temp_dir("service-encrypted-control-decline");
    let source_root = dir.join("source").join("drop");
    let receive_dir = dir.join("receive");
    fs::create_dir_all(&source_root).unwrap();
    fs::create_dir_all(&receive_dir).unwrap();
    fs::write(source_root.join("sample.txt"), b"declined").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = Endpoint::tcp("127.0.0.1", listener.local_addr().unwrap().port());
    let sender = test_identity("neko-device-sender", "Sender Mac");
    let receiver_identity = test_identity("neko-device-receiver", "Receiver Windows");

    let receiver = thread::spawn({
        let receive_dir = receive_dir.clone();
        move || {
            let (mut stream, _) = listener.accept().unwrap();
            accept_incoming_stream_with_encrypted_control_and_cancel(
                &mut stream,
                &receive_dir,
                &receiver_identity,
                |_| false,
                |_| panic!("pairing should not be handled on encrypted transfer path"),
                |_| {},
                || false,
            )
        }
    });

    let plan = create_transfer_plan(&[source_root]).unwrap();
    let send_result = send_plan_with_encrypted_control_and_cancel(
        &endpoint,
        plan,
        &sender,
        |_| {},
        || false,
    );
    let receive_result = receiver.join().unwrap();

    assert!(send_result.is_err());
    assert!(receive_result.is_err());
    assert!(!receive_dir.join("drop/sample.txt").exists());

    fs::remove_dir_all(dir).unwrap();
}
```

- [ ] **Step 2: Run test to verify it fails or exposes missing decline wiring**

Run:

```bash
RUSTC="$(rustup which --toolchain stable rustc)" RUSTDOC="$(rustup which --toolchain stable rustdoc)" rustup run stable cargo test -p nekodrop-service --lib encrypted_control_receiver_declines_before_files_are_sent
```

Expected: FAIL until decline decision uses `write_session_transfer_decision`.

- [ ] **Step 3: Write encrypted decision helper implementation**

Add `accept_transfer_offer_stream_with_encrypted_decision_and_cancel` as a sibling to the plaintext `accept_transfer_offer_stream_with_decision_and_cancel`. It should duplicate only the decision-write portion through `write_session_transfer_decision`; all file receive validation remains the same as the plaintext function.

- [ ] **Step 4: Run service tests**

Run:

```bash
RUSTC="$(rustup which --toolchain stable rustc)" RUSTDOC="$(rustup which --toolchain stable rustdoc)" rustup run stable cargo test -p nekodrop-service --lib
```

Expected: PASS.

## Task 3: Desktop Wiring and Capability Advertisement

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`
- Modify: `apps/desktop/src-tauri/src/device_identity.rs`

- [ ] **Step 1: Update desktop identity test first**

Change the capability test to:

```rust
#[test]
fn desktop_identity_advertises_implemented_desktop_capabilities() {
    let capabilities = desktop_capabilities();

    assert!(capabilities.contains(&Capability::EncryptedSession));
    assert!(!capabilities.contains(&Capability::DesktopAgentHost));
}
```

Run:

```bash
RUSTC="$(rustup which --toolchain stable rustc)" RUSTDOC="$(rustup which --toolchain stable rustdoc)" rustup run stable cargo test -p nekodrop-desktop --lib desktop_identity_advertises_implemented_desktop_capabilities
```

Expected: FAIL until `desktop_capabilities()` includes `Capability::EncryptedSession`.

- [ ] **Step 2: Wire desktop commands**

In `apps/desktop/src-tauri/src/commands/mod.rs`, replace the sender call inside `send_paths_to_endpoint_with_history_id`:

```rust
send_plan_with_encrypted_control_and_cancel(
    &endpoint,
    plan.clone(),
    &sender_identity,
    move |event| { /* existing status handling */ },
    || cancel_for_attempt.load(Ordering::SeqCst),
)
```

In the receive listener thread, pass `&local_identity` into:

```rust
accept_incoming_stream_with_encrypted_control_and_cancel(...)
```

Keep pairing fallback unchanged so older plaintext pairing requests still work.

- [ ] **Step 3: Add capability**

Add `Capability::EncryptedSession` to `desktop_capabilities()` in `apps/desktop/src-tauri/src/device_identity.rs`.

- [ ] **Step 4: Run desktop library tests**

Run:

```bash
RUSTC="$(rustup which --toolchain stable rustc)" RUSTDOC="$(rustup which --toolchain stable rustdoc)" rustup run stable cargo test -p nekodrop-desktop --lib
```

Expected: PASS.

## Task 4: Full Verification and Commit

**Files:**
- All modified files.

- [ ] **Step 1: Rust formatting**

Run:

```bash
rustup run stable cargo fmt --all -- --check
```

Expected: PASS.

- [ ] **Step 2: Rust tests**

Run:

```bash
RUSTC="$(rustup which --toolchain stable rustc)" RUSTDOC="$(rustup which --toolchain stable rustdoc)" rustup run stable cargo test -p nekodrop-network --lib
RUSTC="$(rustup which --toolchain stable rustc)" RUSTDOC="$(rustup which --toolchain stable rustdoc)" rustup run stable cargo test -p nekodrop-service --lib
RUSTC="$(rustup which --toolchain stable rustc)" RUSTDOC="$(rustup which --toolchain stable rustdoc)" rustup run stable cargo test --workspace
```

Expected: PASS.

- [ ] **Step 3: Frontend build and whitespace check**

Run:

```bash
npm run build
git diff --check
```

Expected: PASS.

- [ ] **Step 4: Commit and PR**

Commit:

```bash
git add docs/superpowers/plans/2026-06-14-session-control-desktop-wiring.md crates/nekodrop-service/src/lib.rs apps/desktop/src-tauri/src/commands/mod.rs apps/desktop/src-tauri/src/device_identity.rs
git commit -m "feat: wire encrypted session control into desktop transfers"
```

Push and create PR:

```bash
git push -u origin security/session-control-desktop-wiring
gh pr create --title "feat: wire encrypted session control into desktop transfers" --body "Routes desktop transfer offer and decision control messages through verified encrypted session frames while keeping the file payload stream unchanged."
```

## Self-Review

- Scope is limited to encrypted control messages for desktop transfer offer and decision.
- File payload encryption, iroh transport, bundle format, CCS/OpenNeko bridge, and agent command routing are explicitly out of scope.
- Existing plaintext APIs remain available for sidecar and compatibility.
- Desktop only advertises `encrypted_session` after sender and receiver command paths are wired to the encrypted-control service entry points.
