import React from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";

interface SidebarProps {
  inboxCount: number;
  onToggleInbox: () => void;
}

/**
 * 左侧文字导航栏（Codex 式：无底色、无图标栏，选中只是浅灰一行）
 * Text-only navigation sidebar.
 */
export function Sidebar({ inboxCount, onToggleInbox }: SidebarProps) {
  const {
    mode,
    setMode,
    nearbyDevices,
    trustedDevices,
    transfers,
    selectedDeviceId,
    setSelectedDeviceId,
    snapshot,
    appearance,
    setAppearance,
    setConnectionCodeOpen,
    setConnectionCode
  } = useAppContext();

  const trustedIds = new Set(trustedDevices.map((device) => device.device_id));
  const deviceName = snapshot?.device_name ?? "本机";
  const fingerprint = snapshot?.device_identity.public_key_fingerprint ?? "";

  const navItems: Array<{ id: "send" | "devices" | "transfers" | "settings"; icon:Parameters<typeof Icon>[0]["name"]; label: string; count?: number }> = [
    { id: "send", icon: "send", label: "发送" },
    { id: "devices", icon: "devices", label: "设备", count: nearbyDevices.length || undefined },
    { id: "transfers", icon: "clock", label: "历史", count: transfers.length || undefined },
    { id: "settings", icon: "settings", label: "设置" }
  ];

  const selectDevice = (deviceId: string) => {
    if (selectedDeviceId === deviceId) return;
    setSelectedDeviceId(deviceId);
    setConnectionCodeOpen(false);
    setMode("send");
  };

  return (
    <aside className="sidebar">
      <div className="sidebar-top">
        <span style={{ color: "var(--text)", fontSize: 16, display: "inline-flex" }}>
          <Icon name="paw" />
        </span>
        <strong className="sidebar-title">NekoDrop</strong>
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
        <button
          aria-label="切换主题"
          className="icon-btn"
          onClick={() => setAppearance(appearance === "dark" ? "light" : "dark")}
          title={appearance === "dark" ? "切换至浅色" : "切换至深色"}
          type="button"
        >
          <Icon name={appearance === "dark" ? "sun" : "moon"} />
        </button>
      </div>

      <div className="sidebar-scroll">
        <nav>
          {navItems.map((item) => (
            <button
              key={item.id}
              className={`nav-item ${mode === item.id ? "is-active" : ""}`}
              onClick={() => setMode(item.id)}
              type="button"
            >
              <Icon name={item.icon} />
              <span>{item.label}</span>
              {item.count != null && <span className="nav-count">{item.count}</span>}
            </button>
          ))}
        </nav>

        <div className="nav-section-label">设备</div>
        {nearbyDevices.length === 0 && (
          <div className="sidebar-empty">正在发现附近设备…</div>
        )}
        {nearbyDevices.map((device) => (
          <button
            key={device.id}
            className={`device-row ${selectedDeviceId === device.id ? "is-selected" : ""}`}
            onClick={() => selectDevice(device.id)}
            title={`${device.name} · ${device.host}`}
            type="button"
          >
            <span className={`status-dot is-online`} />
            <span className="device-row-name">{device.name}</span>
            <span className="device-row-tag">{trustedIds.has(device.id) ? "已配对" : ""}</span>
          </button>
        ))}
        {trustedDevices
          .filter((device) => !nearbyDevices.some((nearby) => nearby.id === device.device_id))
          .map((device) => (
            <button
              key={device.device_id}
              className="device-row is-offline"
              onClick={() => selectDevice(device.device_id)}
              title={`${device.device_name}（离线）`}
              type="button"
            >
              <span className="status-dot is-offline" />
              <span className="device-row-name">{device.device_name}</span>
              <span className="device-row-tag">已配对</span>
            </button>
          ))}

        <button
          className="device-row"
          onClick={() => {
            setConnectionCode("");
            setConnectionCodeOpen(true);
            setMode("send");
          }}
          type="button"
        >
          <Icon name="link" />
          <span className="device-row-name">通过连接码发送</span>
        </button>
      </div>

      <div className="sidebar-footer">
        <div className="footer-device">
          <span className="footer-avatar">
            <Icon name="paw" />
          </span>
          <div className="footer-device-info">
            <strong title={deviceName}>{deviceName}</strong>
            <span title={fingerprint}>{fingerprint.slice(0, 24)}…</span>
          </div>
          <button className="icon-btn" onClick={() => setMode("settings")} title="本机设置" type="button">
            <Icon name="settings" />
          </button>
        </div>
      </div>
    </aside>
  );
}
