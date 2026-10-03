import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const updateCheck = readFileSync(new URL("../src/updateCheck.ts", import.meta.url), "utf8");
const contextSource = readFileSync(new URL("../src/context/AppContext.tsx", import.meta.url), "utf8");
const settingsView = readFileSync(new URL("../src/components/SettingsView.tsx", import.meta.url), "utf8");
const tauriConf = readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8");
const releaseWorkflow = readFileSync(new URL("../../../.github/workflows/release.yml", import.meta.url), "utf8");

test("update check compares semver correctly", async () => {
  const { isNewerVersion } = await import("../src/updateCheck.ts");
  assert.equal(isNewerVersion("v0.1.2", "0.1.1"), true);
  assert.equal(isNewerVersion("v0.2.0", "0.1.1"), true);
  assert.equal(isNewerVersion("v1.0.0", "0.9.9"), true);
  assert.equal(isNewerVersion("v0.1.1", "0.1.1"), false);
  assert.equal(isNewerVersion("v0.1.0", "0.1.1"), false);
  // 十位补丁号：0.1.10 > 0.1.9（不是字符串比较）
  assert.equal(isNewerVersion("v0.1.10", "0.1.9"), true);
});

test("update check flows end to end", () => {
  // GitHub API 只读 + CSP 显式放行（不放行就白写）
  assert.match(updateCheck, /api\.github\.com\/repos\/Hisakazu333\/NekoDrop\/releases\/latest/);
  assert.match(tauriConf, /connect-src[^"]*https:\/\/api\.github\.com/);
  // 提示不自动下载（自动更新需要签名，后续接 updater）
  assert.match(contextSource, /checkForUpdate/);
  assert.match(contextSource, /updateInfo/);
  // 设置页有版本行与前往下载
  assert.match(settingsView, /updateInfo/);
  assert.match(settingsView, /前往下载/);
});

test("release workflow builds intel mac too", () => {
  assert.match(releaseWorkflow, /macos-intel:/);
  assert.match(releaseWorkflow, /--target x86_64-apple-darwin/);
  assert.match(releaseWorkflow, /needs: \[macos, macos-intel, windows\]/);
});
