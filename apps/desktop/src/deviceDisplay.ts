import type { DeviceDto, TrustedDeviceDto } from "./types";

export interface NearbyDeviceViewModel {
  statusLabel: string;
  actionLabel: string;
  canPair: boolean;
}

export interface TrustedDeviceViewModel {
  detailLabel: string;
  presenceLabel: string;
  actionLabel: string;
}

export function buildNearbyDeviceViewModel(device: DeviceDto, selected: boolean): NearbyDeviceViewModel {
  if (device.trust_state === "Trusted") {
    return {
      statusLabel: "已信任",
      actionLabel: selected ? "已选" : "选择",
      canPair: false
    };
  }

  if (!device.public_key_fingerprint) {
    return {
      statusLabel: "等待设备身份",
      actionLabel: "不可配对",
      canPair: false
    };
  }

  return {
    statusLabel: trustStateLabel(device.trust_state),
    actionLabel: "配对",
    canPair: true
  };
}

export function buildTrustedDeviceViewModel(
  device: TrustedDeviceDto,
  nowMs: number,
  online: boolean
): TrustedDeviceViewModel {
  return {
    detailLabel: `${platformLabel(device.platform)} · ${device.host}:${device.port}`,
    presenceLabel: online ? "在线" : formatLastSeen(nowMs, device.last_seen_at_ms),
    actionLabel: online ? "选择" : "用历史地址发送"
  };
}

export function platformLabel(platform: string) {
  if (platform === "macos") return "macOS";
  if (platform === "windows") return "Windows";
  if (platform === "linux") return "Linux";
  if (platform === "openharmony") return "OpenHarmony";
  return platform;
}

export function trustStateLabel(trustState: string) {
  if (trustState === "Trusted") return "已信任";
  if (trustState === "Pairing") return "配对中";
  if (trustState === "Blocked") return "已阻止";
  if (trustState === "Local") return "本机";
  return "未配对";
}

function formatLastSeen(nowMs: number, lastSeenAtMs: number) {
  const elapsedSeconds = Math.max(0, Math.floor((nowMs - lastSeenAtMs) / 1000));
  if (elapsedSeconds < 60) return "刚刚";
  const elapsedMinutes = Math.floor(elapsedSeconds / 60);
  if (elapsedMinutes < 60) return `${elapsedMinutes} 分钟前`;
  const elapsedHours = Math.floor(elapsedMinutes / 60);
  if (elapsedHours < 24) return `${elapsedHours} 小时前`;
  const elapsedDays = Math.floor(elapsedHours / 24);
  return `${elapsedDays} 天前`;
}
