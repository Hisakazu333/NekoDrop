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
