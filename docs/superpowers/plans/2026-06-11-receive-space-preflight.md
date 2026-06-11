# Receive Space Preflight Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reject incoming transfers before payload streaming when the receive directory lacks enough free space for remaining bytes.

**Architecture:** Add a `nekodrop-storage` space module for remaining-byte calculation and platform free-space probing. Call it in `nekodrop-service` after resume inspection and before sending `file.accept`. Add desktop friendly error copy for the stable `insufficient receive space` error phrase.

**Tech Stack:** Rust workspace crates, platform `libc::statvfs` on Unix/macOS, `windows-sys` `GetDiskFreeSpaceExW` on Windows, existing Rust unit tests.

---

## File Structure

- Create: `crates/nekodrop-storage/src/space.rs`
  - Owns receive-space estimation, insufficient-space errors, and platform free-space probing.
- Modify: `crates/nekodrop-storage/src/lib.rs`
  - Exports the new receive-space APIs.
- Modify: `crates/nekodrop-storage/Cargo.toml`
  - Adds direct platform dependencies for disk free-space probing.
- Modify: `crates/nekodrop-service/src/lib.rs`
  - Runs the preflight before writing transfer accept and declines when space is insufficient.
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`
  - Maps insufficient receive-space errors to actionable Chinese copy.
- Modify: `docs/STATUS.md`
  - Records the new real capability.

---

### Task 1: Storage Receive-Space Calculation

**Files:**
- Create: `crates/nekodrop-storage/src/space.rs`
- Modify: `crates/nekodrop-storage/src/lib.rs`
- Modify: `crates/nekodrop-storage/Cargo.toml`

- [ ] **Step 1: Write failing storage tests**

Create `crates/nekodrop-storage/src/space.rs` with tests first:

```rust
use nekodrop_core::{NekoDropError, NekoDropResult};

use crate::resume::{ResumeFileState, ResumePlan};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaining_receive_bytes_subtracts_completed_and_partial_resume_bytes() {
        let resume_plan = ResumePlan {
            transfer_id: "transfer-a".to_string(),
            files: vec![
                ResumeFileState {
                    path: "drop/done.bin".to_string(),
                    received_bytes: 40,
                    expected_bytes: 40,
                    sha256: None,
                    completed: true,
                },
                ResumeFileState {
                    path: "drop/partial.bin".to_string(),
                    received_bytes: 15,
                    expected_bytes: 60,
                    sha256: None,
                    completed: false,
                },
            ],
        };

        assert_eq!(remaining_receive_bytes(120, &resume_plan), 65);
    }

    #[test]
    fn receive_space_check_rejects_when_available_bytes_are_insufficient() {
        let resume_plan = ResumePlan {
            transfer_id: "transfer-a".to_string(),
            files: vec![ResumeFileState {
                path: "drop/partial.bin".to_string(),
                received_bytes: 25,
                expected_bytes: 100,
                sha256: None,
                completed: false,
            }],
        };

        let error = check_receive_space_with_available_bytes(100, &resume_plan, 70).unwrap_err();

        assert!(error.to_string().contains("insufficient receive space"));
        assert!(error.to_string().contains("need 75 bytes"));
        assert!(error.to_string().contains("available 70 bytes"));
    }
}
```

- [ ] **Step 2: Run storage tests and verify RED**

Run:

```bash
RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc \
/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo test \
  -p nekodrop-storage receive_space
```

Expected: FAIL because `remaining_receive_bytes` and `check_receive_space_with_available_bytes` are not implemented.

- [ ] **Step 3: Implement storage APIs**

Implement in `space.rs`:

```rust
use std::path::Path;

use nekodrop_core::{NekoDropError, NekoDropResult};

use crate::resume::ResumePlan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiveSpaceStatus {
    pub required_bytes: u64,
    pub available_bytes: u64,
}

pub fn remaining_receive_bytes(total_bytes: u64, resume_plan: &ResumePlan) -> u64 {
    total_bytes.saturating_sub(resume_plan.total_received_bytes())
}

pub fn check_receive_space(
    receive_dir: &Path,
    total_bytes: u64,
    resume_plan: &ResumePlan,
) -> NekoDropResult<ReceiveSpaceStatus> {
    let available_bytes = available_space(receive_dir)?;
    check_receive_space_with_available_bytes(total_bytes, resume_plan, available_bytes)
}

pub fn check_receive_space_with_available_bytes(
    total_bytes: u64,
    resume_plan: &ResumePlan,
    available_bytes: u64,
) -> NekoDropResult<ReceiveSpaceStatus> {
    let required_bytes = remaining_receive_bytes(total_bytes, resume_plan);
    if available_bytes < required_bytes {
        return Err(NekoDropError::Storage(format!(
            "insufficient receive space: need {required_bytes} bytes, available {available_bytes} bytes"
        )));
    }
    Ok(ReceiveSpaceStatus {
        required_bytes,
        available_bytes,
    })
}
```

Add platform `available_space` implementations using `libc::statvfs` for `cfg(unix)` and `GetDiskFreeSpaceExW` for `cfg(windows)`.

- [ ] **Step 4: Export and add dependencies**

In `crates/nekodrop-storage/src/lib.rs`:

```rust
pub mod space;
pub use space::{
    check_receive_space, check_receive_space_with_available_bytes, remaining_receive_bytes,
    ReceiveSpaceStatus,
};
```

In `crates/nekodrop-storage/Cargo.toml`:

```toml
[target.'cfg(unix)'.dependencies]
libc = "0.2"

[target.'cfg(windows)'.dependencies]
windows-sys = { version = "0.61", features = ["Win32_Storage_FileSystem"] }
```

- [ ] **Step 5: Run storage tests and verify GREEN**

Run the same `cargo test -p nekodrop-storage receive_space` command.

Expected: PASS.

---

### Task 2: Service Preflight Before Accept

**Files:**
- Modify: `crates/nekodrop-service/src/lib.rs`

- [ ] **Step 1: Write failing service test**

Add a test in `crates/nekodrop-service/src/lib.rs` inside `mod tests`:

```rust
#[test]
fn receiver_declines_transfer_when_receive_space_is_insufficient() {
    let dir = unique_temp_dir("service-space-preflight");
    let receive_dir = dir.join("receive");
    fs::create_dir_all(&receive_dir).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = Endpoint::tcp("127.0.0.1", listener.local_addr().unwrap().port());

    let receiver = thread::spawn({
        let receive_dir = receive_dir.clone();
        move || accept_transfer_with_decision(&listener, &receive_dir, |_| true, |_| {})
    });

    let mut stream = connect_endpoint(&endpoint).unwrap();
    let offer = TransferOffer::new(
        "transfer-huge",
        "huge",
        vec![TransferOfferFile {
            manifest_path: "huge/video.bin".to_string(),
            size: u64::MAX,
            sha256: "0".repeat(64),
        }],
    );
    write_transfer_offer(&mut stream, &offer).unwrap();
    let decision = read_transfer_decision(&mut stream).unwrap();
    let receive_result = receiver.join().unwrap();

    assert!(!decision.accepted);
    assert!(decision
        .reason
        .as_deref()
        .unwrap_or_default()
        .contains("insufficient receive space"));
    assert!(receive_result.unwrap_err().to_string().contains("insufficient receive space"));

    fs::remove_dir_all(dir).unwrap();
}
```

- [ ] **Step 2: Run service test and verify RED**

Run with local TCP permission:

```bash
RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc \
/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo test \
  -p nekodrop-service receiver_declines_transfer_when_receive_space_is_insufficient
```

Expected: FAIL because the receiver currently accepts the offer before checking disk space.

- [ ] **Step 3: Implement service preflight**

Import `check_receive_space`. In `accept_transfer_offer_stream_with_decision_and_cancel`, after `resume_plan` is built and before `TransferDecision::accept_with_resume(...)`, call:

```rust
if let Err(error) = check_receive_space(receive_dir, offer.total_bytes, &resume_plan) {
    let _ = write_transfer_decision_for_transfer(
        stream,
        &offer.transfer_id,
        &TransferDecision::decline("insufficient receive space"),
    );
    return Err(error);
}
```

- [ ] **Step 4: Run service test and verify GREEN**

Run the same `cargo test -p nekodrop-service receiver_declines_transfer_when_receive_space_is_insufficient` command.

Expected: PASS.

---

### Task 3: Desktop Friendly Error

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Write failing desktop error test**

Add to `commands::tests`:

```rust
#[test]
fn friendly_transfer_error_explains_insufficient_receive_space() {
    let message = friendly_transfer_error(
        "storage error: insufficient receive space: need 100 bytes, available 70 bytes",
    );

    assert!(message.contains("接收目录"));
    assert!(message.contains("空间不足"));
}
```

- [ ] **Step 2: Run desktop test and verify RED**

Run:

```bash
RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc \
/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo test \
  -p nekodrop-desktop friendly_transfer_error_explains_insufficient_receive_space
```

Expected: FAIL because the friendly mapper does not yet recognize this phrase.

- [ ] **Step 3: Implement friendly error branch**

In `friendly_transfer_error`, add before generic network branches:

```rust
if lower.contains("insufficient receive space") || lower.contains("disk full") {
    return "接收目录所在磁盘空间不足。请清理空间，或在设置里选择另一个接收目录后重试。".to_string();
}
```

- [ ] **Step 4: Run desktop test and verify GREEN**

Run the same `cargo test -p nekodrop-desktop friendly_transfer_error_explains_insufficient_receive_space` command.

Expected: PASS.

---

### Task 4: Docs and Verification

**Files:**
- Modify: `docs/STATUS.md`

- [ ] **Step 1: Update status doc**

Add a product capability row:

```markdown
| 接收端磁盘空间预检 | 已接入 | 接收端会在接受传输前按 resume 状态估算剩余写入字节；空间不足时提前拒绝，不进入大文件 payload 传输。 |
```

- [ ] **Step 2: Run full verification**

Run:

```bash
RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc \
/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo fmt --all -- --check
```

Run with local TCP permission:

```bash
RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc \
/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo test --workspace
```

Run:

```bash
npm run build
npm audit --omit=dev
git diff --check
```

- [ ] **Step 3: Commit implementation**

```bash
git add crates/nekodrop-storage/Cargo.toml \
  crates/nekodrop-storage/src/lib.rs \
  crates/nekodrop-storage/src/space.rs \
  crates/nekodrop-service/src/lib.rs \
  apps/desktop/src-tauri/src/commands/mod.rs \
  docs/STATUS.md \
  docs/superpowers/plans/2026-06-11-receive-space-preflight.md
git commit -m "feat: preflight receive disk space"
```
