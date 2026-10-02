import React, { useState } from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";
import { formatBytes } from "../transferProgress";

const BUNDLE_TYPES = ["skill", "session", "workspace", "agent_profile", "config_snapshot"] as const;

/**
 * 发送页：居中提问 + 底部组合输入框（Codex 式）
 * Send page: centered question + bottom composer.
 */
export function SendView() {
  const {
    selectedPaths,
    plan,
    scanStatus,
    pickFiles,
    pickFolders,
    removePath,
    clearQueue,
    selectedDevice,
    connectionCode,
    setConnectionCode,
    connectionCodeOpen,
    setConnectionCodeOpen,
    sendCurrentTransfer,
    busy,
    dragActive,
    receiveSession,
    receiveStatus,
    createManualBundleForSend,
    chooseManualBundleSourceDir,
    manualBundleType,
    setManualBundleType,
    manualBundleDisplayName,
    setManualBundleDisplayName,
    manualBundleSourceApp,
    setManualBundleSourceApp,
    manualBundleSourcePath
  } = useAppContext();

  const [bundleFormOpen, setBundleFormOpen] = useState(false);
  const codeReady = connectionCode.trim().length > 0;
  const canSend = selectedPaths.length > 0 && !busy && (Boolean(selectedDevice) || codeReady);

  const targetChip = selectedDevice ? (
    <button
      className="meta-chip"
      onClick={() => setConnectionCodeOpen(false)}
      title={`发送至 ${selectedDevice.name}`}
      type="button"
    >
      <Icon name="laptop" />
      {selectedDevice.name}
    </button>
  ) : (
    <button
      className={`meta-chip ${connectionCodeOpen ? "is-emphasis" : ""}`}
      onClick={() => setConnectionCodeOpen(!connectionCodeOpen)}
      title="通过连接码发送"
      type="button"
    >
      <Icon name="link" />
      {connectionCodeOpen ? "连接码" : "选择目标"}
    </button>
  );

  return (
    <div className="send-view">
      <div className="send-hero">
        <h2>要发送什么？</h2>
        <p>把文件拖到下方，或点击选择；接收方确认后开始传输</p>
      </div>

      <div className="composer-zone">
        <div className="composer-meta">
          {targetChip}
          {selectedPaths.length > 0 && (
            <button className="meta-chip" onClick={clearQueue} title="清空队列" type="button">
              {selectedPaths.length} 项 · {plan ? formatBytes(plan.total_bytes) : ""}
              <span className="chip-x">✕</span>
            </button>
          )}
          {receiveSession && (
            <span className="meta-live" title={receiveStatus ?? "等待接收中"}>
              <i className="live-dot" />
              收件开启
            </span>
          )}
        </div>

        <div
          className={`composer ${dragActive ? "is-drop-active" : ""}`}
          onClick={() => {
            if (!connectionCodeOpen && selectedPaths.length === 0) pickFiles();
          }}
          role="presentation"
        >
          {connectionCodeOpen ? (
            <input
              className="code-input"
              onChange={(event) => setConnectionCode(event.target.value)}
              placeholder="粘贴接收端连接码，如 NEKO-XXXX-…"
              spellCheck={false}
              value={connectionCode}
            />
          ) : (
            <textarea
              onPointerDown={(event) => event.preventDefault()}
              placeholder={selectedPaths.length === 0 ? "把文件或文件夹拖到这里，或点击选择" : " "}
              readOnly
              rows={1}
              style={{ pointerEvents: "none", caretColor: "transparent" }}
            />
          )}

          {selectedPaths.length > 0 && (
            <div className="composer-queue">
              {selectedPaths.map((path: string) => (
                <div className="queue-line" key={path}>
                  <span className="queue-name" title={path}>
                    {path}
                  </span>
                  <button
                    aria-label="移除"
                    className="queue-x"
                    onClick={(event) => {
                      event.stopPropagation();
                      removePath(path);
                    }}
                    type="button"
                  >
                    ✕
                  </button>
                </div>
              ))}
              {scanStatus && <div className="queue-line">正在扫描…</div>}
            </div>
          )}

          {bundleFormOpen && (
            <div className="bundle-mini" onClick={(event) => event.stopPropagation()} role="presentation">
              <div className="bundle-mini-row">
                <span className="mini-label">资料包</span>
                <input
                  onChange={(event) => setManualBundleDisplayName(event.target.value)}
                  placeholder="名称"
                  value={manualBundleDisplayName}
                />
                <select
                  onChange={(event) => setManualBundleType(event.target.value)}
                  value={manualBundleType}
                >
                  {BUNDLE_TYPES.map((type) => (
                    <option key={type} value={type}>
                      {type}
                    </option>
                  ))}
                </select>
              </div>
              <div className="bundle-mini-row">
                <span className="mini-label">来源</span>
                <input
                  onChange={(event) => setManualBundleSourceApp(event.target.value)}
                  placeholder="来源应用"
                  value={manualBundleSourceApp}
                />
                <button className="text-btn" onClick={chooseManualBundleSourceDir} type="button">
                  {manualBundleSourcePath ? "已选择目录" : "选择目录"}
                </button>
                <button
                  className="text-btn is-primary"
                  disabled={!manualBundleSourcePath}
                  onClick={createManualBundleForSend}
                  type="button"
                >
                  创建
                </button>
              </div>
            </div>
          )}

          <div className="composer-actions" onClick={(event) => event.stopPropagation()} role="presentation">
            <button className="action-chip" onClick={pickFiles} type="button">
              <Icon name="file" /> 文件
            </button>
            <button className="action-chip" onClick={pickFolders} type="button">
              <Icon name="folder" /> 文件夹
            </button>
            <button
              className={`action-chip ${bundleFormOpen ? "is-emphasis" : ""}`}
              onClick={() => setBundleFormOpen(!bundleFormOpen)}
              type="button"
            >
              <Icon name="package" /> 资料包
            </button>
            <button
              aria-label="发送"
              className="send-button"
              disabled={!canSend}
              onClick={sendCurrentTransfer}
              title={selectedDevice ? `发送至 ${selectedDevice.name}` : codeReady ? "通过连接码发送" : "先选择目标"}
              type="button"
            >
              <Icon name="arrow-up" />
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
