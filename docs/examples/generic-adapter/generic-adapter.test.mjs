import assert from "node:assert/strict";
import { existsSync, mkdtempSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  ACTION_LIFECYCLE_STATUSES,
  BUNDLE_DETAIL_STATUSES,
  buildActionResultsRequest,
  buildBundleDetailPreview,
  buildBundleDetailRequest,
  buildEventsPollRequest,
  buildImportRequest,
  buildReceipt,
  buildSendRequest,
  buildWorkflowPlan,
  exportSampleBundle,
  rollbackExportRoot,
  writeReceipt,
} from "./generic-adapter.mjs";

test("exportSampleBundle copies the session summary fixture", () => {
  const outputRoot = mkdtempSync(path.join(os.tmpdir(), "generic-adapter-export-"));
  const snapshot = exportSampleBundle({ outputRoot });

  assert.equal(snapshot.manifest.bundle_type, "session");
  assert.equal(snapshot.manifest.bundle_id, "sample_session_summary");
  assert.equal(snapshot.import_allowed, true);
  assert.equal(snapshot.staging_status, "saved");
  assert.equal(snapshot.files.length, 1);
  assert.equal(snapshot.files[0].path, "files/session.json");

  const receipt = writeReceipt(outputRoot, path.join(outputRoot, "receipt.json"));
  assert.equal(receipt.bundle_detail_staging_status, "imported");
  assert.equal(receipt.action_lifecycle_status, "succeeded");
  assert.equal(receipt.event_lifecycle_status, "succeeded");

  const rollback = rollbackExportRoot(outputRoot);
  assert.equal(rollback.status, "rolled_back");
  assert.equal(existsSync(outputRoot), false);
});

test("workflow plan uses the same lifecycle words across preview and results", () => {
  const outputRoot = mkdtempSync(path.join(os.tmpdir(), "generic-adapter-plan-"));
  const snapshot = exportSampleBundle({ outputRoot });

  const plan = buildWorkflowPlan({
    bundleRoot: outputRoot,
    targetDeviceId: "neko-device-target",
    stagedBundleId: snapshot.manifest.bundle_id,
  });

  assert.equal(plan.requests.send.kind, "bundle.send");
  assert.equal(plan.requests.detail.kind, "bundle.detail");
  assert.equal(plan.requests.events.kind, "events.poll");
  assert.equal(plan.requests.results.kind, "actions.results");
  assert.deepEqual(plan.status_words.action_lifecycle, ACTION_LIFECYCLE_STATUSES);
  assert.deepEqual(plan.status_words.bundle_detail, BUNDLE_DETAIL_STATUSES);
  assert.equal(plan.preview.staging_status, "saved");
  assert.equal(plan.receipt.bundle_detail_staging_status, "imported");
  assert.equal(plan.rollback.action, "delete_export_root");

  rollbackExportRoot(outputRoot);
});

test("request builders keep the protocol shapes stable", () => {
  const bundleRoot = mkdtempSync(path.join(os.tmpdir(), "generic-adapter-request-"));
  assert.equal(
    buildSendRequest({ bundleRoot }).payload.bundle_root,
    path.resolve(bundleRoot),
  );
  assert.equal(
    buildBundleDetailRequest({ stagedBundleId: "bundle_1234567890" }).kind,
    "bundle.detail",
  );
  assert.equal(buildEventsPollRequest().kind, "events.poll");
  assert.equal(buildActionResultsRequest().kind, "actions.results");
  assert.equal(buildImportRequest({ stagedBundleId: "bundle_1234567890" }).kind, "bundle.import");
  assert.equal(
    buildBundleDetailPreview(
      {
        manifest: {
          bundle_id: "bundle_1234567890",
          bundle_type: "session",
          display_name: "planning_notes_session",
          source_app: "Generic Adapter",
          summary: { file_count: 1, total_bytes: 10 },
        },
        bundle_root: "/tmp/exported-bundle",
        import_allowed: true,
      },
      { status: "saved" },
    ).staging_status,
    "saved",
  );
  assert.equal(buildReceipt(
    {
      manifest: {
        bundle_id: "bundle_1234567890",
        bundle_type: "session",
        display_name: "planning_notes_session",
        source_app: "Generic Adapter",
      },
    },
    { detailStatus: "imported" },
  ).bundle_detail_staging_status, "imported");

  rollbackExportRoot(bundleRoot);
});
