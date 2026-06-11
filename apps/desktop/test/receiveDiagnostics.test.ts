import assert from "node:assert/strict";
import { test } from "node:test";

import { hasReceiveDiagnosticsWarning } from "../src/receiveDiagnostics.ts";
import type { ReceivePortDiagnosticsDto } from "../src/types.ts";

function diagnostics(phase: ReceivePortDiagnosticsDto["phase"]): ReceivePortDiagnosticsDto {
  return {
    phase,
    listening: phase !== "closed",
    bind_addr: "0.0.0.0:45821",
    advertised_host: phase === "listening" ? "192.168.1.12" : null,
    port: 45821,
    lan_ips: phase === "no_lan_ip" ? [] : ["192.168.1.12"],
    message: phase,
    checks: []
  };
}

test("flags receive diagnostics that need user attention", () => {
  assert.equal(hasReceiveDiagnosticsWarning(diagnostics("no_lan_ip")), true);
  assert.equal(hasReceiveDiagnosticsWarning(diagnostics("invalid_bind_addr")), true);
});

test("does not flag normal or closed receive diagnostics", () => {
  assert.equal(hasReceiveDiagnosticsWarning(diagnostics("listening")), false);
  assert.equal(hasReceiveDiagnosticsWarning(diagnostics("closed")), false);
  assert.equal(hasReceiveDiagnosticsWarning(null), false);
});
