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

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}
