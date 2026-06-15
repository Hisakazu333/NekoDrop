import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const repoRoot = new URL("../../../", import.meta.url);
const index = readJson(new URL("docs/bundle-samples/index.json", repoRoot));

const allowedTypes = new Set(["skill", "session", "workspace", "agent_profile", "config_snapshot"]);
const allowedScopes = new Set([
  "skill.install",
  "session.import",
  "workspace.import",
  "agent_profile.import",
  "config.import"
]);
const allowedWriteModes = new Set(["create_only", "manual_import"]);

test("bundle samples cover every v1 upper-layer type", () => {
  assert.deepEqual(
    index.samples.map((sample) => sample.bundle_type).sort(),
    [...allowedTypes].sort()
  );
});

test("bundle samples are valid v1 manifests with matching checksums and permissions", () => {
  for (const sample of index.samples) {
    const root = new URL(`docs/bundle-samples/${sample.path}/`, repoRoot);
    const manifest = readJson(new URL("bundle.json", root));
    const checksums = readJson(new URL("checksums.json", root));
    const permissions = readJson(new URL("permissions.json", root));

    assert.equal(manifest.schema, "nekolink.bundle.v1");
    assert.equal(manifest.bundle_type, sample.bundle_type);
    assert.ok(allowedTypes.has(manifest.bundle_type));
    assert.equal(manifest.summary.file_count, manifest.files.length);
    assert.equal(
      manifest.summary.total_bytes,
      manifest.files.reduce((sum, file) => sum + file.size, 0)
    );
    assert.equal(checksums.algorithm, "sha256");
    assert.deepEqual(
      Object.keys(checksums.files).sort(),
      manifest.files.map((file) => file.path).sort()
    );

    for (const file of manifest.files) {
      assert.match(file.path, /^files\/[a-z0-9._/-]+$/);
      assert.match(file.sha256, /^[a-f0-9]{64}$/);
      assert.equal(checksums.files[file.path], file.sha256);
      const bytes = readFileSync(new URL(file.path, root));
      assert.equal(bytes.byteLength, file.size);
      assert.equal(createHash("sha256").update(bytes).digest("hex"), file.sha256);
    }

    for (const scope of permissions.requested_scopes) {
      assert.ok(allowedScopes.has(scope), `${sample.path} has unsupported scope ${scope}`);
    }
    for (const write of permissions.writes) {
      assert.doesNotMatch(write.target, /^(\/|[A-Za-z]:|~)/);
      assert.ok(allowedWriteModes.has(write.mode), `${sample.path} has unsupported write mode ${write.mode}`);
    }
    assert.equal(permissions.secrets.contains_secrets, false);
  }
});

function readJson(url) {
  return JSON.parse(readFileSync(url, "utf8"));
}
