import assert from "node:assert/strict";
import { test } from "node:test";

import {
  buildRecentTransferDetailLine,
  buildTransferHistoryDetailViewModel,
  transferFallbackActionLabel
} from "../src/transferHistoryDetails.ts";
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
    security_mode: null,
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
  assert.equal(model.recoveryLabel, "已传 2.0 GB，剩余 1.0 GB，可继续发送");
  assert.equal(model.adviceLabel, null);
  assert.equal(model.primaryActionLabel, "继续发送");
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
  assert.equal(model.adviceLabel, null);
  assert.equal(model.primaryActionLabel, null);
  assert.equal(model.canContinue, false);
});

test("adds short advice for failed transfers with actionable network errors", () => {
  const model = buildTransferHistoryDetailViewModel(transfer({
    transferred_bytes: 0,
    error_message: "无法连接对方电脑。请确认对方 NekoDrop 正在运行、收件已开启、防火墙允许访问，且两台设备网络互通。"
  }));

  assert.equal(model.adviceLabel, "确认对方已打开收件；Windows 允许专用网络");
  assert.equal(model.primaryActionLabel, "重试");
});

test("uses resend as the primary action for completed send transfers", () => {
  const model = buildTransferHistoryDetailViewModel(transfer({
    status: "completed",
    transferred_bytes: 3 * 1024 * 1024 * 1024,
    error_message: null
  }));

  assert.equal(model.primaryActionLabel, "重发");
});

test("uses fallback code as a recovery action for failed and cancelled sends", () => {
  assert.equal(transferFallbackActionLabel(transfer({ status: "failed" })), "备用码");
  assert.equal(transferFallbackActionLabel(transfer({ status: "cancelled" })), "备用码");
  assert.equal(transferFallbackActionLabel(transfer({ status: "completed" })), null);
  assert.equal(transferFallbackActionLabel(transfer({ direction: "receive", status: "failed" })), null);
});

test("summarizes cancelled send transfers with the next recovery step", () => {
  const recoverable = buildTransferHistoryDetailViewModel(transfer({
    status: "cancelled",
    error_message: null
  }));

  assert.equal(recoverable.recoveryLabel, "已取消，已传 2.0 GB，剩余 1.0 GB，可继续发送");
  assert.equal(recoverable.primaryActionLabel, "继续发送");
  assert.equal(recoverable.canContinue, true);

  const retryable = buildTransferHistoryDetailViewModel(transfer({
    status: "cancelled",
    transferred_bytes: 0,
    progress: 0,
    error_message: null
  }));

  assert.equal(retryable.recoveryLabel, "已取消，可重试");
  assert.equal(retryable.primaryActionLabel, "重试");
  assert.equal(retryable.canContinue, false);
});

test("summarizes non-resumable failed sends with a retry-only recovery step", () => {
  const retryable = buildTransferHistoryDetailViewModel(transfer({
    transferred_bytes: 0,
    progress: 0,
    error_message: "连接中断"
  }));

  assert.equal(retryable.recoveryLabel, "未保留续传进度，可重试");
  assert.equal(retryable.primaryActionLabel, "重试");
  assert.equal(retryable.canContinue, false);
  assert.equal(
    buildRecentTransferDetailLine(transfer({
      transferred_bytes: 0,
      progress: 0,
      error_message: "连接中断"
    })),
    "未保留续传进度，可重试"
  );
});

test("summarizes recent failed transfers with the next recovery step", () => {
  assert.equal(
    buildRecentTransferDetailLine(transfer()),
    "已传 2.0 GB，剩余 1.0 GB，可继续发送"
  );

  assert.equal(
    buildRecentTransferDetailLine(transfer({
      transferred_bytes: 0,
      error_message: "连接超时。常见原因是 Windows 防火墙拦截、两台设备不在同一网段、路由器隔离了有线/无线，或 VPN/代理影响了局域网连接。"
    })),
    "确认同一局域网；关闭 VPN/代理；可用备用码"
  );
});

test("summarizes recent cancelled transfers with the next recovery step", () => {
  assert.equal(
    buildRecentTransferDetailLine(transfer({
      status: "cancelled",
      error_message: null
    })),
    "已取消，已传 2.0 GB，剩余 1.0 GB，可继续发送"
  );

  assert.equal(
    buildRecentTransferDetailLine(transfer({
      status: "cancelled",
      transferred_bytes: 0,
      progress: 0,
      error_message: null
    })),
    "已取消，可重试"
  );
});
