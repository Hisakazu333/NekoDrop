# Transfer Test Result Template

Copy this template into a dated result file when testing a release candidate.
Suggested path:

```text
docs/testing/results/YYYY-MM-DD-<commit>-<tester>.md
```

## Build

| Field | Value |
| --- | --- |
| Date | |
| Tester | |
| Branch | |
| Commit | |
| macOS DMG path | |
| macOS DMG SHA-256 | |
| Windows NSIS path | |
| Windows NSIS SHA-256 | |
| Windows MSI path | |
| Windows MSI SHA-256 | |

## Machines

| Field | macOS | Windows 11 |
| --- | --- | --- |
| Device name | | |
| OS version | | |
| CPU architecture | | |
| App version | | |
| Local IP | | |
| Receive directory | | |
| Receive disk free space | | |
| Firewall state | | |
| VPN / proxy / virtual adapters | | |

## Network

| Field | Value |
| --- | --- |
| Network type | |
| Router / hotspot | |
| Same subnet? | |
| mDNS available? | |
| Notes | |

## Results

| Case ID | Result | Duration | Throughput | Notes / issue link |
| --- | --- | --- | --- | --- |
| `P0-001` | | | | |
| `P0-002` | | | | |
| `P0-003` | | | | |
| `P0-004` | | | | |
| `P0-005` | | | | |
| `P0-006` | | | | |
| `P1-001` | | | | |
| `P1-002` | | | | |
| `P1-003` | | | | |
| `P1-004` | | | | |
| `P1-005` | | | | |
| `P1-006` | | | | |
| `P1-007` | | | | |
| `P1-008` | | | | |
| `P1-009` | | | | |
| `P1-010` | | | | |
| `P1-011` | | | | |
| `P1-012` | | | | |
| `P2-001` | | | | |
| `P2-002` | | | | |
| `P2-003` | | | | |
| `P2-004` | | | | |
| `P2-005` | | | | |
| `P2-006` | | | | |
| `P2-007` | | | | |
| `P2-008` | | | | |

Result values:

- `PASS`
- `FAIL`
- `BLOCKED`
- `SKIPPED`

## Failures

For each failed or blocked case, record:

- Case ID
- Reproduction steps
- Expected behavior
- Actual behavior
- Logs path
- Screenshots path
- Related issue or PR
- Workaround, if any

## Release Decision

| Gate | Decision | Notes |
| --- | --- | --- |
| Internal test build | | |
| Public beta | | |
