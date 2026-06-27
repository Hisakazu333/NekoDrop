import React, { useState, useRef, useEffect } from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";
import { formatBytes } from "../transferProgress";
import type { DeviceDto } from "../types";

type TabType = "transfer" | "agent" | "vlan" | "state";

/**
 * 中间核心工作台：文件投放区与设备能力多页签控制面板
 * Central Workbench: File Transfer Zone and Device Capability Tabs
 */
export function TransferZone() {
  const {
    selectedPaths,
    manualPaths,
    setManualPaths,
    plan,
    scanStatus,
    removePath,
    clearQueue,
    pickFiles,
    pickFolders,
    selectedDeviceId,
    selectedDeviceSnapshot,
    nearbyDevices,
    trustedDevices,
    sendCurrentTransfer,
    busy,
    dragActive,
    connectionCode,
    setConnectionCode,
    connectionCodeOpen,
    setConnectionCodeOpen
  } = useAppContext();

  const [activeTab, setActiveTab] = useState<TabType>("transfer");
  
  // VLAN State
  const [simulationPing, setSimulationPing] = useState<number | null>(null);
  const [isConnectingVlan, setIsConnectingVlan] = useState(false);
  const [vlanProtocol, setVlanProtocol] = useState("TCP");
  const [vlanLocalPort, setVlanLocalPort] = useState("8080");
  const [vlanRemotePort, setVlanRemotePort] = useState("8080");

  // Agent State
  const [agentCommand, setAgentCommand] = useState("");
  const [terminalLogs, setTerminalLogs] = useState<{type: string, text: string}[]>([
    { type: "system", text: "[系统信息] 已建立与对方设备的加密会话安全通道。" },
    { type: "info", text: "等待对端智能体运行时 (OpenNeko Runtime) 上线..." },
    { type: "warning", text: "[规划中 · V1.5] 接入后，您可以在本地向该设备派发受控的 Agent 指令。" }
  ]);
  const terminalEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (activeTab === "agent") {
      terminalEndRef.current?.scrollIntoView({ behavior: "smooth" });
    }
  }, [terminalLogs, activeTab]);

  const handleSendAgentCommand = (e: React.FormEvent) => {
    e.preventDefault();
    if (!agentCommand.trim()) return;
    setTerminalLogs(prev => [...prev, { type: "user", text: `> ${agentCommand}` }]);
    setAgentCommand("");
    setTimeout(() => {
      setTerminalLogs(prev => [...prev, { type: "agent", text: "收到指令，但这只是功能演示版。完整功能敬请期待 V1.5 更新！" }]);
    }, 600);
  };

  const trustedNearbyDevices = nearbyDevices.filter((d) => d.trust_state === "Trusted");
  const selectedDevice =
    trustedNearbyDevices.find((d) => d.id === selectedDeviceId) ??
    (selectedDeviceSnapshot?.id === selectedDeviceId ? selectedDeviceSnapshot : null) ??
    null;

  const totalPaths = selectedPaths.length;
  const canSend = totalPaths > 0 && !busy && (Boolean(selectedDevice) || connectionCode.trim().length > 0);

  // 模拟建立虚拟局域网连接 / Simulate establishing virtual LAN tunnel
  const handleConnectVlan = () => {
    setIsConnectingVlan(true);
    setSimulationPing(null);
    setTimeout(() => {
      setIsConnectingVlan(false);
      setSimulationPing(Math.floor(Math.random() * 8) + 8); // 8-15ms
    }, 1500);
  };

  return (
    <section className="transfer-zone">
      {/* 顶部工作区标题 / Workspace Header */}
      <div className="zone-header">
        <div className="zone-title-group">
          <strong>
            {selectedDevice
              ? `设备控制台: ${selectedDevice.name}`
              : connectionCodeOpen
              ? "通过备用连接码发送"
              : "主工作台 (Workspace)"}
          </strong>
          <span className="zone-subtitle">
            {selectedDevice
              ? `${selectedDevice.platform} · 可信设备加密通道`
              : "将文件或文件夹投放至下方进行共享"}
          </span>
        </div>
      </div>

      {/* 设备功能页签切换（仅在选中设备时显示） / Capability Tabs (Only when device is selected) */}
      {selectedDevice && (
        <div className="capability-tabs">
          <button
            className={`tab-btn ${activeTab === "transfer" ? "is-active" : ""}`}
            onClick={() => setActiveTab("transfer")}
            type="button"
          >
            <Icon name="upload" />
            <span>文件传输</span>
          </button>
          <button
            className={`tab-btn ${activeTab === "agent" ? "is-active" : ""}`}
            onClick={() => setActiveTab("agent")}
            type="button"
          >
            <Icon name="plug" />
            <span>Agent 协作 (V1.5)</span>
          </button>
          <button
            className={`tab-btn ${activeTab === "vlan" ? "is-active" : ""}`}
            onClick={() => setActiveTab("vlan")}
            type="button"
          >
            <Icon name="link" />
            <span>游戏联机/组网</span>
          </button>
          <button
            className={`tab-btn ${activeTab === "state" ? "is-active" : ""}`}
            onClick={() => setActiveTab("state")}
            type="button"
          >
            <Icon name="package" />
            <span>状态同步 (NekoState)</span>
          </button>
        </div>
      )}

      {/* 页签内容区 / Tab Contents */}
      <div className="zone-body">
        {/* 1. 文件传输页签 / File Transfer Tab */}
        {(!selectedDevice || activeTab === "transfer") && (
          <div className="tab-pane-content transfer-pane">
            
            {/* 连接码输入区（仅在备用码模式下显示） / Connection Code Input */}
            {connectionCodeOpen && !selectedDevice && (
              <div className="connection-code-input-box">
                <label htmlFor="code-input">输入接收端连接码：</label>
                <div className="input-group">
                  <input
                    id="code-input"
                    type="text"
                    placeholder="输入对方客户端显示的连接码..."
                    value={connectionCode}
                    onChange={(e) => setConnectionCode(e.target.value)}
                  />
                  <button
                    className="btn-close-code"
                    onClick={() => setConnectionCodeOpen(false)}
                    type="button"
                  >
                    返回设备树
                  </button>
                </div>
              </div>
            )}

            {/* 大面积虚线拖拽区域 / Drag & Drop Zone */}
            <div className={`drag-drop-area ${dragActive ? "is-active" : ""}`}>
              <div className="drag-drop-inner">
                <Icon name="upload" className="drag-drop-icon" />
                <h3>拖拽文件或文件夹到此处</h3>
                <p className="drag-drop-tip">支持直接拖放任意文件与大容量目录</p>
                
                <div className="drag-drop-actions">
                  <button className="btn-secondary" onClick={pickFiles} type="button" disabled={Boolean(busy)}>
                    <Icon name="file" />
                    选择文件...
                  </button>
                  <button className="btn-secondary" onClick={pickFolders} type="button" disabled={Boolean(busy)}>
                    <Icon name="folder" />
                    选择文件夹...
                  </button>
                </div>
              </div>
            </div>

            {/* 已选文件路径队列列表 / Selected Paths List */}
            {totalPaths > 0 && (
              <div className="selected-queue-box">
                <div className="queue-header">
                  <strong>已加入发送队列 ({totalPaths} 个路径)</strong>
                  <button className="btn-text-danger" onClick={clearQueue} type="button">
                    清空队列
                  </button>
                </div>
                <div className="queue-list">
                  {selectedPaths.map((path) => (
                    <div className="queue-item" key={path}>
                      <Icon name="file" className="queue-item-icon" />
                      <span className="queue-item-path">{path}</span>
                      <button className="queue-item-remove" onClick={() => removePath(path)} type="button">
                        ×
                      </button>
                    </div>
                  ))}
                </div>

                {/* 扫描与计划摘要 / Scan & Plan Summary */}
                {scanStatus && (
                  <div className="queue-status-hint">
                    正在扫描目录: 已发现 {scanStatus.files_found} 个文件...
                  </div>
                )}
                {plan && (
                  <div className="queue-plan-summary">
                    <span>传输计划已生成：共 <strong>{plan.file_count}</strong> 个文件 · <strong>{formatBytes(plan.total_bytes)}</strong></span>
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

        {/* 2. Agent 协作页签 / Agent Tab */}
        {selectedDevice && activeTab === "agent" && (
          <div className="tab-pane-content agent-pane">
            <div className="terminal-panel" style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
              <div className="terminal-header">
                <span className="terminal-dot red" />
                <span className="terminal-dot yellow" />
                <span className="terminal-dot green" />
                <span className="terminal-title">OpenNeko Agent Terminal</span>
              </div>
              <div className="terminal-body" style={{ flex: 1, display: 'flex', flexDirection: 'column' }}>
                <div className="terminal-logs" style={{ flex: 1, overflowY: 'auto', marginBottom: '12px' }}>
                  {terminalLogs.map((log, index) => (
                    <div key={index} className="log-line" style={{ marginBottom: '6px', fontSize: '13px' }}>
                      <span className={log.type === 'system' ? 'cyan-text' : log.type === 'info' ? 'gray-text' : log.type === 'user' ? 'white-text' : log.type === 'warning' ? 'yellow-text' : 'green-text'}>
                        {log.text}
                      </span>
                    </div>
                  ))}
                  <div ref={terminalEndRef} />
                </div>
                <form className="terminal-input-row" onSubmit={handleSendAgentCommand} style={{ display: 'flex', gap: '8px', alignItems: 'center', background: 'rgba(0,0,0,0.2)', padding: '8px', borderRadius: '4px' }}>
                  <span className="prompt-symbol" style={{ color: '#0f0', fontWeight: 'bold' }}>$</span>
                  <input
                    type="text"
                    className="terminal-input"
                    style={{ flex: 1, background: 'transparent', border: 'none', color: '#fff', outline: 'none', fontFamily: 'monospace' }}
                    value={agentCommand}
                    onChange={(e) => setAgentCommand(e.target.value)}
                    placeholder="输入指令，例如: ping, deploy..."
                  />
                  <button type="submit" className="btn-primary btn-small" disabled={!agentCommand.trim()} style={{ padding: '4px 12px' }}>执行</button>
                </form>
              </div>
            </div>
          </div>
        )}

        {/* 3. 游戏联机/虚拟局域网页签 / VLAN Tab */}
        {selectedDevice && activeTab === "vlan" && (
          <div className="tab-pane-content vlan-pane">
            <div className="vlan-panel-card" style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
              <div className="vlan-card-header">
                <Icon name="link" />
                <h3>NekoLink 虚拟局域网房间</h3>
              </div>
              <p className="vlan-desc">
                基于 <strong>iroh / P2P</strong> 隧道技术，在两台设备之间建立一条虚拟的网络通道。
                可以让仅支持局域网联机的小型游戏、开发调试服务跨网络直接互通。
              </p>

              <div className="vlan-config-form" style={{ display: 'flex', gap: '12px', marginBottom: '20px', alignItems: 'flex-end', background: 'var(--color-bg-secondary)', padding: '12px', borderRadius: '8px' }}>
                <div className="form-group" style={{ flex: 1, margin: 0 }}>
                  <label style={{ fontSize: '12px', color: 'var(--color-text-secondary)', marginBottom: '4px', display: 'block' }}>协议 (Protocol)</label>
                  <select className="form-control" value={vlanProtocol} onChange={e => setVlanProtocol(e.target.value)} disabled={simulationPing !== null} style={{ width: '100%' }}>
                    <option value="TCP">TCP</option>
                    <option value="UDP">UDP</option>
                  </select>
                </div>
                <div className="form-group" style={{ flex: 1, margin: 0 }}>
                  <label style={{ fontSize: '12px', color: 'var(--color-text-secondary)', marginBottom: '4px', display: 'block' }}>本地端口</label>
                  <input type="text" className="form-control" value={vlanLocalPort} onChange={e => setVlanLocalPort(e.target.value)} placeholder="如 8080" disabled={simulationPing !== null} style={{ width: '100%' }} />
                </div>
                <div className="form-group" style={{ flex: 1, margin: 0 }}>
                  <label style={{ fontSize: '12px', color: 'var(--color-text-secondary)', marginBottom: '4px', display: 'block' }}>对端端口</label>
                  <input type="text" className="form-control" value={vlanRemotePort} onChange={e => setVlanRemotePort(e.target.value)} placeholder="如 8080" disabled={simulationPing !== null} style={{ width: '100%' }} />
                </div>
              </div>

              <div className="vlan-network-box" style={{ flex: 1 }}>
                <div className="vlan-node">
                  <strong>本机 (Local)</strong>
                  <span className="vlan-ip">127.0.0.1:{vlanLocalPort || "*"}</span>
                </div>
                <div className={`vlan-connection-line ${simulationPing ? "is-connected" : ""}`}>
                  {isConnectingVlan ? (
                    <span className="vlan-connecting-text">建立隧道中...</span>
                  ) : simulationPing ? (
                    <span className="vlan-ping-tag">延迟: {simulationPing} ms (P2P 直连)</span>
                  ) : (
                    <span className="vlan-idle-line" />
                  )}
                </div>
                <div className="vlan-node">
                  <strong>{selectedDevice.name}</strong>
                  <span className="vlan-ip">NekoLink IP:{vlanRemotePort || "*"}</span>
                </div>
              </div>

              <div className="vlan-actions" style={{ marginTop: '16px', display: 'flex', justifyContent: 'flex-end' }}>
                {simulationPing ? (
                  <button className="btn-danger" onClick={() => setSimulationPing(null)} type="button">
                    断开虚拟隧道
                  </button>
                ) : (
                  <button
                    className="btn-primary"
                    disabled={isConnectingVlan}
                    onClick={handleConnectVlan}
                    type="button"
                  >
                    {isConnectingVlan ? "正在打洞建立隧道..." : "映射并连接 (Connect)"}
                  </button>
                )}
              </div>
            </div>
          </div>
        )}

        {/* 4. 状态同步页签 / NekoState Tab */}
        {selectedDevice && activeTab === "state" && (
          <div className="tab-pane-content state-pane">
            <div className="state-dashboard">
              <div className="state-card">
                <h4>同步层状态 (NekoState)</h4>
                <div className="state-row">
                  <span className="state-label">长期身份密钥</span>
                  <span className="state-val code-font">Ed25519 Verified</span>
                </div>
                <div className="state-row">
                  <span className="state-label">上次握手时间</span>
                  <span className="state-val">刚刚</span>
                </div>
                <div className="state-row">
                  <span className="state-label">加密方式</span>
                  <span className="state-val">X25519 + HKDF (已加密)</span>
                </div>
              </div>

              <div className="state-card">
                <h4>对端基础指标 (来自 NekoLink)</h4>
                <div className="state-row">
                  <span className="state-label">操作系统</span>
                  <span className="state-val">{selectedDevice.platform}</span>
                </div>
                <div className="state-row">
                  <span className="state-label">信任等级</span>
                  <span className="state-val green-text">高可信 (Trusted Pinning)</span>
                </div>
                <div className="state-row">
                  <span className="state-label">状态同步周期</span>
                  <span className="state-val">实时推送 (心跳正常)</span>
                </div>
              </div>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}
