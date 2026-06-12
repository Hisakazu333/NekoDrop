import assert from "node:assert/strict";
import { test } from "node:test";

import { buildTransferHistoryDetailViewModel } from "../src/transferHistoryDetails.ts";
import type { TransferDto } from "../src/types.ts";

function transfer(overrides: Partial<TransferDto> = {}): TransferDto {
  return {
    id: "transfer-1",
    root_name: "软软",
    peer_device_id: "device-1",
    peer_name: "HISAKAZU",
    target_host: "192.168.1.20:45821",
    source_paths: ["/Users/hisakazu/Downloads/soft"],
    received_paths: [],
    direction: "send",
    status: "failed",
    file_count: 147,
    total_bytes: 3 * 1024 * 1024 * 1024,
    transferred_bytes: 2 * 1024 * 1024 * 1024,
    progress: 2 / 3,
    receive_dir: null,
    error_message: "连接中断",
    created_at_ms: 1780000000000,
    updated_at_ms: 1780000100000,
    ...overrides
  };
}

test("summarizes recoverable failed send transfers", () => {
  const model = buildTransferHistoryDetailViewModel(transfer());

  assert.equal(model.progressLabel, "2.0 GB / 3.0 GB");
  assert.equal(model.peerLabel, "HISAKAZU");
  assert.equal(model.locationLabel, "/Users/hisakazu/Downloads/soft");
  assert.equal(model.errorLabel, "连接中断");
  assert.equal(model.recoveryLabel, "可以继续发送");
  assert.equal(model.canContinue, true);
});

test("uses receive directory and avoids recovery copy for completed receives", () => {
  const model = buildTransferHistoryDetailViewModel(transfer({
    direction: "receive",
    status: "completed",
    transferred_bytes: 3 * 1024 * 1024 * 1024,
    received_paths: ["/Users/hisakazu/Downloads/NekoDrop/soft"],
    receive_dir: "/Users/hisakazu/Downloads/NekoDrop",
    error_message: null
  }));

  assert.equal(model.progressLabel, "3.0 GB / 3.0 GB");
  assert.equal(model.locationLabel, "/Users/hisakazu/Downloads/NekoDrop");
  assert.equal(model.errorLabel, null);
  assert.equal(model.recoveryLabel, null);
  assert.equal(model.canContinue, false);
});
