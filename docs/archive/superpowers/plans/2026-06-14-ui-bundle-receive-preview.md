# UI Bundle Receive Preview Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show a compact received bundle summary in the existing desktop receive UI.

**Architecture:** Consume the `ReceiveReportDto.bundle` field merged in PR #27. Keep the UI inside the existing “最近完成” receive section and status line. Do not add a new page, large card, import button, bridge action, or marketing text.

**Tech Stack:** React, TypeScript, existing CSS utility classes, `npm run build`.

---

### Task 1: Compact Bundle Copy

**Files:**
- Modify: `apps/desktop/src/App.tsx`

- [ ] **Step 1: Add helper labels**

Add small helpers:

```ts
function bundleTypeLabel(type: string): string
function receiveBundleSummary(report: ReceiveReportDto): string | null
function receiveBundleStatus(report: ReceiveReportDto): string | null
```

Expected text:
- type labels: `skill -> Skill`, `session -> Session`, `workspace -> Workspace`, `agent_profile -> Agent profile`, `config_snapshot -> Config`
- import status: `可导入` when `import_allowed`, otherwise `仅保存`

- [ ] **Step 2: Add receive panel preview**

In the existing “最近完成” section, keep the current `result-line` and add one quiet line only when `receiveReport.bundle` exists:

```tsx
{receiveBundleSummary(receiveReport) ? (
  <div className="bundle-line">
    <span>{receiveBundleSummary(receiveReport)}</span>
    <strong>{receiveBundleStatus(receiveReport)}</strong>
  </div>
) : null}
```

### Task 2: Status Line

**Files:**
- Modify: `apps/desktop/src/App.tsx`

- [ ] **Step 1: Adjust status-line copy**

When the latest receive report has a bundle, change the status line detail from only file count to:

```text
<bundle display name> · <bundle type> · <size> · <status>
```

For normal file receives, keep the old copy.

### Task 3: CSS And Verification

**Files:**
- Modify: `apps/desktop/src/styles.css`
- Modify: `docs/STATUS.md`

- [ ] **Step 1: Add restrained CSS**

Add `.bundle-line` near `.result-line`. It should be compact, not card-like:
- flex row, wrapping allowed
- muted text
- small font
- no nested card, no heavy border, no large padding

- [ ] **Step 2: Update status**

Update `docs/STATUS.md` so the bundle row says desktop receive UI shows a compact staged bundle summary. Local bridge import remains pending.

- [ ] **Step 3: Verify**

```bash
npm run build
rustup run stable env RUSTC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc RUSTDOC=/Users/hisakazu/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustdoc cargo test -p nekodrop-desktop receive_report_dto
rustup run stable cargo fmt --all -- --check
git diff --check
```

- [ ] **Step 4: Commit and PR**

```bash
git add apps/desktop/src/App.tsx apps/desktop/src/styles.css docs/STATUS.md docs/superpowers/plans/2026-06-14-ui-bundle-receive-preview.md
git commit -m "feat: show received bundle preview"
git push -u origin ui/bundle-receive-preview
```
