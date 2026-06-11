import assert from "node:assert/strict";
import { test } from "node:test";

import {
  broadcastTroubleshootingHint,
  discoveryTroubleshootingHint,
  unavailableDiscoveryHint
} from "../src/networkPermissionHints.ts";

test("no-device discovery hint names Windows and macOS network permissions", () => {
  const hint = discoveryTroubleshootingHint();

  assert.match(hint, /Windows/);
  assert.match(hint, /专用网络/);
  assert.match(hint, /macOS/);
  assert.match(hint, /本地网络/);
});

test("discovery failure hints keep connection code as the fallback", () => {
  assert.match(unavailableDiscoveryHint(), /备用码/);
  assert.match(broadcastTroubleshootingHint(), /备用码/);
});
