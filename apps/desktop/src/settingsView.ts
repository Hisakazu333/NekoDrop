import { platformLabel } from "./deviceDisplay.ts";
import type {
  AppSnapshot,
  DiscoveryStatusDto,
  ReceivePortDiagnosticsDto,
  ReceiveSessionDto
} from "./types.ts";

export interface SettingsViewModel {
  deviceName: string;
  canSaveDeviceName: boolean;
  platformLabel: string;
  deviceIdLabel: string | null;
  deviceKindLabel: string | null;
  fingerprintLabel: string | null;
  capabilitiesLabel: string | null;
  receiveStateLabel: string;
  receiveAddressLabel: string;
  connectionCodeLabel: string | null;
  defaultReceivePortLabel: string | null;
  discoveryEnabledLabel: string;
  discoveryLabel: string;
  discoveryDetailLabel: string | null;
  lanIpLabel: string | null;
  nearbyDeviceCountLabel: string | null;
  serviceTypeLabel: string | null;
  receiveDiagnosticsLabel: string | null;
  lanIpsLabel: string | null;
  trayLabel: string;
  canSaveReceiveDir: boolean;
  canSaveReceivePort: boolean;
  receiveConfigLocked: boolean;
  receiveDir: string;
  receivePolicyLabel: string;
  bindPort: string;
}

export function buildSettingsViewModel({
  snapshot,
  deviceNameInput,
  discoveryStatus,
  receiveDiagnostics,
  receiveSession,
  receiveDir,
  receivePolicy,
  bindPort
}: {
  snapshot: AppSnapshot | null;
  deviceNameInput?: string;
  discoveryStatus?: DiscoveryStatusDto | null;
  receiveDiagnostics?: ReceivePortDiagnosticsDto | null;
  receiveSession: ReceiveSessionDto | null;
  receiveDir: string;
  receivePolicy: string;
  bindPort: string;
}): SettingsViewModel {
  const deviceName = snapshot?.device_name?.trim() || "这台电脑";
  const nextDeviceName = deviceNameInput?.trim() ?? deviceName;
  const savedReceiveDir = snapshot?.receive_dir?.trim() ?? "";
  const nextReceiveDir = receiveDir.trim();
  const nextReceivePort = parseReceivePortValue(bindPort);
  const receiveConfigLocked = Boolean(receiveSession);
  const identity = snapshot?.device_identity;

  return {
    deviceName,
    canSaveDeviceName: nextDeviceName.length > 0 && nextDeviceName !== deviceName,
    platformLabel: snapshot ? platformLabel(snapshot.device_identity.platform) : "Unknown",
    deviceIdLabel: identity?.device_id ?? null,
    deviceKindLabel: identity?.device_kind ?? null,
    fingerprintLabel: identity?.public_key_fingerprint ?? null,
    capabilitiesLabel: identity?.capabilities?.length
      ? identity.capabilities.join(" · ")
      : null,
    receiveStateLabel: receiveSession ? "收件开启" : "收件关闭",
    receiveAddressLabel: receiveSession?.bind_addr ?? receiveDiagnostics?.bind_addr ?? "未监听",
    connectionCodeLabel: receiveSession?.connection_code ?? null,
    defaultReceivePortLabel:
      snapshot?.receive_port != null ? String(snapshot.receive_port) : null,
    discoveryEnabledLabel: snapshot?.discovery_enabled ? "配置已启用" : "配置已关闭",
    discoveryLabel: discoveryRuntimeLabel(discoveryStatus ?? null),
    discoveryDetailLabel: discoveryStatus?.message ?? null,
    lanIpLabel: discoveryStatus?.lan_ip ?? receiveDiagnostics?.advertised_host ?? null,
    nearbyDeviceCountLabel:
      discoveryStatus != null ? `${discoveryStatus.device_count} 台附近` : null,
    serviceTypeLabel: discoveryStatus?.service_type ?? null,
    receiveDiagnosticsLabel: receiveDiagnostics?.message ?? null,
    lanIpsLabel:
      receiveDiagnostics?.lan_ips?.length ? receiveDiagnostics.lan_ips.join(" · ") : null,
    trayLabel: snapshot?.tray_enabled ? "窗口菜单已启用" : "仅窗口标题",
    canSaveReceiveDir:
      Boolean(snapshot) &&
      !receiveConfigLocked &&
      nextReceiveDir.length > 0 &&
      nextReceiveDir !== savedReceiveDir,
    canSaveReceivePort:
      Boolean(snapshot) &&
      !receiveConfigLocked &&
      nextReceivePort !== null &&
      nextReceivePort !== snapshot?.receive_port,
    receiveConfigLocked,
    receiveDir,
    receivePolicyLabel: receivePolicyDisplayLabel(receivePolicy),
    bindPort
  };
}

export function receivePolicyDisplayLabel(value: string) {
  if (value === "block_all") return "阻止外部接收";
  return "接收前询问";
}

export function discoveryRuntimeLabel(status: DiscoveryStatusDto | null) {
  if (!status) return "未知";
  if (status.phase === "unavailable") return "不可用";
  if (status.advertised) return "已广播";
  if (status.phase === "active") return "扫描中";
  return "未广播";
}

export function parseReceivePortValue(value: string) {
  const trimmed = value.trim();
  if (!/^\d+$/.test(trimmed)) return null;
  const port = Number(trimmed);
  if (!Number.isInteger(port) || port < 1 || port > 65535) return null;
  return port;
}
