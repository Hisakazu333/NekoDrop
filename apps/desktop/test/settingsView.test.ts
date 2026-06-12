import assert from "node:assert/strict";
import { test } from "node:test";

import {
  buildSettingsViewModel,
  receivePolicyDisplayLabel
} from "../src/settingsView.ts";
import type { AppSnapshot, ReceiveSessionDto } from "../src/types.ts";

function snapshot(overrides: Partial<AppSnapshot> = {}): AppSnapshot {
  return {
    device_name: "MacBook",
    receive_dir: "/Users/hisakazu/Downloads/NekoDrop",
    receive_policy: "always_ask",
    discovery_enabled: true,
    tray_enabled: true,
    device_identity: {
      device_id: "device-1",
      device_name: "MacBook",
      device_kind: "desktop",
      platform: "macos",
      public_key_fingerprint: "aa:bb:cc",
      capabilities: ["file_transfer", "sha256"]
    },
    ...overrides
  };
}

function receiveSession(overrides: Partial<ReceiveSessionDto> = {}): ReceiveSessionDto {
  return {
    bind_addr: "0.0.0.0:45821",
    receive_dir: "/Users/hisakazu/Downloads/NekoDrop",
    connection_code: "nekodrop:v1:tcp:192.168.1.10:45821",
    ...overrides
  };
}

test("labels receive policies for settings without exposing unsupported auto accept", () => {
  assert.equal(receivePolicyDisplayLabel("always_ask"), "接收前询问");
  assert.equal(receivePolicyDisplayLabel("block_all"), "阻止外部接收");
  assert.equal(receivePolicyDisplayLabel("auto_accept_trusted"), "接收前询问");
  assert.equal(receivePolicyDisplayLabel("unknown"), "接收前询问");
});

test("builds a restrained settings view model from real app state", () => {
  const model = buildSettingsViewModel({
    snapshot: snapshot(),
    receiveSession: receiveSession(),
    receiveDir: "/Users/hisakazu/Downloads/NekoDrop",
    receivePolicy: "always_ask",
    bindPort: "45821"
  });

  assert.deepEqual(model, {
    deviceName: "MacBook",
    platformLabel: "macOS",
    fingerprintLabel: "aa:bb:cc",
    receiveStateLabel: "收件开启",
    receiveAddressLabel: "0.0.0.0:45821",
    receiveDir: "/Users/hisakazu/Downloads/NekoDrop",
    receivePolicyLabel: "接收前询问",
    bindPort: "45821"
  });
});

test("keeps settings usable before the first snapshot arrives", () => {
  const model = buildSettingsViewModel({
    snapshot: null,
    receiveSession: null,
    receiveDir: "~/Downloads/NekoDrop",
    receivePolicy: "block_all",
    bindPort: "45821"
  });

  assert.equal(model.deviceName, "这台电脑");
  assert.equal(model.platformLabel, "Unknown");
  assert.equal(model.fingerprintLabel, null);
  assert.equal(model.receiveStateLabel, "收件关闭");
  assert.equal(model.receiveAddressLabel, "未监听");
  assert.equal(model.receivePolicyLabel, "阻止外部接收");
});
