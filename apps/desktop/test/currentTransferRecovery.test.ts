import assert from "node:assert/strict";
import { test } from "node:test";

import {
  currentTransferRecoveryActions,
  findCurrentFailedTransfer
} from "../src/currentTransferRecovery.ts";
import type { TransferDto, TransferStatusDto } from "../src/types.ts";

function status(overrides: Partial<TransferStatusDto> = {}): TransferStatusDto {
  return {
    direction: "send",
    phase: "failed",
    root_name: "soft",
    file_count: 2,
    file_index: 1,
    current_file: "soft/a.bin",
    bytes_transferred: 6 * 1024,
    total_bytes: 10 * 1024,
    progress: 0.6,
    message: "连接中断",
    updated_at_ms: 1780000000000,
    ...overrides
  };
}

function transfer(overrides: Partial<TransferDto> = {}): TransferDto {
  return {
    id: "send-1",
    root_name: "soft",
    peer_device_id: "device-1",
    peer_name: "Windows",
    target_host: "192.168.1.20:45821",
    source_paths: ["/tmp/soft"],
    received_paths: [],
    direction: "send",
    status: "failed",
    file_count: 2,
    total_bytes: 10 * 1024,
    transferred_bytes: 6 * 1024,
    progress: 0.6,
    receive_dir: null,
    error_message: "连接中断",
    created_at_ms: 1780000000000,
    updated_at_ms: 1780000001000,
    ...overrides
  };
}

test("finds the latest matching failed send record for the current failed status", () => {
  const older = transfer({ id: "older", updated_at_ms: 1780000000001 });
  const newer = transfer({ id: "newer", updated_at_ms: 1780000000002 });

  assert.equal(findCurrentFailedTransfer(status(), [older, newer])?.id, "newer");
});

test("does not match receive or completed records for current send failures", () => {
  assert.equal(findCurrentFailedTransfer(status(), [
    transfer({ direction: "receive", id: "receive" }),
    transfer({ status: "completed", id: "done" })
  ]), null);
});

test("shows continue and fallback actions for resumable current failures", () => {
  const actions = currentTransferRecoveryActions(status(), transfer());

  assert.deepEqual(actions, {
    primaryLabel: "继续发送",
    fallbackLabel: "备用码"
  });
});

test("shows retry and fallback actions for non-resumable current failures", () => {
  const actions = currentTransferRecoveryActions(status({ bytes_transferred: 0 }), transfer({
    transferred_bytes: 0,
    progress: 0
  }));

  assert.deepEqual(actions, {
    primaryLabel: "重试",
    fallbackLabel: "备用码"
  });
});

test("only shows fallback code when no matching transfer record exists", () => {
  assert.deepEqual(currentTransferRecoveryActions(status(), null), {
    primaryLabel: null,
    fallbackLabel: "备用码"
  });
});
