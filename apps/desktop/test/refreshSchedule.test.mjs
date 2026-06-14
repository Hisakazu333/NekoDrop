import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const refreshScheduleSource = readFileSync(new URL("../src/refreshSchedule.ts", import.meta.url), "utf8");

function exportedNumber(name) {
  const match = refreshScheduleSource.match(new RegExp(`export const ${name} = (\\d+);`));
  assert.ok(match, `${name} should be exported`);
  return Number(match[1]);
}

test("keeps realtime refresh frequent and moves expensive refreshes to slower lanes", () => {
  assert.equal(exportedNumber("REALTIME_REFRESH_INTERVAL_MS"), 1200);
  assert.ok(exportedNumber("DIRECTORY_REFRESH_INTERVAL_MS") >= 4000);
  assert.ok(exportedNumber("DIAGNOSTICS_REFRESH_INTERVAL_MS") >= 10000);
});

test("keeps diagnostics refresh separate from directory refresh", () => {
  assert.match(refreshScheduleSource, /function shouldRunDirectoryRefresh/);
  assert.match(refreshScheduleSource, /function shouldRunDiagnosticsRefresh/);
  assert.match(refreshScheduleSource, /intervalMs = DIRECTORY_REFRESH_INTERVAL_MS/);
  assert.match(refreshScheduleSource, /intervalMs = DIAGNOSTICS_REFRESH_INTERVAL_MS/);
});

test("slow refresh helpers use elapsed time checks", () => {
  const elapsedCheckCount = (refreshScheduleSource.match(/lastRefreshMs <= 0 \|\| nowMs - lastRefreshMs >= intervalMs/g) ?? []).length;
  assert.equal(elapsedCheckCount, 2);
});

test("directory refresh is scoped to pages that need device or transfer lists", () => {
  assert.match(refreshScheduleSource, /export function shouldRefreshDirectoryForMode/);
  assert.match(refreshScheduleSource, /mode === "send"/);
  assert.match(refreshScheduleSource, /mode === "devices"/);
  assert.match(refreshScheduleSource, /mode === "transfers"/);
  assert.match(refreshScheduleSource, /hasActiveTransfer/);
});
