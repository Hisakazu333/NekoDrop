import React from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";
import { formatBytes } from "../transferProgress";

/**
 * 格式化传输速率 / Format transfer speed
 */
function formatSpeed(bytesPerSecond: number | null): string {
  if (bytesPerSecond === null || bytesPerSecond === 0) return "-- KB/s";
  return `${formatBytes(bytesPerSecond)}/s`;
}

/**
 * 格式化剩余时间 / Format remaining ETA
 */
function formatEta(seconds: number | null): string {
  if (seconds === null) return "--";
  if (seconds < 60) return `${seconds} 秒`;
  const mins = Math.floor(seconds / 60);
  const secs = seconds % 60;
  return `${mins} 分 ${secs} 秒`;
}

/**
 * 右侧活动检查器：实时传输进度与历史记录
 * Right Activity Inspector: Real-time Transfers and History
 */
export function ActivityInspector() {
  const {
    transferStatus,
    transferMetrics,
    transfers,
    cancelCurrentTransfer,
    resendTransfer,
    openTransferLocation,
    deleteTransfer,
    clearTransferHistory,
    busy
  } = useAppContext();

  // 判断是否处于活跃传输状态 / Check if there is an active transfer
  const isActive =
    transferStatus &&
    (transferStatus.phase === "transferring" ||
      transferStatus.phase === "connecting" ||
      transferStatus.phase === "verifying");

  const progressPercent = transferStatus
    ? Math.round((transferStatus.bytes_transferred / transferStatus.total_bytes) * 100) || 0
    : 0;

  const isReceiving = transferStatus?.direction === "receive";

  return (
    <section className="activity-inspector">
      {/* 1. 正在进行的活跃传输 / Active Transfers */}
      <div className="inspector-section">
        <div className="section-title-group">
          <strong>实时传输</strong>
          <span className={`section-badge ${isActive ? "is-live" : ""}`}>{isActive ? "1" : "0"}</span>
        </div>

        {isActive && transferStatus ? (
          <div className="active-transfer-card">
            <div className="active-meta">
              <span className={`active-direction-tag ${isReceiving ? "is-receive" : ""}`}>
                <Icon name={isReceiving ? "arrow-up" : "send"} style={isReceiving ? { transform: "rotate(180deg)" } : undefined} />
                {isReceiving ? "接收中" : "发送中"}
              </span>
              <span className="active-speed">{formatSpeed(transferMetrics.speedBytesPerSecond)}</span>
            </div>

            <div className="active-filename" title={transferStatus.root_name ?? undefined}>
              {transferStatus.root_name}
            </div>

            <div className="active-progress-stats">
              <span>
                <strong>{formatBytes(transferStatus.bytes_transferred)}</strong> /{" "}
                {formatBytes(transferStatus.total_bytes)}
              </span>
              <strong>{progressPercent}%</strong>
            </div>

            <div className="active-progress-bar-wrapper">
              <div className="active-progress-bar" style={{ width: `${progressPercent}%` }} />
            </div>

            <div className="active-eta-cancel">
              <span className="active-eta">剩余 {formatEta(transferMetrics.etaSeconds)}</span>
              <button
                className="btn-cancel-transfer"
                onClick={cancelCurrentTransfer}
                disabled={busy === "cancel-transfer"}
                type="button"
              >
                取消
              </button>
            </div>
          </div>
        ) : (
          <div className="active-empty-state">
            <Icon name="paw" className="empty-icon" />
            <p>暂时没有传输任务</p>
          </div>
        )}
      </div>

      {/* 分割线 / Border Divider */}
      <div className="inspector-divider" />

      {/* 2. 传输历史记录 / Transfer History */}
      <div className="inspector-section is-flexible">
        <div className="section-title-group" style={{ padding: "18px 0 12px" }}>
          <strong>传输历史</strong>
          {transfers.length > 0 && (
            <button className="btn-text-action" onClick={clearTransferHistory} type="button">
              清空
            </button>
          )}
        </div>

        <div className="history-list">
          {transfers.length > 0 ? (
            transfers.map((transfer) => {
              const isSuccess = transfer.status === "completed";
              const isSend = transfer.direction === "send";
              const timeStr = new Date(transfer.created_at_ms).toLocaleTimeString([], {
                hour: "2-digit",
                minute: "2-digit"
              });

              return (
                <div className="history-card" key={transfer.id}>
                  <div className="history-card-header">
                    <div className="history-direction-info">
                      <span className={`history-icon-badge ${isSend ? "is-send" : "is-receive"}`}>
                        <Icon
                          name={isSend ? "send" : "arrow-up"}
                          style={isSend ? undefined : { transform: "rotate(180deg)" }}
                        />
                      </span>
                      <span className="history-filename" title={transfer.root_name ?? undefined}>
                        {transfer.root_name}
                      </span>
                    </div>
                    <span className="history-time">{timeStr}</span>
                  </div>

                  <div className="history-card-meta">
                    <span>{formatBytes(transfer.total_bytes)}</span>
                    <span className={`history-status-tag ${isSuccess ? "is-success" : "is-failed"}`}>
                      {isSuccess ? "成功" : "失败"}
                    </span>
                  </div>

                  {/* 历史卡片的动作操作栏 / History card actions */}
                  <div className="history-card-actions">
                    <button
                      className="btn-history-op"
                      onClick={() => openTransferLocation(transfer)}
                      title="打开所在文件夹"
                      type="button"
                    >
                      <Icon name="folder" />
                    </button>
                    {!isSuccess && isSend && (
                      <button
                        className="btn-history-op"
                        onClick={() => resendTransfer(transfer)}
                        title="重新发送"
                        type="button"
                      >
                        <Icon name="refresh" />
                      </button>
                    )}
                    <button
                      className="btn-history-op btn-delete-history"
                      onClick={() => deleteTransfer(transfer)}
                      title="删除记录"
                      type="button"
                    >
                      <Icon name="trash" />
                    </button>
                  </div>
                </div>
              );
            })
          ) : (
            <div className="history-empty-state">
              <Icon name="clock" className="empty-icon" />
              <p>还没有传输记录</p>
            </div>
          )}
        </div>
      </div>
    </section>
  );
}
