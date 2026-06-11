# Large File Scan Status Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show honest transfer-preparation progress while large files or folders are scanned and hashed.

**Architecture:** Add scan-progress callbacks to `nekodrop-storage`, expose them through `nekodrop-service`, emit Tauri `transfer_scan_progress` events from desktop commands, and render a transient preparation status in React. The transfer protocol and final send/receive progress stay unchanged.

**Tech Stack:** Rust workspace crates, Tauri 2 command/event bridge, React/Vite TypeScript.

---

## File Structure

- Modify: `crates/nekodrop-storage/src/manifest_builder.rs`
  - Defines scan progress phase/model and emits progress while building `TransferSourcePlan`.
- Modify: `crates/nekodrop-storage/src/lib.rs`
  - Re-exports the scan progress model and callback constructor.
- Modify: `crates/nekodrop-service/src/lib.rs`
  - Adds a `create_transfer_plan_with_scan_progress` wrapper.
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`
  - Converts scan progress to a Tauri DTO and emits `transfer_scan_progress`.
- Modify: `apps/desktop/src/types.ts`
  - Adds `TransferScanProgressDto`.
- Modify: `apps/desktop/src/App.tsx`
  - Subscribes to scan events and renders preparation status in the queue panel.
- Modify: `docs/STATUS.md`
  - Marks scan/preparation status as connected after implementation.

---

### Task 1: Storage Scan Progress

**Files:**
- Modify: `crates/nekodrop-storage/src/manifest_builder.rs`
- Modify: `crates/nekodrop-storage/src/lib.rs`

- [ ] **Step 1: Write the failing storage test**

Add this test in `crates/nekodrop-storage/src/manifest_builder.rs` inside `mod tests`:

```rust
#[test]
fn emits_scan_progress_while_building_source_plan() {
    let dir = unique_temp_dir("manifest-progress");
    let root = dir.join("drop");
    fs::create_dir_all(root.join("nested")).unwrap();
    fs::write(root.join("nested").join("one.txt"), b"one").unwrap();
    fs::write(root.join("two.txt"), b"two").unwrap();

    let mut events = Vec::new();
    let plan = create_source_plan_from_paths_with_progress(&[root], |event| events.push(event))
        .unwrap();

    assert_eq!(plan.file_count(), 2);
    assert_eq!(plan.total_bytes(), 6);
    assert_eq!(
        events.first().map(|event| event.phase.as_str()),
        Some("started")
    );
    assert_eq!(
        events.last().map(|event| event.phase.as_str()),
        Some("completed")
    );
    assert!(events
        .iter()
        .any(|event| event.phase.as_str() == "scanning" && event.directories_found >= 1));
    assert!(events
        .iter()
        .any(|event| event.phase.as_str() == "hashing"
            && event.current_path.as_deref() == Some("drop/nested/one.txt")));
    assert!(events
        .iter()
        .any(|event| event.phase.as_str() == "hashing"
            && event.files_found == 2
            && event.bytes_found == 6));

    fs::remove_dir_all(dir).unwrap();
}
```

- [ ] **Step 2: Run the test and verify RED**

Run:

```bash
RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc \
/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo test \
  -p nekodrop-storage emits_scan_progress_while_building_source_plan
```

Expected: FAIL because `create_source_plan_from_paths_with_progress` is not defined.

- [ ] **Step 3: Implement scan progress model and callback path**

Add the model near `TransferSourcePlan` in `manifest_builder.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferPlanScanPhase {
    Started,
    Scanning,
    Hashing,
    Completed,
}

impl TransferPlanScanPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Scanning => "scanning",
            Self::Hashing => "hashing",
            Self::Completed => "completed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferPlanScanProgress {
    pub phase: TransferPlanScanPhase,
    pub current_path: Option<String>,
    pub files_found: usize,
    pub directories_found: usize,
    pub bytes_found: u64,
}
```

Add `create_source_plan_from_paths_with_progress(paths, on_progress)` and make `create_source_plan_from_paths` call it with a no-op closure.

- [ ] **Step 4: Run storage test and verify GREEN**

Run the same `cargo test -p nekodrop-storage emits_scan_progress_while_building_source_plan` command.

Expected: PASS.

- [ ] **Step 5: Re-export the new API**

Update `crates/nekodrop-storage/src/lib.rs` so callers can import:

```rust
pub use manifest_builder::{
    create_manifest_from_paths, create_source_plan_from_paths,
    create_source_plan_from_paths_with_progress, TransferPlanScanPhase,
    TransferPlanScanProgress, TransferSourceFile, TransferSourcePlan,
};
```

---

### Task 2: Service and Tauri Event Bridge

**Files:**
- Modify: `crates/nekodrop-service/src/lib.rs`
- Modify: `apps/desktop/src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Add service wrapper**

In `crates/nekodrop-service/src/lib.rs`, import `create_source_plan_from_paths_with_progress`, re-export `TransferPlanScanProgress`, and add:

```rust
pub fn create_transfer_plan_with_scan_progress<F>(
    paths: &[PathBuf],
    on_progress: F,
) -> NekoDropResult<TransferSourcePlan>
where
    F: FnMut(TransferPlanScanProgress),
{
    create_source_plan_from_paths_with_progress(paths, on_progress)
}
```

- [ ] **Step 2: Add desktop DTO and emit helper**

In `apps/desktop/src-tauri/src/commands/mod.rs`, import `tauri::{AppHandle, Emitter, State}` and add:

```rust
const TRANSFER_SCAN_PROGRESS_EVENT: &str = "transfer_scan_progress";

#[derive(Debug, Clone, Serialize)]
pub struct TransferScanProgressDto {
    pub phase: String,
    pub current_path: Option<String>,
    pub files_found: usize,
    pub directories_found: usize,
    pub bytes_found: u64,
}
```

Add a helper that maps `TransferPlanScanProgress` into the DTO and calls `app.emit(...)`.

- [ ] **Step 3: Wire both plan commands**

Change command signatures:

```rust
pub fn create_transfer_plan(
    app: AppHandle,
    paths: Vec<String>,
) -> Result<TransferPlanDto, String>
```

and

```rust
pub fn create_transfer_plan_from_text(
    app: AppHandle,
    paths_text: String,
) -> Result<TransferPlanDto, String>
```

Both commands call `create_transfer_plan_with_scan_progress(&paths, |progress| emit_transfer_scan_progress(&app, progress))`.

---

### Task 3: React Rendering

**Files:**
- Modify: `apps/desktop/src/types.ts`
- Modify: `apps/desktop/src/App.tsx`

- [ ] **Step 1: Add TypeScript DTO**

Add to `apps/desktop/src/types.ts`:

```ts
export interface TransferScanProgressDto {
  phase: "started" | "scanning" | "hashing" | "completed";
  current_path: string | null;
  files_found: number;
  directories_found: number;
  bytes_found: number;
}
```

- [ ] **Step 2: Subscribe to Tauri scan events**

In `App.tsx`, import `listen`:

```ts
import { listen } from "@tauri-apps/api/event";
```

Add `scanStatus` state and a mount effect:

```ts
const [scanStatus, setScanStatus] = useState<TransferScanProgressDto | null>(null);

useEffect(() => {
  let active = true;
  const unlistenPromise = listen<TransferScanProgressDto>("transfer_scan_progress", (event) => {
    if (!active) return;
    setScanStatus(event.payload);
  });

  return () => {
    active = false;
    unlistenPromise.then((unlisten) => unlisten()).catch(() => undefined);
  };
}, []);
```

- [ ] **Step 3: Render honest preparation status**

Pass `scanStatus` to `QueuePanel`. Inside `QueuePanel`, show a compact line when `busy === "scan"` or the phase is not `completed`:

```tsx
{scanStatus ? (
  <div className="transfer-status">
    <div className="transfer-status-head">
      <strong>{scanStatus.phase === "hashing" ? "正在校验文件" : "正在准备传输"}</strong>
      <span>
        {scanStatus.files_found} 个文件 · {formatBytes(scanStatus.bytes_found)}
      </span>
    </div>
    {scanStatus.current_path ? (
      <div className="transfer-status-meta">{scanStatus.current_path}</div>
    ) : null}
  </div>
) : null}
```

Clear `scanStatus` when a scan starts, fails, or finishes after the plan is set.

---

### Task 4: Documentation and Verification

**Files:**
- Modify: `docs/STATUS.md`

- [ ] **Step 1: Update status doc**

Add a row under product capabilities:

```markdown
| 传输前扫描/准备状态 | 已接入 | 大文件或文件夹生成 manifest 和 SHA-256 时会显示真实文件数、累计大小和当前路径，避免准备阶段像卡住。 |
```

- [ ] **Step 2: Run formatting and tests**

Run:

```bash
RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc \
/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo fmt --all -- --check
```

Run:

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
git add crates/nekodrop-storage/src/manifest_builder.rs \
  crates/nekodrop-storage/src/lib.rs \
  crates/nekodrop-service/src/lib.rs \
  apps/desktop/src-tauri/src/commands/mod.rs \
  apps/desktop/src/types.ts \
  apps/desktop/src/App.tsx \
  docs/STATUS.md \
  docs/superpowers/plans/2026-06-11-large-file-scan-status.md
git commit -m "feat: show transfer scan status"
```
