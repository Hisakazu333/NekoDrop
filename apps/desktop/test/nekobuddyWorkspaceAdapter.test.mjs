import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

const repoRoot = new URL("../../../", import.meta.url);
const adapterCli = fileURLToPath(
  new URL("adapters/nekobuddy-workspace-adapter/nekobuddy-workspace-adapter.mjs", repoRoot)
);

test("NekoBuddy workspace adapter exports a sanitized workspace bundle", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "nekodrop-nekobuddy-workspace-export-"));
  const source = join(tempRoot, "workspace");
  const output = join(tempRoot, "out");
  mkdirSync(source, { recursive: true });
  writeFileSync(
    join(source, "workspace.json"),
    JSON.stringify({
      title: "Companion planning",
      token: "must-not-leak",
      auth: {
        refresh_token: "must-not-leak-either",
        cookie: "session-cookie"
      },
      local_path: "/Users/someone/private/workspace",
      recent_files: ["/Users/someone/private/workspace/notes.md"],
      nested: {
        api_key: "api-secret",
        portable_id: "workspace-1"
      }
    })
  );
  writeFileSync(join(source, "notes.md"), "keep this workspace note\n");

  const result = runAdapter("export", [
    "--source",
    source,
    "--output",
    output,
    "--bundle-id",
    "bundle_workspace_real",
    "--name",
    "Companion planning"
  ]);
  const bundleRoot = join(output, "bundle_workspace_real");
  const manifest = readJson(join(bundleRoot, "bundle.json"));
  const checksums = readJson(join(bundleRoot, "checksums.json"));
  const permissions = readJson(join(bundleRoot, "permissions.json"));
  const exportedWorkspace = readJson(join(bundleRoot, "files", "workspace.json"));

  assert.equal(result.bundle_root, bundleRoot);
  assert.equal(manifest.schema, "nekolink.bundle.v1");
  assert.equal(manifest.bundle_type, "workspace");
  assert.equal(manifest.source_app, "NekoBuddy");
  assert.equal(manifest.summary.file_count, 2);
  assert.equal(permissions.transport.requires_trusted_device, true);
  assert.equal(permissions.transport.requires_authenticated_encrypted_session, true);
  assert.equal(permissions.secrets.contains_secrets, false);
  assert.deepEqual(permissions.requested_scopes, ["workspace.import"]);
  assert.equal(exportedWorkspace.token, undefined);
  assert.equal(exportedWorkspace.auth.refresh_token, undefined);
  assert.equal(exportedWorkspace.auth.cookie, undefined);
  assert.equal(exportedWorkspace.local_path, undefined);
  assert.equal(exportedWorkspace.nested.api_key, undefined);
  assert.equal(exportedWorkspace.nested.portable_id, "workspace-1");
  assert.equal(exportedWorkspace.recent_files[0], "[redacted-local-path]");
  assert(!JSON.stringify(exportedWorkspace).includes("must-not-leak"));
  assert(!JSON.stringify(exportedWorkspace).includes("/Users/someone"));

  for (const file of manifest.files) {
    const bytes = readFileSync(join(bundleRoot, file.path));
    assert.equal(bytes.byteLength, file.size);
    assert.equal(createHash("sha256").update(bytes).digest("hex"), file.sha256);
    assert.equal(checksums.files[file.path], file.sha256);
  }

  rmSync(tempRoot, { recursive: true, force: true });
});

test("NekoBuddy workspace adapter descriptor and app manifest are concrete and path-free", () => {
  const descriptor = runAdapter("descriptor");
  assert.equal(descriptor.schema, "nekolink.adapter.v1");
  assert.equal(descriptor.adapter_id, "nekobuddy.workspace.adapter");
  assert.deepEqual(descriptor.bridge.requested_scopes, [
    "bundle.read",
    "bundle.send",
    "bundle.import.request",
    "transfer.status.read"
  ]);
  assert.deepEqual(descriptor.runtime.actions.map((entry) => entry.action), [
    "export_bundle",
    "import_bundle",
    "rollback_import"
  ]);
  assert.equal(descriptor.bundle_types.length, 1);
  assert.equal(descriptor.bundle_types[0].bundle_type, "workspace");
  assert.equal(descriptor.bundle_types[0].sensitive, true);
  assert.equal(descriptor.bundle_types[0].requires_trusted_device, true);
  assert.equal(descriptor.security.refuses_untrusted_sensitive_send, true);
  assertNoLocalPaths(descriptor);

  const manifest = runAdapter("app-manifest");
  assert.equal(manifest.schema, "nekolink.adapter.app_manifest.v1");
  assert.equal(manifest.app_id, "nekobuddy.app");
  assert.equal(manifest.adapter_id, "nekobuddy.workspace.adapter");
  assert.equal(manifest.resources.length, 1);
  assert.equal(manifest.resources[0].bundle_type, "workspace");
  assert.equal(manifest.resources[0].logical_source, "nekobuddy.workspace.selected");
  assert.equal(manifest.resources[0].logical_target, "nekobuddy.workspace");
  assert.equal(manifest.resources[0].sensitive, true);
  assert.equal(manifest.resources[0].requires_trusted_device, true);
  assert.equal(manifest.safety.require_dry_run_before_import, true);
  assert.equal(manifest.safety.require_receipt_for_import, true);
  assertNoLocalPaths(manifest);
});

test("NekoBuddy workspace adapter dry-run reports all stable import states", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "nekodrop-nekobuddy-workspace-dry-run-"));
  const source = join(tempRoot, "workspace");
  const output = join(tempRoot, "out");
  const targetRoot = join(tempRoot, "target");
  mkdirSync(source, { recursive: true });
  writeFileSync(join(source, "workspace.json"), JSON.stringify({ title: "Dry run" }));
  writeFileSync(join(source, "notes.md"), "dry run\n");

  runAdapter("export", [
    "--source",
    source,
    "--output",
    output,
    "--bundle-id",
    "bundle_workspace_dry_run",
    "--name",
    "Dry run"
  ]);
  const bundleRoot = join(output, "bundle_workspace_dry_run");

  const emptyPlan = runAdapter("import-dry-run", [
    "--bundle-root",
    bundleRoot,
    "--target-root",
    targetRoot
  ]);
  assert.equal(emptyPlan.status, "would_import");
  assert.equal(emptyPlan.plan.schema, "nekobuddy.workspace.adapter.import_plan.v1");
  assert.equal(emptyPlan.plan.state, "would_import");
  assert.deepEqual(emptyPlan.plan.would_import_paths, ["files/notes.md", "files/workspace.json"]);

  runAdapter("import-confirm", [
    "--bundle-root",
    bundleRoot,
    "--target-root",
    targetRoot
  ]);

  const conflictPlan = runAdapter("import-dry-run", [
    "--bundle-root",
    bundleRoot,
    "--target-root",
    targetRoot,
    "--conflict-strategy",
    "reject"
  ]);
  assert.equal(conflictPlan.status, "would_conflict");
  assert.equal(conflictPlan.plan.state, "would_conflict");
  assert.equal(conflictPlan.conflict_count, 2);

  const skipPlan = runAdapter("import-dry-run", [
    "--bundle-root",
    bundleRoot,
    "--target-root",
    targetRoot,
    "--conflict-strategy",
    "skip_conflicts"
  ]);
  assert.equal(skipPlan.status, "would_skip");
  assert.equal(skipPlan.plan.state, "would_skip");
  assert.equal(skipPlan.would_import_file_count, 0);
  assert.equal(skipPlan.would_skip_file_count, 2);

  runAdapter("export", [
    "--source",
    source,
    "--output",
    output,
    "--bundle-id",
    "bundle_workspace_secret",
    "--name",
    "Secret workspace",
    "--contains-secrets",
    "true"
  ]);
  const cannotImport = runAdapter("import-dry-run", [
    "--bundle-root",
    join(output, "bundle_workspace_secret"),
    "--target-root",
    targetRoot
  ]);
  assert.equal(cannotImport.status, "cannot_import");
  assert.equal(cannotImport.plan.state, "cannot_import");
  assert.match(cannotImport.reason, /contains secrets/);
  assert.deepEqual(["would_import", "would_conflict", "would_skip", "cannot_import"], [
    emptyPlan.status,
    conflictPlan.status,
    skipPlan.status,
    cannotImport.status
  ]);

  rmSync(tempRoot, { recursive: true, force: true });
});

test("NekoBuddy workspace adapter imports with receipts and rolls back conservatively", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "nekodrop-nekobuddy-workspace-import-"));
  const source = join(tempRoot, "workspace");
  const output = join(tempRoot, "out");
  const targetRoot = join(tempRoot, "target");
  mkdirSync(source, { recursive: true });
  writeFileSync(join(source, "workspace.json"), JSON.stringify({ title: "Rollback" }));
  writeFileSync(join(source, "notes.md"), "rollback me\n");

  runAdapter("export", [
    "--source",
    source,
    "--output",
    output,
    "--bundle-id",
    "bundle_workspace_rollback",
    "--name",
    "Rollback"
  ]);
  const bundleRoot = join(output, "bundle_workspace_rollback");
  const imported = runAdapter("import-confirm", [
    "--bundle-root",
    bundleRoot,
    "--target-root",
    targetRoot
  ]);
  const targetPath = join(targetRoot, "workspaces", "bundle_workspace_rollback");
  assert.equal(imported.status, "imported");
  assert.equal(imported.target_path, targetPath);
  assert.equal(imported.imported_file_count, 2);
  assert.equal(readJson(imported.receipt_path).schema, "nekobuddy.workspace.adapter.import_receipt.v1");

  writeFileSync(join(targetPath, "notes.md"), "user changed this\n");
  const blocked = runAdapter("rollback", ["--receipt", imported.receipt_path]);
  assert.equal(blocked.status, "blocked");
  assert.equal(blocked.reason, "imported_file_missing_changed_or_not_file");
  assert.equal(readFileSync(join(targetPath, "notes.md"), "utf8"), "user changed this\n");

  const renamed = runAdapter("import-confirm", [
    "--bundle-root",
    bundleRoot,
    "--target-root",
    targetRoot,
    "--conflict-strategy",
    "rename"
  ]);
  assert.equal(renamed.status, "imported");
  assert.equal(renamed.target_path, `${targetPath}-2`);
  const rolledBack = runAdapter("rollback", ["--receipt", renamed.receipt_path]);
  assert.equal(rolledBack.status, "rolled_back");
  assert.equal(rolledBack.removed_file_count, 2);
  assert.equal(readFileSync(join(targetPath, "notes.md"), "utf8"), "user changed this\n");

  rmSync(tempRoot, { recursive: true, force: true });
});

test("NekoBuddy workspace adapter bridge workflow aligns events and results by request id", () => {
  const auth = runAdapter("request", ["auth"]);
  assert.equal(auth.kind, "authorization.request");
  assert.deepEqual(auth.payload.requested_scopes, [
    "bundle.read",
    "bundle.send",
    "bundle.import.request",
    "transfer.status.read"
  ]);

  const send = runAdapter("request", [
    "send",
    "--request-id",
    "workspace-send-1",
    "--bundle-root",
    "/tmp/nekobuddy-workspace-bundle",
    "--target-device-id",
    "paired-device-1"
  ]);
  assert.equal(send.kind, "bundle.send");
  assert.equal(send.payload.bundle_type, "workspace");
  assert.equal(send.payload.require_trusted_device, true);

  const events = runAdapter("request", [
    "events",
    "--action-request-id",
    "workspace-send-1"
  ]);
  const results = runAdapter("request", [
    "results",
    "--action-request-id",
    "workspace-send-1"
  ]);
  assert.equal(events.payload.action_request_id, send.payload.request_id);
  assert.equal(results.payload.action_request_id, send.payload.request_id);

  const workflow = runAdapter("workflow", [
    "--source",
    "/tmp/source-workspace",
    "--output",
    "/tmp/out",
    "--bundle-id",
    "bundle_workspace_flow",
    "--name",
    "Flow",
    "--target-device-id",
    "paired-device-1",
    "--staged-bundle-id",
    "bundle_workspace_flow"
  ]);
  const sendStep = workflow.steps.find((step) => step.id === "send_bundle");
  const sendResultStep = workflow.steps.find((step) => step.id === "send_action_result");
  const importStep = workflow.steps.find((step) => step.id === "request_import");
  const importResultStep = workflow.steps.find((step) => step.id === "import_action_result");
  const rollbackStep = workflow.steps.find((step) => step.id === "request_rollback");
  const rollbackResultStep = workflow.steps.find((step) => step.id === "rollback_action_result");
  assert.equal(sendResultStep.request.payload.action_request_id, sendStep.request.payload.request_id);
  assert.equal(importResultStep.request.payload.action_request_id, importStep.request.payload.request_id);
  assert.equal(rollbackResultStep.request.payload.action_request_id, rollbackStep.request.payload.request_id);
  assert.deepEqual(workflow.steps.map((step) => step.id), [
    "export_workspace",
    "authorize_bridge",
    "send_bundle",
    "observe_send",
    "send_action_result",
    "inspect_received_bundle",
    "import_dry_run",
    "request_import",
    "import_action_result",
    "request_rollback",
    "rollback_action_result"
  ]);
});

function runAdapter(command, args = []) {
  const stdout = execFileSync(process.execPath, [adapterCli, command, ...args], {
    encoding: "utf8"
  });
  return JSON.parse(stdout);
}

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function assertNoLocalPaths(value) {
  const serialized = JSON.stringify(value);
  assert(!serialized.includes("/Users/"));
  assert(!serialized.includes("\\Users\\"));
  assert(!/[A-Za-z]:\\\\/.test(serialized));
}
