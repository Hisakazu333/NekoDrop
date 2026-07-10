import React from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";

interface ActivityInspectorProps {
  onClose: () => void;
  onToggleInbox?: () => void;
}

export function ActivityInspector({ onClose, onToggleInbox }: ActivityInspectorProps) {
  const {
    snapshot,
    receiveSession,
    selectedDeviceId,
    selectedDeviceSnapshot,
    nearbyDevices,
    localBridgeAuthorizations,
    localBridgePendingActions,
    pendingReceiveOffer,
    stagedBundles,
    setMode
  } = useAppContext();

  const localDeviceName = snapshot?.device_name ?? "本机";
  const localPort = snapshot?.receive_port ?? 48231;
  const trustedNearbyDevices = nearbyDevices.filter((device) => device.trust_state === "Trusted");
  const selectedDevice =
    trustedNearbyDevices.find((device) => device.id === selectedDeviceId) ??
    (selectedDeviceSnapshot?.id === selectedDeviceId ? selectedDeviceSnapshot : null) ??
    null;
  const pendingRequestsCount = localBridgePendingActions.length + (pendingReceiveOffer ? 1 : 0);
  const stagedBundleCount = stagedBundles.filter((bundle) => bundle.staging_status === "saved").length;

  return (
    <aside aria-label="连接详情" className="activity-inspector" role="dialog">
      <div className="inspector-header" data-tauri-drag-region>
        <strong>连接详情</strong>
        <button className="inspector-close-button" onClick={onClose} title="关闭" type="button">
          <Icon name="x" />
        </button>
      </div>

      <div className="inspector-body">
        <div className="connection-info-card">
          <div className="inspector-section-header">
            <strong>当前连接</strong>
            <button className="btn-add-connection" onClick={() => setMode("settings")} title="连接设置" type="button">
              <Icon name="plus" />
            </button>
          </div>

          <div className="info-group-box">
            <div className="info-group-title">本机</div>
            <div className="info-list">
              <div className="info-item-bold">{localDeviceName}</div>
              <div className="info-item">
                <span>接收</span>
                <span className={receiveSession ? "text-success" : "text-muted"}>{receiveSession ? "已开启" : "已关闭"}</span>
              </div>
              <div className="info-item"><span>端口</span><span>{localPort}</span></div>
            </div>
          </div>

          <div className="info-group-box">
            <div className="info-group-title">目标设备</div>
            {selectedDevice ? (
              <div className="info-list">
                <div className="info-item-device">
                  <Icon name="monitor" />
                  <span>{selectedDevice.name}</span>
                </div>
                <div className="info-item"><span>信任状态</span><span className="text-success">已信任</span></div>
                <div className="info-item"><span>会话</span><span className="text-success">加密就绪</span></div>
              </div>
            ) : (
              <div className="empty-placeholder-text">尚未选择目标设备</div>
            )}
          </div>

          <div className="info-group-box">
            <div className="info-group-title">待处理</div>
            <div className="info-list">
              <div className="info-item"><span>请求</span><span>{pendingRequestsCount}</span></div>
              <div className="info-item"><span>已授权应用</span><span>{localBridgeAuthorizations.length}</span></div>
              <div className="info-item"><span>暂存资料包</span><span>{stagedBundleCount}</span></div>
            </div>
          </div>

          <div className="info-group-box">
            <div className="info-group-title">数据来源</div>
            <div className="info-list">
              <div className="info-item"><span>本地网桥</span><span className="text-success">可用</span></div>
              <div className="info-item"><span>Workspace Bundle</span><span className="cap-badge is-planned">计划中</span></div>
              <div className="info-item"><span>Session Bundle</span><span className="cap-badge is-planned">计划中</span></div>
            </div>
          </div>
        </div>
      </div>

      <div className="inspector-footer">
        <button
          onClick={() => {
            setMode("transfers");
            onClose();
          }}
          type="button"
        >
          <Icon name="clock" />
          <span>传输记录</span>
        </button>
        <button onClick={onToggleInbox} type="button">
          <Icon name="inbox" />
          <span>收件箱</span>
        </button>
      </div>
    </aside>
  );
}
