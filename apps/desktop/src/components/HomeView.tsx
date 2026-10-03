import React, { useEffect, useState } from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";
import { invokeCommand, isTauriRuntime } from "../tauri";
import { copyTextToClipboard } from "../context/helpers";
import type { DeviceDto } from "../types";

const BUNDLE_TYPES = ["skill", "session", "workspace", "agent_profile", "config_snapshot"] as const;

function deviceHue(name: string) {
  let hash = 0;
  for (let i = 0; i < name.length; i++) hash = (hash * 31 + name.charCodeAt(i)) >>> 0;
  return hash % 360;
}

function DeviceAvatar({ name, online }: { name: string; online: boolean }) {
  return (
    <span className="row-avatar" style={{ backgroundColor: `hsl(${deviceHue(name)} 42% 50%)` }}>
      {name.slice(0, 1).toUpperCase()}
      <span className={`status-dot ${online ? "is-online" : "is-offline"}`} />
    </span>
  );
}

/**
 * 主页：按任务组织——
 * ① 接收卡常驻（收件开关 + 连接码大字可复制，不用再进设置）
 * ② 设备即操作（点「发送」选文件即发；拖文件到行上松手即发）
 * ③ 底部文本发送条（打字 ⌘↩ 直发）
 */
export function HomeView() {
  const {
    nearbyDevices,
    trustedDevices,
    receiveSession,
    startReceive,
    stopReceive,
    receiveStatus,
    selectedDeviceId,
    setSelectedDeviceId,
    sendDroppedPathsTo,
    sendCurrentTransfer,
    setConnectionCode,
    connectionCode,
    createManualBundleForSend,
    chooseManualBundleSourceDir,
    manualBundleType,
    setManualBundleType,
    manualBundleDisplayName,
    setManualBundleDisplayName,
    manualBundleSourceApp,
    setManualBundleSourceApp,
    manualBundleSourcePath,
    setMode,
    dragActive,
    setToast,
    setError,
    busy
  } = useAppContext();

  const [text, setText] = useState("");
  const [codeOpen, setCodeOpen] = useState(false);
  const [bundleOpen, setBundleOpen] = useState(false);

  // ⌘V 粘贴进文本框
  useEffect(() => {
    const onPaste = (event: ClipboardEvent) => {
      const pasted = event.clipboardData?.getData("text/plain");
      if (pasted && pasted.trim().length > 0) {
        setText((current) => (current.length === 0 ? pasted : current));
      }
    };
    window.addEventListener("paste", onPaste);
    return () => window.removeEventListener("paste", onPaste);
  }, []);

  const trustedIds = new Set(trustedDevices.map((device) => device.device_id));
  const aliasById = new Map(
    trustedDevices.filter((d) => d.alias).map((d) => [d.device_id, d.alias as string])
  );
  const rows = [
    ...nearbyDevices.map((device) => ({
      id: device.id,
      name: aliasById.get(device.id) ?? device.name,
      raw: device,
      online: true,
      sub: `${device.host} · ${device.platform}${trustedIds.has(device.id) ? " · 已配对" : ""}`
    })),
    ...trustedDevices
      .filter((device) => !nearbyDevices.some((nearby) => nearby.id === device.device_id))
      .map((device) => ({
        id: device.device_id,
        name: device.alias ?? device.device_name,
        raw: null,
        online: false,
        sub: "已配对 · 当前离线"
      }))
  ];

  // 点「发送」：选文件 → 立即发（在线设备）
  const pickAndSend = async (device: DeviceDto) => {
    if (!isTauriRuntime()) {
      setToast("文件选择器仅在桌面端可用");
      return;
    }
    try {
      const paths = await invokeCommand<string[]>("select_send_files");
      if (paths.length === 0) return;
      await sendDroppedPathsTo(device, paths);
    } catch (error) {
      setError(String(error));
    }
  };

  const submitText = async () => {
    if (text.trim().length === 0) return;
    setSelectedDeviceId(selectedDeviceId);
    await sendCurrentTransfer(text);
    setText("");
  };

  const target = nearbyDevices.find((device) => device.id === selectedDeviceId) ?? null;

  return (
    <div className="home-view">
      {/* 接收卡：收件开关 + 码常驻可见 */}
      <section className="receive-card">
        {receiveSession ? (
          <>
            <div className="receive-head">
              <span className="receive-live">
                <i className="live-dot" /> 收件开启
              </span>
              <span className="receive-status">{receiveStatus}</span>
              <button className="btn-mini" onClick={() => void stopReceive()} type="button">
                关闭收件
              </button>
            </div>
            <div className="receive-codes">
              <button
                className="receive-code"
                onClick={() =>
                  void copyTextToClipboard(receiveSession.connection_code)
                    .then(() => setToast("连接码已复制"))
                    .catch(() => setError("剪贴板不可用"))
                }
                title="点击复制"
                type="button"
              >
                <span className="receive-code-label">局域网</span>
                <code>{receiveSession.connection_code}</code>
              </button>
              {receiveSession.irohConnectionCode && (
                <button
                  className="receive-code"
                  onClick={() =>
                    void copyTextToClipboard(receiveSession.irohConnectionCode ?? "")
                      .then(() => setToast("跨网连接码已复制"))
                      .catch(() => setError("剪贴板不可用"))
                  }
                  title="点击复制 · 跨网可用"
                  type="button"
                >
                  <span className="receive-code-label">跨网 iroh</span>
                  <code>{receiveSession.irohConnectionCode}</code>
                </button>
              )}
            </div>
          </>
        ) : (
          <div className="receive-off">
            <div>
              <strong>接收文件</strong>
              <p>打开后，其他设备可向你发送（同网自动发现，或把连接码发给对方）</p>
            </div>
            <button
              className="btn-mini is-primary"
              disabled={busy === "receive"}
              onClick={() => void startReceive({ silent: true })}
              type="button"
            >
              打开收件
            </button>
          </div>
        )}
      </section>

      {/* 设备列表：点发送 / 拖文件到行 */}
      <section className="home-devices">
        <div className="page-section-title">
          {dragActive ? "拖到设备上松手即发" : "附近的设备"}
        </div>
        {rows.length === 0 ? (
          <div className="inline-note">
            还没有发现设备。确认对方也打开了 NekoDrop 且在同一网络；或用下方连接码跨网发送。
          </div>
        ) : (
          <div className="list">
            {rows.map((row) =>
              row.raw ? (
                <div
                  className={`list-row ${dragActive ? "is-drop-hint" : ""}`}
                  data-device-drop-id={row.id}
                  key={row.id}
                >
                  <DeviceAvatar name={row.name} online />
                  <div className="row-main">
                    <div className="row-title">{row.name}</div>
                    <div className="row-sub">{row.sub}</div>
                  </div>
                  <div className="row-ops" style={{ opacity: 1 }}>
                    <button
                      className="btn-mini"
                      onClick={() => {
                        setSelectedDeviceId(row.id);
                        setMode("send");
                      }}
                      type="button"
                    >
                      选为文本目标
                    </button>
                    <button
                      className="btn-mini is-primary"
                      onClick={() => void pickAndSend(row.raw as DeviceDto)}
                      type="button"
                    >
                      发送文件…
                    </button>
                  </div>
                </div>
              ) : (
                <div className="list-row is-offline-row" key={row.id}>
                  <DeviceAvatar name={row.name} online={false} />
                  <div className="row-main">
                    <div className="row-title">{row.name}</div>
                    <div className="row-sub">{row.sub}</div>
                  </div>
                </div>
              )
            )}
          </div>
        )}
      </section>

      {/* 资料包（折叠表单） */}
      <section className="home-code">
        {bundleOpen ? (
          <div className="bundle-mini">
            <div className="bundle-mini-row">
              <span className="mini-label">资料包</span>
              <input
                onChange={(event) => setManualBundleDisplayName(event.target.value)}
                placeholder="名称"
                value={manualBundleDisplayName}
              />
              <select onChange={(event) => setManualBundleType(event.target.value)} value={manualBundleType}>
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
              <button className="btn-mini" onClick={chooseManualBundleSourceDir} type="button">
                {manualBundleSourcePath ? "已选择目录" : "选择目录"}
              </button>
              <button
                className="btn-mini is-primary"
                disabled={!manualBundleSourcePath}
                onClick={() => void createManualBundleForSend()}
                type="button"
              >
                创建并发送
              </button>
              <button className="btn-mini" onClick={() => setBundleOpen(false)} type="button">
                收起
              </button>
            </div>
          </div>
        ) : (
          <button className="home-code-toggle" onClick={() => setBundleOpen(true)} type="button">
            <Icon name="package" /> 从目录打包发送（资料包）
          </button>
        )}
      </section>

      {/* 连接码发送（折叠） */}
      <section className="home-code">
        {codeOpen ? (
          <div className="home-code-open">
            <input
              autoFocus
              className="home-code-input"
              onChange={(event) => setConnectionCode(event.target.value)}
              placeholder="粘贴对方连接码（支持局域网与跨网码）"
              spellCheck={false}
              value={connectionCode}
            />
            <button
              className="btn-mini is-primary"
              disabled={connectionCode.trim().length === 0 || busy === "send"}
              onClick={() => void sendCurrentTransfer()}
              type="button"
            >
              按码发送…
            </button>
            <button className="btn-mini" onClick={() => setCodeOpen(false)} type="button">
              收起
            </button>
          </div>
        ) : (
          <button className="home-code-toggle" onClick={() => setCodeOpen(true)} type="button">
            <Icon name="link" /> 通过连接码发送（对方不在列表时）
          </button>
        )}
      </section>

      {/* 底部文本发送条 */}
      <div className="home-composer">
        <span className="home-composer-target">
          {target ? (
            <>
              <Icon name="laptop" /> {target.name}
            </>
          ) : (
            "未选目标"
          )}
        </span>
        <textarea
          onChange={(event) => setText(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
              event.preventDefault();
              void submitText();
            }
          }}
          placeholder={
            target ? "输入文本，⌘↩ 发送（也可拖文件到上方设备行）" : "先在上方选择目标设备，输入文本 ⌘↩ 发送"
          }
          rows={1}
          value={text}
        />
        <button
          aria-label="发送文本"
          className="home-composer-send"
          disabled={text.trim().length === 0 || !target}
          onClick={() => void submitText()}
          title={target ? `发送至 ${target.name}（⌘↩）` : "先选目标"}
          type="button"
        >
          <Icon name="arrow-up" />
        </button>
      </div>
    </div>
  );
}
