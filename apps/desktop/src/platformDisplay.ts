import type { IconName } from "./components/Icon";

export interface PlatformBadge {
  icon: IconName;
  label: string;
  emoji: string;
}

/**
 * 统一平台展示：图标 + 文案 + emoji
 * Unified platform presentation used across device views.
 */
export function platformBadge(platform: string): PlatformBadge {
  const p = (platform || "").toLowerCase();
  if (p.includes("mac") || p.includes("darwin")) return { icon: "laptop", label: "macOS", emoji: "🖥️" };
  if (p.includes("win")) return { icon: "laptop", label: "Windows", emoji: "💻" };
  if (p.includes("ios") || p.includes("iphone") || p.includes("ipad")) return { icon: "devices", label: "iOS", emoji: "📱" };
  if (p.includes("android")) return { icon: "devices", label: "Android", emoji: "📱" };
  if (p.includes("linux")) return { icon: "laptop", label: "Linux", emoji: "🐧" };
  if (p.includes("harmony")) return { icon: "devices", label: "OpenHarmony", emoji: "📟" };
  return { icon: "laptop", label: platform || "设备", emoji: "💻" };
}
