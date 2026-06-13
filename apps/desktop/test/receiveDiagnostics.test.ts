import assert from "node:assert/strict";
import { test } from "node:test";

import {
  hasReceiveDiagnosticsWarning,
  receiveDiagnosticsAdvice
} from "../src/receiveDiagnostics.ts";
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

test("suggests a short next step for receive diagnostics warnings", () => {
  assert.equal(receiveDiagnosticsAdvice(diagnostics("no_lan_ip")), "没有局域网地址；检查 Wi-Fi/LAN、VPN 或热点隔离");
  assert.equal(receiveDiagnosticsAdvice(diagnostics("invalid_bind_addr")), "监听地址异常；关闭收件后重新打开");
});

test("keeps normal receive diagnostics quiet", () => {
  assert.equal(receiveDiagnosticsAdvice(diagnostics("listening")), null);
  assert.equal(receiveDiagnosticsAdvice(null), null);
});

test("guides closed receive state without treating it as a warning", () => {
  assert.equal(hasReceiveDiagnosticsWarning(diagnostics("closed")), false);
  assert.equal(receiveDiagnosticsAdvice(diagnostics("closed")), "打开收件后会广播本机，也可复制连接码");
});
