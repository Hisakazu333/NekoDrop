# NekoDrop Architecture

## Overview

NekoDrop is a cross-platform desktop app with a Rust core and a Tauri UI shell.

The architecture separates product UI, desktop integration, core domain logic, network transport, and storage handling.

```text
React UI
  |
Tauri commands and events
  |
Rust application service
  |
Core domain / Network / Storage crates
```

## Recommended Repository Layout

```text
NekoDrop/
  apps/
    desktop/
      src/
        app/
        components/
        pages/
        stores/
        styles/
      src-tauri/
        src/
          main.rs
          app_state.rs
          commands/
          events/
          tray/
          platform/
        tauri.conf.json

  crates/
    nekodrop-core/
      src/
        device.rs
        pairing.rs
        transfer.rs
        manifest.rs
        config.rs
        errors.rs
        lib.rs

    nekodrop-network/
      src/
        discovery.rs
        protocol.rs
        server.rs
        client.rs
        transport.rs
        lib.rs

    nekodrop-storage/
      src/
        chunk.rs
        checksum.rs
        receive_dir.rs
        resume.rs
        lib.rs

  docs/
```

## Module Responsibilities

### apps/desktop/src

Frontend UI only.

Responsibilities:

- render pages
- handle drag and drop
- call Tauri commands
- subscribe to transfer events
- show progress and notifications

The frontend should not implement transfer protocol logic.

### apps/desktop/src-tauri

Desktop integration layer.

Responsibilities:

- window lifecycle
- tray menu
- launch at login integration
- file picker commands
- platform-specific receive folder behavior
- command bridge between UI and Rust services

### nekodrop-core

Product domain logic.

Responsibilities:

- device model
- trusted device state
- pairing state
- transfer job state
- file manifest model
- app config model
- shared errors

This crate should not depend on Tauri.

### nekodrop-network

Discovery and transport.

Responsibilities:

- advertise local device
- discover remote devices
- open sender and receiver connections
- encode and decode protocol messages
- manage transfer sessions

MVP transport can use TCP. The transport abstraction should allow QUIC later.

### nekodrop-storage

File system and transfer persistence.

Responsibilities:

- scan selected files and folders
- create transfer manifests
- split files into chunks
- write temporary partial files
- resume interrupted transfers
- compute checksums
- finalize received files atomically

## Runtime Services

The desktop app should start these services after launch:

```text
ConfigService
DeviceIdentityService
DiscoveryService
PairingService
TransferService
ReceiveServer
TrayService
```

## Data Flow: Send File

```text
User drops files on device
  -> UI calls create_transfer_offer
  -> core builds manifest
  -> network sends SEND_OFFER
  -> receiver accepts
  -> transfer session starts
  -> storage streams chunks
  -> receiver writes partial files
  -> checksum verification
  -> completion event
```

## Data Flow: Pair Device

```text
Untrusted nearby device appears
  -> user starts pairing
  -> PAIR_REQ sent
  -> receiver sees confirmation and short code
  -> receiver accepts
  -> both sides store trusted device record
```

## Local Persistence

Suggested local state:

```text
config.json
device_identity.json
trusted_devices.json
transfers.db
partial_transfers/
```

Use OS-specific application data directories:

- macOS: Application Support/NekoDrop
- Windows: AppData/Roaming/NekoDrop

## Cross Platform Notes

macOS:

- Finder reveal support
- launch at login support
- Bonjour/mDNS support is usually available
- local network permission prompts may matter

Windows:

- Explorer reveal support
- firewall prompt may appear for local server
- service discovery may need UDP fallback
- long path handling should be considered

## MVP Technical Choices

Use simple, reliable choices first:

- TCP listener per app instance
- mDNS advertisement for discovery
- UDP broadcast fallback
- JSON or MessagePack control messages
- binary chunk frames for file payload
- SHA-256 checksum per file
- BLAKE3 can be considered later for speed

## Future Architecture Options

After MVP:

- QUIC transport
- relay server for remote transfer
- browser receive mode
- mobile client
- folder sync engine
- automatic rules engine

