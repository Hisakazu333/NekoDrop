import assert from "node:assert/strict";
import { test } from "node:test";

import {
  buildDeviceCapabilitySummary,
  buildNearbyDeviceViewModel,
  buildTrustedDeviceViewModel,
  selectedTrustedTargetCopy
} from "../src/deviceDisplay.ts";
import type { DeviceDto, TrustedDeviceDto } from "../src/types.ts";

function nearby(overrides: Partial<DeviceDto> = {}): DeviceDto {
  return {
    id: "device-1",
    name: "Win11",
    platform: "windows",
    host: "192.168.1.20",
    port: 45821,
    trust_state: "Untrusted",
    public_key_fingerprint: "sha256:abc",
    pairing_code: "123-456",
    ...overrides
  };
}

function trusted(overrides: Partial<TrustedDeviceDto> = {}): TrustedDeviceDto {
  return {
    device_id: "device-1",
    device_name: "Win11",
    platform: "windows",
    host: "192.168.1.20",
    port: 45821,
    public_key_fingerprint: "sha256:abc",
    pairing_code: "123-456",
    paired_at_ms: 1780000000000,
    last_seen_at_ms: 1780000100000,
    ...overrides
  };
}

test("labels trusted nearby devices as directly selectable", () => {
  const model = buildNearbyDeviceViewModel(nearby({ trust_state: "Trusted" }), true);

  assert.equal(model.statusLabel, "已信任");
  assert.equal(model.actionLabel, "已选");
  assert.equal(model.canPair, false);
});

test("explains why an unpaired nearby device cannot be paired", () => {
  const model = buildNearbyDeviceViewModel(nearby({
    public_key_fingerprint: null,
    pairing_code: null
  }), false);

  assert.equal(model.statusLabel, "等待设备身份");
  assert.equal(model.actionLabel, "不可配对");
  assert.equal(model.canPair, false);
});

test("labels offline trusted devices with the last address fallback", () => {
  const model = buildTrustedDeviceViewModel(trusted({
    last_seen_at_ms: 1780000000000
  }), 1780003600000, false);

  assert.equal(model.presenceLabel, "1 小时前");
  assert.equal(model.detailLabel, "Windows · 192.168.1.20:45821");
  assert.equal(model.actionLabel, "用历史地址发送");
});

test("describes selected offline trusted devices as historical-address targets", () => {
  assert.deepEqual(selectedTrustedTargetCopy(trusted(), true), {
    targetLabel: "Win11",
    subtitle: "在线 · 192.168.1.20:45821"
  });

  assert.deepEqual(selectedTrustedTargetCopy(trusted(), false), {
    targetLabel: "Win11",
    subtitle: "使用上次地址 · 192.168.1.20:45821"
  });
});

test("summarizes device capabilities without claiming future agent or transport support", () => {
  assert.deepEqual(buildDeviceCapabilitySummary({
    trusted: true,
    online: true,
    hasPublicKey: true
  }), [
    { label: "文件", state: "ready" },
    { label: "可信", state: "ready" },
    { label: "加密", state: "ready" },
    { label: "资料包", state: "ready" },
    { label: "Agent", state: "off" },
    { label: "跨网络", state: "off" }
  ]);

  assert.deepEqual(buildDeviceCapabilitySummary({
    trusted: false,
    online: true,
    hasPublicKey: true
  }).map((item) => item.state), ["ready", "locked", "locked", "locked", "off", "off"]);
});
