# NekoDrop Product Definition

## One Sentence

NekoDrop lets macOS and Windows computers send files to each other directly on the same local network with minimal setup.

## Product Positioning

NekoDrop should behave like a desktop-native local transfer utility, not a cloud product.

The user should understand the product in one glance:

```text
Nearby computers appear here.
Drop files onto a device.
Confirm once.
The file arrives.
```

## Primary Users

- people who use both a Mac and a Windows PC
- students moving documents, screenshots, media, archives, or project folders
- developers transferring builds, installers, logs, datasets, or design files
- users who do not want to use messaging apps, USB drives, or cloud storage for local transfers

## Core Jobs

1. Send files from Mac to Windows.
2. Send files from Windows to Mac.
3. Send folders without compressing manually.
4. Pair trusted devices once.
5. See transfer speed, progress, and completion state.
6. Recover from interrupted large transfers.

## MVP Experience

### First Launch

- app asks for device name
- app asks for receive folder
- app starts local discovery
- app shows this computer's local status

### Device Discovery

- nearby devices appear automatically
- each device shows name, platform, online state, and trust state
- untrusted devices require pairing before receiving files

### Pairing

- sender starts pairing
- receiver sees a confirmation dialog with device name and short code
- both sides store trusted device keys after confirmation

### Sending

- user drags files or folders onto a trusted device
- receiver confirms the incoming transfer unless auto-accept is enabled for the trusted device
- sender and receiver both show progress
- completed file is verified by checksum

### Receiving

- incoming transfer dialog shows sender, file count, total size, and destination folder
- user can accept, decline, or change destination
- after completion, user can reveal in Finder or Explorer

## Main Pages

### Home

Purpose: fast sending.

Content:

- big drop zone
- nearby trusted devices
- current network state
- quick actions: choose files, choose folder, send clipboard

### Devices

Purpose: discovery and trust management.

Content:

- nearby devices
- trusted devices
- pending pairing requests
- device details
- forget device

### Transfers

Purpose: observe active and past transfers.

Content:

- active transfers
- completed transfers
- failed transfers
- retry and resume
- reveal received files

### Settings

Purpose: local app behavior.

Content:

- device name
- receive folder
- tray/background mode
- launch at login
- receive confirmation behavior
- discovery toggle
- trusted device defaults

## Product Principles

- Local first: do not require accounts for LAN transfer.
- Explicit trust: first-time pairing must be visible.
- Quiet by default: no noisy dashboards or social features.
- Fast path: drag to device should be the primary workflow.
- Recoverable: large transfers should survive common network interruptions.
- Transparent: show where files go and whether verification passed.

## Non Goals

The MVP should not become:

- a cloud drive
- a chat app
- a remote desktop app
- a full sync engine
- a file manager replacement
- a collaboration workspace

