import type { ReceivePortDiagnosticsDto } from "./types";

export function hasReceiveDiagnosticsWarning(diagnostics: ReceivePortDiagnosticsDto | null) {
  return diagnostics?.phase === "no_lan_ip" || diagnostics?.phase === "invalid_bind_addr";
}

export function receiveDiagnosticsAdvice(diagnostics: ReceivePortDiagnosticsDto | null) {
  if (!diagnostics) return null;
  if (diagnostics.phase === "closed") {
    return "打开收件后会广播本机，也可复制连接码";
  }
  if (diagnostics.phase === "no_lan_ip") {
    return "没有局域网地址；检查 Wi-Fi/LAN、VPN 或热点隔离";
  }
  if (diagnostics.phase === "invalid_bind_addr") {
    return "监听地址异常；关闭收件后重新打开";
  }
  return null;
}
