import React from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";
import { formatBytes } from "../transferProgress";

function formatEta(seconds: number | null) {
  if (seconds == null || !Number.isFinite(seconds)) return "";
  if (seconds < 60) return `${Math.max(1, Math.round(seconds))} 秒`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} 分`;
  return `${Math.floor(minutes / 60)} 时`;
}

interface TransferPanelProps {
  onClose: () => void;
}

/**
 * 常驻传输面板（右栏）：来件决策、进行中进度、待发队列。
 * 任何页面都可见——传输状态不再藏进横幅。
 */
export function TransferPanel({ onClose }: TransferPanelProps) {
  const {
    transferStatus,
    transferMetrics,
    pendingReceiveOffer,
    pendingPairingRequest,
    respondReceiveOffer,
    respondPairingRequest,
    cancelCurrentTransfer,
    pauseCurrentTransfer,
    sendQueue,
    resumeQueuedSendById,
    cancelQueuedSend,
    clearSendQueue,
    busy
  } = useAppContext();

  const hasAnything =
    pendingPairingRequest || pendingReceiveOffer || sendQueue.length > 0 || transferStatus;

  return (
    <div className="tp-root">
      <div className="tp-head">
        <strong>传输</strong>
        <button aria-label="收起面板" className="icon-btn" onClick={onClose} title="收起面板" type="button">
          <Icon name="x" />
        </button>
      </div>

      <div className="tp-body">
        {/* 配对请求：最高优先级 */}
        {pendingPairingRequest && (
          <div className="tp-card is-decision">
            <div className="tp-card-title">
              <Icon name="key" /> 配对请求 · {pendingPairingRequest.device_name}
            </div>
            <div className="tp-card-sub">核对两台设备上的确认码一致后接受</div>
            <div className="tp-pair-code">{pendingPairingRequest.pairing_code}</div>
            <div className="tp-actions">
              <button className="btn-mini" onClick={() => respondPairingRequest(false)} type="button">
                拒绝
              </button>
              <button
                className="btn-mini is-primary"
                disabled={busy === "pair"}
                onClick={() => respondPairingRequest(true)}
                type="button"
              >
                接受
              </button>
            </div>
          </div>
        )}

        {/* 来件决策 */}
        {pendingReceiveOffer && (
          <div className="tp-card is-decision">
            <div className="tp-card-title">
              <Icon name="upload" /> {pendingReceiveOffer.sender_device_name ?? "未知设备"} 想发送
            </div>
            <div className="tp-card-sub">
              {pendingReceiveOffer.root_name} · {pendingReceiveOffer.file_count} 个文件 ·{" "}
              {formatBytes(pendingReceiveOffer.total_bytes)}
            </div>
            <div className="tp-actions">
              <button className="btn-mini" onClick={() => respondReceiveOffer(false)} type="button">
                拒绝
              </button>
              <button
                className="btn-mini is-primary"
                onClick={() => respondReceiveOffer(true)}
                type="button"
              >
                接收
              </button>
            </div>
          </div>
        )}

        {/* 进行中的传输 */}
        {(() => {
          if (!transferStatus) return null;
          const phase = transferStatus.phase;
          if (phase === "listening" || phase === "idle") return null;
          const isSend = transferStatus.direction === "send";
          const percent = Math.max(0, Math.min(100, Math.round((transferStatus.progress ?? 0) * 100)));
          const speed = transferMetrics.speedBytesPerSecond;
          const eta = formatEta(transferMetrics.etaSeconds);
          const isTerminal =
            phase === "done" || phase === "succeeded" || phase === "failed" || phase === "cancelled";
          const titleMap: Record<string, string> = {
            awaiting_approval: isSend ? "等待对方确认" : "等待本机确认",
            connecting: "连接中",
            negotiating: "协商加密会话",
            transferring: isSend ? "正在发送" : "正在接收",
            verifying: "校验完整性",
            done: isSend ? "发送完成" : "接收完成",
            succeeded: isSend ? "发送完成" : "接收完成",
            failed: isSend ? "发送失败" : "接收失败",
            cancelled: "已取消"
          };
          return (
            <div className={`tp-card ${phase === "failed" ? "is-failed" : isTerminal ? "is-done" : ""}`}>
              <div className="tp-card-title">
                <Icon name={isSend ? "arrow-up" : "upload"} />
                {titleMap[phase] ?? transferStatus.message}
              </div>
              <div className="tp-card-sub" title={transferStatus.root_name ?? undefined}>
                {transferStatus.root_name ?? transferStatus.message}
              </div>
              {transferStatus.total_bytes > 0 && (
                <>
                  <div className="tp-progress">
                    <i style={{ width: `${percent}%` }} />
                  </div>
                  <div className="tp-card-sub">
                    {formatBytes(transferStatus.bytes_transferred)} / {formatBytes(transferStatus.total_bytes)}
                    {speed != null && speed > 0 && <> · {formatBytes(speed)}/s{eta ? ` · 剩 ${eta}` : ""}</>}
                  </div>
                </>
              )}
              {!isTerminal && (
                <div className="tp-actions">
                  {isSend && (
                    <button
                      className="btn-mini"
                      disabled={busy === "cancel-transfer"}
                      onClick={() => void pauseCurrentTransfer()}
                      title="暂停，继续时从断点接着传"
                      type="button"
                    >
                      暂停
                    </button>
                  )}
                  <button className="btn-mini is-danger" onClick={cancelCurrentTransfer} type="button">
                    取消
                  </button>
                </div>
              )}
            </div>
          );
        })()}

        {/* 待发队列 */}
        {sendQueue.length > 0 && (
          <div className="tp-card">
            <div className="tp-card-title">
              <Icon name="clock" /> 队列 · {sendQueue.length} 项
              <button
                className="btn-mini is-danger tp-clear"
                onClick={clearSendQueue}
                type="button"
              >
                清空
              </button>
            </div>
            <div className="tp-queue">
              {sendQueue.map((entry) => (
                <div className="tp-queue-row" key={entry.id}>
                  <span className={`tp-queue-state ${entry.paused ? "is-paused" : ""}`}>
                    {entry.paused ? "已暂停" : "等待"}
                  </span>
                  <span className="tp-queue-label" title={entry.pathsText}>
                    {entry.label}
                  </span>
                  {entry.paused ? (
                    <button
                      className="btn-mini is-primary"
                      onClick={() => resumeQueuedSendById(entry.id)}
                      title="从断点继续"
                      type="button"
                    >
                      继续
                    </button>
                  ) : (
                    <button className="btn-mini" onClick={() => cancelQueuedSend(entry.id)} type="button">
                      移除
                    </button>
                  )}
                </div>
              ))}
            </div>
          </div>
        )}

        {!hasAnything && (
          <div className="tp-empty">
            <Icon name="check" />
            <p>没有进行中的传输</p>
            <span>拖文件到主页设备行，或在底部输入框发文本</span>
          </div>
        )}
      </div>
    </div>
  );
}
