# Console Shell Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rework the desktop UI shell so NekoDrop feels like a restrained local transfer console instead of a patched file sender.

**Architecture:** Keep existing Tauri commands and state logic. This PR only reshapes the React shell, navigation, page composition, and CSS surfaces. Bundle and bridge pages may expose current read-only/pending state, but must not add import execution, authorization persistence, or localhost runtime.

**Tech Stack:** React, TypeScript, existing Tauri command wrapper, existing CSS.

---

### Task 1: Navigation And Page Names

**Files:**
- Modify: `apps/desktop/src/App.tsx`

- [x] Rename `ComposerMode` to include `overview`, `send`, `receive`, `devices`, `transfers`, `bundles`, `integrations`, and `settings`.
- [x] Replace the old sidebar labels with `概览 / 发送 / 收件 / 设备 / 传输 / 资料包 / 集成 / 设置`.
- [x] Remove the standalone `queue` route from navigation; keep queue controls inside the send page.
- [x] Rename the old history route to `transfers`.

### Task 2: Page Composition

**Files:**
- Modify: `apps/desktop/src/App.tsx`

- [x] Add `OverviewPanel` with current receive state, nearby count, pending receive count, active transfer summary, recent transfer rows, latest bundle state, and bridge placeholder state.
- [x] Keep the send page focused on target, content, queue preview, and send action.
- [x] Keep `ReceivePanel`, `DevicePanel`, `HistoryPanel`, and `SettingsPanel` wired to existing state and handlers.
- [x] Add `BundlePanel` that shows the latest received bundle from `receiveReport` and current staged bundle commands available in this client.
- [x] Add `IntegrationPanel` that shows local bridge status and pending authorization state without coupling protocol copy to a specific app.

### Task 3: Flat Console Styling

**Files:**
- Modify: `apps/desktop/src/styles.css`

- [x] Keep the sidebar and workspace.
- [x] Reduce hero styling on send page.
- [x] Make page sections flat: no heavy card stack, no visible divider lines.
- [x] Use compact rows, soft fills only when an element is interactive or stateful.
- [x] Keep the current light palette and scarce orange accent.

### Task 4: Verification

**Files:**
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/styles.css`
- Create: this plan file

- [x] Run `npm run build`.
- [x] Run `git diff --check`.
- [x] Review changed UI strings for accidental app-specific protocol claims.
