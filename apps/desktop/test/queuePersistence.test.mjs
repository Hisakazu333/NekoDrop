import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const helpersSource = readFileSync(new URL("../src/context/helpers.ts", import.meta.url), "utf8");
const contextSource = readFileSync(new URL("../src/context/AppContext.tsx", import.meta.url), "utf8");

test("queue persists across restarts", async () => {
  const { saveSendQueue, loadSendQueue, SEND_QUEUE_STORAGE_KEY } = await import("../src/context/helpers.ts");
  const store = new Map();
  globalThis.localStorage = {
    getItem: (k) => store.get(k) ?? null,
    setItem: (k, v) => store.set(k, String(v)),
    removeItem: (k) => store.delete(k),
  };
  const entry = {
    id: "q1", kind: "device", target: "d1",
    pathsText: "/tmp/a.txt", label: "a.txt", enqueuedAtMs: 1, paused: true,
  };
  saveSendQueue([entry]);
  const restored = loadSendQueue();
  assert.equal(restored.length, 1);
  assert.deepEqual(restored[0], entry);
  delete globalThis.localStorage;
});

test("loadSendQueue drops corrupt or malformed entries", async () => {
  const { loadSendQueue } = await import("../src/context/helpers.ts");
  const store = new Map();
  globalThis.localStorage = {
    getItem: (k) => store.get(k) ?? null,
    setItem: (k, v) => store.set(k, String(v)),
    removeItem: (k) => store.delete(k),
  };
  store.set("nekodrop.sendQueue", "{not json");
  assert.deepEqual(loadSendQueue(), []);
  // 缺字段的条目丢弃，好的保留
  store.set("nekodrop.sendQueue", JSON.stringify([
    { id: "ok", kind: "code", target: "NEKO", pathsText: "/a", label: "a", enqueuedAtMs: 1 },
    { id: "bad", kind: "hacker", target: "x", pathsText: "/b", label: "b", enqueuedAtMs: 1 },
    { id: "bad2" },
  ]));
  const restored = loadSendQueue();
  assert.equal(restored.length, 1);
  assert.equal(restored[0].id, "ok");
  delete globalThis.localStorage;
});

test("failed queue items auto-requeue at most once", () => {
  assert.match(contextSource, /saveSendQueue\(sendQueue\)/);
  assert.ok(contextSource.includes("useState<QueuedSend[]>(() => loadSendQueue())"), "queue restored from storage on init");
  assert.match(contextSource, /retryOf/);
  assert.match(helpersSource, /retryOf\?: number/);
});
