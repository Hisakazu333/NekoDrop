# Module Boundaries

This document defines how the NekoDrop repository is divided. It is intended for contributors and maintainers.

The current repository contains the NekoDrop desktop product and the early NekoLink protocol code used by that product. Long-term OpenNeko, NekoState, mobile, Relay, and iroh work may integrate with this code later, but those areas should not be mixed into the desktop file transfer path before they are implemented.

## Layers

```text
Desktop UI
  User workflow, visible state, and error presentation

NekoDrop Service
  Application use cases: send, receive, pairing, history, and desktop commands

NekoDrop Storage
  Manifest creation, checksum, safe receive paths, partial files, and resume state

NekoDrop Network
  TCP file frames, connection codes, discovery, transport abstraction

NekoLink Protocol
  Message envelope, device identity, encrypted session, replay window, bundle and bridge models

Future Integrations
  iroh, Relay, mobile apps, NekoState, OpenNeko Agent channels
```

## Current Repository Strategy

NekoLink is currently developed inside this repository because the desktop transfer product is the first real integration target.

Do not split NekoLink into a separate repository until at least one of these is true:

- mobile clients need the same protocol crate or SDK;
- OpenNeko needs NekoLink without depending on NekoDrop desktop internals;
- Relay or iroh transport work needs independent release cadence;
- external contributors need a stable protocol package.

Until then, keep NekoLink code isolated in `crates/nekolink-protocol` and keep product-specific file transfer code outside that crate.

## Module Responsibilities

| Module | Owns | Does Not Own |
| --- | --- | --- |
| `apps/desktop` | React UI, desktop interaction, Tauri IPC calls | Protocol rules, file hashing, transport internals |
| `apps/desktop/src-tauri` | Tauri command handlers, app state, persisted config, device/history stores | Core protocol definitions |
| `crates/nekolink-protocol` | Envelope, message kinds, capabilities, device identity payloads, encrypted session payloads, replay window, encrypted file frame headers, bundle and local bridge JSON models | TCP sockets, files, UI, Tauri |
| `crates/nekodrop-network` | Endpoint parsing, connection code, TCP transport, encrypted/plain file frames, discovery models | UI state, receive directory policy |
| `crates/nekodrop-storage` | Manifest building, SHA-256, safe receive paths, partial files, resume plan inspection, bundle detection and staging | TCP streams, device pairing |
| `crates/nekodrop-service` | Product workflows built from protocol, network, and storage; send/receive; pairing; staged bundle reports; local bridge handler skeleton | Rendering, component layout |
| `apps/sidecar` | CLI/sidecar experiments and diagnostics | Main desktop UX |
| `docs/` | Current status, architecture, protocol, security, roadmap | Unverified feature claims |

## Dependency Direction

Preferred dependency direction:

```text
apps/desktop/src-tauri
  -> nekodrop-service
  -> nekodrop-network
  -> nekodrop-storage
  -> nekodrop-core

nekodrop-network
  -> nekolink-protocol

nekodrop-service
  -> nekolink-protocol
  -> nekodrop-network
  -> nekodrop-storage
```

Avoid reverse dependencies. For example, `nekolink-protocol` must not depend on Tauri, React, TCP sockets, local filesystem behavior, or NekoDrop desktop state.

## Feature Status Policy

The UI and README can only present a feature as available when it is backed by implemented code and tests.

Examples:

- If TCP transfer is implemented, document it as implemented.
- If iroh has only placeholder types, document it as experimental or planned.
- If mobile support is only a future integration target, document it as planned.
- If OpenNeko Agent messages are reserved in the protocol but not wired into the product, document them as planned.

## Current Implemented Areas

Implemented in the current desktop path:

- macOS / Windows desktop app structure
- file and folder selection
- manifest creation
- SHA-256 verification
- transfer offer / accept / decline
- TCP file transfer
- TCP partial offset resume foundation
- send and receive progress
- send and receive cancellation
- mDNS / DNS-SD discovery
- stable device identity
- trusted pairing foundation
- transfer history
- encrypted `session.control` for file offer / accept / decline
- replay window on encrypted transfer control readers
- encrypted file frames on the encrypted session transfer path
- bundle manifest validation, staging, and manual bundle creation
- local bridge protocol model, action result query, and internal read-only handler skeleton
- macOS and Windows packaging scripts

Experimental or planned:

- long-term authenticated device identity keys
- legacy plain transfer migration or retirement policy
- iroh transport
- Relay / P2P transport
- mobile main flow
- NekoState synchronization
- OpenNeko Agent command channel
- local bridge send execution and third-party adapter import execution

See [Current Status](STATUS.md) for the authoritative feature list.

## Open Source Boundary

This repository is licensed under Apache-2.0.

The license applies to source code and documentation committed to this repository. It does not automatically grant rights to:

- signing certificates or private keys;
- user data, device identities, or transfer history;
- OpenNeko commercial client code outside this repository;
- Live2D models, character assets, brand assets, or other commercial resources not committed here.

Keep those assets out of this repository.
