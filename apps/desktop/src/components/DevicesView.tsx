import React, { useState } from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";
import type { DeviceDto, TrustedDeviceDto } from "../types";

function deviceHue(name: string) {
  let hash = 0;
  for (let i = 0; i < name.length; i++) hash = (hash * 31 + name.charCodeAt(i)) >>> 0;
  return hash % 360;
}

function DeviceAvatar({ name, online }: { name: string; online: boolean }) {
  return (
    <span
      className="row-avatar"
      style={{ backgroundColor: `hsl(${deviceHue(name)} 42% 50%)` }}
    >
      {name.slice(0, 1).toUpperCase()}
      <span className={`status-dot ${online ? "is-online" : "is-offline"}`} />
    </span>
  );
}

function formatTime(ms: number | null | undefined) {
  if (!ms) return "";
  const date = new Date(Number(ms));
  return `${date.getMonth() + 1}/${date.getDate()} ${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

/**
 * 设备页：纯列表（无卡片）
 * Devices page: plain rows.
 */
export function DevicesView() {
  const {
    nearbyDevices,
    trustedDevices,
    snapshot,
    requestPairing,
    forgetTrustedDevice,
    setTrustedDeviceAlias,
    setSelectedDeviceId,
    setConnectionCodeOpen,
    setMode,
    sendFilesToDevice,
    busy
  } = useAppContext();
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [aliasDraft, setAliasDraft] = useState("");

  const trustedIds = new Set(trustedDevices.map((device) => device.device_id));

  const aim = (deviceId: string) => {
    setSelectedDeviceId(deviceId);
    setConnectionCodeOpen(false);
    setMode("send");
  };

  const nearbyRow = (device: DeviceDto) => {
    const trusted = trustedIds.has(device.id);
    return (
      <div className="list-row" key={device.id}>
        <span className="row-icon">
          <Icon name="laptop" />
        </span>
        <div className="row-main">
          <div className="row-title">
            {(trusted && trustedDevices.find((d) => d.device_id === device.id)?.alias) || device.name}
          </div>
          <div className="row-sub">
            <span className="mono">{device.host}</span> · {device.platform}
            {device.public_key_fingerprint ? <> · <span className="mono">{device.public_key_fingerprint.slice(0, 16)}…</span></> : null}
          </div>
        </div>
        <div className="row-ops">
          <button className="btn-mini" onClick={() => aim(device.id)} type="button">
            选择
          </button>
          {trusted ? (
            <button
              className="btn-mini is-primary"
              disabled={busy === "send"}
              onClick={() => sendFilesToDevice(device)}
              type="button"
            >
              发送
            </button>
          ) : (
            <button
              className="btn-mini is-primary"
              disabled={busy === "pair"}
              onClick={() => requestPairing(device)}
              type="button"
            >
              配对
            </button>
          )}
        </div>
      </div>
    );
  };

  const trustedRow = (device: TrustedDeviceDto) => {
    const renaming = renamingId === device.device_id;
    const displayName = device.alias ?? device.device_name;
    return (
      <div className="list-row" key={device.device_id}>
        <DeviceAvatar name={displayName} online={false} />
        <div className="row-main">
          {renaming ? (
            <div className="rename-inline">
              <input
                autoFocus
                maxLength={32}
                onChange={(event) => setAliasDraft(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    void setTrustedDeviceAlias(device.device_id, aliasDraft).then(() =>
                      setRenamingId(null)
                    );
                  }
                  if (event.key === "Escape") setRenamingId(null);
                }}
                placeholder={device.device_name}
                value={aliasDraft}
              />
              <button
                className="btn-mini is-primary"
                onClick={() =>
                  void setTrustedDeviceAlias(device.device_id, aliasDraft).then(() =>
                    setRenamingId(null)
                  )
                }
                type="button"
              >
                保存
              </button>
              <button className="btn-mini" onClick={() => setRenamingId(null)} type="button">
                取消
              </button>
            </div>
          ) : (
            <>
              <div className="row-title">
                {displayName}
                {device.alias && <span className="row-origin">（{device.device_name}）</span>}
              </div>
              <div className="row-sub">
                已配对 · 最近见到 {formatTime(device.last_seen_at_ms)} · <span className="mono">{device.public_key_fingerprint.slice(0, 16)}…</span>
              </div>
            </>
          )}
        </div>
        <div className="row-ops" style={{ opacity: renaming ? 1 : undefined }}>
          {!renaming && (
            <>
              <button className="btn-mini" onClick={() => aim(device.device_id)} type="button">
                选择
              </button>
              <button
                className="btn-mini"
                onClick={() => {
                  setRenamingId(device.device_id);
                  setAliasDraft(device.alias ?? "");
                }}
                type="button"
              >
                备注
              </button>
              <button className="btn-mini is-danger" onClick={() => forgetTrustedDevice(device)} type="button">
                忘记
              </button>
            </>
          )}
        </div>
      </div>
    );
  };

  return (
    <div className="page">
      <div className="page-header">
        <h2>设备</h2>
        <p>同一局域网内的 NekoDrop 会自动出现；配对后可免确认接收</p>
      </div>

      <div className="page-section">
        <div className="page-section-title">本机 · {snapshot?.device_name}</div>
        <div className="inline-note">
          设备指纹 <span className="mono">{snapshot?.device_identity.public_key_fingerprint ?? "—"}</span>
          ，平台 {snapshot?.device_identity.platform ?? "—"}。可信设备互发时走已认证加密会话。
        </div>
      </div>

      <div className="page-section">
        <div className="page-section-title">附近设备 · {nearbyDevices.length}</div>
        <div className="list">
          {nearbyDevices.length === 0 ? (
            <div className="inline-note">还没有发现附近设备。确认对方也打开了 NekoDrop，且两台设备在同一网络。</div>
          ) : (
            nearbyDevices.map(nearbyRow)
          )}
        </div>
      </div>

      <div className="page-section">
        <div className="page-section-title">已配对设备 · {trustedDevices.length}</div>
        <div className="list">
          {trustedDevices.length === 0 ? (
            <div className="inline-note">尚未配对任何设备。配对后可校验对方长期公钥并启用可信接收。</div>
          ) : (
            trustedDevices.map(trustedRow)
          )}
        </div>
      </div>
    </div>
  );
}
