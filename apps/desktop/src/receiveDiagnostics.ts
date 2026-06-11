import type { ReceivePortDiagnosticsDto } from "./types";

export function hasReceiveDiagnosticsWarning(diagnostics: ReceivePortDiagnosticsDto | null) {
  return diagnostics?.phase === "no_lan_ip" || diagnostics?.phase === "invalid_bind_addr";
}
