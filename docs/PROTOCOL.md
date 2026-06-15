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
- mDNS for discovery
- trusted pairing before nearby-device sends
- encrypted session handshake on the desktop send/receive path
- `file.offer`, `file.accept`, and `file.decline` inside encrypted `session.control`
- replay-window validation on encrypted control readers
- encrypted file frames for the encrypted session payload path
- SHA-256 verification after files are written

Future:

- long-term identity keys for authenticated sessions
- legacy plain transfer migration or retirement policy
- iroh / relay / P2P transports under the same session and file-frame semantics

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
  "public_key": "base64url-ed25519-public-key",
  "public_key_fingerprint": "sha256:hex"
}
```

Discovery entries are not trusted by themselves. They only make devices visible.

## Device Identity

Current identity fields:

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
    "encrypted_session"
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
session.hello
session.ready
session.identity
session.control
pairing.request
pairing.accept
pairing.reject
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

NekoDrop's current transfer path uses `device.hello`, `pairing.request`, `pairing.accept`, `pairing.reject`, `session.hello`, `session.ready`, `session.identity`, and `session.control`. Desktop file transfer sends `file.offer`, `file.accept`, and `file.decline` inside encrypted `session.control` envelopes. On the encrypted session path, file payloads are sent as encrypted file frames. The older plain file-frame path remains only as a manual compatibility path.

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

### session.hello

Encrypted-session offer payload. Current desktop transfers use this handshake for encrypted control messages and encrypted file frames.

Current protocol labels are `x25519` for key agreement, with `xchacha20poly1305` preferred over `aes256gcm` when both peers support them. Unknown key-agreement and cipher labels are rejected by protocol validation.

```json
{
  "session_id": "session-1781010000000",
  "identity": {
    "device_id": "sender-device-id",
    "device_name": "Hisakazu MacBook",
    "device_kind": "desktop",
    "platform": "macos",
    "public_key_fingerprint": "sha256:hex",
    "capabilities": ["file_transfer", "device_pairing", "encrypted_session"]
  },
  "key_agreement": "x25519",
  "ephemeral_public_key": "base64-public-key",
  "supported_ciphers": ["xchacha20poly1305", "aes256gcm"]
}
```

### session.ready

Encrypted-session response payload. The responder selects a cipher offered by `session.hello` and includes a `handshake_hash` over the hello/ready transcript. The initiator verifies that `handshake_hash` is `sha256:<64 hex chars>` and matches the original hello before deriving session key material.

```json
{
  "session_id": "session-1781010000000",
  "identity": {
    "device_id": "receiver-device-id",
    "device_name": "Peer Windows",
    "device_kind": "desktop",
    "platform": "windows",
    "public_key_fingerprint": "sha256:peer",
    "capabilities": ["file_transfer", "device_pairing", "encrypted_session"]
  },
  "key_agreement": "x25519",
  "ephemeral_public_key": "base64-peer-public-key",
  "cipher": "xchacha20poly1305",
  "handshake_hash": "sha256:hex"
}
```

### Session Key Material

After `session.ready` is verified, the protocol crate can build a key derivation context from the transcript. Desktop transfers use these keys for encrypted control frames and encrypted file frames on the encrypted session path.

Current derivation inputs:

```text
key agreement: x25519
ephemeral public key encoding: base64url without padding, 32 decoded bytes
shared secret length: 32 bytes
traffic key length: 32 bytes
KDF: HKDF-SHA256
salt: handshake_hash decoded from sha256:<64 hex chars>
send info: nekolink/<session_id>/<key_agreement>/<cipher>/<local_device_id>-><peer_device_id>
receive info: nekolink/<session_id>/<key_agreement>/<cipher>/<peer_device_id>-><local_device_id>
```

The same verified handshake produces mirrored directions on both peers: one side's `send_info` is the other side's `receive_info`. `SessionKeyDerivationContext::derive_key_material` returns a send key and receive key for encrypted control and file traffic.

`SessionEphemeralKeyPair` can generate an X25519 ephemeral secret, expose the encoded public key for `session.hello` / `session.ready`, and derive the same 32-byte shared secret from the peer public key on both sides. The secret is not printed by the keypair Debug implementation.

### Session Identity Signature

`SessionIdentityBinding` is the canonical material that long-term device keys sign. It binds:

```text
role
session_id
device_id
public_key_fingerprint
session_ephemeral_public_key
handshake_hash
```

`SignedSessionIdentityBinding` uses Ed25519. The signed payload is the binding's canonical SHA-256 hash, not ad-hoc JSON serialization. The public key and signature are base64url without padding, and the public key fingerprint is derived from the Ed25519 public key bytes.

Desktop authenticated sessions exchange a `session.identity` frame after `session.ready` and before encrypted `session.control` traffic:

```text
initiator -> responder: session.identity signed initiator binding
responder -> initiator: session.identity signed responder binding
```

Each side verifies that the signed binding matches the verified handshake, the peer device identity, and the advertised public-key fingerprint. Trusted-device public-key pinning is tracked separately; until that lands, this proves the peer owns the key advertised in the handshake, but does not yet prove it is the same key recorded during pairing.

### Legacy Plain Policy

The plain `file.offer` / file-frame path is kept for old clients and manual connection-code compatibility only. Desktop receivers mark this path as `legacy_plain`.

`legacy_plain` transfers:

- require explicit user approval
- are never auto-accepted as trusted-device transfers
- do not refresh trusted-device `last_seen` or device name
- are not valid for silent bundle import or local bridge automation

Trusted-device flows should use authenticated encrypted sessions.

### Session Traffic Frames

The protocol crate defines traffic-frame counters and nonce inputs for encrypted control frames and encrypted file frames. Desktop transfers use this for encrypted session control and file payloads. Replay-window enforcement is wired into the encrypted offer/decision readers.

```text
frame kinds: control, file
directions: send, receive
counter: u64, starts at 0 per local send/receive direction
nonce length: 24 bytes for xchacha20poly1305, 12 bytes for aes256gcm
nonce layout today: reserved zero bytes followed by u64 counter in big-endian
```

Send and receive counters are independent local state, but the nonce for a network frame is based on the frame counter and negotiated cipher, not the local send/receive label. The direction is carried in the header for bookkeeping; traffic keys already separate send from receive. Counter exhaustion is rejected before producing another frame header.

### Session Payload AEAD

The protocol crate can seal and open in-memory payloads with the negotiated AEAD:

```text
xchacha20poly1305: 32-byte traffic key, 24-byte nonce
aes256gcm: 32-byte traffic key, 12-byte nonce
associated data: caller-provided session/frame context bytes
```

Tampered ciphertext or mismatched associated data fails to open. The desktop TCP path uses this API for transfer control messages and encrypted file frames.

### session.control

Encrypted control envelope. The outer message kind is `session.control`; the encrypted payload records the original inner control kind and traffic frame header.

```json
{
  "inner_kind": "file.accept",
  "header": {
    "cipher": "xchacha20poly1305",
    "kind": "control",
    "direction": "send",
    "counter": 9,
    "nonce": [0, 0, 0, "..."]
  },
  "ciphertext": [1, 2, 3]
}
```

Associated data binds protocol name, version, session_id, message_id, outer kind, and inner kind. Moving the ciphertext to another envelope or changing the inner kind makes opening fail. Replay-window validation rejects duplicate or out-of-window control counters.

`nekodrop-network` exposes helper functions to write/read this encrypted `session.control` envelope over the existing length-prefixed TCP JSON frame format. It also has typed helpers for encrypted `file.offer`, `file.accept`, and `file.decline` control messages, including inner-kind checks on read. The desktop send/receive workflow uses this encrypted-control path before file bytes start.

### Encrypted File Frames

The encrypted session path sends file payloads as encrypted file frames. The
outer TCP file header remains so the receiver can preserve resume and storage
semantics. The payload following that header is encrypted in session traffic
frames.

Encrypted file frame AAD binds:

```text
transfer_id
manifest_path
offset
plain_size
traffic cipher
traffic kind
traffic direction
traffic counter
traffic nonce
```

Changing the transfer id, path, offset, size, direction, counter, nonce, or
cipher makes decryption fail. SHA-256 still verifies the final file after it is
written.

The receive helper exposes decrypted payload through a streaming reader. It reads
and opens encrypted file frames as the storage layer asks for bytes, instead of
building one full plaintext buffer for the file.

## Pairing Messages

Current desktop pairing runs through the same TCP JSON-frame channel as file offers. The first frame can be either `file.offer` or `pairing.request`.

### pairing.request

Sent by the device requesting trust.

```json
{
  "kind": "pairing.request",
  "request_id": "pairing-1780000000000",
  "device_id": "sender-device-id",
  "device_name": "Hisakazu MacBook",
  "platform": "macos",
  "public_key": "base64url-ed25519-public-key",
  "public_key_fingerprint": "sha256:hex",
  "pairing_code": "A1B-2C3",
  "listen_port": 45821
}
```

### pairing.accept

Sent after user confirmation.

```json
{
  "kind": "pairing.accept",
  "accepted": true,
  "reason": null
}
```

### pairing.reject

```json
{
  "kind": "pairing.reject",
  "accepted": false,
  "reason": "user_declined"
}
```

When accepted, both sides persist `trusted_devices.json` with the peer device ID, Ed25519 public key, public-key fingerprint, endpoint, and pairing code. The current pairing establishes device trust state; the encrypted transfer session is established separately by `session.hello` / `session.ready` / `session.identity`.

## Bundle messages

NekoLink bundle is specified in [BUNDLE_SPEC.md](BUNDLE_SPEC.md). The current protocol does not yet define a dedicated `bundle.offer` message kind. The first implementation should carry bundle directories through the existing file transfer path, then detect and validate `bundle.json`, `checksums.json`, `permissions.json`, and `files/` after receive.

Bundle import is not automatic. Receiving a valid bundle only creates a staged package for an upper layer to inspect and import after explicit confirmation.

## Transfer Messages

### Connection-code TCP v1

The current desktop build uses a compact TCP frame format. The encrypted session path wraps offer/decision messages in `session.control` and sends file payload as encrypted file frames. The plain offer and raw-byte file path is kept as a compatibility path.

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

On the plain compatibility path, after acceptance, the sender writes:

```text
u32 file_count
repeated:
  u32 json_header_length
  FileFrameHeader JSON
  raw file bytes
```

Each `FileFrameHeader` includes `manifest_path`, `size`, and `sha256`. The receiver rejects mismatched path, size, SHA-256, or file count.

On the encrypted session path, the sender writes the same file header shape, then
encrypted file-frame payload bytes instead of raw file bytes.

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
  "kind": "file.accept",
  "payload": {
    "accepted": true,
    "reason": null,
    "resume_files": [
      {
        "manifest_path": "large.mov",
        "received_bytes": 734003200
      }
    ]
  }
}
```

`resume_files` is omitted when the receiver has no usable completed or partial state. When present, each entry means the receiver already has a contiguous prefix for that manifest path and the sender should start that file frame at `received_bytes`.

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

Current TCP resume support is conservative:

- receiver scans completed files and `.nekodrop-part` partial files before accepting an offer
- receiver sends known contiguous offsets in `file.accept.resume_files`
- sender writes each file frame header with the full file size and the selected `offset`
- sender only streams bytes from `offset..file_size`
- receiver requires the incoming header `offset` to match the accepted resume decision
- receiver appends to the matching partial file and verifies final SHA-256 before finalizing

TCP file frame header:

```json
{
  "manifest_path": "large.mov",
  "size": 1048576000,
  "sha256": "hex",
  "offset": 734003200
}
```

`offset` defaults to `0` for normal full-file sends.

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
