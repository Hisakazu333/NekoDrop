# NekoDrop Transfer Protocol

## Goals

The MVP protocol should be simple, inspectable, and reliable on a local network.

It must support:

- device discovery
- trusted pairing
- transfer offer and acceptance
- chunked file transfer
- progress reporting
- checksum verification
- cancellation
- basic resume support

## Transport

MVP:

- TCP for transfer sessions
- mDNS for discovery
- UDP broadcast fallback for discovery

Future:

- QUIC for multiplexing, lower latency, and better interruption handling
- relay transport for non-LAN transfers

## Device Advertisement

Each device advertises:

```json
{
  "protocol": "nekodrop.discovery.v1",
  "device_id": "stable-device-id",
  "device_name": "Hisakazu MacBook",
  "platform": "macos",
  "app_version": "0.1.0",
  "host": "192.168.1.24",
  "port": 45821,
  "public_key_fingerprint": "base64url"
}
```

Discovery entries are not trusted by themselves. They only make devices visible.

## Control Messages

All control messages include:

```json
{
  "type": "MESSAGE_TYPE",
  "protocol_version": 1,
  "session_id": "uuid",
  "message_id": "uuid",
  "sent_at": "2026-06-09T15:00:00Z"
}
```

## Pairing Messages

### PAIR_REQ

Sent by the device requesting trust.

```json
{
  "type": "PAIR_REQ",
  "device_id": "sender-device-id",
  "device_name": "Hisakazu MacBook",
  "platform": "macos",
  "public_key": "base64url",
  "short_code": "483921"
}
```

### PAIR_ACK

Sent after user confirmation.

```json
{
  "type": "PAIR_ACK",
  "accepted": true,
  "device_id": "receiver-device-id",
  "device_name": "Windows PC",
  "public_key": "base64url",
  "short_code": "483921"
}
```

### PAIR_REJECT

```json
{
  "type": "PAIR_REJECT",
  "reason": "user_declined"
}
```

## Transfer Messages

### SEND_OFFER

```json
{
  "type": "SEND_OFFER",
  "transfer_id": "uuid",
  "sender_device_id": "device-a",
  "file_count": 3,
  "total_bytes": 104857600,
  "manifest": {
    "root_name": "Design Assets",
    "items": [
      {
        "path": "logo.png",
        "kind": "file",
        "size": 124932,
        "modified_at": "2026-06-09T14:00:00Z",
        "sha256": null
      },
      {
        "path": "screenshots",
        "kind": "directory"
      }
    ]
  }
}
```

`sha256` may be null in the initial offer when hashing has not completed yet. Final verification still requires file hashes.

### SEND_ACCEPT

```json
{
  "type": "SEND_ACCEPT",
  "transfer_id": "uuid",
  "receive_mode": "default_folder",
  "resume_token": null
}
```

### SEND_DECLINE

```json
{
  "type": "SEND_DECLINE",
  "transfer_id": "uuid",
  "reason": "user_declined"
}
```

### CHUNK_START

```json
{
  "type": "CHUNK_START",
  "transfer_id": "uuid",
  "file_path": "logo.png",
  "file_size": 124932,
  "chunk_size": 1048576,
  "sha256": "hex"
}
```

### CHUNK

Chunk payloads should use a binary frame:

```text
frame_type: CHUNK
transfer_id
file_path_id
offset
length
bytes
```

Do not base64 encode file chunks for the transfer path.

### FILE_COMPLETE

```json
{
  "type": "FILE_COMPLETE",
  "transfer_id": "uuid",
  "file_path": "logo.png",
  "sha256": "hex"
}
```

### TRANSFER_COMPLETE

```json
{
  "type": "TRANSFER_COMPLETE",
  "transfer_id": "uuid",
  "file_count": 3,
  "total_bytes": 104857600,
  "verified": true
}
```

### CANCEL

```json
{
  "type": "CANCEL",
  "transfer_id": "uuid",
  "reason": "user_cancelled"
}
```

## Resume Model

MVP resume can be conservative:

- keep partial files under a transfer-specific temp directory
- store completed byte ranges per file
- on reconnect, receiver sends known completed offsets
- sender restarts from the last verified contiguous offset

Resume message:

```json
{
  "type": "RESUME_REQUEST",
  "transfer_id": "uuid",
  "files": [
    {
      "path": "large.mov",
      "received_bytes": 734003200
    }
  ]
}
```

## Error Codes

Recommended error codes:

```text
unsupported_protocol
device_not_trusted
pairing_required
user_declined
disk_full
permission_denied
file_changed
checksum_failed
network_interrupted
transfer_cancelled
internal_error
```

## Versioning

Use `protocol_version: 1` for MVP.

Breaking changes require a version bump. Devices with unsupported versions should remain visible but show an upgrade-required state.
