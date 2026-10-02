# Receive Space Preflight Design

Date: 2026-06-11

## Goal

Reject large incoming transfers before payload streaming begins when the receive directory does not have enough free space for the remaining bytes. This prevents large transfers from running for minutes and then failing only after the disk is full.

## Non-Goals

- Do not change the TCP transfer protocol or file frame format.
- Do not add chunk-level retry, hash trees, iroh, Relay, P2P, or encryption.
- Do not add a UI redesign or new receive dialog layout in this iteration.
- Do not implement automated 1GB/5GB/10GB soak tests in this PR.

## Recommended Approach

Add a storage-layer receive-space module that can be tested without network sockets. It estimates how many bytes still need to be written by subtracting already completed or partial resume bytes from the accepted offer total. The service layer runs this check after the receiver decides to accept and after resume state is inspected, but before it writes `file.accept` to the sender.

This keeps responsibilities clean:

- `nekodrop-storage` owns available-space probing and remaining-byte estimation.
- `nekodrop-service` owns transfer offer accept/decline timing.
- `apps/desktop/src-tauri` owns user-facing error wording.

## Data Flow

1. Receiver reads `file.offer`.
2. Receiver policy/UI decides whether the transfer is acceptable.
3. Service builds the existing `ResumePlan`.
4. Service asks storage to check receive space for the remaining bytes.
5. If space is sufficient, service writes `file.accept` with resume offsets and receives frames.
6. If space is insufficient, service writes `file.decline` with a disk-space reason and returns a storage error.

## Space Calculation

Required bytes:

```text
sum(expected file sizes) - sum(resume_plan received bytes)
```

The calculation saturates at zero to avoid underflow if resume state is already complete. Completed files and partial files both reduce the remaining write requirement. Directory manifest entries do not count.

## Platform Support

Use platform-specific available-space probing:

- Unix/macOS: `libc::statvfs`
- Windows: `GetDiskFreeSpaceExW` through `windows-sys`

If probing fails, return a storage error before accepting the transfer. Silent fallback is not acceptable because the receiver would lose the protection this feature exists to provide.

## Error Handling

Storage errors should include a stable phrase such as `insufficient receive space`. Desktop friendly error mapping should convert that into Chinese copy explaining that the receive directory disk space is not enough and the user should free space or choose another receive directory.

The sender sees a normal decline before file frames are sent. The receiver sees a local receive failure with the disk-space reason.

## Tests

Use TDD:

1. Storage unit test: remaining bytes subtract completed and partial resume bytes.
2. Storage unit test: insufficient available bytes returns an error containing `insufficient receive space`.
3. Service unit test or pure helper test: insufficient space is converted into a transfer decline before accept.
4. Desktop command test: friendly transfer errors explain insufficient receive space.

## Success Criteria

- Large incoming transfers are rejected before payload streaming if remaining bytes exceed available receive-directory space.
- Resume state is respected, so already received bytes are not counted twice.
- Existing resume, checksum, cancellation, and transfer tests continue to pass.
- Error copy is actionable for the user.
