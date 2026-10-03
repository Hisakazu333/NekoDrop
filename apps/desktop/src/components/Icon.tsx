import React from "react";

// 统一图标系统：24×24 网格、1.7 描边、圆角端点，几何精确到 0.1
// Unified icon system on a 24×24 grid with 1.7 stroke and round caps.
export type IconName =
  | "appearance"
  | "arrow-up"
  | "check"
  | "clock"
  | "copy"
  | "devices"
  | "file"
  | "folder"
  | "inbox"
  | "key"
  | "laptop"
  | "link"
  | "list"
  | "moon"
  | "overview"
  | "package"
  | "plus"
  | "paw"
  | "plug"
  | "refresh"
  | "search"
  | "settings"
  | "send"
  | "shield"
  | "sparkle"
  | "sun"
  | "trash"
  | "upload"
  | "x";

interface IconProps {
  className?: string;
  name: IconName;
  style?: React.CSSProperties;
}

const ICONS: Record<IconName, React.ReactNode> = {
  appearance: (
    <>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 3v2M12 19v2M3 12h2M19 12h2M5.6 5.6l1.4 1.4M17 17l1.4 1.4M18.4 5.6 17 7M7 17l-1.4 1.4" />
    </>
  ),
  "arrow-up": <path d="M12 19V5M5.5 11.5 12 5l6.5 6.5" />,
  check: <path d="M4.5 12.5 9.5 17.5 19.5 7" />,
  clock: (
    <>
      <circle cx="12" cy="12" r="8.5" />
      <path d="M12 7.5V12l3 2" />
    </>
  ),
  copy: (
    <>
      <rect height="12" rx="2.5" width="12" x="8.5" y="8.5" />
      <path d="M15.5 5.5v-1a2 2 0 0 0-2-2h-9a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h1" />
    </>
  ),
  devices: (
    <>
      <rect height="9.5" rx="1.8" width="14" x="2.5" y="4" />
      <path d="M9.5 13.5v3M6.5 16.5h6" />
      <rect height="8" rx="1.2" width="4.5" x="17.5" y="10.5" />
    </>
  ),
  file: <path d="M13.5 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8.5L13.5 3Zm0 0v5.5H19" />,
  folder: (
    <path d="M3.5 7A2.5 2.5 0 0 1 6 4.5h3.2a2 2 0 0 1 1.6.8l1 1.7H18A2.5 2.5 0 0 1 20.5 9.5V17A2.5 2.5 0 0 1 18 19.5H6A2.5 2.5 0 0 1 3.5 17V7Z" />
  ),
  inbox: (
    <path d="M3.5 13.5 6 5.6A2 2 0 0 1 7.9 4h8.2a2 2 0 0 1 1.9 1.6l2.5 7.9M3.5 13.5h4l1.5 2.5h6l1.5-2.5h4M3.5 13.5V18a2 2 0 0 0 2 2h13a2 2 0 0 0 2-2v-4.5" />
  ),
  key: (
    <>
      <circle cx="7.5" cy="16.5" r="3.5" />
      <path d="M10.2 13.8 20 4M17.4 6.6l2.3 2.3M14.6 9.4l2.3 2.3" />
    </>
  ),
  laptop: (
    <>
      <rect height="10" rx="1.5" width="15" x="4.5" y="4.5" />
      <path d="M3 18.5h18M5.2 15h13.6l2.2 3.5H3L5.2 15Z" />
    </>
  ),
  link: (
    <path d="M9.8 13.2a4.2 4.2 0 0 0 5.94 0l2.86-2.86a4.2 4.2 0 0 0-5.94-5.94l-1.43 1.43M14.2 10.8a4.2 4.2 0 0 0-5.94 0l-2.86 2.86a4.2 4.2 0 0 0 5.94 5.94l1.43-1.43" />
  ),
  list: <path d="M8.5 6h12M8.5 12h12M8.5 18h12M3.8 6h.01M3.8 12h.01M3.8 18h.01" />,
  moon: <path d="M20.2 13.9A8.5 8.5 0 1 1 10.1 3.8a6.7 6.7 0 0 0 10.1 10.1Z" />,
  overview: (
    <>
      <rect height="8.5" rx="1.5" width="7" x="3.5" y="4" />
      <rect height="5" rx="1.5" width="7" x="13.5" y="4" />
      <rect height="5" rx="1.5" width="7" x="3.5" y="15" />
      <rect height="8.5" rx="1.5" width="7" x="13.5" y="11.5" />
    </>
  ),
  package: (
    <path d="M4 8l8-4.2L20 8v8.2L12 20.4 4 16.2V8Zm8 4.2L4 8m8 4.2L20 8m-8 4.2v8.2" />
  ),
  plus: <path d="M12 5v14M5 12h14" />,
  paw: (
    <>
      <path d="M12 13.8c1.9 0 4.6 1.5 4.6 3.9 0 1.6-1.2 2.7-2.8 2.7-.7 0-1.2-.35-1.8-.35s-1.1.35-1.8.35c-1.6 0-2.8-1.1-2.8-2.7 0-2.4 2.7-3.9 4.6-3.9Z" />
      <ellipse cx="6.1" cy="11" rx="1.5" ry="1.85" transform="rotate(-18 6.1 11)" />
      <ellipse cx="17.9" cy="11" rx="1.5" ry="1.85" transform="rotate(18 17.9 11)" />
      <ellipse cx="9.7" cy="7.4" rx="1.5" ry="1.95" transform="rotate(-8 9.7 7.4)" />
      <ellipse cx="14.3" cy="7.4" rx="1.5" ry="1.95" transform="rotate(8 14.3 7.4)" />
    </>
  ),
  plug: (
    <>
      <path d="M9 2.8v3.4M15 2.8v3.4M6.5 6.2h11v4.3a5.5 5.5 0 0 1-11 0V6.2ZM12 16v5.2" />
    </>
  ),
  refresh: (
    <>
      <path d="M19.8 12a7.8 7.8 0 1 1-2.29-5.51" />
      <path d="M17.5 2.8v3.7h3.7" />
    </>
  ),
  search: (
    <>
      <circle cx="11" cy="11" r="7.2" />
      <path d="M16.2 16.2 21 21" />
    </>
  ),
  settings: (
    <>
      <circle cx="12" cy="12" r="3.1" />
      <circle cx="12" cy="12" r="6.4" />
      <path d="M12 2.7v2.4M12 18.9v2.4M2.7 12h2.4M18.9 12h2.4M7.1 7.1l1.7 1.7M16.9 7.1l-1.7 1.7M7.1 16.9l1.7-1.7M16.9 16.9l-1.7-1.7" />
    </>
  ),
  send: (
    <>
      <path d="M20.5 3.5 3.8 10.2c-.8.3-.8 1.5.1 1.7l6.6 1.6 1.6 6.6c.2.9 1.4.9 1.7.1L20.5 3.5Z" />
      <path d="M20.5 3.5 10.5 13.5" />
    </>
  ),
  shield: (
    <>
      <path d="M12 3.2 19.5 6v5.8c0 4.8-7.5 8.6-7.5 8.6s-7.5-3.8-7.5-8.6V6L12 3.2Z" />
      <path d="m9 11.8 2.1 2.1 4-4" />
    </>
  ),
  sparkle: (
    <path d="M12 3.5c.6 3.9 2.6 5.9 6.5 6.5-3.9.6-5.9 2.6-6.5 6.5-.6-3.9-2.6-5.9-6.5-6.5 3.9-.6 5.9-2.6 6.5-6.5ZM18.6 15c.3 1.9 1.2 2.8 3.1 3.1-1.9.3-2.8 1.2-3.1 3.1-.3-1.9-1.2-2.8-3.1-3.1 1.9-.3 2.8-1.2 3.1-3.1Z" />
  ),
  sun: (
    <>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 3v2M12 19v2M3 12h2M19 12h2M5.6 5.6l1.4 1.4M17 17l1.4 1.4M18.4 5.6 17 7M7 17l-1.4 1.4" />
    </>
  ),
  trash: (
    <>
      <path d="M4.5 6.5h15M9.5 6.5V4.8c0-.7.6-1.3 1.3-1.3h2.4c.7 0 1.3.6 1.3 1.3v1.7" />
      <path d="M6.5 6.5l.8 12a2 2 0 0 0 2 1.9h5.4a2 2 0 0 0 2-1.9l.8-12" />
      <path d="M10 10.5v6M14 10.5v6" />
    </>
  ),
  upload: (
    <>
      <path d="M12 15.5V4.5M7.5 9 12 4.5 16.5 9" />
      <path d="M4.5 16.5v2a2 2 0 0 0 2 2h11a2 2 0 0 0 2-2v-2" />
    </>
  ),
  x: <path d="M6 6l12 12M18 6 6 18" />,
};

/**
 * 共享图标组件：统一 24 网格 / 1.7 描边 / 圆角端点
 * Shared icon component on a unified 24-grid system.
 */
export function Icon({ className, name, style }: IconProps) {
  return (
    <svg
      aria-hidden="true"
      className={className ? `icon ${className}` : "icon"}
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      viewBox="0 0 24 24"
      style={{ width: "1em", height: "1em", display: "inline-block", verticalAlign: "middle", ...style }}
    >
      {ICONS[name]}
    </svg>
  );
}
