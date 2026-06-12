import assert from "node:assert/strict";
import { test } from "node:test";

import {
  buildTransferProgressViewModel,
  formatBytes,
  formatDuration,
  progressPercent
} from "../src/transferProgress.ts";
import type { TransferStatusDto } from "../src/types.ts";

function status(overrides: Partial<TransferStatusDto> = {}): TransferStatusDto {
  return {
    direction: "receive",
    phase: "transferring",
    root_name: "软软",
    file_count: 147,
    file_index: 12,
    current_file: "软软/IMG_0012.jpg",
    bytes_transferred: 2 * 1024 * 1024 * 1024,
    total_bytes: 3 * 1024 * 1024 * 1024,
    progress: 2 / 3,
    message: "正在接收",
    updated_at_ms: 1780000000000,
    ...overrides
  };
}

test("formats bytes and durations for transfer progress", () => {
  assert.equal(formatBytes(512), "512 B");
  assert.equal(formatBytes(1536), "1.5 KB");
  assert.equal(formatBytes(2 * 1024 * 1024), "2.0 MB");
  assert.equal(formatBytes(3 * 1024 * 1024 * 1024), "3.0 GB");
  assert.equal(formatDuration(45), "45s");
  assert.equal(formatDuration(125), "2m 5s");
  assert.equal(formatDuration(3720), "1h 2m");
});

test("clamps progress percent into a stable display range", () => {
  assert.equal(progressPercent(status({ progress: -0.2 })), 0);
  assert.equal(progressPercent(status({ progress: 0.666 })), 67);
  assert.equal(progressPercent(status({ progress: 2 })), 100);
});

test("builds a user-facing active transfer progress model", () => {
  const model = buildTransferProgressViewModel(status(), {
    speedBytesPerSecond: 42 * 1024 * 1024,
    etaSeconds: 25
  });

  assert.equal(model.title, "正在接收");
  assert.equal(model.rootName, "软软");
  assert.equal(model.percentLabel, "67%");
  assert.equal(model.bytesLabel, "2.0 GB / 3.0 GB");
  assert.equal(model.fileIndexLabel, "12 / 147");
  assert.equal(model.speedLabel, "42.0 MB/s");
  assert.equal(model.etaLabel, "剩余 25s");
  assert.equal(model.currentFileLabel, "软软/IMG_0012.jpg");
});

test("labels failed and cancelled transfer phases clearly", () => {
  assert.equal(buildTransferProgressViewModel(status({ phase: "failed" }), {
    speedBytesPerSecond: null,
    etaSeconds: null
  }).title, "传输失败");
  assert.equal(buildTransferProgressViewModel(status({ phase: "cancelled" }), {
    speedBytesPerSecond: null,
    etaSeconds: null
  }).title, "已取消");
});
