# NekoDrop Transfer Protocol

## Goals

NekoLink should be simple, inspectable, and reliable on a local network.

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

Current implemented path:

- TCP for transfer sessions
- connection code contains host, port, and the receiver's public device identity
- receiver opens an explicit one-shot receive listener
- sender sends a `nekolink` Envelope with `file_offer` before any file bytes
- receiver responds with `file_accept` or `file_decline` before file bytes are sent
- receiver validates every incoming file header against the accepted offer
- file contents are streamed as binary bytes and verified with SHA-256

Planned LAN product path:

- mDNS for discovery
- UDP broadcast fallback for discovery if mDNS is unreliable
- trusted pairing before device-to-device offers

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

## Device Identity

Current V0.4 identity fields:

```json
{
  "device_id": "neko-device-0f4a11c8b9a2d311",
  "device_name": "Hisakazu MacBook",
  "device_kind": "desktop",
  "platform": "macos",
  "public_key_fingerprint": "sha256:hex",
  "capabilities": [
    "file_transfer",
    "file_send",
    "file_receive",
    "file_sha256",
    "device_pairing",
    "encrypted_session",
    "desktop_agent_host"
  ]
}
```

Supported device kinds:

```text
desktop
phone
tablet
openharmony
web
nas
agent_node
unknown
```

Supported platforms:

```text
macos
windows
linux
ios
android
openharmony
web
unknown
```

Desktop builds persist `device_identity.json` in the OS application data directory. The current implementation stores a random local seed and derives a SHA-256 public fingerprint from it. This is the stable identity foundation for trusted pairing; it is not the final encrypted-session key exchange.

## Envelope

Current control messages are wrapped in a NekoLink envelope:

```json
{
  "protocol": "nekolink",
  "version": 1,
  "session_id": "uuid",
  "message_id": "uuid",
  "kind": "file.offer",
  "sent_at_ms": 1781010000000,
  "capabilities": ["file_transfer", "file_sha256"],
  "payload": {}
}
```

Implemented message kinds:

```text
device.hello
device.heartbeat
file.offer
file.accept
file.decline
file.header
file.complete
transfer.complete
error
agent.command
agent.result
companion.state
state.sync
```

Only `file.offer`, `file.accept`, and `file.decline` are currently used by NekoDrop transfer sessions. The other kinds reserve the protocol surface for device identity, OpenNeko Agent commands, companion state, and future state sync.

### DEVICE_HELLO

Reserved NekoLink identity handshake payload:

```json
{
  "identity": {
    "device_id": "neko-device-0f4a11c8b9a2d311",
    "device_name": "Hisakazu MacBook",
    "device_kind": "desktop",
    "platform": "macos",
    "public_key_fingerprint": "sha256:hex",
    "capabilities": ["file_transfer", "device_pairing"]
  },
  "app_name": "NekoDrop",
  "app_version": "0.1.0"
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

### Current connection-code TCP v1

The current desktop build uses a compact TCP frame format with a NekoLink envelope before the full trusted-device protocol is introduced.

Connection code:

```text
nekodrop-v1;transport=tcp;host=192.168.1.24;port=45821;device_id=neko-device-0f4a11c8b9a2d311;name=Hisakazu%20MacBook;kind=desktop;platform=macos;fingerprint=sha256:hex
```

Older connection codes that only include `transport`, `host`, `port`, and `name` remain parseable.

Transfer offer envelope:

```json
{
  "protocol": "nekolink",
  "version": 1,
  "session_id": "transfer-1781010000000",
  "message_id": "transfer-1781010000000:offer",
  "kind": "file.offer",
  "sent_at_ms": 1781010000001,
  "capabilities": ["file_transfer", "file_sha256"],
  "payload": {
    "transfer_id": "transfer-1781010000000",
    "root_name": "Design Assets",
    "file_count": 2,
    "total_bytes": 2048,
    "files": [
      {
        "manifest_path": "Design Assets/logo.png",
        "size": 1024,
        "sha256": "hex"
      }
    ]
  }
}
```

Decision envelope:

```json
{
  "protocol": "nekolink",
  "version": 1,
  "session_id": "transfer-1781010000000",
  "message_id": "transfer-1781010000000:decision",
  "kind": "file.accept",
  "sent_at_ms": 1781010000002,
  "capabilities": ["file_transfer"],
  "payload": {
    "accepted": true,
    "reason": null
  }
}
```

After acceptance, the sender writes:

```text
u32 file_count
repeated:
  u32 json_header_length
  FileFrameHeader JSON
  raw file bytes
```

Each `FileFrameHeader` includes `manifest_path`, `size`, and `sha256`. The receiver rejects mismatched path, size, SHA-256, or file count.

### Target trusted-device messages

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
        "sha256": "hex"
      },
      {
        "path": "screenshots",
        "kind": "directory"
      }
    ]
  }
}
```

The current implementation computes SHA-256 before sending the offer. Later trusted-device flows may introduce a lighter pre-hash offer, but file bytes must still be verified before completion.

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
