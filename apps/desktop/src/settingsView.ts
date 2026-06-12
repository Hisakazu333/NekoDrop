import { platformLabel } from "./deviceDisplay.ts";
import type { AppSnapshot, ReceiveSessionDto } from "./types.ts";

export interface SettingsViewModel {
  deviceName: string;
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
  receiveSession,
  receiveDir,
  receivePolicy,
  bindPort
}: {
  snapshot: AppSnapshot | null;
  receiveSession: ReceiveSessionDto | null;
  receiveDir: string;
  receivePolicy: string;
  bindPort: string;
}): SettingsViewModel {
  return {
    deviceName: snapshot?.device_name?.trim() || "这台电脑",
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
