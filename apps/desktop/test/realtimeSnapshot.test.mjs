import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const appSource = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
const tauriSource = readFileSync(new URL("../src/tauri.ts", import.meta.url), "utf8");
const mainSource = readFileSync(new URL("../src-tauri/src/main.rs", import.meta.url), "utf8");
const commandsSource = readFileSync(new URL("../src-tauri/src/commands/mod.rs", import.meta.url), "utf8");
const commandDtoSource = readFileSync(new URL("../src-tauri/src/commands/dto.rs", import.meta.url), "utf8");

function functionBody(source, name) {
  const start = source.indexOf(`async function ${name}(`);
  assert.notEqual(start, -1, `${name} should exist`);
  const bodyStart = source.indexOf("{", start);
  let depth = 0;
  for (let index = bodyStart; index < source.length; index += 1) {
    const char = source[index];
    if (char === "{") depth += 1;
    if (char === "}") {
      depth -= 1;
      if (depth === 0) return source.slice(bodyStart, index + 1);
    }
  }
  assert.fail(`${name} body should close`);
}

test("realtime refresh uses one desktop snapshot IPC instead of separate status invokes", () => {
  const body = functionBody(appSource, "refreshRealtimeState");

  assert.match(body, /invokeCommand<DesktopRealtimeSnapshotDto>\("get_desktop_realtime_snapshot"\)/);
  assert.doesNotMatch(body, /"get_receive_status"/);
  assert.doesNotMatch(body, /"get_receive_session"/);
  assert.doesNotMatch(body, /"get_last_receive_report"/);
  assert.doesNotMatch(body, /"get_pending_receive_offer"/);
  assert.doesNotMatch(body, /"get_pending_pairing_request"/);
  assert.doesNotMatch(body, /"get_transfer_status"/);
  assert.doesNotMatch(body, /"get_discovery_status"/);
});

test("desktop realtime snapshot command is typed and registered", () => {
  assert.match(tauriSource, /"get_desktop_realtime_snapshot"/);
  assert.match(commandDtoSource, /pub struct DesktopRealtimeSnapshotDto/);
  assert.match(commandsSource, /pub fn get_desktop_realtime_snapshot/);
  assert.match(mainSource, /commands::get_desktop_realtime_snapshot/);
});

test("startup defers slow receive diagnostics and directory refresh work", () => {
  assert.match(appSource, /STARTUP_SLOW_REFRESH_DELAY_MS/);
  assert.match(appSource, /window\.setTimeout\(\(\) => \{/);
  assert.match(appSource, /refreshReceiveState\(\{ includeDiagnostics: true, includeDirectoryState: true \}\)/);

  const startupBlock = appSource.match(/useEffect\(\(\) => \{[\s\S]+?STARTUP_SLOW_REFRESH_DELAY_MS[\s\S]+?\}, \[\]\);/);
  assert.ok(startupBlock, "startup effect should defer slow refresh work");
  assert.doesNotMatch(startupBlock[0].split("window.setTimeout")[0], /includeDiagnostics: true/);
  assert.doesNotMatch(startupBlock[0].split("window.setTimeout")[0], /includeDirectoryState: true/);
});
