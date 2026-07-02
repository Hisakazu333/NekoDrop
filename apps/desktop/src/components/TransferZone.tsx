import React, { useState } from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";
import { formatBytes } from "../transferProgress";
import { platformBadge } from "../platformDisplay";
import type { IconName } from "./Icon";

type TabType = "transfer" | "agent" | "vlan" | "state";

interface ComingSoonCopy {
  mascot: string;
  title: string;
  desc: string;
}

// 未实现能力：诚实标注为“即将推出”，不展示任何假数据或假控件
// Unimplemented capabilities: honestly marked "coming soon", never fake data.
const COMING_SOON: Record<Exclude<TabType, "transfer">, ComingSoonCopy> = {
  agent: {
    mascot: "🤖",
    title: "Agent 协作",
    desc: "跨设备 Agent 指令通道会作为 NekoLink 的上层能力接入，走统一的加密 session 与 local bridge，而不是写死到桌面端。当前版本尚未开放。"
  },
  vlan: {
    mascot: "🎮",
    title: "游戏联机 / 组网",
    desc: "基于 iroh / relay 的 P2P 虚拟局域网会在 NekoLink transport 就绪后接入。当前主线仍是同局域网 TCP 传输，跨公网组网尚未开放。"
  },
  state: {
    mascot: "🔄",
    title: "状态同步 NekoState",
    desc: "session、workspace、skill、agent profile 的跨设备迁移会通过可校验的 bundle 进行。协议模型已在推进，自动同步入口尚未开放。"
  }
};

/**
 * 中间核心工作台：文件投放区与设备能力多页签控制面板
 * Central Workbench: File Transfer Zone and Device Capability Tabs
 */
export function TransferZone() {
  const {
    selectedPaths,
    manualPaths,
    plan,
    scanStatus,
    removePath,
    clearQueue,
    pickFiles,
    pickFolders,
    selectedDeviceId,
    selectedDeviceSnapshot,
    nearbyDevices,
    sendCurrentTransfer,
    busy,
    dragActive,
    connectionCode,
    setConnectionCode,
    connectionCodeOpen,
    setConnectionCodeOpen
  } = useAppContext();

  const [activeTab, setActiveTab] = useState<TabType>("transfer");

  const trustedNearbyDevices = nearbyDevices.filter((d) => d.trust_state === "Trusted");
  const selectedDevice =
    trustedNearbyDevices.find((d) => d.id === selectedDeviceId) ??
    (selectedDeviceSnapshot?.id === selectedDeviceId ? selectedDeviceSnapshot : null) ??
    null;

  const totalPaths = selectedPaths.length;
  const canSend = totalPaths > 0 && !busy && (Boolean(selectedDevice) || connectionCode.trim().length > 0);

  const tabs: { id: TabType; icon: IconName; label: string; soon: boolean }[] = [
    { id: "transfer", icon: "upload", label: "文件传输", soon: false },
    { id: "agent", icon: "plug", label: "Agent 协作", soon: true },
    { id: "vlan", icon: "link", label: "游戏联机 / 组网", soon: true },
    { id: "state", icon: "package", label: "状态同步", soon: true }
  ];

  return (
    <section className="transfer-zone">
      {/* 顶部工作区标题 / Workspace Header */}
      <div className="zone-header">
        <div className="zone-title-group">
          <strong>
            {selectedDevice
              ? `发送到 ${selectedDevice.name}`
              : connectionCodeOpen
              ? "通过连接码发送"
              : "主工作台"}
          </strong>
          <span className="zone-subtitle">
            {selectedDevice
              ? `${platformBadge(selectedDevice.platform).label} · 可信设备加密通道`
              : "把文件或文件夹丢到下面，选好设备就能发"}
          </span>
        </div>
      </div>

      {/* 设备功能页签切换 / Capability Tabs */}
      <div className="capability-tabs">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            className={`tab-btn ${activeTab === tab.id ? "is-active" : ""}`}
            onClick={() => setActiveTab(tab.id)}
            type="button"
          >
            <Icon name={tab.icon} />
            <span>{tab.label}</span>
            {tab.soon && <span className="tab-soon-dot" title="即将推出" />}
          </button>
        ))}
      </div>

      {/* 页签内容区 / Tab Contents */}
      <div className="zone-body">
        {/* 1. 文件传输页签 / File Transfer Tab */}
        {activeTab === "transfer" && (
          <div className="tab-pane-content transfer-pane">
            {/* 连接码输入区（仅在备用码模式下显示） / Connection Code Input */}
            {connectionCodeOpen && !selectedDevice && (
              <div className="connection-code-input-box">
                <label htmlFor="code-input">输入接收端连接码</label>
                <div className="input-group">
                  <input
                    id="code-input"
                    type="text"
                    placeholder="粘贴对方客户端显示的连接码..."
                    value={connectionCode}
                    onChange={(e) => setConnectionCode(e.target.value)}
                  />
                  <button
                    className="btn-close-code"
                    onClick={() => setConnectionCodeOpen(false)}
                    type="button"
                  >
                    返回设备列表
                  </button>
                </div>
              </div>
            )}

            {/* 大面积虚线拖拽区域 / Drag & Drop Zone */}
            <div className={`drag-drop-area ${dragActive ? "is-active" : ""}`}>
              <div className="drag-drop-inner">
                <div className="drag-drop-mascot">
                  <Icon name="paw" />
                </div>
                <h3>把文件或文件夹丢到这里</h3>
                <p className="drag-drop-tip">支持直接拖放任意文件与大容量目录</p>

                <div className="drag-drop-actions">
                  <button className="btn-secondary" onClick={pickFiles} type="button" disabled={Boolean(busy)}>
                    <Icon name="file" />
                    选择文件
                  </button>
                  <button className="btn-secondary" onClick={pickFolders} type="button" disabled={Boolean(busy)}>
                    <Icon name="folder" />
                    选择文件夹
                  </button>
                </div>
              </div>
            </div>

            {/* 已选文件路径队列列表 / Selected Paths List */}
            {totalPaths > 0 && (
              <div className="selected-queue-box">
                <div className="queue-header">
                  <strong>发送队列 · {totalPaths} 个路径</strong>
                  <button className="btn-text-danger" onClick={clearQueue} type="button">
                    清空
                  </button>
                </div>
                <div className="queue-list">
                  {selectedPaths.map((path) => (
                    <div className="queue-item" key={path}>
                      <Icon name="file" className="queue-item-icon" />
                      <span className="queue-item-path">{path}</span>
                      <button className="queue-item-remove" onClick={() => removePath(path)} type="button" title="移除">
                        <Icon name="x" />
                      </button>
                    </div>
                  ))}
                </div>

                {/* 扫描与计划摘要 / Scan & Plan Summary */}
                {scanStatus && (
                  <div className="queue-status-hint">
                    正在扫描目录：已发现 {scanStatus.files_found} 个文件...
                  </div>
                )}
                {plan && !scanStatus && (
                  <div className="queue-plan-summary">
                    <Icon name="check" />
                    <span>
                      传输计划已生成：<strong>{plan.file_count}</strong> 个文件 ·{" "}
                      <strong>{formatBytes(plan.total_bytes)}</strong>
                    </span>
                  </div>
                )}
              </div>
            )}

            {/* 底部发送控制栏 / Send Controls */}
            <div className="send-action-bar">
              <button
                className="btn-primary btn-large"
                disabled={!canSend}
                onClick={sendCurrentTransfer}
                type="button"
              >
                <Icon name="send" />
                <span>
                  {selectedDevice
                    ? `发送至 ${selectedDevice.name}`
                    : connectionCode.trim()
                    ? "通过连接码发送"
                    : "请选择接收目标"}
                </span>
              </button>
            </div>
          </div>
        )}

        {/* 2-4. 未实现能力：诚实占位 / Unimplemented: honest placeholder */}
        {activeTab !== "transfer" && (
          <div className="tab-pane-content">
            <div className="coming-soon">
              <span className="coming-soon-badge">
                <Icon name="sparkle" />
                即将推出
              </span>
              <div className="coming-soon-mascot">{COMING_SOON[activeTab].mascot}</div>
              <h3>{COMING_SOON[activeTab].title}</h3>
              <p>{COMING_SOON[activeTab].desc}</p>
              <div className="coming-soon-note">属于 NekoLink 后续版本 · 当前不影响文件传输</div>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}
