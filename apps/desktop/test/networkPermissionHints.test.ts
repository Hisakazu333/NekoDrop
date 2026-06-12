import assert from "node:assert/strict";
import { test } from "node:test";

import {
  buildDiscoveryCopy,
  broadcastTroubleshootingHint,
  discoveryTroubleshootingHint,
  unavailableDiscoveryHint
} from "../src/networkPermissionHints.ts";
import type { DiscoveryStatusDto } from "../src/types.ts";

function discovery(overrides: Partial<DiscoveryStatusDto> = {}): DiscoveryStatusDto {
  return {
    phase: "running",
    message: "running",
    service_type: "_nekodrop._tcp.local.",
    advertised: true,
    lan_ip: "192.168.1.12",
    port: 45821,
    device_count: 0,
    last_seen_seconds_ago: null,
    last_error: null,
    ...overrides
  };
}

test("no-device discovery hint names Windows and macOS network permissions", () => {
  const hint = discoveryTroubleshootingHint();

  assert.match(hint, /Windows/);
  assert.match(hint, /专用网络/);
  assert.match(hint, /macOS/);
  assert.match(hint, /本地网络/);
});

test("discovery failure hints keep connection code as the fallback", () => {
  assert.match(unavailableDiscoveryHint(), /备用码/);
  assert.match(broadcastTroubleshootingHint(), /备用码/);
});

test("builds actionable copy for unavailable discovery", () => {
  const copy = buildDiscoveryCopy(discovery({
    phase: "unavailable",
    advertised: false,
    last_error: "mdns failed"
  }), 0);

  assert.equal(copy.label, "发现异常");
  assert.equal(copy.isError, true);
  assert.match(copy.emptyBody, /防火墙/);
  assert.match(copy.emptyBody, /备用码/);
});

test("builds actionable copy when discovery is not advertised because of a network error", () => {
  const copy = buildDiscoveryCopy(discovery({
    advertised: false,
    last_error: "no interface"
  }), 0);

  assert.equal(copy.label, "广播异常");
  assert.equal(copy.isError, true);
  assert.match(copy.emptyBody, /专用网络|本地网络/);
  assert.match(copy.targetLabel, /权限|网络/);
});

test("keeps no-device discovery copy short and non-error", () => {
  const copy = buildDiscoveryCopy(discovery(), 0);

  assert.equal(copy.label, "扫描中");
  assert.equal(copy.isError, false);
  assert.match(copy.emptyBody, /同一局域网/);
  assert.match(copy.emptyBody, /备用码/);
});
