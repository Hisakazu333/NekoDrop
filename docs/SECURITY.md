# NekoDrop Security Model

## Security Goals

NekoDrop should make local transfer convenient without making nearby devices implicitly trusted.

The security model must provide:

- explicit first-time pairing
- stable trusted device identity
- encrypted transfer sessions
- integrity verification
- clear user confirmation for incoming files
- safe default receive behavior

## Threat Model

MVP should protect against:

- unknown nearby device sending files silently
- device impersonation after pairing
- accidental acceptance from the wrong computer
- corrupted files during transfer
- path traversal inside received folder manifests
- overwriting important local files without confirmation

MVP does not fully solve:

- compromised trusted device
- malicious files opened by the user after transfer
- hostile network with advanced traffic analysis
- remote relay abuse, because relay is out of scope

## Device Identity

Each installation generates:

- stable device ID
- local secret seed for identity derivation
- public SHA-256 fingerprint
- user-visible device name

The secret seed stays local.

The public key fingerprint is shown during pairing and used in trusted device records.

Current status:

- desktop builds persist `device_identity.json` in the OS application data directory
- new desktop identities use schema v2 with a persisted Ed25519 signing seed
- old schema v1 desktop identities are migrated to schema v2 on load
- connection codes include the receiver's public identity fields
- this is the foundation for trusted pairing and current desktop identity checks
- protocol-level signed session identity bindings exist, but the desktop session handshake does not exchange or verify them yet

The later trusted-pairing stage should exchange and verify these long-term public keys between paired devices.

## Pairing

First-time pairing requires user confirmation on both sides.

Recommended MVP flow:

1. Sender requests pairing.
2. Receiver sees sender name, platform, and short code.
3. Sender sees the same short code.
4. User confirms that the codes match.
5. Both devices store each other's public key and device metadata.

Do not silently trust devices only because they are on the same Wi-Fi.

## Trusted Device Record

Store:

```json
{
  "device_id": "stable-device-id",
  "device_name": "Windows PC",
  "platform": "windows",
  "public_key": "base64url",
  "fingerprint": "base64url",
  "paired_at": "2026-06-09T15:00:00Z",
  "last_seen_at": "2026-06-09T15:12:00Z",
  "auto_accept": false
}
```

## Session Encryption

Current desktop transfers have an encrypted session path:

- `session.hello` / `session.ready` perform an ephemeral X25519 handshake
- HKDF-SHA256 derives per-direction traffic keys from the verified transcript
- `file.offer`, `file.accept`, and `file.decline` are sealed inside encrypted `session.control`
- encrypted control readers use a replay window
- encrypted file frames protect file payloads on the encrypted session path
- file-frame AAD binds transfer id, manifest path, offset, plain size, cipher, direction, counter, and nonce
- encrypted receive reads decrypt frames on demand instead of buffering a whole file payload
- protocol-level Ed25519 signed session identity bindings exist
- desktop identities can sign a session identity binding with the persisted local signing key

Remaining work:

- exchange and verify signed session identity bindings in the desktop session handshake
- decide how and when to retire the plain compatibility transfer path

Do not describe a transfer as fully authenticated just because it uses the current encrypted session path. It has confidentiality and integrity for encrypted frames, plus replay protection for encrypted control readers, but long-term device-key authentication is not complete.

## File Manifest Safety

Received manifests must be normalized before writing.

Reject:

- absolute paths
- parent directory traversal
- empty paths
- reserved Windows device names
- paths with invalid platform separators
- paths that escape the destination folder after normalization

Never write directly to the final destination until the file is complete and verified.

## Receive Confirmation

Default behavior:

- trusted devices still require confirmation before sending files
- auto-accept can be enabled per trusted device later
- untrusted devices cannot send files

Incoming dialog must show:

- sender device name
- file count
- total size
- destination folder
- whether sender is trusted

## Overwrite Policy

Default:

- do not overwrite existing files silently
- if a name exists, create a unique name such as `file (1).ext`
- later versions can offer overwrite/skip/rename options

## Integrity

Each completed file should be verified with a cryptographic hash.

MVP:

- SHA-256 per file
- transfer-level total byte count

Future:

- BLAKE3 for faster hashing
- per-chunk hash tree for better resume validation

## Local Data Protection

Sensitive local files:

- device private key
- trusted device list
- transfer history

Use OS application data directories. Private keys should use platform key storage when practical.

Future:

- macOS Keychain
- Windows Credential Manager or DPAPI

## User-Facing Security States

Show clear states:

```text
Untrusted
Pairing
Trusted
Receiving
Verified
Failed verification
```

Avoid vague states such as "secure" unless the connection is actually authenticated and encrypted.

## Dependency Audits

Run the supported-platform audit before merging security-sensitive changes:

```bash
npm run security:audit
```

The script checks:

- `npm audit --json`
- Cargo dependency graphs for the supported desktop targets:
  - `aarch64-apple-darwin`
  - `x86_64-apple-darwin`
  - `x86_64-pc-windows-msvc`
- OSV advisories for crates that are present in those target graphs

GitHub Dependabot also scans the full `Cargo.lock`. That can include crates for targets we do not ship yet.

Current known alert:

```text
GHSA-wrw7-89jp-8q8g / RUSTSEC-2024-0429
crate: glib 0.18.5
chain: tauri -> tauri-runtime-wry -> wry -> webkit2gtk / gtk -> glib
status: Linux GTK/WebKit dependency in Cargo.lock; not part of the macOS or Windows release graph
upstream fix needed: webkit2gtk/wry stack moving from glib 0.18 to glib >= 0.20
```

Do not mark this as fixed by editing around `Cargo.lock`. Re-check it when Tauri, wry, or webkit2gtk updates their Linux GTK stack.
