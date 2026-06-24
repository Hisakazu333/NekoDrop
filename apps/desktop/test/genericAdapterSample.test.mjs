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

test("generic adapter sample rejects untrusted sends for sensitive bundle types", () => {
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
        "session",
        "--require-trusted-device",
        "false"
      ],
      { encoding: "utf8", stdio: "pipe" }
    ),
    /session bundles require --require-trusted-device true/
  );

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
});

test("generic adapter sample derives the next event cursor", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "nekodrop-generic-adapter-cursor-"));
  const responsePath = join(tempRoot, "events-response.json");
  writeFileSync(responsePath, JSON.stringify({
    events_next_after_id: "bridge-event-42",
    events_cursor_state: "ok"
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

  rmSync(tempRoot, { recursive: true, force: true });
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
        message: "local bridge action is queued for the desktop runtime"
      },
      {
        request_id: "adapter-import-001",
        action_kind: "bundle.import",
        status: "running",
        lifecycle_status: "running",
        message: "local bridge bundle import is running"
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
  const missing = JSON.parse(execFileSync(
    process.execPath,
    [sampleCli, "action-state", "--response", responsePath, "--action-request-id", "adapter-missing-001"],
    { encoding: "utf8" }
  ));

  assert.equal(pending.state, "pending");
  assert.equal(pending.final, false);
  assert.equal(running.state, "running");
  assert.equal(running.final, false);
  assert.equal(result.state, "result");
  assert.equal(result.final, true);
  assert.equal(result.lifecycle_status, "succeeded");
  assert.equal(result.bundle_id, "bundle_received_1");
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
    events_cursor_state: "missing"
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

  rmSync(tempRoot, { recursive: true, force: true });
});

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}
