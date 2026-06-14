import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const appSource = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");

test("inactive transfer metrics reuse a stable empty state object", () => {
  assert.match(appSource, /const EMPTY_TRANSFER_METRICS = Object\.freeze/);
  assert.match(appSource, /function resetTransferMetrics/);
  assert.match(appSource, /useState<TransferMetrics>\(EMPTY_TRANSFER_METRICS\)/);
  assert.doesNotMatch(appSource, /setTransferMetrics\(\{\s*speedBytesPerSecond:\s*null,\s*etaSeconds:\s*null\s*\}\)/);
});

test("state equality skips serialization for identical references and empty values", () => {
  const keepBody = appSource.match(/function keepIfEqual<T>\(current: T, next: T\): T \{[\s\S]+?\n\}/);
  assert.ok(keepBody, "keepIfEqual should exist");
  assert.match(keepBody[0], /Object\.is\(current, next\)/);
  assert.match(keepBody[0], /current == null \|\| next == null/);
  assert.match(keepBody[0], /stableJson\(current\) === stableJson\(next\)/);

  const resetBody = appSource.match(/function resetTransferMetrics\([\s\S]+?\n\}/);
  assert.ok(resetBody, "resetTransferMetrics should exist");
  assert.match(resetBody[0], /keepIfEqual\(current, EMPTY_TRANSFER_METRICS\)/);
});
