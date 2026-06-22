import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const appSource = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
const stylesSource = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");
const localBridgeStateSource = readFileSync(new URL("../src/localBridgeState.ts", import.meta.url), "utf8");

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
  const modeBlock = appSource.match(/type ComposerMode =[\s\S]+?;/);
  assert.ok(modeBlock, "ComposerMode should exist");
  assert.doesNotMatch(modeBlock[0], /"bundles"/);
  assert.doesNotMatch(modeBlock[0], /"integrations"/);
  assert.doesNotMatch(appSource, /setMode\("bundles"\)/);
  assert.doesNotMatch(appSource, /setMode\("integrations"\)/);
});

test("send flow owns manual bundle creation instead of a separate bundle page", () => {
  assert.match(appSource, /<ManualBundleComposer[\s\S]+onCreateManualBundle=\{createManualBundleForSend\}/);
  assert.doesNotMatch(appSource, /mode === "bundles"/);
  assert.doesNotMatch(appSource, /onSelectMode\("bundles"\)/);
  assert.match(appSource, /资料包目录/);
});

test("received bundle state explains why import is or is not available", () => {
  assert.match(appSource, /receiveBundleImportHint/);
  assert.match(appSource, /stagedBundles=\{stagedBundles\}/);
  assert.match(appSource, /visibleStagedBundles/);
  assert.match(appSource, /receivedBundleHint/);
  assert.match(appSource, /\{receivedBundleHint \? <small>\{receivedBundleHint\}<\/small> : null\}/);
});

test("local integration status lives in settings instead of a separate integration page", () => {
  assert.match(appSource, /<IntegrationSettings[\s\S]+onRunLocalBridgeSelfCheck/);
  assert.doesNotMatch(appSource, /mode === "integrations"/);
  assert.doesNotMatch(appSource, /onSelectMode\("integrations"\)/);
});

test("local integration settings expose a generic read-only bridge self check", () => {
  assert.match(appSource, /const \[localBridgeStatus, setLocalBridgeStatus\]/);
  assert.match(appSource, /invokeCommand<LocalBridgeRuntimeStatusDto>\("get_local_bridge_runtime_status"/);
  assert.match(appSource, /invokeCommand<LocalBridgeAuthorizationListDto>\("list_local_bridge_authorizations"/);
  assert.match(appSource, /invokeCommand<LocalBridgeAuthorizationRevokeDto>\("revoke_local_bridge_authorization"/);
  assert.match(appSource, /invokeCommand<LocalBridgeAuthorizationListDto>\("prune_local_bridge_authorizations"/);
  assert.match(appSource, /localBridgeRuntimeLine/);
  assert.match(appSource, /const \[localBridgeCheck, setLocalBridgeCheck\]/);
  assert.match(appSource, /const \[localBridgeAuthorizations, setLocalBridgeAuthorizations\]/);
  assert.match(appSource, /const \[localBridgeActionResults, setLocalBridgeActionResults\]/);
  assert.match(appSource, /function runLocalBridgeSelfCheck/);
  assert.match(appSource, /invokeCommand<LocalBridgeResponseDto>\("handle_local_bridge_request"/);
  assert.match(appSource, /invokeCommand<LocalBridgeAuthorizationDto>\("confirm_local_bridge_authorization"/);
  assert.match(appSource, /invokeCommand<LocalBridgePendingActionResultListDto>\("list_local_bridge_pending_action_results"/);
  assert.match(appSource, /localBridgeAuthorizationCode/);
  assert.match(appSource, /onRevokeLocalBridgeAuthorization/);
  assert.match(appSource, /onPruneLocalBridgeAuthorizations/);
  assert.match(appSource, /"kind": "devices.list"/);
  assert.match(appSource, /<IntegrationSettings[\s\S]+localBridgeStatus=\{localBridgeStatus\}/);
  assert.match(appSource, /<IntegrationSettings[\s\S]+localBridgeCheck=\{localBridgeCheck\}/);
  assert.match(appSource, /<IntegrationSettings[\s\S]+localBridgeActionResults=\{localBridgeActionResults\}/);
  assert.match(appSource, /onRunLocalBridgeSelfCheck=\{runLocalBridgeSelfCheck\}/);
  assert.match(appSource, /<SettingsRow label="待授权">/);
  assert.match(appSource, /<SettingsRow label="执行结果">/);
  assert.match(appSource, /localBridgePendingActionStateLine/);
  assert.match(appSource, /localBridgeActionResultDetailLine/);
  assert.match(appSource, /className="console-copy"/);
  assert.match(localBridgeStateSource, /localBridgeActionResultReasonLabel/);
  assert.match(localBridgeStateSource, /bundle_import_conflict/);
});

test("device overview uses discovery guidance when no nearby devices are online", () => {
  assert.match(appSource, /function DevicePanel[\s\S]+const discoveryCopy = buildDiscoveryCopy\(discoveryStatus, nearbyDevices\.length, localPlatform\)/);
  assert.match(appSource, /nearbyDevices\.length > 0 \? nearbyDevices\.length : discoveryCopy\.label/);
  assert.match(appSource, /nearbyDevices\.length > 0 \? "附近在线" : discoveryCopy\.targetLabel/);
});
