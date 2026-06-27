import React, { useState } from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";
import { buildDiscoveryCopy } from "../networkPermissionHints";
import type { DeviceDto, TrustedDeviceDto } from "../types";

/**
 * 平台标识转换函数 / Platform Label converter
 */
function getPlatformIcon(platform: string): string {
  const p = platform.toLowerCase();
  if (p.includes("mac") || p.includes("darwin")) return " Mac";
  if (p.includes("win")) return "⊞ Windows";
  if (p.includes("ios") || p.includes("iphone") || p.includes("ipad")) return "📱 iOS";
  if (p.includes("android")) return "🤖 Android";
  if (p.includes("linux")) return "🐧 Linux";
  return "💻 设备";
}

/**
 * 左侧导航与设备树组件
 * Left Sidebar with Activity Bar and Device Tree
 */
export function LeftSidebar() {
  const {
    nearbyDevices,
    trustedDevices,
    selectedDeviceId,
    setSelectedDeviceId,
    setConnectionCodeOpen,
    setConnectionCode,
    requestPairing,
    forgetTrustedDevice,
    mode,
    setMode,
    discoveryStatus
  } = useAppContext();

  // 控制折叠面板的展开/收起 / Collapsible panels state
  const [nearbyExpanded, setNearbyExpanded] = useState(true);
  const [trustedExpanded, setTrustedExpanded] = useState(true);
  const [searchQuery, setSearchQuery] = useState("");

  const discoveryCopy = buildDiscoveryCopy(discoveryStatus, nearbyDevices.length);

  // 过滤设备列表 / Filter devices based on search query
  const filteredNearby = nearbyDevices.filter((d) =>
    d.name.toLowerCase().includes(searchQuery.toLowerCase())
  );
  const filteredTrusted = trustedDevices.filter((d) =>
    d.device_name.toLowerCase().includes(searchQuery.toLowerCase())
  );

  const handleSelectTrusted = (device: TrustedDeviceDto) => {
    setSelectedDeviceId(device.device_id);
    setConnectionCodeOpen(false);
    setConnectionCode("");
    setMode("send");
  };

  const handleSelectNearby = (device: DeviceDto) => {
    if (device.trust_state !== "Trusted") {
      // 未配对设备触发配对请求 / Trigger pairing request for untrusted devices
      requestPairing(device);
    } else {
      setSelectedDeviceId(device.id);
      setConnectionCodeOpen(false);
      setConnectionCode("");
      setMode("send");
    }
  };

  return (
    <aside className="left-sidebar">
      {/* 极窄活动导航栏 / Vertical Activity Bar */}
      <div className="activity-bar">
        <div className="activity-bar-top">
          <button
            className={`activity-btn ${mode === "send" ? "is-active" : ""}`}
            onClick={() => setMode("send")}
            title="发送文件"
            type="button"
          >
            <Icon name="send" />
          </button>
          <button
            className={`activity-btn ${mode === "devices" ? "is-active" : ""}`}
            onClick={() => setMode("devices")}
            title="设备管理"
            type="button"
          >
            <Icon name="devices" />
          </button>
          <button
            className={`activity-btn ${mode === "transfers" ? "is-active" : ""}`}
            onClick={() => setMode("transfers")}
            title="传输历史"
            type="button"
          >
            <Icon name="clock" />
          </button>
        </div>
        <div className="activity-bar-bottom">
          <button
            className={`activity-btn ${mode === "settings" ? "is-active" : ""}`}
            onClick={() => setMode("settings")}
            title="系统设置"
            type="button"
          >
            <Icon name="settings" />
          </button>
        </div>
      </div>

      {/* 设备管理与发现列表树 / Device Tree Pane */}
      <div className="device-tree-pane">
        {/* 顶部搜索框 / Search Input */}
        <div className="sidebar-search">
          <div className="search-input-wrapper">
            <Icon name="settings" className="search-icon" style={{ transform: "rotate(45deg)" }} />
            <input
              type="text"
              placeholder="搜索设备..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
            />
          </div>
        </div>

        {/* 设备列表滚动区 / Scrollable List Area */}
        <div className="device-tree-scroll">
          
          {/* 1. 附近在线发现设备 / Nearby Devices */}
          <div className="tree-section">
            <button
              className="tree-section-header"
              onClick={() => setNearbyExpanded(!nearbyExpanded)}
              type="button"
            >
              <span className={`chevron ${nearbyExpanded ? "is-expanded" : ""}`}>▸</span>
              <strong>附近设备</strong>
              <span className="tree-badge">{filteredNearby.length}</span>
            </button>

            {nearbyExpanded && (
              <div className="tree-section-content">
                {filteredNearby.length > 0 ? (
                  filteredNearby.map((device) => {
                    const isTrusted = device.trust_state === "Trusted";
                    const isSelected = selectedDeviceId === device.id;
                    return (
                      <div
                        key={device.id}
                        className={`tree-node ${isSelected ? "is-selected" : ""}`}
                        onClick={() => handleSelectNearby(device)}
                      >
                        <span className="node-status-dot is-online" />
                        <div className="node-info">
                          <span className="node-name">{device.name}</span>
                          <span className="node-meta">
                            {getPlatformIcon(device.platform)} · {isTrusted ? "已信任" : "待配对"}
                          </span>
                        </div>
                        {!isTrusted && (
                          <button
                            className="node-action-btn"
                            onClick={(e) => {
                              e.stopPropagation();
                              requestPairing(device);
                            }}
                            type="button"
                          >
                            配对
                          </button>
                        )}
                      </div>
                    );
                  })
                ) : (
                  <div className="tree-node-empty">{discoveryCopy.label}</div>
                )}
              </div>
            )}
          </div>

          {/* 2. 已配对可信设备列表 / Trusted Devices */}
          <div className="tree-section">
            <button
              className="tree-section-header"
              onClick={() => setTrustedExpanded(!trustedExpanded)}
              type="button"
            >
              <span className={`chevron ${trustedExpanded ? "is-expanded" : ""}`}>▸</span>
              <strong>可信设备</strong>
              <span className="tree-badge">{filteredTrusted.length}</span>
            </button>

            {trustedExpanded && (
              <div className="tree-section-content">
                {filteredTrusted.length > 0 ? (
                  filteredTrusted.map((device) => {
                    const isOnline = nearbyDevices.some((n) => n.id === device.device_id);
                    const isSelected = selectedDeviceId === device.device_id;
                    return (
                      <div
                        key={device.device_id}
                        className={`tree-node ${isSelected ? "is-selected" : ""} ${!isOnline ? "is-offline" : ""}`}
                        onClick={() => handleSelectTrusted(device)}
                      >
                        <span className={`node-status-dot ${isOnline ? "is-online" : "is-offline"}`} />
                        <div className="node-info">
                          <span className="node-name">{device.device_name}</span>
                          <span className="node-meta">
                            {getPlatformIcon(device.platform)} · {isOnline ? "在线" : "离线"}
                          </span>
                        </div>
                        <Icon name="shield" className="node-trust-icon" />
                      </div>
                    );
                  })
                ) : (
                  <div className="tree-node-empty">暂无可信设备</div>
                )}
              </div>
            )}
          </div>
        </div>

        {/* 底部连接码兜底入口 / Bottom Fallback Connection */}
        <div className="sidebar-footer">
          <button
            className="btn-link-code"
            onClick={() => {
              setConnectionCodeOpen(true);
              setSelectedDeviceId(null);
              setMode("send");
            }}
            type="button"
          >
            <Icon name="link" />
            <span>用连接码连接</span>
          </button>
        </div>
      </div>
    </aside>
  );
}
