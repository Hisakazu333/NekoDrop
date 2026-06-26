import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync, writeFileSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { execFileSync } from "node:child_process";
import { test } from "node:test";

const repoRoot = new URL("../../../", import.meta.url);
const sampleCli = fileURLToPath(new URL("docs/examples/generic-adapter/generic-adapter.mjs", repoRoot));

test("generic adapter sample exports a valid sanitized bundle", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "nekodrop-generic-adapter-"));
  const source = join(tempRoot, "source");
  const output = join(tempRoot, "out");
  mkdirSync(source, { recursive: true });
  writeFileSync(join(source, "session.json"), JSON.stringify({
    title: "handoff",
    summary: "Continue the task on another device",
    token: "must-not-leak"
  }));
  writeFileSync(join(source, "notes.md"), "next step: run tests\n");

  const stdout = execFileSync(
    process.execPath,
    [
      sampleCli,
      "export",
      "--source",
      source,
      "--output",
      output,
      "--bundle-id",
      "bundle_adapter_sample",
      "--type",
      "session",
      "--name",
      "Adapter sample session",
      "--source-app",
      "Generic Adapter",
      "--strip-field",
      "token"
    ],
    { encoding: "utf8" }
  );

  const result = JSON.parse(stdout);
  const bundleRoot = join(output, "bundle_adapter_sample");
  const manifest = readJson(join(bundleRoot, "bundle.json"));
  const checksums = readJson(join(bundleRoot, "checksums.json"));
  const permissions = readJson(join(bundleRoot, "permissions.json"));
  const exportedSession = readJson(join(bundleRoot, "files", "session.json"));

  assert.equal(result.bundle_root, bundleRoot);
  assert.equal(manifest.schema, "nekolink.bundle.v1");
  assert.equal(manifest.bundle_id, "bundle_adapter_sample");
  assert.equal(manifest.bundle_type, "session");
  assert.equal(manifest.display_name, "Adapter sample session");
  assert.equal(manifest.source_app, "Generic Adapter");
  assert.equal(manifest.summary.file_count, 2);
  assert.equal(permissions.secrets.contains_secrets, false);
  assert.deepEqual(permissions.requested_scopes, ["session.import"]);
  assert.equal(exportedSession.token, undefined);

  for (const file of manifest.files) {
    const bytes = readFileSync(join(bundleRoot, file.path));
    assert.equal(bytes.byteLength, file.size);
    assert.equal(createHash("sha256").update(bytes).digest("hex"), file.sha256);
    assert.equal(checksums.files[file.path], file.sha256);
  }

  rmSync(tempRoot, { recursive: true, force: true });
});

test("generic adapter sample imports a checked bundle into an adapter-owned target", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "nekodrop-generic-adapter-import-target-"));
  const source = join(tempRoot, "source");
  const output = join(tempRoot, "out");
  const targetRoot = join(tempRoot, "adapter-data");
  mkdirSync(source, { recursive: true });
  writeFileSync(join(source, "workspace.json"), JSON.stringify({ name: "demo workspace" }));
  writeFileSync(join(source, "notes.md"), "import me\n");

  execFileSync(
    process.execPath,
    [
      sampleCli,
      "export",
      "--source",
      source,
      "--output",
      output,
      "--bundle-id",
      "bundle_workspace_import",
      "--type",
      "workspace",
      "--name",
      "Workspace import"
    ],
    { encoding: "utf8" }
  );
  const bundleRoot = join(output, "bundle_workspace_import");

  const stdout = execFileSync(
    process.execPath,
    [
      sampleCli,
      "import-target",
      "--bundle-root",
      bundleRoot,
      "--target-root",
      targetRoot,
      "--type",
      "workspace"
    ],
    { encoding: "utf8" }
  );
  const imported = JSON.parse(stdout);
  const targetPath = join(targetRoot, "workspace", "bundle_workspace_import");

  assert.equal(imported.status, "imported");
  assert.equal(imported.target_path, targetPath);
  assert.equal(imported.imported_file_count, 2);
  assert.equal(imported.skipped_file_count, 0);
  assert.equal(readFileSync(join(targetPath, "notes.md"), "utf8"), "import me\n");
  const receiptPath = imported.receipt_path;
  assert.equal(readJson(receiptPath).schema, "generic.adapter.import_receipt.v1");

  const conflictStdout = execFileSync(
    process.execPath,
    [
      sampleCli,
      "import-target",
      "--bundle-root",
      bundleRoot,
      "--target-root",
      targetRoot,
      "--type",
      "workspace",
      "--conflict-strategy",
      "reject"
    ],
    { encoding: "utf8" }
  );
  const conflict = JSON.parse(conflictStdout);
  assert.equal(conflict.status, "conflict");
  assert.equal(conflict.imported_file_count, 0);
  assert.equal(conflict.receipt_path, null);

  const renameStdout = execFileSync(
    process.execPath,
    [
      sampleCli,
      "import-target",
      "--bundle-root",
      bundleRoot,
      "--target-root",
      targetRoot,
      "--type",
      "workspace",
      "--conflict-strategy",
      "rename"
    ],
    { encoding: "utf8" }
  );
  const renamed = JSON.parse(renameStdout);
  assert.equal(renamed.status, "imported");
  assert.equal(renamed.target_path, `${targetPath}-2`);

  writeFileSync(join(targetPath, "notes.md"), "keep existing\n");
  const blockedRollbackStdout = execFileSync(
    process.execPath,
    [
      sampleCli,
      "rollback-target",
      "--receipt",
      receiptPath
    ],
    { encoding: "utf8" }
  );
  const blockedRollback = JSON.parse(blockedRollbackStdout);
  assert.equal(blockedRollback.status, "blocked");
  assert.equal(blockedRollback.reason, "imported_file_missing_changed_or_not_file");
  assert.equal(readFileSync(join(targetPath, "notes.md"), "utf8"), "keep existing\n");

  const skipStdout = execFileSync(
    process.execPath,
    [
      sampleCli,
      "import-target",
      "--bundle-root",
      bundleRoot,
      "--target-root",
      targetRoot,
      "--type",
      "workspace",
      "--conflict-strategy",
      "skip_conflicts"
    ],
    { encoding: "utf8" }
  );
  const skipped = JSON.parse(skipStdout);
  assert.equal(skipped.status, "imported");
  assert.equal(skipped.skipped_file_count, 2);
  assert.equal(readFileSync(join(targetPath, "notes.md"), "utf8"), "keep existing\n");

  const skipRollbackStdout = execFileSync(
    process.execPath,
    [
      sampleCli,
      "rollback-target",
      "--receipt",
      skipped.receipt_path
    ],
    { encoding: "utf8" }
  );
  const skipRollback = JSON.parse(skipRollbackStdout);
  assert.equal(skipRollback.status, "rolled_back");
  assert.equal(skipRollback.removed_file_count, 0);
  assert.equal(readFileSync(join(targetPath, "notes.md"), "utf8"), "keep existing\n");

  const rollbackStdout = execFileSync(
    process.execPath,
    [
      sampleCli,
      "rollback-target",
      "--receipt",
      renamed.receipt_path
    ],
    { encoding: "utf8" }
  );
  const rolledBack = JSON.parse(rollbackStdout);
  assert.equal(rolledBack.status, "rolled_back");
  assert.equal(rolledBack.removed_file_count, 2);
  assert.equal(readFileSync(join(targetPath, "notes.md"), "utf8"), "keep existing\n");
  assert.equal(readJson(join(renamed.target_path, ".generic-adapter-rollback-receipt.json")).bundle_id, "bundle_workspace_import");

  rmSync(tempRoot, { recursive: true, force: true });
});

test("generic adapter sample rejects rollback receipts outside their target", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "nekodrop-generic-adapter-rollback-target-"));
  const target = join(tempRoot, "adapter-data", "workspace", "bundle_workspace_import");
  mkdirSync(target, { recursive: true });
  writeFileSync(join(target, "notes.md"), "keep\n");
  const outsideReceipt = join(tempRoot, "receipt.json");
  writeFileSync(outsideReceipt, JSON.stringify({
    schema: "generic.adapter.import_receipt.v1",
    bundle_id: "bundle_workspace_import",
    bundle_type: "workspace",
    display_name: "Workspace import",
    source_app: "Generic Adapter",
    target_path: target,
    conflict_strategy: "reject",
    imported_manifest_paths: ["files/notes.md"],
    imported_files: [{
      manifest_path: "files/notes.md",
      size: 5,
      sha256: createHash("sha256").update("keep\n").digest("hex")
    }],
    skipped_manifest_paths: [],
    imported_at: "2026-06-25T00:00:00.000Z"
  }));

  assert.throws(
    () => execFileSync(
      process.execPath,
      [
        sampleCli,
        "rollback-target",
        "--receipt",
        outsideReceipt
      ],
      { encoding: "utf8", stdio: "pipe" }
    ),
    /adapter import receipt must live inside its target_path/
  );
  assert.equal(readFileSync(join(target, "notes.md"), "utf8"), "keep\n");

  rmSync(tempRoot, { recursive: true, force: true });
});

test("generic adapter sample refuses to import bundles marked as containing secrets", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "nekodrop-generic-adapter-import-secret-"));
  const source = join(tempRoot, "source");
  const output = join(tempRoot, "out");
  mkdirSync(source, { recursive: true });
  writeFileSync(join(source, "session.json"), JSON.stringify({ token: "secret" }));
  execFileSync(
    process.execPath,
    [
      sampleCli,
      "export",
      "--source",
      source,
      "--output",
      output,
      "--bundle-id",
      "bundle_secret_session",
      "--type",
      "session",
      "--name",
      "Secret session",
      "--contains-secrets",
      "true"
    ],
    { encoding: "utf8" }
  );

  assert.throws(
    () => execFileSync(
      process.execPath,
      [
        sampleCli,
        "import-target",
        "--bundle-root",
        join(output, "bundle_secret_session"),
        "--target-root",
        join(tempRoot, "adapter-data"),
        "--type",
        "session"
      ],
      { encoding: "utf8", stdio: "pipe" }
    ),
    /bundle contains secrets and must not be imported automatically/
  );

  rmSync(tempRoot, { recursive: true, force: true });
});

test("generic adapter sample prints local bridge request envelopes", () => {
  const stdout = execFileSync(
    process.execPath,
    [
      sampleCli,
      "request",
      "send",
      "--bundle-root",
      "/tmp/bundle_adapter_sample",
      "--target-device-id",
      "neko-device-target",
      "--type",
      "workspace"
    ],
    { encoding: "utf8" }
  );

  const request = JSON.parse(stdout);

  assert.equal(request.kind, "bundle.send");
  assert.equal(request.payload.client.client_id, "generic.adapter.sample");
  assert.equal(request.payload.bundle_root, "/tmp/bundle_adapter_sample");
  assert.equal(request.payload.target_device_id, "neko-device-target");
  assert.equal(request.payload.bundle_type, "workspace");
  assert.equal(request.payload.require_trusted_device, true);
});

test("generic adapter sample defaults auth to the smallest read scope", () => {
  const stdout = execFileSync(
    process.execPath,
    [
      sampleCli,
      "request",
      "auth"
    ],
    { encoding: "utf8" }
  );

  const request = JSON.parse(stdout);

  assert.equal(request.kind, "authorization.request");
  assert.deepEqual(request.payload.requested_scopes, ["bundle.read"]);
});

test("generic adapter sample prints and validates an adapter descriptor", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "nekodrop-generic-adapter-descriptor-"));
  const descriptorPath = join(tempRoot, "adapter.json");
  const stdout = execFileSync(
    process.execPath,
    [
      sampleCli,
      "descriptor",
      "--type",
      "session",
      "--type",
      "workspace"
    ],
    { encoding: "utf8" }
  );
  writeFileSync(descriptorPath, stdout);

  const descriptor = JSON.parse(stdout);
  assert.equal(descriptor.schema, "nekolink.adapter.v1");
  assert.equal(descriptor.client.client_id, "generic.adapter.sample");
  assert.deepEqual(
    descriptor.bridge.requested_scopes,
    ["bundle.read", "bundle.send", "bundle.import.request", "transfer.status.read"]
  );
  assert.deepEqual(
    descriptor.bundle_types.map((entry) => entry.bundle_type),
    ["session", "workspace"]
  );
  assert.equal(descriptor.bundle_types[0].sensitive, true);
  assert.equal(descriptor.bundle_types[0].requires_trusted_device, true);

  const validation = JSON.parse(execFileSync(
    process.execPath,
    [sampleCli, "validate-descriptor", "--descriptor", descriptorPath],
    { encoding: "utf8" }
  ));
  assert.equal(validation.schema, "nekolink.adapter.v1");
  assert.equal(validation.bundle_type_count, 2);
  assert.deepEqual(validation.sensitive_bundle_types, ["session", "workspace"]);

  rmSync(tempRoot, { recursive: true, force: true });
});

test("generic adapter sample can derive auth scopes from an adapter descriptor", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "nekodrop-generic-adapter-descriptor-auth-"));
  const descriptorPath = join(tempRoot, "adapter.json");
  const descriptor = JSON.parse(execFileSync(
    process.execPath,
    [
      sampleCli,
      "descriptor",
      "--type",
      "workspace",
      "--scope",
      "bundle.read",
      "--scope",
      "bundle.import.request",
      "--ttl-seconds",
      "7200"
    ],
    { encoding: "utf8" }
  ));
  writeFileSync(descriptorPath, JSON.stringify(descriptor));

  const auth = JSON.parse(execFileSync(
    process.execPath,
    [sampleCli, "request", "auth", "--descriptor", descriptorPath],
    { encoding: "utf8" }
  ));
  assert.deepEqual(auth.payload.requested_scopes, ["bundle.read", "bundle.import.request"]);
  assert.equal(auth.payload.ttl_seconds, 7200);

  const workflow = JSON.parse(execFileSync(
    process.execPath,
    [
      sampleCli,
      "workflow",
      "--mode",
      "import",
      "--descriptor",
      descriptorPath,
      "--staged-bundle-id",
      "bundle_received_1",
      "--type",
      "workspace"
    ],
    { encoding: "utf8" }
  ));
  assert.deepEqual(workflow.steps[0].request.payload.requested_scopes, [
    "bundle.read",
    "bundle.import.request"
  ]);

  rmSync(tempRoot, { recursive: true, force: true });
});

test("generic adapter sample gates send and import types by descriptor", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "nekodrop-generic-adapter-descriptor-gates-"));
  const descriptorPath = join(tempRoot, "adapter.json");
  const descriptor = JSON.parse(execFileSync(
    process.execPath,
    [sampleCli, "descriptor", "--type", "workspace"],
    { encoding: "utf8" }
  ));
  writeFileSync(descriptorPath, JSON.stringify(descriptor));

  const send = JSON.parse(execFileSync(
    process.execPath,
    [
      sampleCli,
      "request",
      "send",
      "--descriptor",
      descriptorPath,
      "--bundle-root",
      "/tmp/bundle_workspace",
      "--target-device-id",
      "neko-device-target",
      "--type",
      "workspace"
    ],
    { encoding: "utf8" }
  ));
  assert.equal(send.payload.bundle_type, "workspace");

  assert.throws(
    () => execFileSync(
      process.execPath,
      [
        sampleCli,
        "request",
        "send",
        "--descriptor",
        descriptorPath,
        "--bundle-root",
        "/tmp/bundle_session",
        "--target-device-id",
        "neko-device-target",
        "--type",
        "session"
      ],
      { encoding: "utf8", stdio: "pipe" }
    ),
    /descriptor does not declare bundle type: session/
  );

  assert.throws(
    () => execFileSync(
      process.execPath,
      [
        sampleCli,
        "request",
        "import",
        "--descriptor",
        descriptorPath,
        "--staged-bundle-id",
        "bundle_session",
        "--type",
        "session"
      ],
      { encoding: "utf8", stdio: "pipe" }
    ),
    /descriptor does not declare bundle type: session/
  );

  rmSync(tempRoot, { recursive: true, force: true });
});

test("generic adapter sample gates send and import capabilities by descriptor", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "nekodrop-generic-adapter-descriptor-capabilities-"));
  const importOnlyDescriptorPath = join(tempRoot, "import-only.json");
  const exportOnlyDescriptorPath = join(tempRoot, "export-only.json");
  const importOnlyDescriptor = JSON.parse(execFileSync(
    process.execPath,
    [sampleCli, "descriptor", "--type", "workspace", "--capability", "import"],
    { encoding: "utf8" }
  ));
  const exportOnlyDescriptor = JSON.parse(execFileSync(
    process.execPath,
    [sampleCli, "descriptor", "--type", "workspace", "--capability", "export"],
    { encoding: "utf8" }
  ));
  writeFileSync(importOnlyDescriptorPath, JSON.stringify(importOnlyDescriptor));
  writeFileSync(exportOnlyDescriptorPath, JSON.stringify(exportOnlyDescriptor));

  assert.equal(importOnlyDescriptor.bundle_types[0].can_export, false);
  assert.equal(importOnlyDescriptor.bundle_types[0].can_import, true);
  assert.equal(exportOnlyDescriptor.bundle_types[0].can_export, true);
  assert.equal(exportOnlyDescriptor.bundle_types[0].can_import, false);

  const allowedImport = JSON.parse(execFileSync(
    process.execPath,
    [
      sampleCli,
      "request",
      "import",
      "--descriptor",
      importOnlyDescriptorPath,
      "--staged-bundle-id",
      "bundle_workspace",
      "--type",
      "workspace"
    ],
    { encoding: "utf8" }
  ));
  assert.equal(allowedImport.payload.expected_bundle_type, "workspace");

  const allowedSend = JSON.parse(execFileSync(
    process.execPath,
    [
      sampleCli,
      "request",
      "send",
      "--descriptor",
      exportOnlyDescriptorPath,
      "--bundle-root",
      "/tmp/bundle_workspace",
      "--target-device-id",
      "neko-device-target",
      "--type",
      "workspace"
    ],
    { encoding: "utf8" }
  ));
  assert.equal(allowedSend.payload.bundle_type, "workspace");

  assert.throws(
    () => execFileSync(
      process.execPath,
      [
        sampleCli,
        "request",
        "send",
        "--descriptor",
        importOnlyDescriptorPath,
        "--bundle-root",
        "/tmp/bundle_workspace",
        "--target-device-id",
        "neko-device-target",
        "--type",
        "workspace"
      ],
      { encoding: "utf8", stdio: "pipe" }
    ),
    /descriptor does not allow exporting workspace/
  );

  assert.throws(
    () => execFileSync(
      process.execPath,
      [
        sampleCli,
        "request",
        "import",
        "--descriptor",
        exportOnlyDescriptorPath,
        "--staged-bundle-id",
        "bundle_workspace",
        "--type",
        "workspace"
      ],
      { encoding: "utf8", stdio: "pipe" }
    ),
    /descriptor does not allow importing workspace/
  );

  rmSync(tempRoot, { recursive: true, force: true });
});

test("generic adapter sample gates import conflict strategies by descriptor", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "nekodrop-generic-adapter-descriptor-conflict-"));
  const descriptorPath = join(tempRoot, "adapter.json");
  const descriptor = JSON.parse(execFileSync(
    process.execPath,
    [
      sampleCli,
      "descriptor",
      "--type",
      "workspace",
      "--conflict-strategy",
      "reject"
    ],
    { encoding: "utf8" }
  ));
  writeFileSync(descriptorPath, JSON.stringify(descriptor));

  assert.deepEqual(descriptor.bundle_types[0].conflict_strategies, ["reject"]);

  const allowedImport = JSON.parse(execFileSync(
    process.execPath,
    [
      sampleCli,
      "request",
      "import",
      "--descriptor",
      descriptorPath,
      "--staged-bundle-id",
      "bundle_workspace",
      "--type",
      "workspace"
    ],
    { encoding: "utf8" }
  ));
  assert.equal(allowedImport.payload.conflict_strategy, "reject");

  assert.throws(
    () => execFileSync(
      process.execPath,
      [
        sampleCli,
        "request",
        "import",
        "--descriptor",
        descriptorPath,
        "--staged-bundle-id",
        "bundle_workspace",
        "--type",
        "workspace",
        "--conflict-strategy",
        "rename"
      ],
      { encoding: "utf8", stdio: "pipe" }
    ),
    /descriptor does not allow rename imports for workspace/
  );

  rmSync(tempRoot, { recursive: true, force: true });
});

test("generic adapter sample rejects unsafe adapter descriptors", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "nekodrop-generic-adapter-descriptor-invalid-"));
  const descriptorPath = join(tempRoot, "adapter.json");
  const descriptor = JSON.parse(execFileSync(
    process.execPath,
    [sampleCli, "descriptor", "--type", "session"],
    { encoding: "utf8" }
  ));
  descriptor.bundle_types[0].requires_trusted_device = false;
  writeFileSync(descriptorPath, JSON.stringify(descriptor));

  assert.throws(
    () => execFileSync(
      process.execPath,
      [sampleCli, "validate-descriptor", "--descriptor", descriptorPath],
      { encoding: "utf8", stdio: "pipe" }
    ),
    /session descriptor must mark sensitive and require trusted device/
  );

  descriptor.bundle_types[0].requires_trusted_device = true;
  descriptor.security.refuses_untrusted_sensitive_send = false;
  writeFileSync(descriptorPath, JSON.stringify(descriptor));
  assert.throws(
    () => execFileSync(
      process.execPath,
      [sampleCli, "validate-descriptor", "--descriptor", descriptorPath],
      { encoding: "utf8", stdio: "pipe" }
    ),
    /adapter descriptor must refuse untrusted sensitive sends/
  );

  rmSync(tempRoot, { recursive: true, force: true });
});

test("generic adapter sample rejects untrusted sends for sensitive bundle types", () => {
  for (const type of ["skill", "session", "workspace", "agent_profile"]) {
    assert.throws(
      () => execFileSync(
        process.execPath,
        [
          sampleCli,
          "request",
          "send",
          "--bundle-root",
          "/tmp/bundle_adapter_sample",
          "--target-device-id",
          "neko-device-target",
          "--type",
          type,
          "--require-trusted-device",
          "false"
        ],
        { encoding: "utf8", stdio: "pipe" }
      ),
      new RegExp(`${type} bundles require --require-trusted-device true`)
    );
  }

  const stdout = execFileSync(
    process.execPath,
    [
      sampleCli,
      "request",
      "send",
      "--bundle-root",
      "/tmp/bundle_adapter_sample",
      "--target-device-id",
      "neko-device-target",
      "--type",
      "config_snapshot",
      "--require-trusted-device",
      "false"
    ],
    { encoding: "utf8" }
  );
  const request = JSON.parse(stdout);
  assert.equal(request.payload.bundle_type, "config_snapshot");
  assert.equal(request.payload.require_trusted_device, false);
});

test("generic adapter sample prints import and detail request envelopes", () => {
  const detailStdout = execFileSync(
    process.execPath,
    [
      sampleCli,
      "request",
      "detail",
      "--staged-bundle-id",
      "bundle_received_1"
    ],
    { encoding: "utf8" }
  );
  const importStdout = execFileSync(
    process.execPath,
    [
      sampleCli,
      "request",
      "import",
      "--staged-bundle-id",
      "bundle_received_1",
      "--type",
      "session",
      "--conflict-strategy",
      "rename"
    ],
    { encoding: "utf8" }
  );

  const detailRequest = JSON.parse(detailStdout);
  const importRequest = JSON.parse(importStdout);

  assert.equal(detailRequest.kind, "bundle.detail");
  assert.equal(detailRequest.payload.client.client_id, "generic.adapter.sample");
  assert.equal(detailRequest.payload.staged_bundle_id, "bundle_received_1");
  assert.equal(importRequest.kind, "bundle.import");
  assert.equal(importRequest.payload.staged_bundle_id, "bundle_received_1");
  assert.equal(importRequest.payload.expected_bundle_type, "session");
  assert.equal(importRequest.payload.conflict_strategy, "rename");
});

test("generic adapter sample prints action result query envelopes", () => {
  const stdout = execFileSync(
    process.execPath,
    [
      sampleCli,
      "request",
      "results",
      "--action-request-id",
      "adapter-import-001",
      "--after-claimed-at-ms",
      "1234",
      "--limit",
      "5"
    ],
    { encoding: "utf8" }
  );

  const request = JSON.parse(stdout);

  assert.equal(request.kind, "actions.results");
  assert.equal(request.payload.client.client_id, "generic.adapter.sample");
  assert.equal(request.payload.action_request_id, "adapter-import-001");
  assert.equal(request.payload.after_claimed_at_ms, 1234);
  assert.equal(request.payload.limit, 5);
});

test("generic adapter sample classifies mutation retry responses", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "nekodrop-generic-adapter-retry-state-"));
  const pendingPath = join(tempRoot, "pending.json");
  const conflictPath = join(tempRoot, "conflict.json");
  const terminalPath = join(tempRoot, "terminal.json");
  writeFileSync(pendingPath, JSON.stringify({
    status: "pending_runtime",
    message: "local bridge action is already queued for the desktop runtime",
    action_results: [{
      request_id: "adapter-import-001",
      action_kind: "bundle.import",
      lifecycle_status: "queued",
      status: "queued",
      message: "local bridge action is queued for the desktop runtime",
      bundle_id: "bundle_received_1",
      bundle_type: "workspace",
      conflict_strategy: "rename"
    }]
  }));
  writeFileSync(conflictPath, JSON.stringify({
    status: "conflict",
    message: "local bridge request_id already belongs to a different payload",
    action_results: [{
      request_id: "adapter-import-001",
      action_kind: "bundle.import",
      lifecycle_status: "queued",
      status: "queued",
      message: "local bridge action is queued for the desktop runtime",
      bundle_id: "bundle_received_1",
      bundle_type: "workspace",
      conflict_strategy: "rename"
    }]
  }));
  writeFileSync(terminalPath, JSON.stringify({
    status: "ok",
    message: "local bridge action result snapshot",
    action_results: [{
      request_id: "adapter-import-001",
      action_kind: "bundle.import",
      lifecycle_status: "succeeded",
      status: "completed",
      message: "local bridge bundle was imported by the desktop runtime",
      bundle_id: "bundle_received_1",
      bundle_type: "workspace",
      conflict_strategy: "rename",
      has_import_receipt: true,
      can_request_rollback: true,
      rollback_file_count: 2
    }]
  }));

  const pending = JSON.parse(execFileSync(
    process.execPath,
    [sampleCli, "retry-state", "--response", pendingPath, "--action-request-id", "adapter-import-001"],
    { encoding: "utf8" }
  ));
  const conflict = JSON.parse(execFileSync(
    process.execPath,
    [sampleCli, "retry-state", "--response", conflictPath, "--action-request-id", "adapter-import-001"],
    { encoding: "utf8" }
  ));
  const terminal = JSON.parse(execFileSync(
    process.execPath,
    [sampleCli, "retry-state", "--response", terminalPath, "--action-request-id", "adapter-import-001"],
    { encoding: "utf8" }
  ));

  assert.equal(pending.state, "pending_retry");
  assert.equal(pending.final, false);
  assert.equal(pending.next_action, "observe_events_or_query_actions_results");
  assert.equal(pending.existing_action.bundle_id, "bundle_received_1");
  assert.equal(conflict.state, "payload_conflict");
  assert.equal(conflict.final, true);
  assert.equal(conflict.next_action, "reuse_original_request_id_payload_or_create_new_user_action");
  assert.equal(terminal.state, "terminal_result");
  assert.equal(terminal.final, true);
  assert.equal(terminal.next_action, "query_receipt_or_request_rollback");

  rmSync(tempRoot, { recursive: true, force: true });
});

test("generic adapter sample prints rollback request envelopes", () => {
  const stdout = execFileSync(
    process.execPath,
    [
      sampleCli,
      "request",
      "rollback",
      "--bundle-id",
      "bundle_received_1"
    ],
    { encoding: "utf8" }
  );

  const request = JSON.parse(stdout);

  assert.equal(request.kind, "bundle.rollback");
  assert.equal(request.payload.client.client_id, "generic.adapter.sample");
  assert.equal(request.payload.bundle_id, "bundle_received_1");
});

test("generic adapter sample prints a generic roundtrip workflow", () => {
  const stdout = execFileSync(
    process.execPath,
    [
      sampleCli,
      "workflow",
      "--mode",
      "roundtrip",
      "--bundle-root",
      "/tmp/bundle_adapter_sample",
      "--target-device-id",
      "neko-device-target",
      "--staged-bundle-id",
      "bundle_received_1",
      "--type",
      "workspace",
      "--conflict-strategy",
      "skip_conflicts"
    ],
    { encoding: "utf8" }
  );

  const workflow = JSON.parse(stdout);

  assert.equal(workflow.client.client_id, "generic.adapter.sample");
  assert.deepEqual(workflow.steps.map((step) => step.step), [
    "authorize",
    "send",
    "observe",
    "inspect",
    "import",
    "inspect_after_import",
    "results"
  ]);
  assert.deepEqual(workflow.steps[0].request.payload.requested_scopes, [
    "bundle.read",
    "bundle.send",
    "bundle.import.request",
    "transfer.status.read"
  ]);
  assert.equal(workflow.steps[1].request.kind, "bundle.send");
  assert.equal(workflow.steps[3].request.kind, "bundle.detail");
  assert.equal(workflow.steps[4].request.kind, "bundle.import");
  assert.equal(workflow.steps[4].request.payload.conflict_strategy, "skip_conflicts");
  assert.equal(workflow.steps[5].request.kind, "bundle.detail");
  assert.equal(workflow.steps[6].request.kind, "actions.results");
  assert.equal(workflow.steps[6].request.payload.action_request_id, "adapter-import-001");
});

test("generic adapter sample prints a full export send import rollback loop", () => {
  const stdout = execFileSync(
    process.execPath,
    [
      sampleCli,
      "workflow",
      "--mode",
      "full-loop",
      "--source",
      "/tmp/adapter-source",
      "--output",
      "/tmp/adapter-out",
      "--bundle-id",
      "bundle_full_loop",
      "--name",
      "Full loop workspace",
      "--target-device-id",
      "neko-device-target",
      "--staged-bundle-id",
      "bundle_full_loop",
      "--type",
      "workspace",
      "--conflict-strategy",
      "rename",
      "--strip-field",
      "auth.token"
    ],
    { encoding: "utf8" }
  );

  const workflow = JSON.parse(stdout);

  assert.equal(workflow.mode, "full-loop");
  assert.deepEqual(workflow.steps.map((step) => step.step), [
    "export",
    "authorize",
    "send",
    "observe_send",
    "send_action_state",
    "inspect_received_bundle",
    "import",
    "observe_import",
    "import_action_state",
    "query_import_receipt",
    "receipt_state",
    "rollback",
    "observe_rollback",
    "rollback_action_state",
    "query_after_rollback",
    "rollback_receipt_state"
  ]);
  assert.equal(workflow.steps[0].command.at(-2), "--strip-field");
  assert.equal(workflow.steps[0].command.at(-1), "auth.token");
  assert.equal(workflow.steps[2].request.kind, "bundle.send");
  assert.equal(workflow.steps[2].request.payload.request_id, "adapter-send-001");
  assert.equal(workflow.steps[2].request.payload.require_trusted_device, true);
  assert.equal(workflow.steps[4].request.payload.action_request_id, "adapter-send-001");
  assert.equal(workflow.steps[6].request.payload.request_id, "adapter-import-001");
  assert.equal(workflow.steps[6].request.payload.conflict_strategy, "rename");
  assert.equal(workflow.steps[8].request.payload.action_request_id, "adapter-import-001");
  assert.equal(workflow.steps[9].request.kind, "bundle.detail");
  assert.equal(workflow.steps[10].command.at(2), "receipt-state");
  assert.equal(workflow.steps[11].request.kind, "bundle.rollback");
  assert.equal(workflow.steps[11].request.payload.request_id, "adapter-rollback-001");
  assert.equal(workflow.steps[13].request.payload.action_request_id, "adapter-rollback-001");
  assert.equal(workflow.steps[14].request.kind, "bundle.detail");
  assert.equal(workflow.steps[15].command.at(2), "receipt-state");
  assert.match(workflow.notes.join("\n"), /Rollback only removes files imported into NekoDrop/);
  assert.match(workflow.notes.join("\n"), /Sensitive bundle types require trusted authenticated targets/);
  assert.match(workflow.notes.join("\n"), /retry the same action kind with the same request_id/);
});

test("generic adapter sample keeps action request ids stable for retry and result lookup", () => {
  const stdout = execFileSync(
    process.execPath,
    [
      sampleCli,
      "workflow",
      "--mode",
      "full-loop",
      "--source",
      "/tmp/source",
      "--output",
      "/tmp/out",
      "--bundle-id",
      "bundle_workspace_demo",
      "--name",
      "Workspace demo",
      "--target-device-id",
      "device-a",
      "--staged-bundle-id",
      "bundle_workspace_demo",
      "--type",
      "workspace",
      "--send-request-id",
      "stable-send-request",
      "--import-request-id",
      "stable-import-request",
      "--rollback-request-id",
      "stable-rollback-request"
    ],
    { encoding: "utf8" }
  );

  const workflow = JSON.parse(stdout);
  const byStep = Object.fromEntries(workflow.steps.map((step) => [step.step, step]));

  assert.equal(byStep.send.request.payload.request_id, "stable-send-request");
  assert.equal(byStep.send_action_state.request.payload.action_request_id, "stable-send-request");
  assert.equal(byStep.import.request.payload.request_id, "stable-import-request");
  assert.equal(byStep.import_action_state.request.payload.action_request_id, "stable-import-request");
  assert.equal(byStep.rollback.request.payload.request_id, "stable-rollback-request");
  assert.equal(byStep.rollback_action_state.request.payload.action_request_id, "stable-rollback-request");
});

test("generic adapter sample derives the next event cursor", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "nekodrop-generic-adapter-cursor-"));
  const responsePath = join(tempRoot, "events-response.json");
  writeFileSync(responsePath, JSON.stringify({
    events_next_after_id: "bridge-event-42",
    events_cursor_state: "ok",
    events_visible_first_id: "bridge-event-1",
    events_visible_last_id: "bridge-event-42",
    events_visible_count: 42
  }));

  const stdout = execFileSync(
    process.execPath,
    [sampleCli, "cursor", "--response", responsePath],
    { encoding: "utf8" }
  );

  const cursor = JSON.parse(stdout);
  assert.equal(cursor.after_event_id, "bridge-event-42");
  assert.equal(cursor.cursor_state, "ok");
  assert.equal(cursor.reset_required, false);
  assert.equal(cursor.visible_first_event_id, "bridge-event-1");
  assert.equal(cursor.visible_last_event_id, "bridge-event-42");
  assert.equal(cursor.visible_event_count, 42);

  rmSync(tempRoot, { recursive: true, force: true });
});

test("generic adapter sample summarizes event poll responses for a watch loop", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "nekodrop-generic-adapter-event-state-"));
  const responsePath = join(tempRoot, "events-response.json");
  writeFileSync(responsePath, JSON.stringify({
    events_next_after_id: "bridge-event-45",
    events_cursor_state: "ok",
    events_has_more: true,
    events_visible_first_id: "bridge-event-40",
    events_visible_last_id: "bridge-event-45",
    events_visible_count: 6,
    events: [
      {
        kind: "action.updated",
        payload: {
          request_id: "adapter-import-001",
          action_kind: "bundle.import",
          status: "running",
          reason: null,
          bundle_id: "bundle_workspace_demo",
          bundle_type: "workspace"
        }
      },
      {
        kind: "action.updated",
        payload: {
          request_id: "adapter-import-001",
          action_kind: "bundle.import",
          status: "conflict",
          reason: "bundle_import_conflict",
          bundle_id: "bundle_workspace_demo",
          bundle_type: "workspace"
        }
      },
      {
        kind: "bundle.received",
        payload: {
          bundle_id: "bundle_workspace_demo"
        }
      },
      {
        kind: "transfer.updated",
        payload: {
          transfer_id: "transfer-1",
          phase: "completed"
        }
      }
    ]
  }));

  const stdout = execFileSync(
    process.execPath,
    [
      sampleCli,
      "event-state",
      "--response",
      responsePath,
      "--action-request-id",
      "adapter-import-001"
    ],
    { encoding: "utf8" }
  );
  const state = JSON.parse(stdout);

  assert.equal(state.cursor.after_event_id, "bridge-event-45");
  assert.equal(state.stream_window.visible_first_event_id, "bridge-event-40");
  assert.equal(state.stream_window.visible_last_event_id, "bridge-event-45");
  assert.equal(state.stream_window.visible_event_count, 6);
  assert.equal(state.has_more, true);
  assert.equal(state.should_poll_again, true);
  assert.equal(state.should_query_result, true);
  assert.equal(state.action_events.length, 2);
  assert.equal(state.action_state.lifecycle_status, "conflict");
  assert.equal(state.action_state.final, true);
  assert.equal(state.action_state.next_action, "query_result_and_choose_import_conflict_strategy");
  assert.deepEqual(state.received_bundle_ids, ["bundle_workspace_demo"]);
  assert.equal(state.transfer_event_count, 1);

  rmSync(tempRoot, { recursive: true, force: true });
});

test("generic adapter sample can build action-scoped event poll requests", () => {
  const stdout = execFileSync(
    process.execPath,
    [
      sampleCli,
      "request",
      "events",
      "--request-id",
      "adapter-events-1",
      "--after-event-id",
      "bridge-event-1",
      "--action-request-id",
      "adapter-send-001",
      "--timeout-ms",
      "250"
    ],
    { encoding: "utf8" }
  );

  const request = JSON.parse(stdout);
  assert.equal(request.kind, "events.poll");
  assert.equal(request.payload.after_event_id, "bridge-event-1");
  assert.equal(request.payload.action_request_id, "adapter-send-001");
  assert.equal(request.payload.timeout_ms, 250);
});

test("generic adapter sample derives action state from precise result lookups", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "nekodrop-generic-adapter-action-state-"));
  const responsePath = join(tempRoot, "results-response.json");
  writeFileSync(responsePath, JSON.stringify({
    action_results: [
      {
        request_id: "adapter-send-001",
        action_kind: "bundle.send",
        status: "queued",
        lifecycle_status: "queued",
        message: "local bridge action is queued for the desktop runtime",
        bundle_type: "skill",
        target_device_id: "device-a",
        require_trusted_device: true
      },
      {
        request_id: "adapter-import-001",
        action_kind: "bundle.import",
        status: "running",
        lifecycle_status: "running",
        message: "local bridge bundle import is running",
        bundle_id: "bundle_received_import",
        bundle_type: "workspace",
        conflict_strategy: "rename"
      },
      {
        request_id: "adapter-import-conflict-001",
        action_kind: "bundle.import",
        status: "failed",
        lifecycle_status: "conflict",
        reason: "bundle_import_conflict",
        message: "conflict detected"
      },
      {
        request_id: "adapter-rollback-001",
        action_kind: "bundle.rollback",
        status: "completed",
        lifecycle_status: "succeeded",
        reason: null,
        message: "rollback completed",
        bundle_id: "bundle_received_1",
        can_request_rollback: false
      },
      {
        request_id: "adapter-rollback-failed-001",
        action_kind: "bundle.rollback",
        status: "failed",
        lifecycle_status: "failed",
        reason: "bundle_rollback_blocked",
        message: "rollback blocked",
        rollback_blocking_reason: "destination_missing"
      }
    ]
  }));

  const pending = JSON.parse(execFileSync(
    process.execPath,
    [sampleCli, "action-state", "--response", responsePath, "--action-request-id", "adapter-send-001"],
    { encoding: "utf8" }
  ));
  const running = JSON.parse(execFileSync(
    process.execPath,
    [sampleCli, "action-state", "--response", responsePath, "--action-request-id", "adapter-import-001"],
    { encoding: "utf8" }
  ));
  const result = JSON.parse(execFileSync(
    process.execPath,
    [sampleCli, "action-state", "--response", responsePath, "--action-request-id", "adapter-rollback-001"],
    { encoding: "utf8" }
  ));
  const conflict = JSON.parse(execFileSync(
    process.execPath,
    [sampleCli, "action-state", "--response", responsePath, "--action-request-id", "adapter-import-conflict-001"],
    { encoding: "utf8" }
  ));
  const rollbackFailed = JSON.parse(execFileSync(
    process.execPath,
    [sampleCli, "action-state", "--response", responsePath, "--action-request-id", "adapter-rollback-failed-001"],
    { encoding: "utf8" }
  ));
  const missing = JSON.parse(execFileSync(
    process.execPath,
    [sampleCli, "action-state", "--response", responsePath, "--action-request-id", "adapter-missing-001"],
    { encoding: "utf8" }
  ));

  assert.equal(pending.state, "pending");
  assert.equal(pending.final, false);
  assert.equal(pending.bundle_type, "skill");
  assert.equal(pending.target_device_id, "device-a");
  assert.equal(pending.require_trusted_device, true);
  assert.equal(running.state, "running");
  assert.equal(running.final, false);
  assert.equal(running.bundle_id, "bundle_received_import");
  assert.equal(running.bundle_type, "workspace");
  assert.equal(running.conflict_strategy, "rename");
  assert.equal(result.state, "result");
  assert.equal(result.final, true);
  assert.equal(result.lifecycle_status, "succeeded");
  assert.equal(result.bundle_id, "bundle_received_1");
  assert.equal(result.next_action, "query_rollback_status");
  assert.equal(conflict.state, "result");
  assert.equal(conflict.next_action, "choose_import_conflict_strategy");
  assert.equal(rollbackFailed.next_action, "show_rollback_blocking_reason");
  assert.equal(missing.state, "missing");
  assert.equal(missing.final, false);

  rmSync(tempRoot, { recursive: true, force: true });
});

test("generic adapter sample derives receipt state from bundle detail responses", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "nekodrop-generic-adapter-receipt-state-"));
  const responsePath = join(tempRoot, "detail-response.json");
  writeFileSync(responsePath, JSON.stringify({
    staged_bundles: [
      {
        bundle_id: "bundle_imported_1",
        bundle_type: "workspace",
        display_name: "Imported workspace",
        staging_status: "imported",
        import_allowed: true,
        has_import_receipt: true,
        imported_with_strategy: "rename",
        import_skipped_file_count: 1,
        rollback_file_count: 3,
        can_request_rollback: true,
        can_rollback_now: true,
        rollback_blocking_reason: null,
        rolled_back_file_count: 0
      },
      {
        bundle_id: "bundle_rolled_back_1",
        bundle_type: "workspace",
        display_name: "Rolled back workspace",
        staging_status: "rolled_back",
        import_allowed: true,
        has_import_receipt: true,
        imported_with_strategy: "rename",
        import_skipped_file_count: 0,
        rollback_file_count: 3,
        can_request_rollback: false,
        can_rollback_now: false,
        rollback_blocking_reason: "already_rolled_back",
        rolled_back_file_count: 3
      }
    ]
  }));

  const imported = JSON.parse(execFileSync(
    process.execPath,
    [sampleCli, "receipt-state", "--response", responsePath, "--bundle-id", "bundle_imported_1"],
    { encoding: "utf8" }
  ));
  const rolledBack = JSON.parse(execFileSync(
    process.execPath,
    [sampleCli, "receipt-state", "--response", responsePath, "--bundle-id", "bundle_rolled_back_1"],
    { encoding: "utf8" }
  ));
  const missing = JSON.parse(execFileSync(
    process.execPath,
    [sampleCli, "receipt-state", "--response", responsePath, "--bundle-id", "bundle_missing_1"],
    { encoding: "utf8" }
  ));

  assert.equal(imported.state, "imported_can_rollback");
  assert.equal(imported.imported_with_strategy, "rename");
  assert.equal(imported.import_skipped_file_count, 1);
  assert.equal(imported.rollback_file_count, 3);
  assert.equal(rolledBack.state, "rolled_back");
  assert.equal(rolledBack.rollback_blocking_reason, "already_rolled_back");
  assert.equal(rolledBack.rolled_back_file_count, 3);
  assert.equal(missing.state, "missing");
  assert.equal(missing.can_request_rollback, false);

  rmSync(tempRoot, { recursive: true, force: true });
});

test("generic adapter sample resets missing event cursors", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "nekodrop-generic-adapter-cursor-missing-"));
  const responsePath = join(tempRoot, "events-response.json");
  writeFileSync(responsePath, JSON.stringify({
    events_next_after_id: null,
    events_cursor_state: "missing",
    events_visible_first_id: "bridge-event-100",
    events_visible_last_id: "bridge-event-120",
    events_visible_count: 21
  }));

  const stdout = execFileSync(
    process.execPath,
    [sampleCli, "cursor", "--response", responsePath],
    { encoding: "utf8" }
  );

  const cursor = JSON.parse(stdout);
  assert.equal(cursor.after_event_id, null);
  assert.equal(cursor.cursor_state, "missing");
  assert.equal(cursor.reset_required, true);
  assert.equal(cursor.visible_first_event_id, "bridge-event-100");
  assert.equal(cursor.visible_last_event_id, "bridge-event-120");
  assert.equal(cursor.visible_event_count, 21);

  rmSync(tempRoot, { recursive: true, force: true });
});

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}
