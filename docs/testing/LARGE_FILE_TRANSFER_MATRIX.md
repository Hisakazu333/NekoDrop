# Large File Transfer Test Matrix

This matrix is the release gate for NekoDrop desktop transfer reliability. It
turns Mac / Windows testing into repeatable evidence instead of ad hoc checks.

Use this document before publishing a DMG, NSIS installer, MSI, or GitHub
release. A transfer is not considered verified unless the result is recorded
with the exact build, artifact hash, operating systems, network, payload, and
tester notes.

## Release Gate

For an internal test build:

- All `P0` cases must pass on `macOS -> Windows 11` and `Windows 11 -> macOS`.
- No completed transfer may fail SHA-256 verification.
- A failed or cancelled transfer must not leave a misleading final file.
- The result record must include the installer path and SHA-256.

For a public beta:

- All `P0` and `P1` cases must pass on clean installs.
- Any remaining failure must be documented as a known issue with a workaround.
- The tested build must be produced from a tagged commit or release candidate
  branch.

## Test Environments

Record these fields for every run:

- Build commit and branch
- macOS version and CPU architecture
- Windows 11 version and CPU architecture
- DMG path and SHA-256
- NSIS / MSI path and SHA-256
- Sender machine name and local IP
- Receiver machine name and local IP
- Network type: same Wi-Fi, wired plus Wi-Fi, hotspot, or isolated network
- Windows firewall state
- VPN / proxy / virtual adapter state
- Receive directory path and available disk space

Use [RESULT_TEMPLATE.md](RESULT_TEMPLATE.md) for the actual run log.

## Test Payloads

Create fresh payloads for each release candidate.

| Payload ID | Contents | Purpose |
| --- | --- | --- |
| `small-single` | One text file under 1 MB | Basic send / receive smoke test |
| `small-folder` | Nested folder with 10-30 files | Folder manifest and structure preservation |
| `unicode-path` | Unicode path format: `xxx\xxx\文件名.ext` on Windows, `xxx/xxx/文件名.ext` on macOS | Detect path mojibake such as `�` and unsafe path handling |
| `spaces-symbols` | File and folder names containing spaces, parentheses, `+`, `-`, `_`, and `.` | Common filename compatibility |
| `large-1g` | One 1 GiB file | Baseline large transfer |
| `large-5g` | One 5 GiB file | Practical large transfer |
| `large-10g` | One 10 GiB file | Stress transfer |
| `many-small` | 2,000 small files in nested folders | Manifest scale and folder performance |

Suggested macOS payload commands:

```bash
mkdir -p ~/NekoDropTest/payloads/large
mkfile 1g ~/NekoDropTest/payloads/large/large-1g.bin
mkfile 5g ~/NekoDropTest/payloads/large/large-5g.bin
mkfile 10g ~/NekoDropTest/payloads/large/large-10g.bin
```

Suggested Windows PowerShell payload commands:

```powershell
New-Item -ItemType Directory -Force "$env:USERPROFILE\NekoDropTest\payloads\large"
fsutil file createnew "$env:USERPROFILE\NekoDropTest\payloads\large\large-1g.bin" 1073741824
fsutil file createnew "$env:USERPROFILE\NekoDropTest\payloads\large\large-5g.bin" 5368709120
fsutil file createnew "$env:USERPROFILE\NekoDropTest\payloads\large\large-10g.bin" 10737418240
```

## P0 Smoke Cases

| ID | Direction | Discovery | Payload | Steps | Expected |
| --- | --- | --- | --- | --- | --- |
| `P0-001` | macOS -> Windows 11 | Nearby device | `small-single` | Install both apps, open receive on Windows, pair or trust if needed, send one file from macOS. | Receiver prompts or auto-accepts by policy, file lands in receive directory, SHA-256 passes, history records both sides. |
| `P0-002` | Windows 11 -> macOS | Nearby device | `small-single` | Repeat `P0-001` in reverse. | Same as `P0-001`. |
| `P0-003` | macOS -> Windows 11 | Connection code | `small-folder` | Disable reliance on nearby list by using the receiver connection code. | Folder structure is preserved and history can reveal/open location. |
| `P0-004` | Windows 11 -> macOS | Connection code | `small-folder` | Repeat `P0-003` in reverse. | Same as `P0-003`. |
| `P0-005` | macOS -> Windows 11 | Manual `IP:port` | `small-single` | Send using manual endpoint fallback. | Transfer succeeds or fails with a clear reachable-address error. |
| `P0-006` | Windows 11 -> macOS | Manual `IP:port` | `small-single` | Repeat `P0-005` in reverse. | Transfer succeeds or fails with a clear reachable-address error. |

## P1 Release-Blocking Reliability Cases

| ID | Direction | Discovery | Payload | Steps | Expected |
| --- | --- | --- | --- | --- | --- |
| `P1-001` | macOS -> Windows 11 | Nearby device | `unicode-path` | Pick a source file whose full path contains Chinese characters. | The selected path, transfer offer, manifest path, received filename, and history do not contain `�`. |
| `P1-002` | Windows 11 -> macOS | Nearby device | `unicode-path` | Repeat `P1-001` in reverse. | Same as `P1-001`. |
| `P1-003` | macOS -> Windows 11 | Nearby device | `large-1g` | Send one 1 GiB file. | Progress, speed, ETA, completion, and SHA-256 all work. |
| `P1-004` | Windows 11 -> macOS | Nearby device | `large-1g` | Repeat `P1-003` in reverse. | Same as `P1-003`. |
| `P1-005` | macOS -> Windows 11 | Nearby device | `large-5g` | Send one 5 GiB file. | Transfer completes without UI freeze or incorrect failure state. |
| `P1-006` | Windows 11 -> macOS | Nearby device | `large-5g` | Repeat `P1-005` in reverse. | Same as `P1-005`. |
| `P1-007` | macOS -> Windows 11 | Nearby device | `large-1g` | Cancel during transfer, then retry or continue from history. | Partial file is cleaned or resumed according to current product behavior; final SHA-256 passes. |
| `P1-008` | Windows 11 -> macOS | Nearby device | `large-1g` | Repeat `P1-007` in reverse. | Same as `P1-007`. |
| `P1-009` | macOS -> Windows 11 | Nearby device | `many-small` | Send a nested folder with about 2,000 small files. | Manifest scan, transfer, and receive history complete without missing files. |
| `P1-010` | Windows 11 -> macOS | Nearby device | `many-small` | Repeat `P1-009` in reverse. | Same as `P1-009`. |
| `P1-011` | macOS -> Windows 11 | Nearby device | `large-5g` | Choose a receive directory with less free space than the remaining transfer size. | Receiver declines before payload streaming or fails early with a clear disk-space message. |
| `P1-012` | Windows 11 -> macOS | Nearby device | `large-5g` | Repeat `P1-011` in reverse. | Same as `P1-011`. |

## P2 Stress and Network Cases

| ID | Direction | Network | Payload | Steps | Expected |
| --- | --- | --- | --- | --- | --- |
| `P2-001` | macOS -> Windows 11 | Same Wi-Fi | `large-10g` | Send one 10 GiB file. | Transfer completes or fails with a recoverable, documented reason. |
| `P2-002` | Windows 11 -> macOS | Same Wi-Fi | `large-10g` | Repeat `P2-001` in reverse. | Same as `P2-001`. |
| `P2-003` | macOS -> Windows 11 | Wired plus Wi-Fi on same router | `large-1g` | Send one 1 GiB file. | Nearby discovery works or connection code fallback works. |
| `P2-004` | Windows 11 -> macOS | Wired plus Wi-Fi on same router | `large-1g` | Repeat `P2-003` in reverse. | Same as `P2-003`. |
| `P2-005` | macOS -> Windows 11 | Isolated Wi-Fi or guest network | `small-single` | Attempt nearby discovery, then connection code fallback. | Discovery failure is clear; fallback either works or explains unreachable network. |
| `P2-006` | Windows 11 -> macOS | Windows firewall blocks app | `small-single` | Block inbound traffic for NekoDrop on Windows and try to send to it. | Sender shows a clear timeout/firewall-style error; app does not hang. |
| `P2-007` | macOS -> Windows 11 | Same Wi-Fi | `large-1g` | Kill or close receiver during transfer, reopen, then retry. | Sender exits failure state cleanly; retry path is understandable. |
| `P2-008` | Windows 11 -> macOS | Same Wi-Fi | `large-1g` | Repeat `P2-007` in reverse. | Same as `P2-007`. |

## Failure Triage

When a case fails, record the observed phase:

- File selection
- Manifest scan
- Nearby discovery
- Pairing / trust check
- Transfer offer
- Accept / decline decision
- Payload streaming
- Cancellation
- Resume / retry
- Finalization
- SHA-256 verification
- History update
- Reveal/open location

Common signatures:

- A path containing `�` means the Windows file picker, shell output encoding,
  IPC boundary, manifest builder, or UI rendering must be inspected.
- Large files failing before the receiver accepts the offer usually point to
  manifest scan, offer validation, free-space preflight, or timeout behavior.
- Large files failing during payload streaming usually point to network reset,
  cancellation handling, partial file behavior, or sender-side retry logic.
- Nearby devices not appearing usually points to mDNS, firewall, network
  isolation, VPN, proxy, or virtual adapters.
- Successful transfer with wrong output path is a release blocker even if
  checksum passes.

## Result Rules

- Do not mark a case as passed without the exact artifact hash.
- Do not reuse a previous result for a new commit or installer.
- Do not mark "not applicable" unless the reason is written.
- Attach logs or screenshots when an issue is opened.
- If a manual workaround is required, the case is not a public-beta pass.
