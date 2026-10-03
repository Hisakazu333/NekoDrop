import React, { useState } from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";
import { formatBytes } from "../transferProgress";

const BUNDLE_TYPES = ["skill", "session", "workspace", "agent_profile", "config_snapshot"] as const;

function hueFor(name: string) {
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = (hash * 31 + name.charCodeAt(i)) >>> 0;
  }
  return hash % 360;
}

/**
 * 发送页（1:1 Notion AI 首页版式）：居中头像 + 大标题 + 白卡输入框
 * + 卡下挂浅灰附件条（设备圆点）+ 四个快捷动作。
 * Send page replicating the reference layout.
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
    nearbyDevices,
    trustedDevices,
    setSelectedDeviceId,
    setMode,
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
  const [stripOpen, setStripOpen] = useState(true);
  const codeReady = connectionCode.trim().length > 0;
  const canSend = selectedPaths.length > 0 && !busy && (Boolean(selectedDevice) || codeReady);

  // 橙色目标动作：已选设备显示去向；点按跳设备页换目标（连接码模式则先收起）
  const targetLabel = connectionCodeOpen
    ? "改用设备直发"
    : selectedDevice
      ? `发送至 ${selectedDevice.name} · 更换`
      : "选择发送目标";
  const onTargetClick = () => {
    if (connectionCodeOpen) {
      setConnectionCodeOpen(false);
      return;
    }
    setMode("devices");
  };

  // 状态位只说真话：队列条目 / 接收中 / 待选择文件
  const noteText =
    plan && selectedPaths.length > 0
      ? `${selectedPaths.length} 项 · ${formatBytes(plan.total_bytes)}`
      : receiveSession
        ? "收件开启"
        : selectedDevice
          ? `待发往 ${selectedDevice.name}`
          : "待选择文件";

  const stripDevices = [
    ...nearbyDevices.map((device) => ({ id: device.id, name: device.name, online: true })),
    ...trustedDevices
      .filter((device) => !nearbyDevices.some((nearby) => nearby.id === device.device_id))
      .map((device) => ({ id: device.device_id, name: device.device_name, online: false }))
  ];

  const pickStripDevice = (deviceId: string) => {
    setSelectedDeviceId(deviceId);
    setConnectionCodeOpen(false);
  };

  return (
    <div className="send-view">
      <div className="send-hero">
        <div className="hero-avatar">
          <Icon name="mascot" />
        </div>
        <h2>要发送什么？</h2>
      </div>

      <div className="composer-zone">
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
              placeholder={selectedPaths.length === 0 ? "把文件拖到这里，或点击选择" : " "}
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

          <div className="composer-bar" onClick={(event) => event.stopPropagation()} role="presentation">
            <button
              aria-label="选择文件"
              className="bar-plus"
              onClick={pickFiles}
              title="选择文件"
              type="button"
            >
              <Icon name="plus" />
            </button>
            <button
              className="bar-target"
              onClick={onTargetClick}
              title={connectionCodeOpen ? "收起连接码，改选设备" : "去设备页选择或配对目标"}
              type="button"
            >
              {targetLabel}
            </button>
            <span
              className="bar-note"
              title={receiveSession ? receiveStatus ?? "等待接收中" : undefined}
            >
              {noteText}
            </span>
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

        {stripOpen && (
          <div className="composer-strip">
            <span className="strip-label">点设备直接发送</span>
            <div className="strip-dots">
              {stripDevices.length === 0 && <span className="strip-none">等待发现设备…</span>}
              {stripDevices.slice(0, 8).map((device) =>
                device.online ? (
                  <button
                    className="strip-dot"
                    key={device.id}
                    onClick={() => pickStripDevice(device.id)}
                    style={{ backgroundColor: `hsl(${hueFor(device.name)} 48% 52%)` }}
                    title={`${device.name} · 点按设为发送目标`}
                    type="button"
                  >
                    {device.name.slice(0, 1).toUpperCase()}
                  </button>
                ) : (
                  <span
                    className="strip-dot is-offline"
                    key={device.id}
                    style={{ backgroundColor: `hsl(${hueFor(device.name)} 48% 52%)` }}
                    title={`${device.name}（离线）`}
                  >
                    {device.name.slice(0, 1).toUpperCase()}
                  </span>
                )
              )}
            </div>
            <button
              aria-label="收起"
              className="strip-x"
              onClick={() => setStripOpen(false)}
              title="收起"
              type="button"
            >
              <Icon name="x" />
            </button>
          </div>
        )}

        <div className="quick-row">
          <button onClick={pickFiles} type="button">
            <Icon name="file" /> 选择文件
          </button>
          <button onClick={pickFolders} type="button">
            <Icon name="folder" /> 文件夹
          </button>
          <button
            className={bundleFormOpen ? "is-emphasis" : ""}
            onClick={() => setBundleFormOpen(!bundleFormOpen)}
            type="button"
          >
            <Icon name="package" /> {bundleFormOpen ? "收起资料包" : "资料包"}
          </button>
          <button
            className={connectionCodeOpen ? "is-emphasis" : ""}
            onClick={() => setConnectionCodeOpen(!connectionCodeOpen)}
            type="button"
          >
            <Icon name="link" /> {connectionCodeOpen ? "收起连接码" : "连接码"}
          </button>
        </div>
      </div>
    </div>
  );
}
