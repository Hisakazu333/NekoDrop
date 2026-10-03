import React from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";
import { copyTextToClipboard } from "../context/helpers";

const RECEIVE_POLICY_OPTIONS = [
  { value: "always_ask", label: "每次询问" },
  { value: "block_all", label: "全部拒绝" }
] as const;

/**
 * 设置页：表单行（无卡片），本地桥简化为状态行 + 操作
 * Settings page: form rows, no cards.
 */
export function SettingsView() {
  const {
    snapshot,
    receiveDir,
    chooseReceiveDir,
    bindPort,
    setBindPort,
    saveReceiveDir,
    saveReceivePort,
    receivePolicy,
    updateReceivePolicy,
    deviceNameInput,
    setDeviceNameInput,
    saveDeviceName,
    appearance,
    setAppearance,
    localBridgeStatus,
    runLocalBridgeSelfCheck,
    localBridgeAuthorizationCode,
    setLocalBridgeAuthorizationCode,
    confirmLocalBridgeAuthorization,
    localBridgePendingActions,
    localBridgeAuthorizations,
    revokeLocalBridgeAuthorization,
    pruneLocalBridgeAuthorizations,
    autoCopyTextSnippets,
    setTextSnippetAutoCopy,
    sendLimitInput,
    setSendLimitInput,
    saveSendLimit,
    updateOrganizeByDevice,
    updateIrohReceiveMode,
    updateInfo,
    receiveSession,
    setToast,
    setError,
    setMode,
    busy
  } = useAppContext();

  return (
    <div className="page">
      <div className="page-header">
        <h2>设置</h2>
        <p>接收、本机身份与本地桥</p>
      </div>

      <div className="page-section">
        <div className="page-section-title">接收</div>
        {receiveSession && (
          <>
            <div className="form-row">
              <div className="form-label">
                局域网连接码<small>同网设备输入此码发送</small>
              </div>
              <span className="form-value" title={receiveSession.connection_code}>
                {receiveSession.connection_code}
              </span>
              <button
                className="btn-mini"
                onClick={() =>
                  void copyTextToClipboard(receiveSession.connection_code)
                    .then(() => setToast?.("连接码已复制"))
                    .catch(() => setError?.("剪贴板不可用"))
                }
                type="button"
              >
                复制
              </button>
            </div>
            {receiveSession.irohConnectionCode && (
              <div className="form-row">
                <div className="form-label">
                  跨网连接码<small>不同网络也能发到这（iroh）</small>
                </div>
                <span className="form-value" title={receiveSession.irohConnectionCode}>
                  {receiveSession.irohConnectionCode}
                </span>
                <button
                  className="btn-mini"
                  onClick={() =>
                    void copyTextToClipboard(receiveSession.irohConnectionCode ?? "")
                      .then(() => setToast?.("跨网连接码已复制"))
                      .catch(() => setError?.("剪贴板不可用"))
                  }
                  type="button"
                >
                  复制
                </button>
              </div>
            )}
          </>
        )}
        <div className="form-row">
          <div className="form-label">
            跨网收件<small>中继模式经 n0 公共中继转发密文，元数据对中继可见</small>
          </div>
          <div className="policy-segment">
            <button
              className={snapshot?.iroh_receive_mode !== "direct" && snapshot?.iroh_receive_mode !== "relay" ? "is-active" : ""}
              onClick={() => void updateIrohReceiveMode("off")}
              type="button"
            >
              关闭
            </button>
            <button
              className={snapshot?.iroh_receive_mode === "direct" ? "is-active" : ""}
              onClick={() => void updateIrohReceiveMode("direct")}
              title="纯打洞直连，不经任何第三方"
              type="button"
            >
              直连
            </button>
            <button
              className={snapshot?.iroh_receive_mode === "relay" ? "is-active" : ""}
              onClick={() => void updateIrohReceiveMode("relay")}
              title="打洞失败时经 n0 公共中继（仅密文）"
              type="button"
            >
              中继
            </button>
          </div>
        </div>
        <div className="form-row">
          <div className="form-label">
            接收目录<small>收到的文件落在这里</small>
          </div>
          <span className="form-value" title={receiveDir}>
            {receiveDir}
          </span>
          <button className="btn-mini" disabled={busy === "pick-receive"} onClick={chooseReceiveDir} type="button">
            更改
          </button>
          <button
            className="btn-mini is-primary"
            disabled={receiveDir === snapshot?.receive_dir || busy === "pick-receive"}
            onClick={saveReceiveDir}
            type="button"
          >
            保存
          </button>
        </div>
        <div className="form-row">
          <div className="form-label">
            监听端口<small>被占用时自动向后找</small>
          </div>
          <input
            id="bind-port"
            onBlur={() => {
              if (bindPort.trim() && bindPort.trim() !== String(snapshot?.receive_port ?? "")) {
                saveReceivePort();
              }
            }}
            onChange={(event) => setBindPort(event.target.value)}
            pattern="[0-9]*"
            value={bindPort}
          />
        </div>
        <div className="form-row">
          <div className="form-label">
            按设备归档<small>收到的文件放入「接收目录/设备名/」</small>
          </div>
          <div className="policy-segment">
            <button
              className={snapshot?.organize_receive_by_device ? "is-active" : ""}
              onClick={() => void updateOrganizeByDevice(true)}
              type="button"
            >
              归档
            </button>
            <button
              className={!snapshot?.organize_receive_by_device ? "is-active" : ""}
              onClick={() => void updateOrganizeByDevice(false)}
              type="button"
            >
              平铺
            </button>
          </div>
        </div>
        <div className="form-row">
          <div className="form-label">
            发送限速<small>KB/s，0 = 不限速</small>
          </div>
          <input
            inputMode="numeric"
            onBlur={() => {
              void saveSendLimit();
            }}
            onChange={(event) => setSendLimitInput(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void saveSendLimit();
            }}
            pattern="[0-9]*"
            value={sendLimitInput}
          />
        </div>
        <div className="form-row">
          <div className="form-label">
            未配对设备发送时<small>可信设备不受此限制</small>
          </div>
          <div className="policy-segment" id="receive-policy">
            {RECEIVE_POLICY_OPTIONS.map((option) => (
              <button
                className={receivePolicy === option.value ? "is-active" : ""}
                disabled={busy === "receive-policy"}
                key={option.value}
                onClick={() => updateReceivePolicy(option.value)}
                type="button"
              >
                {option.label}
              </button>
            ))}
          </div>
        </div>
      </div>

      <div className="page-section">
        <div className="page-section-title">关于</div>
        <div className="form-row">
          <div className="form-label">
            版本<small>Apache-2.0 开源 · Rust + Tauri</small>
          </div>
          {updateInfo ? (
            <>
              <span className="form-value" style={{ color: "var(--ok)", fontWeight: 600 }}>
                有新版本 {updateInfo.tagName}
              </span>
              <a
                className="btn-mini is-primary"
                href={updateInfo.url}
                target="_blank"
                rel="noopener noreferrer"
              >
                前往下载
              </a>
            </>
          ) : (
            <span className="form-value">0.1.1 · 已是最新</span>
          )}
        </div>
        <div className="page-section-title">本机</div>
        <div className="form-row">
          <div className="form-label">设备名称</div>
          <input
            id="device-name"
            onChange={(event) => setDeviceNameInput(event.target.value)}
            placeholder={snapshot?.device_name ?? "NekoDrop"}
            value={deviceNameInput}
          />
          <button
            className="btn-mini is-primary"
            disabled={!deviceNameInput.trim() || deviceNameInput.trim() === snapshot?.device_name}
            onClick={saveDeviceName}
            type="button"
          >
            保存
          </button>
        </div>
        <div className="form-row">
          <div className="form-label">外观</div>
          <div className="policy-segment">
            <button
              className={appearance === "light" ? "is-active" : ""}
              onClick={() => setAppearance("light")}
              type="button"
            >
              浅色
            </button>
            <button
              className={appearance === "dark" ? "is-active" : ""}
              onClick={() => setAppearance("dark")}
              type="button"
            >
              深色
            </button>
          </div>
        </div>
        <div className="form-row">
          <div className="form-label">
            文本快送接收<small>收到文本片段时自动复制到剪贴板</small>
          </div>
          <div className="policy-segment">
            <button
              className={autoCopyTextSnippets ? "is-active" : ""}
              onClick={() => setTextSnippetAutoCopy(true)}
              type="button"
            >
              自动复制
            </button>
            <button
              className={!autoCopyTextSnippets ? "is-active" : ""}
              onClick={() => setTextSnippetAutoCopy(false)}
              type="button"
            >
              手动复制
            </button>
          </div>
        </div>
      </div>

      <div className="page-section">
        <div className="page-section-title">本地桥 · 本机应用接入</div>
        <div className="form-row">
          <div className="form-label">
            运行状态<small>localhost 请求入口</small>
          </div>
          <span className="form-value">
            {localBridgeStatus?.active ? `运行中 · ${localBridgeStatus.bind_host}:${localBridgeStatus.port}` : "未运行"}
          </span>
          <button className="btn-mini" onClick={runLocalBridgeSelfCheck} type="button">
            自检
          </button>
        </div>
        <div className="form-row">
          <div className="form-label">
            授权码确认<small>核对请求方展示的确认码</small>
          </div>
          <input
            className="auth-code-input"
            onChange={(event) => setLocalBridgeAuthorizationCode(event.target.value)}
            placeholder="XXX-XXX"
            value={localBridgeAuthorizationCode}
          />
          <button
            className="btn-mini is-primary"
            disabled={!localBridgeAuthorizationCode.trim()}
            onClick={confirmLocalBridgeAuthorization}
            type="button"
          >
            确认授权
          </button>
        </div>
        <div className="form-row">
          <div className="form-label">
            待处理请求<small>外部应用想执行的动作</small>
          </div>
          <span className="form-value">{localBridgePendingActions.length} 项待确认</span>
          <button className="btn-mini" onClick={() => setMode("send")} type="button">
            去收件箱处理
          </button>
        </div>
        <div className="form-row">
          <div className="form-label">
            已授权应用<small>{localBridgeAuthorizations.length} 个</small>
          </div>
          <span className="form-value">
            {localBridgeAuthorizations.map((auth) => auth.display_name).join("、") || "无"}
          </span>
          <button className="btn-mini" onClick={pruneLocalBridgeAuthorizations} type="button">
            清理过期
          </button>
        </div>
        {localBridgeAuthorizations.length > 0 && (
          <div className="list">
            {localBridgeAuthorizations.map((auth) =>
              auth.scopes.map((scope) => (
                <div className="list-row" key={`${auth.client_id}-${scope}`}>
                  <span className="row-icon">
                    <Icon name="plug" />
                  </span>
                  <div className="row-main">
                    <div className="row-title">{auth.display_name}</div>
                    <div className="row-sub">
                      <span className="mono">{auth.client_id}</span> · {scope}
                    </div>
                  </div>
                  <div className="row-ops">
                    <button
                      className="btn-mini is-danger"
                      onClick={() => revokeLocalBridgeAuthorization(auth, scope)}
                      type="button"
                    >
                      撤销
                    </button>
                  </div>
                </div>
              ))
            )}
          </div>
        )}
      </div>
    </div>
  );
}
