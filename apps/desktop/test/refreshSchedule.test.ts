import assert from "node:assert/strict";
import { test } from "node:test";

import {
  DIRECTORY_REFRESH_INTERVAL_MS,
  REALTIME_REFRESH_INTERVAL_MS,
  shouldRunDirectoryRefresh
} from "../src/refreshSchedule.ts";

test("keeps realtime refresh frequent but moves directory refresh to a slower lane", () => {
  assert.equal(REALTIME_REFRESH_INTERVAL_MS, 1200);
  assert.ok(DIRECTORY_REFRESH_INTERVAL_MS >= 4000);
});

test("runs directory refresh immediately when there is no previous refresh", () => {
  assert.equal(shouldRunDirectoryRefresh(1000, 0), true);
});

test("skips directory refresh until the slower interval has elapsed", () => {
  assert.equal(shouldRunDirectoryRefresh(5999, 1000), false);
  assert.equal(shouldRunDirectoryRefresh(6000, 1000), true);
});

