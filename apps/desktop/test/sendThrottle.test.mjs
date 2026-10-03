import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const settingsDomainSource = readFileSync(new URL("../src/context/settings.ts", import.meta.url), "utf8");
const settingsViewSource = readFileSync(new URL("../src/components/SettingsView.tsx", import.meta.url), "utf8");
const typesSource = readFileSync(new URL("../src/types.ts", import.meta.url), "utf8");
const rustSend = readFileSync(new URL("../src-tauri/src/commands/send.rs", import.meta.url), "utf8");
const rustSettings = readFileSync(new URL("../src-tauri/src/commands/settings.rs", import.meta.url), "utf8");
const coreConfig = readFileSync(new URL("../../../crates/nekodrop-core/src/config.rs", import.meta.url), "utf8");
const sidecar = readFileSync(new URL("../../../apps/sidecar/src/main.rs", import.meta.url), "utf8");

test("send throttle flows end to end", () => {
  // 桌面发送链路用 pacer 包进度回调（块间隙就地睡眠 = 真实限速）
  assert.match(rustSend, /SendPacer::from_kbps\(send_limit_kbps\)/);
  assert.match(rustSend, /pacer_for_attempt\.observe/);
  // 设置命令 + 配置字段持久化
  assert.match(rustSettings, /pub fn set_send_limit/);
  assert.match(coreConfig, /send_limit_kbps: u32/);
  // 快照与前端设置行
  assert.match(typesSource, /send_limit_kbps: number/);
  assert.match(settingsDomainSource, /set_send_limit/);
  assert.match(settingsViewSource, /sendLimitInput/);
  assert.match(settingsViewSource, /发送限速/);
  // sidecar --limit 标志
  assert.match(sidecar, /split_limit_flag/);
  assert.match(sidecar, /--limit <KB\/s>/);
});
