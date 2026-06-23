import assert from "node:assert/strict";
import { test } from "node:test";

import {
  localBridgeActionResultDetailLine,
  localBridgeActionResultSummary,
  localBridgePendingActionStateLine,
  localBridgeRuntimeStatusLine
} from "../src/localBridgeState.ts";
import type {
  LocalBridgePendingActionDto,
  LocalBridgePendingActionResultDto,
  LocalBridgeRuntimeStatusDto
} from "../src/types.ts";

function pendingAction(overrides: Partial<LocalBridgePendingActionDto> = {}): LocalBridgePendingActionDto {
  return {
    request_id: "bridge-send-1",
    action_kind: "bundle.send",
    client_id: "sample.adapter",
    client_display_name: "Sample Adapter",
    bundle_type: "workspace",
    target_device_id: "device-win",
    staged_bundle_id: null,
    expected_bundle_type: null,
    conflict_strategy: null,
    require_trusted_device: true,
    requested_at_ms: 1_000,
    bundle_root: "/tmp/bundle",
    ...overrides
  };
}

function actionResult(overrides: Partial<LocalBridgePendingActionResultDto> = {}): LocalBridgePendingActionResultDto {
  return {
    request_id: "bridge-import-1",
    action_kind: "bundle.import",
    client_id: "sample.adapter",
    client_display_name: "Sample Adapter",
    status: "failed",
    lifecycle_status: "conflict",
    reason: "bundle_import_conflict",
    message: "import conflict",
    bundle_id: "bundle_123",
    bundle_type: "workspace",
    bundle_root: null,
    target_device_id: null,
    require_trusted_device: null,
    conflict_strategy: "reject",
    skipped_file_count: 0,
    requested_at_ms: 1_000,
    claimed_at_ms: 2_000,
    ...overrides
  };
}

function runtimeStatus(overrides: Partial<LocalBridgeRuntimeStatusDto> = {}): LocalBridgeRuntimeStatusDto {
  return {
    active: true,
    bind_host: "127.0.0.1",
    port: 17328,
    request_path: "/v1/local-bridge",
    max_request_bytes: 65536,
    pending_authorization_client: null,
    authorization_count: 1,
    pending_action_count: 0,
    last_error: null,
    ...overrides
  };
}

test("local bridge runtime line surfaces pending authorization and action counts", () => {
  assert.equal(
    localBridgeRuntimeStatusLine(runtimeStatus({
      pending_authorization_client: "Sample Adapter",
      pending_action_count: 2
    })),
    "127.0.0.1:17328/v1/local-bridge · 待授权 Sample Adapter · 待执行 2"
  );
});

test("pending send action state calls out missing target selection", () => {
  assert.equal(
    localBridgePendingActionStateLine(pendingAction({ target_device_id: null })),
    "等待执行 · 需要选择可信设备"
  );
});

test("pending import action state names the staged bundle", () => {
  assert.equal(
    localBridgePendingActionStateLine(pendingAction({
      action_kind: "bundle.import",
      bundle_type: null,
      expected_bundle_type: "session",
      conflict_strategy: "rename",
      target_device_id: null,
      staged_bundle_id: "bundle_123",
      require_trusted_device: null,
      bundle_root: null
    })),
    "等待执行 · 导入 bundle_123"
  );
});

test("local bridge action result detail separates conflict reason from summary", () => {
  const result = actionResult();

  assert.equal(localBridgeActionResultSummary(result), "导入资料包 · 有冲突 · 已存在同名导入 · bundle_123");
  assert.equal(localBridgeActionResultDetailLine(result), "冲突：已存在同名导入");
});

test("local bridge action result detail keeps failed preflight actionable", () => {
  assert.equal(
    localBridgeActionResultDetailLine(actionResult({
      action_kind: "bundle.send",
      lifecycle_status: "failed_preflight",
      reason: "trusted_target_missing",
      bundle_id: null,
      target_device_id: "device-win"
    })),
    "预检失败：目标未配对"
  );
});
