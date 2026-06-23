import assert from "node:assert/strict";
import { test } from "node:test";

import {
  bundleImportPlanLine,
  bundleCanUseImportStrategy,
  bundleImportStrategyLabel,
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
    import_destination: "/tmp/imports/bundle_123",
    import_conflict: false,
    import_blocking_reason: null,
    import_plan_files: [],
    import_conflict_count: 0,
    import_conflict_strategies: ["reject"],
    imported_with_strategy: null,
    import_skipped_file_count: 0,
    import_receipt_path: null,
    imported_manifest_paths: [],
    skipped_manifest_paths: [],
    rollback_file_count: 0,
    can_rollback_now: false,
    rollback_blocking_reason: null,
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

test("labels import conflicts before the user retries import", () => {
  const conflicted = bundle({
    can_import_now: false,
    import_conflict: true,
    import_blocking_reason: "destination_exists"
  });

  assert.equal(bundleStatusLabel(conflicted), "已存在");
  assert.equal(receiveBundleImportHint(conflicted), "同名资料已存在，可重命名导入");
  assert.equal(markBundleImportFailed(conflicted).can_import_now, false);
});

test("labels import conflicts with conflicting file counts", () => {
  const conflicted = bundle({
    can_import_now: false,
    import_conflict: true,
    import_blocking_reason: "destination_exists",
    import_conflict_count: 2
  });

  assert.equal(receiveBundleImportHint(conflicted), "有 2 个目标文件已存在，可重命名或跳过冲突");
});

test("labels import conflict strategies for compact receive actions", () => {
  const conflicted = bundle({
    can_import_now: false,
    import_conflict: true,
    import_conflict_strategies: ["reject", "rename", "skip_conflicts"]
  });

  assert.equal(bundleCanUseImportStrategy(conflicted, "rename"), true);
  assert.equal(bundleCanUseImportStrategy(conflicted, "skip_conflicts"), true);
  assert.equal(bundleImportStrategyLabel("rename"), "重命名");
  assert.equal(bundleImportStrategyLabel("skip_conflicts"), "跳过冲突");
});

test("summarizes renamed imports and skipped conflict counts", () => {
  const imported = bundle({
    staging_status: "imported",
    import_path: "/tmp/imports/bundle_123-2",
    imported_with_strategy: "skip_conflicts",
    import_skipped_file_count: 2,
    import_receipt_path: "/tmp/imports/.nekodrop_import_receipts/bundle_123-1.json",
    rollback_file_count: 1,
    can_rollback_now: true
  });

  assert.equal(receiveBundleImportHint(imported), "已导入到 /tmp/imports/bundle_123-2，跳过 2 个冲突 · 跳过冲突 · 可撤回 1 个");
});

test("summarizes import plan file counts for importable bundles", () => {
  const importable = bundle({
    import_plan_files: [
      {
        manifest_path: "sessions/main.json",
        destination_path: "/tmp/imports/sessions/main.json",
        size: 12,
        sha256: "a".repeat(64),
        destination_exists: false
      },
      {
        manifest_path: "workspace/state.json",
        destination_path: "/tmp/imports/workspace/state.json",
        size: 30,
        sha256: "b".repeat(64),
        destination_exists: false
      }
    ]
  });

  assert.equal(bundleImportPlanLine(importable), "将导入 2 个文件");
});

test("summarizes concrete import plan conflicts before import", () => {
  const conflicted = bundle({
    can_import_now: false,
    import_conflict: true,
    import_blocking_reason: "destination_exists",
    import_conflict_count: 3,
    import_plan_files: [
      {
        manifest_path: "sessions/main.json",
        destination_path: "/tmp/imports/sessions/main.json",
        size: 12,
        sha256: "a".repeat(64),
        destination_exists: true
      },
      {
        manifest_path: "workspace/state.json",
        destination_path: "/tmp/imports/workspace/state.json",
        size: 30,
        sha256: "b".repeat(64),
        destination_exists: true
      },
      {
        manifest_path: "skills/code.json",
        destination_path: "/tmp/imports/skills/code.json",
        size: 9,
        sha256: "c".repeat(64),
        destination_exists: true
      }
    ]
  });

  assert.equal(bundleImportPlanLine(conflicted), "冲突：sessions/main.json、workspace/state.json 等");
});

test("does not show import plan lines for closed staged bundle states", () => {
  assert.equal(bundleImportPlanLine(bundle({ staging_status: "imported" })), null);
  assert.equal(bundleImportPlanLine(bundle({ staging_status: "deleted" })), null);
});
