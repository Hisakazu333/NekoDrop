import assert from "node:assert/strict";
import { test } from "node:test";

import {
  bundleStatusLabel,
  markBundleDeleted,
  markBundleImportFailed,
  receiveBundleImportHint,
  receiveBundleStatusLabel
} from "../src/bundleState.ts";
import type { ReceivedBundleDto, ReceiveReportDto } from "../src/types.ts";

function bundle(overrides: Partial<ReceivedBundleDto> = {}): ReceivedBundleDto {
  return {
    bundle_id: "bundle_123",
    bundle_type: "workspace",
    display_name: "workspace",
    source_app: "NekoDrop",
    file_count: 2,
    total_bytes: 42,
    staging_path: "/tmp/bundle_123",
    import_allowed: true,
    staging_status: "saved",
    can_import_now: true,
    import_path: null,
    ...overrides
  };
}

function report(receivedBundle: ReceivedBundleDto): ReceiveReportDto {
  return {
    transfer_id: "receive-1",
    root_name: "workspace",
    security_mode: "authenticated_encrypted_session",
    sender_device_id: "device-a",
    sender_device_name: "MacBook",
    sender_public_key_fingerprint: "sha256:abc",
    file_count: 2,
    bundle: receivedBundle,
    files: []
  };
}

test("labels deleted staged bundles without hiding the receive history", () => {
  const deleted = markBundleDeleted(bundle());

  assert.equal(deleted.staging_status, "deleted");
  assert.equal(deleted.can_import_now, false);
  assert.equal(bundleStatusLabel(deleted), "已删除");
  assert.equal(receiveBundleStatusLabel(report(deleted)), "已删除");
  assert.equal(receiveBundleImportHint(deleted), "暂存已删除，历史记录保留");
});

test("keeps failed imports retryable when the bundle allows import", () => {
  const failed = markBundleImportFailed(bundle());

  assert.equal(failed.staging_status, "import_failed");
  assert.equal(failed.can_import_now, true);
  assert.equal(receiveBundleStatusLabel(report(failed)), "导入失败");
  assert.equal(receiveBundleImportHint(failed), "导入没有完成，暂存仍可重试");
});

test("labels expired staged bundles as cleaned up", () => {
  const expired = bundle({
    staging_status: "expired",
    can_import_now: false
  });

  assert.equal(receiveBundleStatusLabel(report(expired)), "已过期");
  assert.equal(receiveBundleImportHint(expired), "暂存已过期清理");
});
