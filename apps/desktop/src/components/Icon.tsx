import React from "react";

// 定义图标名称类型 / Define the IconName type
export type IconName =
  | "appearance"
  | "arrow-up"
  | "clock"
  | "devices"
  | "file"
  | "folder"
  | "inbox"
  | "link"
  | "list"
  | "overview"
  | "package"
  | "plug"
  | "settings"
  | "send"
  | "shield"
  | "trash"
  | "upload";

interface IconProps {
  className?: string;
  name: IconName;
  style?: React.CSSProperties;
}

/**
 * 共享图标组件，根据名称渲染对应的 SVG 路径
 * Shared Icon component that renders the corresponding SVG path based on the name
 */
export function Icon({ className, name, style }: IconProps) {
  return (
    <svg
      aria-hidden="true"
      className={className ? `icon ${className}` : "icon"}
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      viewBox="0 0 24 24"
      style={{ width: "1em", height: "1em", display: "inline-block", verticalAlign: "middle", ...style }}
    >
      {name === "appearance" ? <path d="M12 8a4 4 0 1 1 0 8 4 4 0 0 1 0-8Zm0-5v3m0 12v3M4.9 4.9 7 7m10 10 2.1 2.1M3 12h3m12 0h3M4.9 19.1 7 17m10-10 2.1-2.1" /> : null}
      {name === "arrow-up" ? <path d="M12 19V5m0 0 6 6M12 5l-6 6" /> : null}
      {name === "clock" ? <path d="M12 6v6l4 2m5-2a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z" /> : null}
      {name === "devices" ? <path d="M7 8a4 4 0 1 1 8 0 4 4 0 0 1-8 0Zm-3 13a7 7 0 0 1 14 0M17 11a3 3 0 0 1 0 6m3-8a6 6 0 0 1 0 10" /> : null}
      {name === "file" ? <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8l-5-5Zm0 0v5h5" /> : null}
      {name === "folder" ? <path d="M3 7a2 2 0 0 1 2-2h5l2 2h7a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Z" /> : null}
      {name === "inbox" ? <path d="M4 4h16v11l-3 5H7l-3-5V4Zm0 11h5l2 2h2l2-2h5" /> : null}
      {name === "link" ? <path d="M10 13a5 5 0 0 0 7.07 0l2-2A5 5 0 0 0 12 4l-1.2 1.2M14 11a5 5 0 0 0-7.07 0l-2 2A5 5 0 0 0 12 20l1.2-1.2" /> : null}
      {name === "list" ? <path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01" /> : null}
      {name === "overview" ? <path d="M4 5h7v6H4V5Zm9 0h7v4h-7V5ZM4 13h7v6H4v-6Zm9-2h7v8h-7v-8Z" /> : null}
      {name === "package" ? <path d="m4 7 8-4 8 4-8 4-8-4Zm0 0v10l8 4m0-10v10m0-10 8-4m0 0v10l-8 4" /> : null}
      {name === "plug" ? <path d="M9 7V3m6 4V3M7 7h10v5a5 5 0 0 1-10 0V7Zm5 10v4" /> : null}
      {name === "settings" ? (
        <>
          <path d="M12 15.2a3.2 3.2 0 1 0 0-6.4 3.2 3.2 0 0 0 0 6.4Z" />
          <path d="M19.4 15a1.7 1.7 0 0 0 .34 1.88l.06.06a2.05 2.05 0 1 1-2.9 2.9l-.06-.06A1.7 1.7 0 0 0 15 19.4a1.7 1.7 0 0 0-1 .6 1.7 1.7 0 0 0-.4 1.1V21a2.05 2.05 0 0 1-4.1 0v-.1A1.7 1.7 0 0 0 8.1 19.2a1.7 1.7 0 0 0-1.88.34l-.06.06a2.05 2.05 0 1 1-2.9-2.9l.06-.06A1.7 1.7 0 0 0 3.6 15a1.7 1.7 0 0 0-.6-1 1.7 1.7 0 0 0-1.1-.4H2a2.05 2.05 0 0 1 0-4.1h.1A1.7 1.7 0 0 0 3.8 8.1a1.7 1.7 0 0 0-.34-1.88l-.06-.06a2.05 2.05 0 1 1 2.9-2.9l.06.06A1.7 1.7 0 0 0 8.1 3.8a1.7 1.7 0 0 0 1-.6 1.7 1.7 0 0 0 .4-1.1V2a2.05 2.05 0 0 1 4.1 0v.1A1.7 1.7 0 0 0 15 3.8a1.7 1.7 0 0 0 1.88-.34l.06-.06a2.05 2.05 0 1 1 2.9 2.9l-.06.06A1.7 1.7 0 0 0 19.4 8c.08.36.28.7.6 1 .3.27.7.43 1.1.43h.1a2.05 2.05 0 0 1 0 4.1h-.1a1.7 1.7 0 0 0-1.7 1.47Z" />
        </>
      ) : null}
      {name === "send" ? <path d="m4 12 16-8-8 16-2-7-6-1Z" /> : null}
      {name === "shield" ? <path d="M12 3l8 3v6c0 5-8 9-8 9s-8-4-8-9V6l8-3Zm-3 9 2 2 4-4" /> : null}
      {name === "trash" ? <path d="M4 7h16M9 7V4h6v3m-8 0 1 14h8l1-14" /> : null}
      {name === "upload" ? <path d="M12 16V6m0 0 5 5m-5-5-5 5M4 18h16" /> : null}
    </svg>
  );
}
