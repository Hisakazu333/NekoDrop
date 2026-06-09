# NekoDrop Roadmap

## Phase 0: Product and Architecture

Goal: lock the first version scope.

Deliverables:

- product definition
- architecture plan
- protocol draft
- security model
- UI flow sketches

Exit criteria:

- MVP scope is small enough to build
- no account/cloud/relay requirements in MVP
- project structure is agreed

## Phase 1: Desktop Skeleton

Goal: launch a working cross-platform desktop shell.

Deliverables:

- Tauri desktop app
- React/Vite frontend
- Rust workspace
- tray menu
- settings persistence
- basic pages: Home, Devices, Transfers, Settings

Exit criteria:

- app launches on macOS
- app can be prepared for Windows build
- UI can call Rust commands

## Phase 2: Local File Selection and Transfer Model

Goal: model transfers before networking.

Status: implemented for the connection-code flow.

Deliverables:

- drag and drop files
- choose files/folders
- manifest generation
- transfer job state model
- transfer history UI
- checksum calculation

Exit criteria:

- selected files become a transfer manifest
- UI displays file count and total size
- transfer state is backed by real scan and checksum data

## Phase 3: Connection-Code Transfer MVP

Goal: make transfer real before automatic discovery and pairing.

Status: implemented for local TCP connection codes.

Deliverables:

- open receive listener on the desktop app
- generate a connection code for the other computer
- send a transfer offer before file bytes
- accept or decline incoming transfer inside the app
- stream files over TCP
- show real progress, speed, ETA, and current file
- verify incoming files with SHA-256
- close an idle receive listener

Exit criteria:

- two app instances can transfer files by connection code
- receiver can reject before any file bytes are sent
- received files fail if headers do not match the accepted offer
- no fake devices, fake history, or simulated transfer rows appear in the UI

## Phase 4: LAN Discovery

Goal: show nearby devices.

Deliverables:

- local receive server
- mDNS advertisement
- mDNS discovery
- UDP broadcast fallback if needed
- online/offline state

Exit criteria:

- two app instances on the same LAN can see each other
- device name and platform appear correctly

## Phase 5: Pairing

Goal: trusted device relationship.

Deliverables:

- device identity generation
- pairing request
- confirmation dialog
- trusted device storage
- forget device

Exit criteria:

- untrusted device cannot receive file offers
- trusted devices persist after restart

## Phase 6: File Transfer Productization

Goal: move from connection-code transfer to trusted-device transfer.

Deliverables:

- transfer offer
- receive confirmation
- TCP chunk streaming
- progress events
- speed and ETA display
- close receive listener
- cancellation
- SHA-256 verification
- reveal received file

Exit criteria:

- Mac to Windows file transfer works
- Windows to Mac file transfer works
- completed file verifies correctly

## Phase 7: Folder Transfer and Resume

Goal: make large practical transfers reliable.

Deliverables:

- recursive folder manifests
- safe destination path handling
- partial file storage
- interrupted transfer resume
- retry failed transfer

Exit criteria:

- folder transfer preserves structure
- interrupted large file can resume from a partial state

## Phase 8: Polish

Goal: make the app feel like a real utility.

Deliverables:

- native notifications
- receive folder picker
- launch at login
- speed and ETA display
- empty states
- error recovery
- onboarding

Exit criteria:

- a non-technical user can install, pair, and transfer a file

## Later

Potential later features:

- remote relay mode
- browser receive link
- mobile client
- clipboard transfer
- automatic rules
- local folder sync
- QR code pairing
- transfer compression option
