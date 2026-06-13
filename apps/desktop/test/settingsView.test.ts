import assert from "node:assert/strict";
import { test } from "node:test";

import {
  buildSettingsViewModel,
  discoveryRuntimeLabel,
  receivePolicyDisplayLabel
} from "../src/settingsView.ts";
import type { AppSnapshot, DiscoveryStatusDto, ReceiveSessionDto } from "../src/types.ts";

function snapshot(overrides: Partial<AppSnapshot> = {}): AppSnapshot {
  return {
    device_name: "MacBook",
    receive_dir: "/Users/hisakazu/Downloads/NekoDrop",
    receive_port: 45821,
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

function discoveryStatus(overrides: Partial<DiscoveryStatusDto> = {}): DiscoveryStatusDto {
  return {
    phase: "active",
    message: "本机已广播，正在扫描附近设备",
    service_type: "_nekodrop._tcp.local.",
    advertised: true,
    lan_ip: "192.168.1.10",
    port: 45821,
    device_count: 1,
    last_seen_seconds_ago: 5,
    last_error: null,
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
    discoveryStatus: discoveryStatus(),
    receiveSession: receiveSession(),
    receiveDir: "/Users/hisakazu/Downloads/NekoDrop",
    receivePolicy: "always_ask",
    bindPort: "45821"
  });

  assert.deepEqual(model, {
    deviceName: "MacBook",
    canSaveDeviceName: false,
    platformLabel: "macOS",
    deviceIdLabel: "device-1",
    deviceKindLabel: "desktop",
    fingerprintLabel: "aa:bb:cc",
    capabilitiesLabel: "file_transfer · sha256",
    receiveStateLabel: "收件开启",
    receiveAddressLabel: "0.0.0.0:45821",
    connectionCodeLabel: "nekodrop:v1:tcp:192.168.1.10:45821",
    defaultReceivePortLabel: "45821",
    discoveryEnabledLabel: "配置已启用",
    discoveryLabel: "已广播",
    discoveryDetailLabel: "本机已广播，正在扫描附近设备",
    lanIpLabel: "192.168.1.10",
    nearbyDeviceCountLabel: "1 台附近",
    serviceTypeLabel: "_nekodrop._tcp.local.",
    receiveDiagnosticsLabel: null,
    lanIpsLabel: null,
    trayLabel: "窗口菜单已启用",
    canSaveReceiveDir: false,
    canSaveReceivePort: false,
    receiveConfigLocked: true,
    receiveDir: "/Users/hisakazu/Downloads/NekoDrop",
    receivePolicyLabel: "接收前询问",
    bindPort: "45821"
  });
});

test("keeps settings usable before the first snapshot arrives", () => {
  const model = buildSettingsViewModel({
    snapshot: null,
    discoveryStatus: null,
    receiveSession: null,
    receiveDir: "~/Downloads/NekoDrop",
    receivePolicy: "block_all",
    bindPort: "45821"
  });

  assert.equal(model.deviceName, "这台电脑");
  assert.equal(model.canSaveDeviceName, false);
  assert.equal(model.platformLabel, "Unknown");
  assert.equal(model.deviceIdLabel, null);
  assert.equal(model.fingerprintLabel, null);
  assert.equal(model.receiveStateLabel, "收件关闭");
  assert.equal(model.receiveAddressLabel, "未监听");
  assert.equal(model.discoveryLabel, "未知");
  assert.equal(model.trayLabel, "仅窗口标题");
  assert.equal(model.canSaveReceiveDir, false);
  assert.equal(model.canSaveReceivePort, false);
  assert.equal(model.receiveConfigLocked, false);
  assert.equal(model.receivePolicyLabel, "阻止外部接收");
});

test("labels discovery runtime state from diagnostics instead of config flags", () => {
  assert.equal(discoveryRuntimeLabel(discoveryStatus({ advertised: true })), "已广播");
  assert.equal(discoveryRuntimeLabel(discoveryStatus({ advertised: false })), "扫描中");
  assert.equal(discoveryRuntimeLabel(discoveryStatus({
    advertised: false,
    phase: "unavailable"
  })), "不可用");
  assert.equal(discoveryRuntimeLabel(discoveryStatus({
    advertised: false,
    phase: "starting"
  })), "未广播");
});

test("only enables saving device name when the trimmed value changes", () => {
  assert.equal(buildSettingsViewModel({
    snapshot: snapshot(),
    deviceNameInput: "  Work Mac  ",
    receiveSession: null,
    receiveDir: "~/Downloads/NekoDrop",
    receivePolicy: "always_ask",
    bindPort: "45821"
  }).canSaveDeviceName, true);

  assert.equal(buildSettingsViewModel({
    snapshot: snapshot(),
    deviceNameInput: "  MacBook  ",
    receiveSession: null,
    receiveDir: "~/Downloads/NekoDrop",
    receivePolicy: "always_ask",
    bindPort: "45821"
  }).canSaveDeviceName, false);

  assert.equal(buildSettingsViewModel({
    snapshot: snapshot(),
    deviceNameInput: "  ",
    receiveSession: null,
    receiveDir: "~/Downloads/NekoDrop",
    receivePolicy: "always_ask",
    bindPort: "45821"
  }).canSaveDeviceName, false);
});

test("only enables saving receive directory when stopped and the trimmed value changed", () => {
  assert.equal(buildSettingsViewModel({
    snapshot: snapshot(),
    receiveSession: null,
    receiveDir: "  /Users/hisakazu/Downloads/NekoDrop/manual  ",
    receivePolicy: "always_ask",
    bindPort: "45821"
  }).canSaveReceiveDir, true);

  assert.equal(buildSettingsViewModel({
    snapshot: snapshot(),
    receiveSession: null,
    receiveDir: "  /Users/hisakazu/Downloads/NekoDrop  ",
    receivePolicy: "always_ask",
    bindPort: "45821"
  }).canSaveReceiveDir, false);

  assert.equal(buildSettingsViewModel({
    snapshot: snapshot(),
    receiveSession: null,
    receiveDir: "  ",
    receivePolicy: "always_ask",
    bindPort: "45821"
  }).canSaveReceiveDir, false);

  assert.equal(buildSettingsViewModel({
    snapshot: snapshot(),
    receiveSession: receiveSession(),
    receiveDir: "/Users/hisakazu/Downloads/NekoDrop/manual",
    receivePolicy: "always_ask",
    bindPort: "45821"
  }).canSaveReceiveDir, false);
});

test("locks receive directory edits while the receiver is running", () => {
  assert.equal(buildSettingsViewModel({
    snapshot: snapshot(),
    receiveSession: null,
    receiveDir: "/Users/hisakazu/Downloads/NekoDrop",
    receivePolicy: "always_ask",
    bindPort: "45821"
  }).receiveConfigLocked, false);

  assert.equal(buildSettingsViewModel({
    snapshot: snapshot(),
    receiveSession: receiveSession(),
    receiveDir: "/Users/hisakazu/Downloads/NekoDrop",
    receivePolicy: "always_ask",
    bindPort: "45821"
  }).receiveConfigLocked, true);
});

test("only enables saving receive port when stopped and the port changed", () => {
  assert.equal(buildSettingsViewModel({
    snapshot: snapshot(),
    receiveSession: null,
    receiveDir: "/Users/hisakazu/Downloads/NekoDrop",
    receivePolicy: "always_ask",
    bindPort: "45999"
  }).canSaveReceivePort, true);

  assert.equal(buildSettingsViewModel({
    snapshot: snapshot(),
    receiveSession: null,
    receiveDir: "/Users/hisakazu/Downloads/NekoDrop",
    receivePolicy: "always_ask",
    bindPort: "45821"
  }).canSaveReceivePort, false);

  assert.equal(buildSettingsViewModel({
    snapshot: snapshot(),
    receiveSession: null,
    receiveDir: "/Users/hisakazu/Downloads/NekoDrop",
    receivePolicy: "always_ask",
    bindPort: "0"
  }).canSaveReceivePort, false);

  assert.equal(buildSettingsViewModel({
    snapshot: snapshot(),
    receiveSession: receiveSession(),
    receiveDir: "/Users/hisakazu/Downloads/NekoDrop",
    receivePolicy: "always_ask",
    bindPort: "45999"
  }).canSaveReceivePort, false);
});
