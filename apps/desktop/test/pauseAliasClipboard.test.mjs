import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const helpersSource = readFileSync(new URL("../src/context/helpers.ts", import.meta.url), "utf8");
const contextSource = readFileSync(new URL("../src/context/AppContext.tsx", import.meta.url), "utf8");
const bannerSource = readFileSync(new URL("../src/components/TransferPanel.tsx", import.meta.url), "utf8");
const historySource = readFileSync(new URL("../src/components/HistoryView.tsx", import.meta.url), "utf8");
const devicesSource = readFileSync(new URL("../src/components/DevicesView.tsx", import.meta.url), "utf8");
const sidebarSource = readFileSync(new URL("../src/components/HomeView.tsx", import.meta.url), "utf8");
const rustHistory = readFileSync(new URL("../src-tauri/src/commands/history.rs", import.meta.url), "utf8");
const rustDevices = readFileSync(new URL("../src-tauri/src/commands/devices.rs", import.meta.url), "utf8");
const rustStore = readFileSync(new URL("../src-tauri/src/trusted_devices.rs", import.meta.url), "utf8");

test("queue supports paused entries and resume skips them", async () => {
  const { enqueueSend, dequeueSend, resumeQueuedSend } = await import("../src/context/helpers.ts");
  const entry = (id, paused = false) => ({
    id,
    kind: "device",
    target: "d1",
    pathsText: "/tmp/a.txt",
    label: "a.txt",
    enqueuedAtMs: 1,
    paused
  });
  const queue = enqueueSend(enqueueSend([], entry("waiting")), entry("paused", true));
  // 出队跳过暂停条目
  const first = dequeueSend(queue);
  assert.equal(first.head.id, "waiting");
  const second = dequeueSend(first.rest);
  assert.equal(second.head, null); // 只剩暂停的
  assert.equal(second.rest.length, 1);
  // 唤醒后可出队
  const resumed = resumeQueuedSend(second.rest, "paused");
  assert.equal(resumed[0].paused, false);
  assert.equal(dequeueSend(resumed).head.id, "paused");
});

test("context implements pause and resume of active send", () => {
  assert.match(contextSource, /activeSendRef/);
  assert.match(contextSource, /async function pauseCurrentTransfer/);
  assert.match(contextSource, /paused: true/);
  assert.match(contextSource, /function resumeQueuedSendById/);
});

test("banner offers pause during send and per-entry resume", () => {
  assert.match(bannerSource, /pauseCurrentTransfer/);
  assert.match(bannerSource, /resumeQueuedSendById/);
  assert.match(bannerSource, /已暂停/);
  assert.match(bannerSource, /继续/);
});

test("history exposes one-tap copy for received text snippets", () => {
  assert.match(historySource, /read_received_text/);
  assert.match(historySource, /copyTextToClipboard/);
  assert.match(historySource, /isTextSnippet/);
  assert.match(rustHistory, /pub fn read_received_text/);
  assert.match(rustHistory, /MAX_RECEIVED_TEXT_BYTES/);
});

test("trusted device alias flows end to end", () => {
  assert.match(rustStore, /pub alias: Option<String>/);
  assert.match(rustStore, /pub fn set_alias_on_trusted_device/);
  assert.match(rustDevices, /pub fn set_trusted_device_alias/);
  assert.match(contextSource, /setTrustedDeviceAlias/);
  // 设备页提供行内重命名
  assert.match(devicesSource, /renamingId/);
  assert.match(devicesSource, /setTrustedDeviceAlias\(device\.device_id, aliasDraft\)/);
  // 侧栏优先显示备注名
  assert.match(sidebarSource, /aliasById/);
  assert.match(sidebarSource, /device\.alias \?\? device\.device_name/);
});
