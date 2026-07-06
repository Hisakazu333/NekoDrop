import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const contextSource = readFileSync(new URL("../src/context/AppContext.tsx", import.meta.url), "utf8");
const devicesSource = readFileSync(new URL("../src/components/DevicesManager.tsx", import.meta.url), "utf8");
const inboxSource = readFileSync(new URL("../src/components/InboxDrawer.tsx", import.meta.url), "utf8");
const leftSidebarSource = readFileSync(new URL("../src/components/LeftSidebar.tsx", import.meta.url), "utf8");
const settingsSource = readFileSync(new URL("../src/components/SettingsManager.tsx", import.meta.url), "utf8");
const transferZoneSource = readFileSync(new URL("../src/components/TransferZone.tsx", import.meta.url), "utf8");
const stylesSource = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");
const localBridgeStateSource = readFileSync(new URL("../src/localBridgeState.ts", import.meta.url), "utf8");

test("receive policy segment columns match the visible policy options", () => {
  const optionsBlock = settingsSource.match(/const RECEIVE_POLICY_OPTIONS[\s\S]+?\];/);
  assert.ok(optionsBlock, "RECEIVE_POLICY_OPTIONS should exist");
  const optionCount = [...optionsBlock[0].matchAll(/value:\s*"[^"]+"/g)].length;

  const segmentRule = stylesSource.match(/\.policy-segment\s*\{[\s\S]+?\}/);
  assert.ok(segmentRule, ".policy-segment rule should exist");
  const repeat = segmentRule[0].match(/grid-template-columns:\s*repeat\((\d+),/);
  assert.ok(repeat, ".policy-segment should use an explicit repeat column count");

  assert.equal(Number(repeat[1]), optionCount);
});

test("device surfaces include discovery guidance instead of only a count", () => {
  assert.match(leftSidebarSource, /const discoveryCopy = buildDiscoveryCopy\(discoveryStatus, nearbyDevices\.length, localPlatform\)/);
  assert.match(leftSidebarSource, /tree-node-empty">\{discoveryCopy\.label\}/);
  assert.match(devicesSource, /const discoveryCopy = buildDiscoveryCopy\(discoveryStatus, nearbyDevices\.length, localPlatform\)/);
  assert.match(devicesSource, /nearbyDevices\.length > 0 \? nearbyDevices\.length : discoveryCopy\.label/);
  assert.match(devicesSource, /discoveryCopy\.emptyBody/);
});

test("bundle and integration are not top-level navigation destinations", () => {
  const modeBlock = contextSource.match(/type ComposerMode =[\s\S]+?;/);
  assert.ok(modeBlock, "ComposerMode should exist");
  assert.doesNotMatch(modeBlock[0], /"bundles"/);
  assert.doesNotMatch(modeBlock[0], /"integrations"/);
  assert.doesNotMatch(contextSource, /setMode\("bundles"\)/);
  assert.doesNotMatch(contextSource, /setMode\("integrations"\)/);
  assert.doesNotMatch(leftSidebarSource, /setMode\("bundles"\)/);
  assert.doesNotMatch(leftSidebarSource, /setMode\("integrations"\)/);
});

test("send flow owns manual bundle creation instead of a separate bundle page", () => {
  assert.match(contextSource, /async function createManualBundleForSend\(\)/);
  assert.match(transferZoneSource, /createManualBundleForSend/);
  assert.match(transferZoneSource, /资料包目录/);
  assert.doesNotMatch(contextSource, /mode === "bundles"/);
  assert.doesNotMatch(transferZoneSource, /onSelectMode\("bundles"\)/);
});

test("received bundle state explains why import is or is not available", () => {
  assert.match(inboxSource, /bundleImportStatusView/);
  assert.match(inboxSource, /bundleImportPlanLine/);
  assert.match(inboxSource, /pendingBundles/);
  assert.match(inboxSource, /bundle\.can_import_now/);
  assert.match(inboxSource, /bundleCanUseImportStrategy/);
  assert.match(inboxSource, /rollbackCurrentBundle/);
});

test("local integration status lives in settings instead of a separate integration page", () => {
  assert.match(settingsSource, /runLocalBridgeSelfCheck/);
  assert.doesNotMatch(contextSource, /mode === "integrations"/);
  assert.doesNotMatch(settingsSource, /onSelectMode\("integrations"\)/);
});

test("local integration settings expose a generic read-only bridge self check", () => {
  assert.match(contextSource, /const \[localBridgeStatus, setLocalBridgeStatus\]/);
  assert.match(contextSource, /invokeCommand<LocalBridgeRuntimeStatusDto>\("get_local_bridge_runtime_status"/);
  assert.match(contextSource, /invokeCommand<LocalBridgeAuthorizationListDto>\("list_local_bridge_authorizations"/);
  assert.match(contextSource, /"revoke_local_bridge_authorization"/);
  assert.match(contextSource, /invokeCommand<LocalBridgeAuthorizationListDto>\("prune_local_bridge_authorizations"/);
  assert.match(contextSource, /const \[localBridgeCheck, setLocalBridgeCheck\]/);
  assert.match(contextSource, /const \[localBridgeAuthorizations, setLocalBridgeAuthorizations\]/);
  assert.match(contextSource, /const \[localBridgeActionResults, setLocalBridgeActionResults\]/);
  assert.match(contextSource, /function runLocalBridgeSelfCheck/);
  assert.match(contextSource, /invokeCommand<LocalBridgeResponseDto>\("handle_local_bridge_request"/);
  assert.match(contextSource, /invokeCommand<LocalBridgeAuthorizationDto>\("confirm_local_bridge_authorization"/);
  assert.match(contextSource, /invokeCommand<LocalBridgePendingActionResultListDto>\("list_local_bridge_pending_action_results"/);
  assert.match(contextSource, /localBridgeAuthorizationCode/);
  assert.match(contextSource, /"kind": "devices.list"/);
  assert.match(settingsSource, /localBridgeRuntimeLine/);
  assert.match(settingsSource, /revokeLocalBridgeAuthorization/);
  assert.match(settingsSource, /pruneLocalBridgeAuthorizations/);
  assert.match(settingsSource, /localBridgeStatus/);
  assert.match(settingsSource, /localBridgeCheck/);
  assert.match(settingsSource, /localBridgeActionResults/);
  assert.match(settingsSource, /runLocalBridgeSelfCheck/);
  assert.match(settingsSource, /待执行/);
  assert.match(settingsSource, /执行结果/);
  assert.match(settingsSource, /localBridgePendingActionStateLine/);
  assert.match(settingsSource, /localBridgeActionResultLifecycleView/);
  assert.match(settingsSource, /local-bridge-result-status/);
  assert.match(settingsSource, /className="console-copy"/);
  assert.match(localBridgeStateSource, /localBridgeActionResultDetailLine/);
  assert.match(localBridgeStateSource, /localBridgeActionResultReasonLabel/);
  assert.match(localBridgeStateSource, /bundle_import_conflict/);
});

test("device overview uses discovery guidance when no nearby devices are online", () => {
  assert.match(devicesSource, /const discoveryCopy = buildDiscoveryCopy\(discoveryStatus, nearbyDevices\.length, localPlatform\)/);
  assert.match(devicesSource, /nearbyDevices\.length > 0 \? nearbyDevices\.length : discoveryCopy\.label/);
  assert.match(devicesSource, /discoveryCopy\.emptyBody/);
});
