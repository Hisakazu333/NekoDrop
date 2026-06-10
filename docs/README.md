# NekoDrop Documentation

This directory contains product, architecture, protocol, security, and roadmap documents for NekoDrop.

NekoDrop is an early desktop file transfer project. Some documents describe current behavior, while others describe planned protocol and ecosystem work. Use the status labels below to avoid mixing shipped features with future direction.

## Start Here

For users and release readers:

- [Project README](../README.md): what NekoDrop is, how to build it, and how to use the current desktop app.
- [Current Status](STATUS.md): source of truth for what is implemented, experimental, or planned.
- [Security Model](SECURITY.md): trust, pairing, receive safety, and known security limits.

For developers:

- [Development](DEVELOPMENT.md): local setup, tests, packaging, and workflow.
- [Architecture](ARCHITECTURE.md): workspace layout and responsibility boundaries.
- [Protocol](PROTOCOL.md): NekoLink message envelope, transfer flow, and TCP file frame behavior.
- [Modules](MODULES.md): module ownership and dependency direction.

For maintainers and planning:

- [Product Definition](PRODUCT.md): product scope and user jobs.
- [Roadmap](ROADMAP.md): versioned implementation phases.
- [Future Iteration Plan](FUTURE_ITERATION_PLAN.md): long-term planning notes.
- [Module Roadmap](modules/MODULE_ROADMAP.md): module-by-module future work.

## Status Labels

Use these labels consistently:

- `Implemented`: code exists and the current desktop app can use it.
- `Experimental`: code or interfaces exist, but the feature is not a supported user workflow yet.
- `Planned`: product or protocol direction only.
- `Out of scope`: intentionally not part of the current phase.

Chinese status labels in existing documents map to the same meaning:

- `已接入` = Implemented
- `实验中` = Experimental
- `待接入` = Planned
- `不做` = Out of scope

## Documentation Rules

- User-facing documents should describe current behavior first.
- Planned OpenNeko, NekoState, Relay, P2P, iroh, or mobile work must be marked as planned or experimental unless it is already implemented.
- Internal product judgment should be written as project policy, not as conversation notes.
- Protocol changes should update [Protocol](PROTOCOL.md), [Status](STATUS.md), and the relevant module document.
- Release notes should only claim behavior verified by tests or manual packaging checks.
