# NekoBuddy Workspace Adapter

This is the first concrete upper-application adapter for NekoDrop / NekoLink. It handles one resource only: a NekoBuddy workspace.

It is intentionally narrower than `docs/examples/generic-adapter/`: the generic adapter documents the contract, while this adapter proves a real application can export, send, import, and roll back its own workspace data through NekoDrop.

## Commands

```bash
node adapters/nekobuddy-workspace-adapter/nekobuddy-workspace-adapter.mjs descriptor
node adapters/nekobuddy-workspace-adapter/nekobuddy-workspace-adapter.mjs app-manifest
node adapters/nekobuddy-workspace-adapter/nekobuddy-workspace-adapter.mjs export \
  --source ./workspace \
  --output ./out \
  --bundle-id bundle_workspace_demo \
  --name "Workspace demo"
node adapters/nekobuddy-workspace-adapter/nekobuddy-workspace-adapter.mjs import-dry-run \
  --bundle-root ./out/bundle_workspace_demo \
  --target-root ./adapter-data
node adapters/nekobuddy-workspace-adapter/nekobuddy-workspace-adapter.mjs import-confirm \
  --bundle-root ./out/bundle_workspace_demo \
  --target-root ./adapter-data \
  --conflict-strategy reject
node adapters/nekobuddy-workspace-adapter/nekobuddy-workspace-adapter.mjs rollback \
  --receipt ./adapter-data/workspaces/bundle_workspace_demo/.nekobuddy-workspace-import-receipt-*.json
node adapters/nekobuddy-workspace-adapter/nekobuddy-workspace-adapter.mjs discover-bridge
node adapters/nekobuddy-workspace-adapter/nekobuddy-workspace-adapter.mjs post events \
  --bridge-url http://127.0.0.1:45921/bridge/request \
  --action-request-id bundle_workspace_demo-send
node adapters/nekobuddy-workspace-adapter/nekobuddy-workspace-adapter.mjs send-workspace \
  --source ./workspace \
  --output ./out \
  --bundle-id bundle_workspace_demo \
  --name "Workspace demo" \
  --target-device-id paired-device-1
node adapters/nekobuddy-workspace-adapter/nekobuddy-workspace-adapter.mjs receive-workspace \
  --staged-bundle-id bundle_workspace_demo \
  --bundle-root ./out/bundle_workspace_demo \
  --target-root ./adapter-data \
  --conflict-strategy reject
node adapters/nekobuddy-workspace-adapter/nekobuddy-workspace-adapter.mjs rollback-workspace \
  --bundle-id bundle_workspace_demo \
  --receipt ./adapter-data/workspaces/bundle_workspace_demo/.nekobuddy-workspace-import-receipt-*.json
```

## Boundaries

- `bundle_type` is always `workspace`.
- Sensitive workspace bundles require trusted devices and authenticated encrypted sessions.
- Export strips JSON fields that look like tokens, cookies, private keys, passwords, credentials, or local absolute paths.
- `contains_secrets=true` bundles can be saved and previewed but are rejected by adapter import.
- Import must be preceded by `import-dry-run`.
- Conflict strategies are only `reject`, `rename`, and `skip_conflicts`.
- Import writes an adapter-owned receipt.
- Rollback only deletes files recorded in the receipt and refuses to delete files that were changed after import.

## Local Bridge Flow

The adapter does not call NekoDrop internals. It emits local bridge request envelopes:

```text
authorization.request
-> bundle.send
-> events.poll / actions.results
-> bundle.detail
-> bundle.import
-> actions.results
-> bundle.rollback
```

The required scope set is:

```text
bundle.read
bundle.send
bundle.import.request
transfer.status.read
```

Use `workflow` to print the complete request sequence for one workspace handoff.

Use `discover-bridge` to resolve the localhost endpoint from `--bridge-url`, `--port`, a runtime status file, a bridge config file, or the adapter default. `send-workspace`, `receive-workspace`, and `rollback-workspace` then POST the same request envelopes to the local bridge and return both the raw bridge responses and adapter-side reconciliation for scopes, `request_id`, events, and `actions.results`.
