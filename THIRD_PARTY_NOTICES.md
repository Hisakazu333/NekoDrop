# Third Party Notices

NekoDrop uses third-party open source software. This file is a human-readable notice for notable direct dependencies. It is not a complete generated software bill of materials.

The exact dependency graph is recorded in:

- `Cargo.lock`
- `package-lock.json`

Before a stable public release, generate a complete third-party license report from the lockfiles and include it with release artifacts.

## Notable Direct Dependencies

| Dependency | Area | License |
| --- | --- | --- |
| Tauri | Desktop runtime and bundling | Apache-2.0 OR MIT |
| `@tauri-apps/api` | JavaScript bridge to Tauri commands | Apache-2.0 OR MIT |
| React / React DOM | Desktop WebView UI | MIT |
| Vite / `@vitejs/plugin-react` | Frontend build tooling | MIT |
| TypeScript | Type checking and frontend build support | Apache-2.0 |
| `serde` / `serde_json` | Rust serialization | MIT OR Apache-2.0 |
| `mdns-sd` | Local network service discovery | Apache-2.0 OR MIT |
| `sha2` / `hex` | Checksums and encoding | MIT OR Apache-2.0 |
| `walkdir` | Directory traversal for transfer manifests | Unlicense OR MIT |
| `getrandom` | Randomness used by desktop identity code | MIT OR Apache-2.0 |

## Distribution Notes

- Keep this file and the root `LICENSE` with source releases.
- Packaged desktop releases should include a complete generated third-party notice before a stable release.
- Do not place signing certificates, private keys, local device identities, user transfer history, or OpenNeko commercial assets in this repository.
- Third-party project names and trademarks belong to their respective owners.
