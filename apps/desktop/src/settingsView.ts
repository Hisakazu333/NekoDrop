import { platformLabel } from "./deviceDisplay.ts";
import type { AppSnapshot, DiscoveryStatusDto, ReceiveSessionDto } from "./types.ts";

export interface SettingsViewModel {
  deviceName: string;
  canSaveDeviceName: boolean;
  platformLabel: string;
  fingerprintLabel: string | null;
  receiveStateLabel: string;
  receiveAddressLabel: string;
  discoveryLabel: string;
  trayLabel: string;
  canSaveReceiveDir: boolean;
  receiveConfigLocked: boolean;
  receiveDir: string;
  receivePolicyLabel: string;
  bindPort: string;
}

export function buildSettingsViewModel({
  snapshot,
  deviceNameInput,
  discoveryStatus,
  receiveSession,
  receiveDir,
  receivePolicy,
  bindPort
}: {
  snapshot: AppSnapshot | null;
  deviceNameInput?: string;
  discoveryStatus?: DiscoveryStatusDto | null;
  receiveSession: ReceiveSessionDto | null;
  receiveDir: string;
  receivePolicy: string;
  bindPort: string;
}): SettingsViewModel {
  const deviceName = snapshot?.device_name?.trim() || "这台电脑";
  const nextDeviceName = deviceNameInput?.trim() ?? deviceName;
  const savedReceiveDir = snapshot?.receive_dir?.trim() ?? "";
  const nextReceiveDir = receiveDir.trim();
  const receiveConfigLocked = Boolean(receiveSession);

  return {
    deviceName,
    canSaveDeviceName: nextDeviceName.length > 0 && nextDeviceName !== deviceName,
    platformLabel: snapshot ? platformLabel(snapshot.device_identity.platform) : "Unknown",
    fingerprintLabel: snapshot?.device_identity.public_key_fingerprint ?? null,
    receiveStateLabel: receiveSession ? "收件开启" : "收件关闭",
    receiveAddressLabel: receiveSession?.bind_addr ?? "未监听",
    discoveryLabel: discoveryRuntimeLabel(discoveryStatus ?? null),
    trayLabel: "基础窗口菜单",
    canSaveReceiveDir: Boolean(snapshot) && !receiveConfigLocked && nextReceiveDir.length > 0 && nextReceiveDir !== savedReceiveDir,
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
