import React from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";

interface RailProps {
  inboxCount: number;
  onToggleInbox: () => void;
}

/**
 * 窄导航栏：主页（发送/接收）/ 设备 / 历史 / 设置 + 收件箱 + 主题。
 * macOS 交通灯浮在栏顶部（唯一保留 chrome 的地方）。
 */
export function Rail({ inboxCount, onToggleInbox }: RailProps) {
  const { mode, setMode, appearance, setAppearance } = useAppContext();

  const items: Array<{ id: string; icon: Parameters<typeof Icon>[0]["name"]; label: string; activeMode: "send" | "devices" | "transfers" | "settings" }> = [
    { id: "home", icon: "send", label: "主页", activeMode: "send" },
    { id: "devices", icon: "devices", label: "设备", activeMode: "devices" },
    { id: "transfers", icon: "clock", label: "历史", activeMode: "transfers" },
    { id: "settings", icon: "settings", label: "设置", activeMode: "settings" }
  ];

  return (
    <nav className="rail">
      <div className="rail-top">
        <span className="rail-logo" title="NekoDrop">
          <Icon name="paw" />
        </span>
      </div>
      <div className="rail-nav">
        {items.map((item) => (
          <button
            key={item.id}
            aria-label={item.label}
            className={`rail-btn ${mode === item.activeMode ? "is-active" : ""}`}
            onClick={() => setMode(item.activeMode)}
            title={item.label}
            type="button"
          >
            <Icon name={item.icon} />
            <span className="rail-label">{item.label}</span>
          </button>
        ))}
      </div>
      <div className="rail-bottom">
        <button
          aria-label="收件箱"
          className={`rail-btn ${inboxCount > 0 ? "has-badge" : ""}`}
          onClick={onToggleInbox}
          title="收件箱"
          type="button"
        >
          <Icon name="inbox" />
          {inboxCount > 0 && <span className="rail-dot" />}
        </button>
        <button
          aria-label="切换主题"
          className="rail-btn"
          onClick={() => setAppearance(appearance === "dark" ? "light" : "dark")}
          title={appearance === "dark" ? "切换至浅色" : "切换至深色"}
          type="button"
        >
          <Icon name={appearance === "dark" ? "sun" : "moon"} />
        </button>
      </div>
    </nav>
  );
}
