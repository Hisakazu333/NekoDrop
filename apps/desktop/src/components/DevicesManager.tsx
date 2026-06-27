import React from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";
import type { TrustedDeviceDto, DeviceDto } from "../types";

function getPlatformIcon(platform: string): string {
  const p = platform.toLowerCase();
  if (p.includes("mac") || p.includes("darwin")) return " Mac";
  if (p.includes("win")) return "⊞ Windows";
  if (p.includes("ios") || p.includes("iphone") || p.includes("ipad")) return "📱 iOS";
  if (p.includes("android")) return "🤖 Android";
  return "💻 设备";
}

/**
 * 设备管理控制台组件
 * Devices Manager Console Component
 */
export function DevicesManager() {
  const {
    snapshot,
    trustedDevices,
    nearbyDevices,
    forgetTrustedDevice,
    requestPairing,
    pendingPairingRequest,
    respondPairingRequest,
    busy
  } = useAppContext();

  const handleForget = async (device: TrustedDeviceDto) => {
    if (window.confirm(`确定要解除对设备 "${device.device_name}" 的信任吗？`)) {
      await forgetTrustedDevice(device);
    }
  };

  return (
    <div className="manager-pane devices-manager">
      <div className="manager-header">
        <h2>设备与可信网络 (Devices & Trust Network)</h2>
        <p>管理这台电脑的身份以及在局域网内与之配对的受信任设备。</p>
      </div>

      <div className="manager-body">
        {/* 本机身份卡片 / Local Device Identity Card */}
        <div className="identity-section-card">
          <div className="identity-header">
            <Icon name="package" className="identity-icon" />
            <div className="identity-title">
              <h3>本机身份认证 (Local Identity)</h3>
              <span>已基于长期 Ed25519 密钥对进行保护</span>
            </div>
          </div>
          <div className="identity-details">
            <div className="detail-row">
              <span className="label">本机设备名:</span>
              <strong className="val">{snapshot?.device_name || "这台电脑"}</strong>
            </div>
            <div className="detail-row">
              <span className="label">身份指纹 (Fingerprint):</span>
              <span className="val code-font">
                {snapshot?.device_identity.public_key_fingerprint || "正在加载..."}
              </span>
            </div>
            <div className="detail-row">
              <span className="label">操作系统类型:</span>
              <span className="val">{snapshot?.device_identity.platform || "本机系统"}</span>
            </div>
          </div>
        </div>

        {/* 待处理的配对请求（高亮展示） / Pending Pairing Requests */}
        {pendingPairingRequest && (
          <div className="pairing-request-alert-card">
            <div className="alert-header">
              <Icon name="shield" className="alert-icon" />
              <div>
                <h4>收到配对请求</h4>
                <p>来自设备: <strong>{pendingPairingRequest.device_name}</strong> ({pendingPairingRequest.platform})</p>
              </div>
            </div>
            <div className="pairing-code-display">
              配对确认码: <span>{pendingPairingRequest.pairing_code}</span>
            </div>
            <div className="alert-actions">
              <button
                className="btn-v-danger"
                onClick={() => respondPairingRequest(false)}
                disabled={busy === "pair"}
              >
                拒绝配对
              </button>
              <button
                className="btn-v-success"
                onClick={() => respondPairingRequest(true)}
                disabled={busy === "pair"}
              >
                接受配对
              </button>
            </div>
          </div>
        )}

        {/* 1. 已信任的可信设备列表 / Trusted Devices List */}
        <div className="manager-section">
          <h3 className="section-title">已信任的设备 ({trustedDevices.length})</h3>
          <div className="devices-grid-list">
            {trustedDevices.length > 0 ? (
              trustedDevices.map((device) => {
                const isOnline = nearbyDevices.some((n) => n.id === device.device_id);
                const pairedDate = new Date(device.paired_at_ms).toLocaleDateString();

                return (
                  <div key={device.device_id} className={`device-info-card ${!isOnline ? "is-offline" : ""}`}>
                    <div className="device-card-main">
                      <div className="device-title-row">
                        <h4>{device.device_name}</h4>
                        <span className={`status-badge ${isOnline ? "is-online" : "is-offline"}`}>
                          {isOnline ? "在线" : "离线"}
                        </span>
                      </div>
                      <p className="device-platform-tag">{getPlatformIcon(device.platform)}</p>
                      <div className="device-fingerprint-row">
                        <span className="label">指纹:</span>
                        <span className="val code-font">{device.public_key_fingerprint}</span>
                      </div>
                      <p className="device-date">配对时间: {pairedDate}</p>
                    </div>
                    <div className="device-card-footer">
                      <button
                        className="btn-delete-device"
                        onClick={() => handleForget(device)}
                        disabled={busy === "forget"}
                      >
                        <Icon name="trash" />
                        <span>解除信任</span>
                      </button>
                    </div>
                  </div>
                );
              })
            ) : (
              <div className="manager-empty-state">
                <Icon name="shield" className="empty-icon" />
                <p>暂无已信任设备，在下方附近设备中发起配对建立可信网络。</p>
              </div>
            )}
          </div>
        </div>

        {/* 2. 附近发现的设备列表 / Discovered Nearby Devices */}
        <div className="manager-section">
          <h3 className="section-title">附近发现的设备 ({nearbyDevices.length})</h3>
          <div className="devices-table">
            {nearbyDevices.length > 0 ? (
              <div className="table-wrapper">
                <table className="standard-table">
                  <thead>
                    <tr>
                      <th>设备名</th>
                      <th>平台</th>
                      <th>地址与端口</th>
                      <th>关系状态</th>
                      <th className="align-right">操作</th>
                    </tr>
                  </thead>
                  <tbody>
                    {nearbyDevices.map((device) => {
                      const isTrusted = device.trust_state === "Trusted";
                      return (
                        <tr key={device.id}>
                          <td>
                            <div className="table-device-cell">
                              <span className="node-status-dot is-online" />
                              <strong>{device.name}</strong>
                            </div>
                          </td>
                          <td>{getPlatformIcon(device.platform)}</td>
                          <td className="code-font">{device.host}:{device.port}</td>
                          <td>
                            <span className={`trust-badge ${isTrusted ? "is-trusted" : "is-untrusted"}`}>
                              {isTrusted ? "已受信任" : "未配对"}
                            </span>
                          </td>
                          <td className="align-right">
                            {!isTrusted ? (
                              <button
                                className="btn-table-action"
                                onClick={() => requestPairing(device)}
                                disabled={busy === "pair"}
                              >
                                配对
                              </button>
                            ) : (
                              <span className="text-muted-tag">已连接</span>
                            )}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            ) : (
              <div className="manager-empty-state">
                <Icon name="devices" className="empty-icon" />
                <p>未在局域网内发现其他 NekoDrop 节点。请确保对方已开启客户端并在同一 Wi-Fi 下。</p>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
