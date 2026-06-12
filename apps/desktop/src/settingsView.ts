import { platformLabel } from "./deviceDisplay.ts";
import type { AppSnapshot, ReceiveSessionDto } from "./types.ts";

export interface SettingsViewModel {
  deviceName: string;
  canSaveDeviceName: boolean;
  platformLabel: string;
  fingerprintLabel: string | null;
  receiveStateLabel: string;
  receiveAddressLabel: string;
  receiveDir: string;
  receivePolicyLabel: string;
  bindPort: string;
}

export function buildSettingsViewModel({
  snapshot,
  deviceNameInput,
  receiveSession,
  receiveDir,
  receivePolicy,
  bindPort
}: {
  snapshot: AppSnapshot | null;
  deviceNameInput?: string;
  receiveSession: ReceiveSessionDto | null;
  receiveDir: string;
  receivePolicy: string;
  bindPort: string;
}): SettingsViewModel {
  const deviceName = snapshot?.device_name?.trim() || "这台电脑";
  const nextDeviceName = deviceNameInput?.trim() ?? deviceName;

  return {
    deviceName,
    canSaveDeviceName: nextDeviceName.length > 0 && nextDeviceName !== deviceName,
    platformLabel: snapshot ? platformLabel(snapshot.device_identity.platform) : "Unknown",
    fingerprintLabel: snapshot?.device_identity.public_key_fingerprint ?? null,
    receiveStateLabel: receiveSession ? "收件开启" : "收件关闭",
    receiveAddressLabel: receiveSession?.bind_addr ?? "未监听",
    receiveDir,
    receivePolicyLabel: receivePolicyDisplayLabel(receivePolicy),
    bindPort
  };
}

export function receivePolicyDisplayLabel(value: string) {
  if (value === "block_all") return "阻止外部接收";
  return "接收前询问";
}
