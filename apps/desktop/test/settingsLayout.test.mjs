import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const contextSource = readFileSync(new URL("../src/context/AppContext.tsx", import.meta.url), "utf8");
const inboxSource = readFileSync(new URL("../src/components/InboxDrawer.tsx", import.meta.url), "utf8");
const sidebarSource = readFileSync(new URL("../src/components/Sidebar.tsx", import.meta.url), "utf8");
const settingsSource = readFileSync(new URL("../src/components/SettingsView.tsx", import.meta.url), "utf8");
const sendSource = readFileSync(new URL("../src/components/SendView.tsx", import.meta.url), "utf8");
const composerSource = readFileSync(new URL("../src/context/composer.ts", import.meta.url), "utf8");
const bridgeSource = readFileSync(new URL("../src/context/bridge.ts", import.meta.url), "utf8");
const stylesSource = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");
const helpersSource = readFileSync(new URL("../src/context/helpers.ts", import.meta.url), "utf8");

test("receive policy segment columns match the visible policy options", () => {
  const optionsBlock = settingsSource.match(/const RECEIVE_POLICY_OPTIONS[\s\S]+?\]\s*(?:as const)?;/);
  assert.ok(optionsBlock, "RECEIVE_POLICY_OPTIONS should exist");
  const optionCount = [...optionsBlock[0].matchAll(/value:\s*"[^"]+"/g)].length;

  const segmentRule = stylesSource.match(/\.policy-segment\s*\{[\s\S]+?\}/);
  assert.ok(segmentRule, ".policy-segment rule should exist");
  const repeat = segmentRule[0].match(/grid-template-columns:\s*repeat\((\d+),/);
  assert.ok(repeat, ".policy-segment should use an explicit repeat column count");

  assert.equal(Number(repeat[1]), optionCount);
});

test("inbox allow/deny actions carry distinct semantics", () => {
  assert.match(inboxSource, /respondLocalBridgePendingAction\(action, false\)/);
  assert.match(inboxSource, /respondLocalBridgePendingAction\(action, true\)/);
  assert.match(inboxSource, /importCurrentStagedBundle/);
  assert.match(inboxSource, /rollbackCurrentBundle/);
  assert.match(inboxSource, /deleteCurrentStagedBundle/);
});

test("send flow owns manual bundle creation instead of a separate bundle page", () => {
  assert.match(composerSource, /async function createManualBundleForSend\(\)/);
  assert.match(sendSource, /createManualBundleForSend/);
  assert.match(sendSource, /资料包/);
  assert.match(sendSource, /connectionCode/);
});

test("bundle and integration are not top-level navigation destinations", () => {
  const modeBlock = helpersSource.match(/type ComposerMode =[\s\S]+?;/);
  assert.ok(modeBlock, "ComposerMode should exist");
  assert.doesNotMatch(modeBlock[0], /"bundles"/);
  assert.doesNotMatch(modeBlock[0], /"integrations"/);
  assert.doesNotMatch(sidebarSource, /setMode\("bundles"\)/);
  assert.doesNotMatch(sidebarSource, /setMode\("integrations"\)/);
});

test("local bridge surface stays wired through the context", () => {
  const bridgeAssertions = bridgeSource;
  assert.match(bridgeSource, /const \[localBridgeStatus, setLocalBridgeStatus\]/);
  assert.match(bridgeAssertions, /invokeCommand<LocalBridgeRuntimeStatusDto>\("get_local_bridge_runtime_status"/);
  assert.match(bridgeAssertions, /invokeCommand<LocalBridgeAuthorizationListDto>\("list_local_bridge_authorizations"/);
  assert.match(bridgeAssertions, /"revoke_local_bridge_authorization"/);
  assert.match(bridgeAssertions, /invokeCommand<LocalBridgeAuthorizationListDto>\("prune_local_bridge_authorizations"/);
  assert.match(bridgeAssertions, /const \[localBridgeAuthorizationCode, setLocalBridgeAuthorizationCode\]/);
  assert.match(bridgeAssertions, /invokeCommand<LocalBridgeAuthorizationDto>\("confirm_local_bridge_authorization"/);
  assert.match(settingsSource, /runLocalBridgeSelfCheck/);
  assert.match(settingsSource, /revokeLocalBridgeAuthorization/);
  assert.match(settingsSource, /pruneLocalBridgeAuthorizations/);
  assert.match(settingsSource, /confirmLocalBridgeAuthorization/);
});
