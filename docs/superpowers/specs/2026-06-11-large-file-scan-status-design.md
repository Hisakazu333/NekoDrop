# Large File Scan Status Design

Date: 2026-06-11

## Goal

Make large file and folder preparation visible before transfer starts. Today `create_transfer_plan` performs metadata traversal and SHA-256 hashing synchronously. For large folders or large media files, the desktop UI can look stuck even though the backend is working. This iteration turns that invisible preparation into a clear "preparing transfer" state with real counts and byte totals.

## Non-Goals

- Do not change the TCP file transfer protocol.
- Do not add encrypted sessions, iroh, Relay, P2P, or background queues.
- Do not replace final transfer progress, speed, ETA, or history.
- Do not skip SHA-256 hashing; integrity remains required.
- Do not move file scanning or hashing into React.

## Recommended Approach

Add progress callbacks to the existing storage manifest builder, surface those callbacks through the service layer, and emit a Tauri event while the scan runs. The front end listens for the event and renders a preparation status near the composer.

This keeps ownership clean:

- `nekodrop-storage` owns filesystem traversal, metadata, and hashing.
- `nekodrop-service` owns transfer-plan construction API shape.
- `apps/desktop/src-tauri` owns Tauri command/event bridging.
- `apps/desktop/src` only renders the current scan status.

## Data Model

Introduce a scan progress model with these fields:

- `phase`: `started`, `scanning`, `hashing`, `completed`
- `current_path`: current filesystem path or manifest path
- `files_found`: number of files discovered so far
- `directories_found`: number of directories discovered so far
- `bytes_found`: cumulative file bytes discovered so far

Errors continue to flow through the existing command error path. The UI clears scan status when a scan succeeds, fails, or a new scan starts.

## Backend Flow

`create_source_plan_from_paths_with_progress(paths, on_progress)` performs the same work as the current function but reports progress:

1. Validate that at least one path is selected.
2. Emit `started`.
3. During traversal, emit `scanning` as files and directories are discovered.
4. Before hashing each file, emit `hashing` with the current manifest path and cumulative totals.
5. Build the same `TransferSourcePlan` as today.
6. Emit `completed`.

The existing `create_source_plan_from_paths` remains as a convenience wrapper using a no-op callback so sidecar and tests do not need to change.

## Desktop Command Flow

Keep existing command names to avoid a large front-end rewrite:

- `create_transfer_plan(paths)` emits `transfer_scan_progress` events while it runs.
- `create_transfer_plan_from_text(paths_text)` does the same.

The event payload mirrors the backend progress model using snake_case fields, matching existing DTO conventions.

## Frontend Flow

React adds a `scanStatus` state and subscribes to `transfer_scan_progress` on mount. During `scanPaths`:

- set busy state to `scan`
- clear previous plan and report
- show "正在准备传输"
- render discovered file count and accumulated size as progress details
- show current path while scanning or hashing

The UI must not show a fake percentage because the full total is unknown until the scan finishes. Counts and byte totals are honest and enough to prove the app is working.

## Error Handling

- Filesystem and hash errors remain command errors shown by the existing error banner.
- The front end clears scan status in `finally` only after success or error has been rendered.
- If a stale event arrives after scan completion, it can update the transient status but should not affect the selected `TransferPlanDto`.

## Tests

Use TDD:

1. Add storage tests that verify progress events are emitted for directory traversal and file hashing.
2. Add desktop command tests for DTO conversion if a helper is introduced.
3. Run `cargo test -p nekodrop-storage` during the first loop.
4. Run `cargo test --workspace`, `npm run build`, `npm audit --omit=dev`, and `git diff --check` before commit.

## Success Criteria

- Selecting a large file/folder gives immediate visible preparation feedback.
- Scan progress includes real file count, directory count, cumulative bytes, and current path.
- Existing transfer plans, SHA-256 values, file counts, and total bytes remain unchanged.
- The final transfer behavior and protocol remain compatible with the current release.
