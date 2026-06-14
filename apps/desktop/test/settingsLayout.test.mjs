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

test("bundle and integration are not top-level navigation destinations", () => {
  const navBlock = appSource.match(/const NAV_ITEMS[\s\S]+?\];/);
  assert.ok(navBlock, "NAV_ITEMS should exist");
  assert.doesNotMatch(navBlock[0], /mode:\s*"bundles"/);
  assert.doesNotMatch(navBlock[0], /mode:\s*"integrations"/);
  assert.doesNotMatch(navBlock[0], /label:\s*"资料包"/);
  assert.doesNotMatch(navBlock[0], /label:\s*"集成"/);
});

test("send flow owns manual bundle creation instead of a separate bundle page", () => {
  assert.match(appSource, /<ManualBundleComposer[\s\S]+onCreateManualBundle=\{createManualBundleForSend\}/);
  assert.doesNotMatch(appSource, /mode === "bundles"/);
  assert.doesNotMatch(appSource, /onSelectMode\("bundles"\)/);
  assert.match(appSource, /资料包目录/);
});

test("received bundle state explains why import is or is not available", () => {
  assert.match(appSource, /function receiveBundleImportHint\(bundle: ReceivedBundleDto\)/);
  assert.match(appSource, /bundle\.can_import_now/);
  assert.match(appSource, /bundle\.import_allowed/);
  assert.match(appSource, /receivedBundleHint/);
  assert.match(appSource, /\{receivedBundleHint \? <small>\{receivedBundleHint\}<\/small> : null\}/);
});

test("local integration status lives in settings instead of a separate integration page", () => {
  assert.match(appSource, /<IntegrationSettings \/>/);
  assert.doesNotMatch(appSource, /mode === "integrations"/);
  assert.doesNotMatch(appSource, /onSelectMode\("integrations"\)/);
});

test("device overview uses discovery guidance when no nearby devices are online", () => {
  assert.match(appSource, /function DevicePanel[\s\S]+const discoveryCopy = buildDiscoveryCopy\(discoveryStatus, nearbyDevices\.length, localPlatform\)/);
  assert.match(appSource, /nearbyDevices\.length > 0 \? nearbyDevices\.length : discoveryCopy\.label/);
  assert.match(appSource, /nearbyDevices\.length > 0 \? "附近在线" : discoveryCopy\.targetLabel/);
});
