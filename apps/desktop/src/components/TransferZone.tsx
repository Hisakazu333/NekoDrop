import React, { useState } from "react";
import { bundleTypeLabel } from "../bundleState";
import { formatBytes, shouldShowActiveTransferBar } from "../transferProgress";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";

type SendMode = "file" | "bundle";

interface TransferZoneProps {
  inboxOpen?: boolean;
  inspectorOpen?: boolean;
  onToggleInbox?: () => void;
  onToggleInspector?: () => void;
}

function fileName(path: string): string {
  return path.replace(/\\/g, "/").split("/").filter(Boolean).pop() ?? path;
}

function formatSpeed(bytesPerSecond: number | null): string {
  if (!bytesPerSecond) return "-- KB/s";
  return `${formatBytes(bytesPerSecond)}/s`;
}

function formatEta(seconds: number | null): string {
  if (seconds === null) return "--";
  if (seconds < 60) return `${seconds} 秒`;
  return `${Math.floor(seconds / 60)} 分 ${seconds % 60} 秒`;
}

export function TransferZone({
  inboxOpen = false,
  inspectorOpen = false,
  onToggleInbox,
  onToggleInspector
}: TransferZoneProps) {
  const {
    selectedPaths,
    plan,
    scanStatus,
    manualBundleType,
    manualBundleSourcePath,
    manualBundleDisplayName,
    manualBundleSourceApp,
    createdManualBundle,
    setManualBundleType,
    setManualBundleDisplayName,
    setManualBundleSourceApp,
    chooseManualBundleSourceDir,
    createManualBundleForSend,
    removePath,
    clearQueue,
    pickFiles,
    pickFolders,
    selectedDeviceId,
    selectedDeviceSnapshot,
    setSelectedDeviceId,
    nearbyDevices,
    sendCurrentTransfer,
    busy,
    dragActive,
    connectionCode,
    setConnectionCode,
    connectionCodeOpen,
    setConnectionCodeOpen,
    transferStatus,
    transferMetrics,
    receiveSession,
    startReceive,
    stopReceive,
    localBridgePendingActions,
    stagedBundles,
    setMode
  } = useAppContext();

  const [sendMode, setSendMode] = useState<SendMode>("file");
  const trustedNearbyDevices = nearbyDevices.filter((device) => device.trust_state === "Trusted");
  const selectedDevice =
    trustedNearbyDevices.find((device) => device.id === selectedDeviceId) ??
    (selectedDeviceSnapshot?.id === selectedDeviceId ? selectedDeviceSnapshot : null) ??
    null;

  const totalPaths = selectedPaths.length;
  const canSend = totalPaths > 0 && !busy && (Boolean(selectedDevice) || connectionCode.trim().length > 0);
  const isActive = Boolean(transferStatus && shouldShowActiveTransferBar(transferStatus));
  const progressPercent =
    transferStatus && transferStatus.total_bytes > 0
      ? Math.min(100, Math.round((transferStatus.bytes_transferred / transferStatus.total_bytes) * 100))
      : 0;
  const pendingNotificationCount =
    localBridgePendingActions.length + stagedBundles.filter((bundle) => bundle.staging_status === "saved").length;

  const openConnectionCode = () => {
    setSelectedDeviceId(null);
    setConnectionCodeOpen(true);
  };

  const toggleReceiving = () => {
    if (receiveSession) {
      stopReceive();
    } else {
      startReceive();
    }
  };

  return (
    <section className="transfer-zone">
      <div className="zone-header" data-tauri-drag-region>
        <div className="zone-header-drag-space" data-tauri-drag-region />
        <div className="zone-toolbar">
          <label className="receiving-toggle-group" title={receiveSession ? "停止接收" : "开始接收"}>
            <span>接收</span>
            <span className="toggle-switch">
              <input
                checked={Boolean(receiveSession)}
                disabled={busy === "receive" || busy === "stop-receive"}
                onChange={toggleReceiving}
                type="checkbox"
              />
              <span className="toggle-slider" />
            </span>
          </label>
          <button
            className={`workspace-tool-btn ${inboxOpen ? "is-active" : ""}`}
            onClick={onToggleInbox}
            title="收件箱"
            type="button"
          >
            <Icon name="inbox" />
            {pendingNotificationCount > 0 && <span className="workspace-tool-badge" />}
          </button>
          <button
            className={`workspace-tool-btn ${inspectorOpen ? "is-active" : ""}`}
            onClick={onToggleInspector}
            title="连接详情"
            type="button"
          >
            <Icon name="panel-right" />
          </button>
        </div>
      </div>

      <div className="zone-body">
        {sendMode === "file" ? (
          <div className={`tab-pane-content transfer-pane codex-workspace ${dragActive ? "is-dragging" : ""}`}>
            <div className={`drag-drop-area ${dragActive ? "is-active" : ""}`}>
              <div className="workspace-intro">
                <div className="workspace-mark" aria-hidden="true">
                  <Icon name="devices" />
                </div>
                <h1>
                  {selectedDevice ? (
                    <>
                      发送到{" "}
                      <button className="workspace-target-link" onClick={() => setMode("devices")} type="button">
                        {selectedDevice.name}
                      </button>{" "}
                      什么？
                    </>
                  ) : connectionCodeOpen ? (
                    "通过连接码发送什么？"
                  ) : (
                    "想把什么发送到其他设备？"
                  )}
                </h1>

                <div className="quick-action-grid">
                  <button className="quick-action-card tone-blue" disabled={Boolean(busy)} onClick={pickFiles} type="button">
                    <Icon name="file" />
                    <span>发送文件</span>
                  </button>
                  <button className="quick-action-card tone-violet" disabled={Boolean(busy)} onClick={pickFolders} type="button">
                    <Icon name="folder" />
                    <span>发送文件夹</span>
                  </button>
                  <button className="quick-action-card tone-green" onClick={() => setSendMode("bundle")} type="button">
                    <Icon name="package" />
                    <span>创建资料包</span>
                  </button>
                  <button className="quick-action-card tone-orange" onClick={openConnectionCode} type="button">
                    <Icon name="key" />
                    <span>使用连接码</span>
                  </button>
                </div>
              </div>
            </div>

            {isActive && transferStatus && (
              <div className="transfer-live-strip">
                <div className="transfer-live-heading">
                  <span className="transfer-live-icon"><Icon name="file" /></span>
                  <div>
                    <strong>{transferStatus.root_name}</strong>
                    <span>{progressPercent}% · {formatSpeed(transferMetrics.speedBytesPerSecond)} · 剩余 {formatEta(transferMetrics.etaSeconds)}</span>
                  </div>
                  <Icon className="verified-shield-icon" name="shield" />
                </div>
                <div className="progress-bar-container">
                  <div className="progress-bar-fill" style={{ width: `${progressPercent}%` }} />
                </div>
              </div>
            )}

            <div className="bottom-action-bar">
              <div className="composer-context-row">
                <button onClick={() => setMode("devices")} title="选择目标设备" type="button">
                  <Icon name="monitor" />
                  <span>{selectedDevice?.name ?? (connectionCodeOpen ? "连接码模式" : "选择设备")}</span>
                </button>
                <span><Icon name="link" />本地网络</span>
                <span><Icon name="shield" />加密会话</span>
              </div>

              <div className="action-bar-inner">
                {totalPaths > 0 && (
                  <div className="composer-files">
                    {selectedPaths.slice(0, 4).map((path) => (
                      <span className="composer-file-chip" key={path} title={path}>
                        <Icon name="file" />
                        <span>{fileName(path)}</span>
                        <button onClick={() => removePath(path)} title="移除" type="button">
                          <Icon name="x" />
                        </button>
                      </span>
                    ))}
                    {totalPaths > 4 && <span className="composer-more-count">+{totalPaths - 4}</span>}
                    <button className="composer-clear-button" onClick={clearQueue} type="button">清空</button>
                  </div>
                )}

                <input
                  aria-label="连接码"
                  onChange={(event) => setConnectionCode(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" && canSend) sendCurrentTransfer();
                  }}
                  placeholder={connectionCodeOpen ? "粘贴接收端连接码" : "粘贴连接码或选择要发送的内容"}
                  type="text"
                  value={connectionCode}
                />

                {(scanStatus || plan) && (
                  <div className="composer-plan-line">
                    {scanStatus ? (
                      <span>正在扫描 · {scanStatus.files_found} 个文件</span>
                    ) : plan ? (
                      <span>{plan.file_count} 个文件 · {formatBytes(plan.total_bytes)}</span>
                    ) : null}
                  </div>
                )}

                <div className="composer-footer">
                  <div className="composer-footer-actions">
                    <button className="composer-icon-button" disabled={Boolean(busy)} onClick={pickFiles} title="添加文件" type="button">
                      <Icon name="plus" />
                    </button>
                    <button className="composer-text-button" disabled={Boolean(busy)} onClick={pickFolders} type="button">
                      <Icon name="folder" />
                      <span>文件夹</span>
                    </button>
                  </div>
                  <button
                    className={`btn-action-send ${canSend ? "can-submit" : ""}`}
                    disabled={!canSend}
                    onClick={sendCurrentTransfer}
                    title={selectedDevice ? `发送到 ${selectedDevice.name}` : "发送"}
                    type="button"
                  >
                    <Icon name="arrow-up" />
                  </button>
                </div>
              </div>
            </div>
          </div>
        ) : (
          <div className="tab-pane-content bundle-pane codex-workspace">
            <section className="manual-bundle-composer">
              <button className="bundle-back-button" onClick={() => setSendMode("file")} type="button">
                <Icon name="arrow-left" />
                <span>返回</span>
              </button>
              <div className="bundle-composer-header">
                <span className="bundle-heading-icon"><Icon name="package" /></span>
                <div>
                  <h1>创建资料包</h1>
                  <strong>资料包目录</strong>
                  <span>{manualBundleSourcePath || "尚未选择目录"}</span>
                </div>
              </div>

              <div className="bundle-composer-grid">
                <label>
                  <span>类型</span>
                  <select value={manualBundleType} onChange={(event) => setManualBundleType(event.target.value)}>
                    <option value="workspace">Workspace</option>
                    <option value="session">Session</option>
                    <option value="skill">Skill</option>
                    <option value="agent_profile">Agent profile</option>
                    <option value="config_snapshot">Config</option>
                  </select>
                </label>
                <label>
                  <span>名称</span>
                  <input
                    onChange={(event) => setManualBundleDisplayName(event.target.value)}
                    placeholder="资料包名称"
                    value={manualBundleDisplayName}
                  />
                </label>
                <label>
                  <span>来源</span>
                  <input
                    onChange={(event) => setManualBundleSourceApp(event.target.value)}
                    placeholder="NekoDrop"
                    value={manualBundleSourceApp}
                  />
                </label>
              </div>

              <div className="bundle-composer-actions">
                <button className="btn-secondary" disabled={busy === "pick-folders"} onClick={chooseManualBundleSourceDir} type="button">
                  <Icon name="folder" />
                  <span>选择目录</span>
                </button>
                <button
                  className="btn-primary"
                  disabled={!manualBundleSourcePath || busy === "scan"}
                  onClick={createManualBundleForSend}
                  type="button"
                >
                  <Icon name="package" />
                  <span>加入发送</span>
                </button>
              </div>

              {createdManualBundle && (
                <div className="bundle-created-summary">
                  <Icon name="check" />
                  <span>
                    {createdManualBundle.display_name} · {bundleTypeLabel(createdManualBundle.bundle_type)} ·{" "}
                    {createdManualBundle.file_count} 个文件 · {formatBytes(createdManualBundle.total_bytes)}
                  </span>
                </div>
              )}
            </section>
          </div>
        )}
      </div>
    </section>
  );
}
