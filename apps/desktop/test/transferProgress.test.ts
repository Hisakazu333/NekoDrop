import assert from "node:assert/strict";
import { test } from "node:test";

import {
  buildTransferProgressViewModel,
  formatBytes,
  formatDuration,
  progressPercent,
  shouldShowActiveTransferBar,
  shouldShowTransferProgressMeter
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
  assert.equal(model.adviceLabel, null);
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

test("adds short advice to failed transfer status messages", () => {
  const model = buildTransferProgressViewModel(status({
    phase: "failed",
    message: "连接超时。常见原因是 Windows 防火墙拦截、两台设备不在同一网段、路由器隔离了有线/无线，或 VPN/代理影响了局域网连接。"
  }), {
    speedBytesPerSecond: null,
    etaSeconds: null
  });

  assert.equal(model.adviceLabel, "确认同一局域网；关闭 VPN/代理；可用备用码");
});

test("hides idle receive listening from the active transfer bar", () => {
  assert.equal(shouldShowActiveTransferBar(status({ phase: "listening", total_bytes: 0 })), false);
  assert.equal(shouldShowActiveTransferBar(status({ phase: "closed" })), false);
  assert.equal(shouldShowActiveTransferBar(status({ phase: "transferring" })), true);
  assert.equal(shouldShowActiveTransferBar(status({ phase: "connecting" })), true);
  assert.equal(shouldShowActiveTransferBar(status({ phase: "failed" })), true);
});

test("only shows progress meter when bytes are moving", () => {
  assert.equal(shouldShowTransferProgressMeter(status({ phase: "listening", bytes_transferred: 0, total_bytes: 0 })), false);
  assert.equal(shouldShowTransferProgressMeter(status({ phase: "connecting", bytes_transferred: 0, total_bytes: 0 })), false);
  assert.equal(shouldShowTransferProgressMeter(status({ phase: "transferring" })), true);
  assert.equal(shouldShowTransferProgressMeter(status({ phase: "verifying", bytes_transferred: 0, total_bytes: 0 })), true);
});
