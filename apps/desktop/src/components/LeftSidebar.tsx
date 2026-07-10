import React, { useState } from "react";
import { useAppContext } from "../context/AppContext";
import { buildDiscoveryCopy } from "../networkPermissionHints";
import { platformBadge } from "../platformDisplay";
import type { DeviceDto, TrustedDeviceDto } from "../types";
import { Icon } from "./Icon";

interface LeftSidebarProps {
  onToggleInbox?: () => void;
}

export function LeftSidebar({ onToggleInbox }: LeftSidebarProps) {
  const {
    snapshot,
    nearbyDevices,
    trustedDevices,
    selectedDeviceId,
    setSelectedDeviceId,
    setConnectionCodeOpen,
    setConnectionCode,
    requestPairing,
    mode,
    setMode,
    discoveryStatus
  } = useAppContext();

  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const isMac = typeof navigator !== "undefined" && navigator.userAgent.toLowerCase().includes("mac");
  const localPlatform = snapshot?.device_identity.platform ?? null;
  const discoveryCopy = buildDiscoveryCopy(discoveryStatus, nearbyDevices.length, localPlatform);

  const normalizedQuery = searchQuery.trim().toLowerCase();
  const filteredNearby = nearbyDevices.filter((device) =>
    device.name.toLowerCase().includes(normalizedQuery)
  );
  const filteredTrusted = trustedDevices.filter((device) =>
    device.device_name.toLowerCase().includes(normalizedQuery)
  );

  const handleSelectTrusted = (device: TrustedDeviceDto) => {
    setSelectedDeviceId(device.device_id);
    setConnectionCodeOpen(false);
    setConnectionCode("");
    setMode("send");
  };

  const handleSelectNearby = (device: DeviceDto) => {
    if (device.trust_state !== "Trusted") {
      requestPairing(device);
      return;
    }

    setSelectedDeviceId(device.id);
    setConnectionCodeOpen(false);
    setConnectionCode("");
    setMode("send");
  };

  return (
    <aside className="left-sidebar">
      <div className="sidebar-header" data-tauri-drag-region>
        {isMac && <div className="sidebar-mac-spacer" data-tauri-drag-region />}
        <div className="sidebar-controls">
          <button className="sidebar-icon-btn" title="收起侧边栏" type="button">
            <Icon name="panel-left" />
          </button>
          <button className="sidebar-icon-btn" disabled title="后退" type="button">
            <Icon name="arrow-left" />
          </button>
          <button className="sidebar-icon-btn" disabled title="前进" type="button">
            <Icon name="arrow-right" />
          </button>
        </div>
      </div>

      <div className="sidebar-brand-row">
        <button className="sidebar-brand" onClick={() => setMode("send")} type="button">
          <span>Neko</span>
          <span className="sidebar-brand-accent">Drop</span>
        </button>
        <button
          aria-expanded={searchOpen}
          className={searchOpen ? "sidebar-search-toggle is-active" : "sidebar-search-toggle"}
          onClick={() => setSearchOpen((current) => !current)}
          title="搜索设备"
          type="button"
        >
          <Icon name="search" />
        </button>
      </div>

      {searchOpen && (
        <div className="sidebar-search-panel">
          <Icon className="search-icon" name="search" />
          <input
            autoFocus
            onChange={(event) => setSearchQuery(event.target.value)}
            placeholder="搜索设备"
            type="search"
            value={searchQuery}
          />
          {searchQuery && (
            <button onClick={() => setSearchQuery("")} title="清除搜索" type="button">
              <Icon name="x" />
            </button>
          )}
        </div>
      )}

      <nav className="sidebar-menu" aria-label="主导航">
        <button
          className={mode === "send" ? "sidebar-menu-btn is-active" : "sidebar-menu-btn"}
          onClick={() => setMode("send")}
          type="button"
        >
          <Icon name="compose" />
          <span>新建传输</span>
        </button>
        <button
          className={mode === "devices" ? "sidebar-menu-btn is-active" : "sidebar-menu-btn"}
          onClick={() => setMode("devices")}
          type="button"
        >
          <Icon name="devices" />
          <span>设备</span>
        </button>
        <button
          className={mode === "transfers" ? "sidebar-menu-btn is-active" : "sidebar-menu-btn"}
          onClick={() => setMode("transfers")}
          type="button"
        >
          <Icon name="clock" />
          <span>传输记录</span>
        </button>
        <button className="sidebar-menu-btn" onClick={onToggleInbox} type="button">
          <Icon name="inbox" />
          <span>收件箱</span>
        </button>
      </nav>

      <div className="sidebar-scroll">
        <section className="sidebar-section">
          <div className="sidebar-section-title">
            <span>附近设备</span>
            {filteredNearby.length > 0 && <span>{filteredNearby.length}</span>}
          </div>
          <div className="sidebar-section-content">
            {filteredNearby.length > 0 ? (
              filteredNearby.map((device) => {
                const badge = platformBadge(device.platform);
                const isSelected = selectedDeviceId === device.id;
                const isTrusted = device.trust_state === "Trusted";

                return (
                  <button
                    className={isSelected ? "sidebar-device-item is-selected" : "sidebar-device-item"}
                    key={device.id}
                    onClick={() => handleSelectNearby(device)}
                    title={`${badge.label} · ${isTrusted ? "已信任" : "点击配对"}`}
                    type="button"
                  >
                    <span className="device-name-group">
                      <Icon name="monitor" />
                      <span>{device.name}</span>
                    </span>
                    <span className={isTrusted ? "status-dot is-trusted" : "status-dot is-online"} />
                  </button>
                );
              })
            ) : (
              <div className="tree-node-empty">{discoveryCopy.label}</div>
            )}
          </div>
        </section>

        <section className="sidebar-section">
          <div className="sidebar-section-title">
            <span>可信设备</span>
            {filteredTrusted.length > 0 && <span>{filteredTrusted.length}</span>}
          </div>
          <div className="sidebar-section-content">
            {filteredTrusted.length > 0 ? (
              filteredTrusted.map((device) => {
                const badge = platformBadge(device.platform);
                const isOnline = nearbyDevices.some((nearby) => nearby.id === device.device_id);
                const isSelected = selectedDeviceId === device.device_id;

                return (
                  <button
                    className={`sidebar-device-item ${isSelected ? "is-selected" : ""} ${!isOnline ? "is-offline" : ""}`}
                    key={device.device_id}
                    onClick={() => handleSelectTrusted(device)}
                    title={`${badge.label} · ${isOnline ? "在线" : "离线"}`}
                    type="button"
                  >
                    <span className="device-name-group">
                      <Icon name="monitor" />
                      <span>{device.device_name}</span>
                    </span>
                    <span className={`status-dot ${isOnline ? "is-online" : "is-offline"}`} />
                  </button>
                );
              })
            ) : (
              <div className="tree-node-empty">暂无可信设备</div>
            )}
          </div>
        </section>

        <section className="sidebar-section sidebar-tool-section">
          <div className="sidebar-section-title">工具</div>
          <button className="sidebar-device-item" onClick={() => setMode("settings")} type="button">
            <span className="device-name-group">
              <Icon name="link" />
              <span>本地网桥</span>
            </span>
          </button>
          <button
            className="sidebar-device-item"
            onClick={() => {
              setConnectionCodeOpen(true);
              setSelectedDeviceId(null);
              setMode("send");
            }}
            type="button"
          >
            <span className="device-name-group">
              <Icon name="key" />
              <span>使用连接码</span>
            </span>
          </button>
        </section>
      </div>

      <div className="sidebar-footer-actions">
        <button
          className={mode === "settings" ? "sidebar-footer-button is-active" : "sidebar-footer-button"}
          onClick={() => setMode("settings")}
          type="button"
        >
          <Icon name="settings" />
          <span>设置</span>
        </button>
      </div>
    </aside>
  );
}
