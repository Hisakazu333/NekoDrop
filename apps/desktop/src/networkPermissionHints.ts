import type { DiscoveryStatusDto } from "./types";

export interface DiscoveryCopy {
  label: string;
  subtitle: string;
  emptyTitle: string;
  emptyBody: string;
  targetLabel: string;
  isError: boolean;
}

export type NetworkGuidancePlatform = string | null | undefined;

export function discoveryTroubleshootingHint(platform?: NetworkGuidancePlatform) {
  if (isWindows(platform)) return "确认同一局域网；Windows 允许专用网络；可用备用码";
  if (isMacos(platform)) return "确认同一局域网；macOS 允许本地网络；可用备用码";
  return "确认同一局域网；Windows 允许专用网络；macOS 允许本地网络；可用备用码";
}

export function unavailableDiscoveryHint(platform?: NetworkGuidancePlatform) {
  if (isWindows(platform)) return "检查 Windows 防火墙和专用网络权限；可用备用码";
  if (isMacos(platform)) return "检查 macOS 本地网络权限；可用备用码";
  return "检查 Windows 防火墙或 macOS 本地网络权限；可用备用码";
}

export function broadcastTroubleshootingHint(platform?: NetworkGuidancePlatform) {
  if (isWindows(platform)) return "允许 NekoDrop 访问 Windows 专用网络；可用备用码";
  if (isMacos(platform)) return "允许 NekoDrop 访问 macOS 本地网络；可用备用码";
  return "允许 NekoDrop 访问专用网络或本地网络；可用备用码";
}

export function buildDiscoveryCopy(
  status: DiscoveryStatusDto | null,
  deviceCount: number,
  platform?: NetworkGuidancePlatform
): DiscoveryCopy {
  if (!status) {
    return {
      label: "启动中",
      subtitle: "初始化",
      emptyTitle: "启动中",
      emptyBody: "正在准备自动发现",
      targetLabel: "启动中",
      isError: false
    };
  }

  if (status.phase === "unavailable") {
    return {
      label: "发现异常",
      subtitle: status.last_error ? "mDNS 异常" : "不可用",
      emptyTitle: "发现异常",
      emptyBody: unavailableDiscoveryHint(platform),
      targetLabel: "发现异常 · 备用码",
      isError: true
    };
  }

  if (!status.advertised) {
    const hasNetworkError = Boolean(status.last_error);
    return {
      label: hasNetworkError ? "广播异常" : "未广播",
      subtitle: hasNetworkError ? "检查网络" : "收件关闭",
      emptyTitle: hasNetworkError ? "广播异常" : "未广播",
      emptyBody: hasNetworkError ? broadcastTroubleshootingHint(platform) : "打开收件后会广播本机",
      targetLabel: hasNetworkError ? "广播异常 · 权限/网络" : "未广播 · 打开收件",
      isError: hasNetworkError
    };
  }

  if (deviceCount > 0) {
    return {
      label: `${deviceCount} 台在线`,
      subtitle: status.last_seen_seconds_ago == null ? "在线" : `${status.last_seen_seconds_ago}s 前`,
      emptyTitle: "",
      emptyBody: "",
      targetLabel: `${deviceCount} 台在线`,
      isError: false
    };
  }

  return {
    label: "扫描中",
    subtitle: "搜索中",
    emptyTitle: "无设备",
    emptyBody: discoveryTroubleshootingHint(platform),
    targetLabel: "扫描中 · 权限/同网段",
    isError: false
  };
}

function isWindows(platform?: NetworkGuidancePlatform) {
  return platform?.toLowerCase() === "windows";
}

function isMacos(platform?: NetworkGuidancePlatform) {
  const normalized = platform?.toLowerCase();
  return normalized === "macos" || normalized === "darwin";
}
