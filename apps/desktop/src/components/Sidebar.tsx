import React, { useState } from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";

interface SidebarProps {
  collapsed?: boolean;
  inboxCount: number;
  onToggleInbox: () => void;
}

/**
 * 侧栏：每个控件都有明确职能——
 · 主页芯片 → 发送页；图标行只放应用级动作（收件箱 / 外观 / 筛选）
 · 引导卡 → 按真实完成度推进（启动/发现/配对/首传），点击去设置补齐
 · 设备区 → 选择发送目标；工具行只放不重复的动作（连接码 / 帮助）
 · 底部胶囊 → 新传输 = 清空队列并复位输入（真动作，⌘N 同）
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
    clearQueue,
    clearSendQueue,
    appearance,
    setAppearance,
    dragActive
  } = useAppContext();

  const [filterOpen, setFilterOpen] = useState(false);
  const [filter, setFilter] = useState("");

  const trustedIds = new Set(trustedDevices.map((device) => device.device_id));
  // 备注名优先显示：id → alias 映射（附近设备也套用）
  const aliasById = new Map(
    trustedDevices
      .filter((device) => device.alias)
      .map((device) => [device.device_id, device.alias as string])
  );

  // 引导进度对应四件真事：启动 → 发现设备 → 配对 → 首次传输
  const setupSteps = [
    nearbyDevices.length > 0,
    trustedDevices.length > 0,
    transfers.length > 0
  ];
  const setupDone = 1 + setupSteps.filter(Boolean).length;
  const setupPercent = Math.round((setupDone / 4) * 100);

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

  // 新传输：清空文件队列与待发队列、关掉连接码、回到发送页（目标保留，方便连发）
  const newTransfer = () => {
    clearQueue();
    clearSendQueue();
    setConnectionCode("");
    setConnectionCodeOpen(false);
    setMode("send");
  };

  const openHelp = () => {
    try {
      window.open("https://github.com/Hisakazu333/NekoDrop/tree/main/docs", "_blank", "noopener");
    } catch {
      /* webview 拦截弹窗时静默忽略 / ignore when popups are blocked */
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
          aria-label="筛选设备"
          className={`icon-btn ${filterOpen ? "is-open" : ""}`}
          onClick={() => {
            setFilterOpen(!filterOpen);
            setFilter("");
          }}
          title="筛选设备"
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

      <button className="setup-card" onClick={() => setMode("settings")} type="button">
        <span className="setup-title">完成初始设置（{setupDone}/4）</span>
        <span className="setup-track">
          <i style={{ width: `${setupPercent}%` }} />
          <span className="setup-knob" style={{ left: `${setupPercent}%` }}>
            <Icon name="paw" />
          </span>
        </span>
      </button>

      <div className="side-scroll">
        <div className="side-label">{dragActive ? "设备 · 松手即发" : "设备"}</div>
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
              className={`side-row ${selectedDeviceId === device.id ? "is-selected" : ""} ${dragActive ? "is-drop-hint" : ""}`}
              data-device-drop-id={device.id}
              onClick={() => selectDevice(device.id)}
              title={`${device.name} · ${device.host} · 拖文件到这行直接发送`}
              type="button"
            >
              <Icon name="laptop" />
              <span className="row-name">{aliasById.get(device.id) ?? device.name}</span>
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
              title={`${device.device_name}（离线，打开后可直接发）`}
              type="button"
            >
              <Icon name="laptop" />
              <span className="row-name">{device.alias ?? device.device_name}</span>
              <span className="side-tag">已配对</span>
            </button>
          ))}
      </div>

      <div className="side-utility">
        <button className="side-row" onClick={openConnectionCode} type="button">
          <Icon name="link" />
          <span className="row-name">通过连接码发送</span>
        </button>
        <button className="side-row" onClick={openHelp} type="button">
          <Icon name="help" />
          <span className="row-name">帮助</span>
        </button>
      </div>

      <div className="side-foot">
        <button className="new-chat-pill" onClick={newTransfer} type="button">
          <Icon name="sparkle" />
          <span>新传输</span>
          <kbd>⌘N</kbd>
        </button>
        <button aria-label="新传输" className="compose-circle" onClick={newTransfer} title="新传输" type="button">
          <Icon name="compose" />
        </button>
      </div>
    </aside>
  );
}
