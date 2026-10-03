import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const helpersSource = readFileSync(new URL("../src/context/helpers.ts", import.meta.url), "utf8");
const contextSource = readFileSync(new URL("../src/context/AppContext.tsx", import.meta.url), "utf8");
const sidebarSource = readFileSync(new URL("../src/components/HomeView.tsx", import.meta.url), "utf8");
const dragDropSource = readFileSync(new URL("../src/dragDrop.ts", import.meta.url), "utf8");
const receiveRust = readFileSync(new URL("../src-tauri/src/commands/receive.rs", import.meta.url), "utf8");
const settingsRust = readFileSync(new URL("../src-tauri/src/commands/settings.rs", import.meta.url), "utf8");

test("drag-to-device drop targeting is wired", () => {
  // drop 事件携带物理坐标
  assert.match(dragDropSource, /DropPosition = \{ x: number; y: number \}/);
  assert.match(dragDropSource, /event\.payload\.position\.x/);
  // 命中检测按 dpr 换算并查 data-device-drop-id
  assert.match(helpersSource, /export function deviceIdAtDropPosition/);
  assert.match(helpersSource, /devicePixelRatio \|\| 1/);
  // 命中后走 dispatchSend 直发（复用忙线入队语义）
  assert.match(contextSource, /deviceIdAtDropPosition\(position, ratio\)/);
  assert.match(contextSource, /sendDroppedPathsTo/);
  assert.match(contextSource, /async function dispatchSend/);
  // 侧栏行带命中属性 + 拖拽高亮
  assert.match(sidebarSource, /data-device-drop-id=\{row\.id\}/);
  assert.match(sidebarSource, /is-drop-hint/);
  assert.match(sidebarSource, /松手即发/);
});

test("organize-by-device flows end to end", () => {
  assert.match(receiveRust, /organize_received_files_by_sender/);
  assert.match(receiveRust, /sanitize_device_folder_name/);
  assert.match(settingsRust, /pub fn set_organize_receive_by_device/);
  assert.match(contextSource, /updateOrganizeByDevice/);
});
