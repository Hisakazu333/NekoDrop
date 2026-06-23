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
  assert.equal(request.payload.after_claimed_at_ms, 1234);
  assert.equal(request.payload.limit, 5);
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
    "results"
  ]);
  assert.equal(workflow.steps[1].request.kind, "bundle.send");
  assert.equal(workflow.steps[3].request.kind, "bundle.detail");
  assert.equal(workflow.steps[4].request.kind, "bundle.import");
  assert.equal(workflow.steps[4].request.payload.conflict_strategy, "skip_conflicts");
  assert.equal(workflow.steps[5].request.kind, "actions.results");
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
