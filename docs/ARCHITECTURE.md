# NekoDrop Architecture

This document is the map for the current repository. For feature status, use
[STATUS.md](STATUS.md). For contribution boundaries, use [MODULES.md](MODULES.md).

NekoDrop is the desktop product. NekoLink is the protocol layer being grown
inside this repository until it has more than one stable integration target.

## Current Shape

```text
apps/desktop
  React UI and Tauri desktop shell

apps/desktop/src-tauri
  Tauri commands, app state, desktop lifecycle, platform integration

crates/nekodrop-service
  Product workflows: send, receive, pairing, history, bundle staging reports

crates/nekodrop-network
  Discovery, connection code, TCP transport, file frames, session wire helpers

crates/nekodrop-storage
  Manifest building, checksum, safe receive paths, partial/resume, bundle staging

crates/nekodrop-core
  Device, pairing, transfer, manifest, config, shared errors

crates/nekolink-protocol
  Envelope, identity, encrypted session model, replay window, bundle and bridge JSON models
```

The dependency direction should stay one way:

```text
desktop UI
  -> Tauri commands
  -> nekodrop-service
  -> nekodrop-network / nekodrop-storage / nekodrop-core
  -> nekolink-protocol
```

`nekolink-protocol` must stay free of Tauri, React, sockets, local filesystem
rules, and NekoDrop UI state.

## Runtime Services

The desktop app starts these runtime pieces from the Tauri side:

```text
App config
Device identity
Discovery advertisement and scan
Receive listener
Pairing state
Transfer history
Trusted device store
Desktop snapshot refresh
Tray/window integration
```

The React side renders state and calls commands. It does not scan folders,
hash files, open sockets, parse protocol frames, or decide trust policy.

## Main Data Flows

### Plain Compatibility Transfer

This path exists for compatibility and tests.

```text
sender selects files
  -> storage builds manifest
  -> service builds transfer offer
  -> network sends plain file offer
  -> receiver accepts or declines
  -> network sends legacy file frames
  -> storage writes partial files and finalizes after SHA-256
```

Do not expand this path for new security-sensitive features.

### Encrypted Session Transfer

This is the desktop mainline after the session work.

```text
sender selects files
  -> storage builds manifest
  -> session.hello / session.ready
  -> X25519 + HKDF-SHA256 session keys
  -> file.offer / file.accept / file.decline inside encrypted session.control
  -> replay window validates control counters
  -> file payload is sent as encrypted file frames
  -> storage writes partial files and finalizes after SHA-256
```

Encrypted file frame AAD binds transfer id, manifest path, offset, plain size,
traffic kind, direction, counter, nonce, and cipher. The current receive helper
still decrypts a complete single-file payload before handing it to storage; the
next security/performance step is streaming decrypt on receive.

Long-term device identity keys are not wired yet. Current session encryption is
ephemeral and tied to the existing desktop identity checks, not a full
authenticated device-key system.

### Bundle Staging

Bundles are upper-layer data packages for things like skills, sessions,
workspace fragments, agent profiles, and config snapshots.

```text
selected bundle directory
  -> storage validates bundle.json / checksums.json / permissions.json
  -> service sends it through the normal transfer path
  -> receiver detects valid bundle structure
  -> storage saves to application staging
  -> UI shows save/import state
  -> upper-layer application can request import later
```

Bundle staging must not silently modify another application's config. Import is
a separate user or upper-layer decision.

### Local Bridge

Local Bridge is the future local API for desktop apps or plugins that want to
use NekoLink without implementing the network protocol themselves.

Current repository status:

```text
local app request JSON
  -> LocalBridgeRequest model in nekolink-protocol
  -> desktop internal handler for read-only snapshots
  -> pending_auth for send/import/authorization paths
```

It is not a public localhost server yet. Persistent authorization, tokens,
authorization code flow, and import execution are still pending.

## Transport Boundary

Current stable transport:

- LAN TCP for desktop transfers
- mDNS / DNS-SD for discovery
- connection code and `IP:port` fallback

Experimental placeholders:

- iroh
- relay
- P2P / NAT traversal

Future transports must sit under the same NekoLink session, file frame, and
bundle semantics. They should not create a second product-specific protocol.

## Safety Boundaries

NekoDrop currently enforces these boundaries:

- incoming transfers require confirmation
- untrusted nearby devices cannot silently send files
- trusted-device sends are checked by device id and fingerprint
- received paths are normalized and kept inside the receive directory
- SHA-256 verifies completed files
- encrypted session control frames use replay windows
- encrypted session file frames bind metadata into AAD
- bundle import is not automatic

Known gaps:

- encrypted receive path needs streaming decrypt for large files
- long-term identity keys are not wired into session authentication
- local bridge has no runtime server or persisted authorization yet
- iroh / relay / P2P are not implemented transports
- mobile and Agent command channels are not current product paths

## Where New Work Goes

Use this rule when adding code:

| Work | Place |
| --- | --- |
| React view state, page layout, copy | `apps/desktop/src` |
| Tauri commands, app state, platform glue | `apps/desktop/src-tauri` |
| Send/receive/pairing workflow | `crates/nekodrop-service` |
| TCP, discovery, connection code, file frames | `crates/nekodrop-network` |
| Manifests, checksum, receive paths, staging | `crates/nekodrop-storage` |
| Device, transfer, config domain models | `crates/nekodrop-core` |
| Protocol JSON, session frame model, bundle/bridge types | `crates/nekolink-protocol` |

If a feature is only useful for one third-party application, keep it behind an
adapter or local bridge layer. NekoLink itself should stay application-neutral.
