import assert from "node:assert/strict";
import { test } from "node:test";

import {
  buildTransferSecurityViewModel,
  receiveSecuritySummaryLine
} from "../src/securityState.ts";
import type { ReceiveReportDto } from "../src/types.ts";

function receiveReport(overrides: Partial<ReceiveReportDto> = {}): ReceiveReportDto {
  return {
    transfer_id: "receive-1",
    root_name: "drop",
    security_mode: "authenticated_encrypted_session",
    sender_device_id: "device-1",
    sender_device_name: "MacBook",
    sender_public_key_fingerprint: "aa:bb:cc",
    file_count: 2,
    bundle: null,
    files: [],
    ...overrides
  };
}

test("labels authenticated encrypted transfers without vague secure copy", () => {
  const model = buildTransferSecurityViewModel("authenticated_encrypted_session");

  assert.equal(model.label, "已认证加密");
  assert.equal(model.tone, "trusted");
  assert.equal(model.detail, "双方身份已验签，文件流已加密");
});

test("labels legacy plain transfers as compatibility mode", () => {
  const model = buildTransferSecurityViewModel("legacy_plain");

  assert.equal(model.label, "兼容明文");
  assert.equal(model.tone, "warning");
  assert.equal(model.detail, "仅手动确认，不会刷新可信设备");
});

test("returns null for unknown or missing security state", () => {
  assert.equal(buildTransferSecurityViewModel(null), null);
  assert.equal(buildTransferSecurityViewModel("future_mode"), null);
});

test("builds a compact receive security summary", () => {
  assert.equal(
    receiveSecuritySummaryLine(receiveReport()),
    "已认证加密 · MacBook · aa:bb:cc"
  );
  assert.equal(
    receiveSecuritySummaryLine(receiveReport({
      security_mode: "encrypted_session",
      sender_device_name: null,
      sender_public_key_fingerprint: null
    })),
    "已加密"
  );
});
