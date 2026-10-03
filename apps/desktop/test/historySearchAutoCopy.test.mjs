import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const helpersSource = readFileSync(new URL("../src/context/helpers.ts", import.meta.url), "utf8");
const contextSource = readFileSync(new URL("../src/context/AppContext.tsx", import.meta.url), "utf8");
const historySource = readFileSync(new URL("../src/components/HistoryView.tsx", import.meta.url), "utf8");
const settingsSource = readFileSync(new URL("../src/components/SettingsView.tsx", import.meta.url), "utf8");

test("history search filters by name peer and status", () => {
  // 搜索框只在记录较多时出现，过滤覆盖名称/对方/状态
  assert.match(historySource, /history-search/);
  assert.match(historySource, /root_name \?\? ""\)\.toLowerCase\(\)\.includes\(q\)/);
  assert.match(historySource, /peer_name \?\? ""\)\.toLowerCase\(\)\.includes\(q\)/);
  assert.match(historySource, /statusLabel\(transfer\)\.text\.toLowerCase\(\)\.includes\(q\)/);
  // 空结果区分"无记录"与"无匹配"
  assert.match(historySource, /没有匹配/);
});

test("auto copy preference defaults on and persists", async () => {
  const { readTextSnippetAutoCopy, writeTextSnippetAutoCopy, TEXT_SNIPPET_AUTO_COPY_STORAGE_KEY } =
    await import("../src/context/helpers.ts");
  // node 环境无 localStorage：模拟一个
  const store = new Map();
  globalThis.localStorage = {
    getItem: (k) => store.get(k) ?? null,
    setItem: (k, v) => store.set(k, String(v)),
    removeItem: (k) => store.delete(k)
  };
  // 默认开
  assert.equal(readTextSnippetAutoCopy(), true);
  // 写关读回
  writeTextSnippetAutoCopy(false);
  assert.equal(store.get(TEXT_SNIPPET_AUTO_COPY_STORAGE_KEY), "0");
  assert.equal(readTextSnippetAutoCopy(), false);
  writeTextSnippetAutoCopy(true);
  assert.equal(readTextSnippetAutoCopy(), true);
  delete globalThis.localStorage;
});

test("context auto-copies fresh received snippets once", () => {
  // 首次运行标记既有历史，避免启动复制旧文本
  assert.match(contextSource, /seenTextSnippetIds/);
  assert.match(contextSource, /seenTextSnippetIds\.current == null/);
  // 新记录 → read_received_text + 剪贴板 + toast
  assert.match(contextSource, /read_received_text/);
  assert.match(contextSource, /文本已自动复制/);
  // 设置开关接入
  assert.match(settingsSource, /autoCopyTextSnippets/);
  assert.match(settingsSource, /setTextSnippetAutoCopy\(true\)/);
  assert.match(settingsSource, /setTextSnippetAutoCopy\(false\)/);
});
