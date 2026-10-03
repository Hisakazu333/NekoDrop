import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const helpersSource = readFileSync(new URL("../src/context/helpers.ts", import.meta.url), "utf8");
const contextSource = readFileSync(new URL("../src/context/AppContext.tsx", import.meta.url), "utf8");
const sendSource = readFileSync(new URL("../src/components/HomeView.tsx", import.meta.url), "utf8");
const bannerSource = readFileSync(new URL("../src/components/TransferPanel.tsx", import.meta.url), "utf8");
const tauriSource = readFileSync(new URL("../src/tauri.ts", import.meta.url), "utf8");
const mainSource = readFileSync(new URL("../src-tauri/src/main.rs", import.meta.url), "utf8");
const sendCommandsSource = readFileSync(new URL("../src-tauri/src/commands/send.rs", import.meta.url), "utf8");

test("helpers expose send queue primitives", () => {
  assert.match(helpersSource, /export interface QueuedSend/);
  assert.match(helpersSource, /export function enqueueSend/);
  assert.match(helpersSource, /export function dequeueSend/);
  assert.match(helpersSource, /export function queuedSendLabel/);
  // 队列上限存在且为 20
  const cap = helpersSource.match(/MAX_SEND_QUEUE = (\d+)/);
  assert.ok(cap, "MAX_SEND_QUEUE should be defined");
  assert.equal(Number(cap[1]), 20);
});

test("queuedSendLabel summarizes paths", async () => {
  const { queuedSendLabel } = await import("../src/context/helpers.ts");
  assert.equal(queuedSendLabel("/tmp/a.txt"), "a.txt");
  assert.equal(queuedSendLabel("/tmp/a.txt\n/tmp/b.txt"), "a.txt 等 2 项");
  assert.equal(queuedSendLabel(""), "空传输");
  // Windows 分隔符也能取末段
  assert.equal(queuedSendLabel("C:\\Users\\me\\笔记.md"), "笔记.md");
});

test("enqueue caps at 20 and dequeue returns head and rest", async () => {
  const { enqueueSend, dequeueSend } = await import("../src/context/helpers.ts");
  const entry = (id) => ({
    id,
    kind: "device",
    target: "device-1",
    pathsText: "/tmp/a.txt",
    label: "a.txt",
    enqueuedAtMs: 1
  });
  let queue = [];
  for (let i = 0; i < 30; i++) queue = enqueueSend(queue, entry(`q-${i}`));
  assert.equal(queue.length, 20);
  const { head, rest } = dequeueSend(queue);
  assert.equal(head.id, "q-0");
  assert.equal(rest.length, 19);
  const empty = dequeueSend([]);
  assert.equal(empty.head, null);
});

test("context wires queue lifecycle", () => {
  // 忙线入队 + 空闲出队 + 可取消/清空
  assert.match(contextSource, /enqueueSend\(sendQueue, entry\)/);
  assert.match(contextSource, /dequeueSend\(sendQueue\)/);
  assert.match(contextSource, /function cancelQueuedSend/);
  assert.match(contextSource, /function clearSendQueue/);
  // 文本快送走 stage_text_snippet
  assert.match(contextSource, /"stage_text_snippet"/);
  assert.match(contextSource, /sendCurrentTransfer\(textSnippet\?/);
});

test("send view composer is a real text input", () => {
  // textarea 可编辑：有 value/onChange，不再是 readOnly 占位
  assert.match(sendSource, /value=\{text\}/);
  assert.match(sendSource, /setText\(event\.target\.value\)/);
  assert.doesNotMatch(sendSource, /readOnly\s*\n\s*rows=\{1\}/);
  // ⌘↩ 发送 与 ⌘V 粘贴
  assert.match(sendSource, /event\.key === "Enter" && \(event\.metaKey \|\| event\.ctrlKey\)/);
  assert.match(sendSource, /addEventListener\("paste", onPaste\)/);
  // 文本条发送键：有目标+有文本即可发（忙线自动入队）
  assert.match(sendSource, /home-composer-send/);
  assert.match(sendSource, /disabled=\{text\.trim\(\)\.length === 0 \|\| !target\}/);
});

test("banner surfaces queue state", () => {
  assert.match(bannerSource, /sendQueue\.length > 0/);
  assert.match(bannerSource, /cancelQueuedSend/);
  assert.match(bannerSource, /tp-queue/);
});

test("rust side registers text snippet staging", () => {
  assert.match(mainSource, /commands::stage_text_snippet/);
  assert.match(sendCommandsSource, /pub fn stage_text_snippet/);
  assert.match(sendCommandsSource, /MAX_TEXT_SNIPPET_BYTES/);
  assert.match(tauriSource, /"stage_text_snippet"/);
});
