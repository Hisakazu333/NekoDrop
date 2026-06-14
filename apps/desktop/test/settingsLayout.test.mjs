import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const appSource = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
const stylesSource = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");

test("receive policy segment columns match the visible policy options", () => {
  const optionsBlock = appSource.match(/const RECEIVE_POLICY_OPTIONS[\s\S]+?\];/);
  assert.ok(optionsBlock, "RECEIVE_POLICY_OPTIONS should exist");
  const optionCount = [...optionsBlock[0].matchAll(/value:\s*"[^"]+"/g)].length;

  const segmentRule = stylesSource.match(/\.policy-segment\s*\{[\s\S]+?\}/);
  assert.ok(segmentRule, ".policy-segment rule should exist");
  const repeat = segmentRule[0].match(/grid-template-columns:\s*repeat\((\d+),/);
  assert.ok(repeat, ".policy-segment should use an explicit repeat column count");

  assert.equal(Number(repeat[1]), optionCount);
});

test("overview nearby status includes discovery guidance instead of only a count", () => {
  assert.match(appSource, /<OverviewPanel[\s\S]+discoveryStatus=\{discoveryStatus\}/);
  assert.match(appSource, /const discoveryCopy = buildDiscoveryCopy\(discoveryStatus, nearbyDevices\.length, localPlatform\)/);
  assert.match(appSource, /className=\{discoveryCopy\.isError \? "overview-status-item is-warning" : "overview-status-item"\}/);
});
