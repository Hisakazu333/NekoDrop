import React, { useState } from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";

interface SidebarProps {
  collapsed?: boolean;
  inboxCount: number;
  onToggleInbox: () => void;
}

/**
 * 侧栏（1:1 Notion 式）：主页芯片 + 图标行 + 设置进度卡 + 设备行 + 工具行 + 底部胶囊
 * Notion-style sidebar: home chip, icon row, setup card, device rows, footer pill.
 */
export function Sidebar({ collapsed = false, inboxCount, onToggleInbox }: SidebarProps) {
  const {
    mode,
    setMode,
    nearbyDevices,
    trustedDevices,
    transfers,
    selectedDeviceId,
    setSelectedDeviceId,
    setConnectionCodeOpen,
    setConnectionCode,
    appearance,
    setAppearance
  } = useAppContext();

  const [filterOpen, setFilterOpen] = useState(false);
  const [filter, setFilter] = useState("");

  const trustedIds = new Set(trustedDevices.map((device) => device.device_id));

  // 初始设置进度：装好即 25%，发现设备 / 完成配对 / 完成传输各 +25%
  const setupPercent =
    25 +
    (nearbyDevices.length > 0 ? 25 : 0) +
    (trustedDevices.length > 0 ? 25 : 0) +
    (transfers.length > 0 ? 25 : 0);

  const normalizedFilter = filter.trim().toLowerCase();
  const matches = (name: string) =>
    !filterOpen || normalizedFilter.length === 0 || name.toLowerCase().includes(normalizedFilter);

  const selectDevice = (deviceId: string) => {
    if (selectedDeviceId === deviceId) {
      setMode("send");
      return;
    }
    setSelectedDeviceId(deviceId);
    setConnectionCodeOpen(false);
    setMode("send");
  };

  const openConnectionCode = () => {
    setConnectionCode("");
    setConnectionCodeOpen(true);
    setMode("send");
  };

  const openHelp = () => {
    try {
      window.open("https://github.com/Hisakazu333/NekoDrop/tree/main/docs", "_blank", "noopener");
    } catch {
      /* 桌面端无默认浏览器句柄时静默忽略 / ignore when webview blocks popups */
    }
  };

  return (
    <aside className={`sidebar ${collapsed ? "is-collapsed" : ""}`}>
      <div className="side-top">
        <button
          className={`home-chip ${mode === "send" ? "is-active" : ""}`}
          onClick={() => setMode("send")}
          type="button"
        >
          <Icon name="home" />
          <span>主页</span>
        </button>
        <button
          aria-label="收件箱"
          className={`icon-btn ${inboxCount > 0 ? "has-badge" : ""}`}
          onClick={onToggleInbox}
          title="收件箱"
          type="button"
        >
          <Icon name="inbox" />
          {inboxCount > 0 && <span className="dot-badge" />}
        </button>
        <button aria-label="设备" className="icon-btn" onClick={() => setMode("devices")} title="设备" type="button">
          <Icon name="devices" />
        </button>
        <button aria-label="历史" className="icon-btn" onClick={() => setMode("transfers")} title="历史" type="button">
          <Icon name="clock" />
        </button>
        <button
          aria-label="切换主题"
          className="icon-btn"
          onClick={() => setAppearance(appearance === "dark" ? "light" : "dark")}
          title={appearance === "dark" ? "切换至浅色" : "切换至深色"}
          type="button"
        >
          <Icon name={appearance === "dark" ? "sun" : "moon"} />
        </button>
        <span className="spacer" />
        <button
          aria-label="搜索设备"
          className={`icon-btn ${filterOpen ? "is-open" : ""}`}
          onClick={() => {
            setFilterOpen(!filterOpen);
            setFilter("");
          }}
          title="搜索设备"
          type="button"
        >
          <Icon name="search" />
        </button>
      </div>

      {filterOpen && (
        <div className="side-filter">
          <input
            autoFocus
            onChange={(event) => setFilter(event.target.value)}
            placeholder="筛选设备…"
            value={filter}
          />
        </div>
      )}

      <button className="setup-card" onClick={() => setMode("settings")} title="查看设置" type="button">
        <span className="setup-title">设置你的工作空间</span>
        <span className="setup-track">
          <i style={{ width: `${setupPercent}%` }} />
          <span className="setup-knob" style={{ left: `${setupPercent}%` }}>
            <Icon name="paw" />
          </span>
        </span>
      </button>

      <div className="side-scroll">
        <div className="side-label">设备</div>
        {nearbyDevices.length === 0 && !filterOpen && (
          <div className="side-empty">正在发现附近设备…</div>
        )}
        {filterOpen && nearbyDevices.length === 0 && trustedDevices.length === 0 && (
          <div className="side-empty">没有设备可以筛选。</div>
        )}
        {nearbyDevices
          .filter((device) => matches(device.name))
          .map((device) => (
            <button
              key={device.id}
              className={`side-row ${selectedDeviceId === device.id ? "is-selected" : ""}`}
              onClick={() => selectDevice(device.id)}
              title={`${device.name} · ${device.host}`}
              type="button"
            >
              <Icon name="laptop" />
              <span className="row-name">{device.name}</span>
              {trustedIds.has(device.id) && <span className="side-tag">已配对</span>}
            </button>
          ))}
        {trustedDevices
          .filter(
            (device) =>
              !nearbyDevices.some((nearby) => nearby.id === device.device_id) &&
              matches(device.device_name)
          )
          .map((device) => (
            <button
              key={device.device_id}
              className="side-row is-offline"
              onClick={() => selectDevice(device.device_id)}
              title={`${device.device_name}（离线）`}
              type="button"
            >
              <Icon name="laptop" />
              <span className="row-name">{device.device_name}</span>
              <span className="side-tag">已配对</span>
            </button>
          ))}
      </div>

      <div className="side-utility">
        <button className="side-row" onClick={openConnectionCode} type="button">
          <Icon name="link" />
          <span className="row-name">通过连接码发送</span>
        </button>
        <button className="side-row" onClick={() => setMode("transfers")} type="button">
          <Icon name="clock" />
          <span className="row-name">历史</span>
        </button>
        <button className="side-row" onClick={openHelp} type="button">
          <span className="row-icon-wrap">
            <Icon name="help" />
            <span className="dot" />
          </span>
          <span className="row-name">帮助</span>
        </button>
        <button className="side-row" onClick={() => setMode("settings")} type="button">
          <Icon name="settings" />
          <span className="row-name">设置</span>
        </button>
      </div>

      <div className="side-foot">
        <button className="new-chat-pill" onClick={() => setMode("send")} type="button">
          <Icon name="sparkle" />
          <span>新传输</span>
          <kbd>⌘N</kbd>
        </button>
        <button aria-label="通过连接码发送" className="compose-circle" onClick={openConnectionCode} title="通过连接码发送" type="button">
          <Icon name="compose" />
        </button>
      </div>
    </aside>
  );
}
