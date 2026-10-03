import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const typesSource = readFileSync(new URL("../src/types.ts", import.meta.url), "utf8");
const settingsDomain = readFileSync(new URL("../src/context/settings.ts", import.meta.url), "utf8");
const settingsView = readFileSync(new URL("../src/components/SettingsView.tsx", import.meta.url), "utf8");
const tauriSource = readFileSync(new URL("../src/tauri.ts", import.meta.url), "utf8");
const coreConfig = readFileSync(new URL("../../../crates/nekodrop-core/src/config.rs", import.meta.url), "utf8");
const receiveRust = readFileSync(new URL("../src-tauri/src/commands/receive.rs", import.meta.url), "utf8");
const settingsRust = readFileSync(new URL("../src-tauri/src/commands/settings.rs", import.meta.url), "utf8");

test("desktop iroh receive integration is wired", () => {
  // 配置枚举三态 + 默认关闭（零第三方）
  assert.match(coreConfig, /pub enum IrohReceiveMode/);
  assert.match(coreConfig, /iroh_receive_mode: IrohReceiveMode/);
  // 收件线程：iroh 接受复用同一处理器，取消可停
  assert.match(receiveRust, /ReceiveLoopCtx/);
  assert.match(receiveRust, /fn handle_receive_connection/);
  assert.match(receiveRust, /accept_transfer_stream_with_deadline/);
  assert.match(receiveRust, /iroh_connection_code/);
  // 设置命令与前端
  assert.match(settingsRust, /pub fn set_iroh_receive_mode/);
  assert.match(tauriSource, /"set_iroh_receive_mode"/);
  assert.match(settingsDomain, /updateIrohReceiveMode/);
  // UI：三态开关 + 两种连接码展示与复制（修复码不可见缺口）
  assert.match(settingsView, /跨网收件/);
  assert.match(settingsView, /局域网连接码/);
  assert.match(settingsView, /跨网连接码/);
  assert.match(settingsView, /updateIrohReceiveMode\("off"\)/);
  assert.match(settingsView, /updateIrohReceiveMode\("direct"\)/);
  assert.match(settingsView, /updateIrohReceiveMode\("relay"\)/);
  // DTO 带跨网码
  assert.match(typesSource, /irohConnectionCode\?: string/);
});
