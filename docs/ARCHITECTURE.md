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

## Architecture Health

The repository is not treated as a rewrite target. The current layering is
usable, tests exist across the protocol, network, storage, service, and desktop
surfaces, and the main flows are still understandable. The risk is different:
several files became catch-all implementation points during fast iteration.

Current hotspots:

```text
apps/desktop/src-tauri/src/commands/mod.rs
crates/nekolink-protocol/src/lib.rs
apps/desktop/src/App.tsx
apps/desktop/src/styles.css
crates/nekodrop-network/src/tcp_file.rs
crates/nekodrop-service/src/lib.rs
```

These files can still receive small fixes, but new feature work should not make
them larger by default. When a change adds another workflow, command group,
view, or protocol domain, split the target first or as part of the same PR.

Preferred next splits:

```text
apps/desktop/src-tauri/src/commands/
  mod.rs
  transfer.rs
  devices.rs
  bundles.rs
  bridge.rs
  settings.rs
  security.rs

apps/desktop/src/views/
  OverviewView.tsx
  SendView.tsx
  ReceiveView.tsx
  DevicesView.tsx
  TransfersView.tsx
  SettingsView.tsx

apps/desktop/src/styles/
  base.css
  layout.css
  views.css
  components.css

crates/nekolink-protocol/src/
  envelope.rs
  identity.rs
  session.rs
  bundle.rs
  bridge.rs
  crypto.rs
```

Do not split just to move code around. Split when the new boundary makes the
next feature smaller, gives tests a clearer target, or keeps product code out of
protocol code.

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
Local bridge localhost runtime
Local bridge pending-action worker
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
traffic kind, direction, counter, nonce, and cipher. The receive side exposes a
streaming reader that decrypts frames as storage reads, so it does not need a
full single-file plaintext buffer.

After `session.ready`, both sides exchange signed `session.identity` bindings.
The desktop identity store persists an Ed25519 signing seed, verifies the peer
binding, and pins authenticated trusted sessions to the public key saved in the
trusted-device record. Manual connection-code transfers can still authenticate a
session without becoming trusted devices.

This is not yet a complete device-management system. Key rotation, OS keychain
storage, and cross-platform identity policy still need separate work.

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
  -> manual import writes into NekoDrop local import storage
  -> upper-layer adapter can import into its own app later
```

Bundle staging must not silently modify another application's config. Import is
a separate user or upper-layer decision. Current desktop import uses a temporary
directory and refuses same-name conflicts instead of overwriting.

### Local Bridge

Local Bridge is the loopback API for desktop apps or plugins that want to use
NekoLink without implementing the network protocol themselves.

Current repository status:

```text
local app request JSON
  -> LocalBridgeRequest model in nekolink-protocol
  -> localhost POST /bridge/request bound to 127.0.0.1
  -> scoped authorization with short confirmation code
  -> read-only snapshots or queued mutating actions
  -> worker executes authorized bundle.send / bundle.import
  -> events.poll returns transfer, bundle, and action lifecycle events
```

The bridge is not a LAN API. It must stay loopback-only, scoped, revocable, and
careful about paths. Bridge responses and events must not expose local
`bundle_root` values to ordinary clients.

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

- key rotation and OS-level private-key protection are not implemented
- legacy plain transfer compatibility still needs a retirement or migration policy
- upper-layer adapters do not yet perform real app import/export
- local bridge has short polling, but not a long-lived event stream
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

## When To Stop And Split

Use this rule before adding a new feature:

```text
If the change only fixes a bug in an existing flow:
  keep it local.

If the change adds a new command family:
  create or extend a command module, not the central commands file.

If the change adds a new page state machine:
  create a view module and keep App.tsx as shell/routing glue.

If the change adds a new protocol object:
  put protocol types and validation in nekolink-protocol, then call them from
  service/network code.

If the change adds third-party app behavior:
  model it as bundle/adapter/local-bridge behavior, not as a hard-coded product
  path in NekoLink.
```

The goal is not a perfect folder tree. The goal is that a contributor can open
one feature area, understand the owning module, run the relevant tests, and make
a change without touching unrelated product flows.
