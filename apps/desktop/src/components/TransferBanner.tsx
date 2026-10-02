import React from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";
import { formatBytes } from "../transferProgress";

function formatEta(seconds: number | null) {
  if (seconds == null || !Number.isFinite(seconds)) return "";
  if (seconds < 60) return `${Math.max(1, Math.round(seconds))} 秒`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} 分 ${Math.round(seconds % 60)} 秒`;
  return `${Math.floor(minutes / 60)} 时 ${minutes % 60} 分`;
}

/**
 * 活动传输横幅：来件/配对决策与进行中的传输（唯一的强调区块）
 * Live transfer banner: decisions and progress.
 */
export function TransferBanner() {
  const {
    transferStatus,
    transferMetrics,
    pendingReceiveOffer,
    pendingPairingRequest,
    respondReceiveOffer,
    respondPairingRequest,
    cancelCurrentTransfer,
    busy
  } = useAppContext();

  if (pendingPairingRequest) {
    return (
      <div className="transfer-banner is-decision">
        <Icon name="key" className="banner-icon" />
        <div className="banner-body">
          <div className="banner-title">配对请求 · {pendingPairingRequest.device_name}</div>
          <div className="banner-sub">核对两台设备上的确认码一致后接受</div>
        </div>
        <span className="pair-code">{pendingPairingRequest.pairing_code}</span>
        <div className="banner-ops">
          <button className="text-btn" onClick={() => respondPairingRequest(false)} type="button">
            拒绝
          </button>
          <button
            className="text-btn is-primary"
            disabled={busy === "pair"}
            onClick={() => respondPairingRequest(true)}
            type="button"
          >
            接受
          </button>
        </div>
      </div>
    );
  }

  if (pendingReceiveOffer) {
    return (
      <div className="transfer-banner is-decision">
        <Icon name="upload" className="banner-icon" />
        <div className="banner-body">
          <div className="banner-title">
            {pendingReceiveOffer.sender_device_name ?? "未知设备"} 想发送 {pendingReceiveOffer.root_name}
          </div>
          <div className="banner-sub">
            {pendingReceiveOffer.file_count} 个文件 · {formatBytes(pendingReceiveOffer.total_bytes)}
          </div>
        </div>
        <div className="banner-ops">
          <button className="text-btn" onClick={() => respondReceiveOffer(false)} type="button">
            拒绝
          </button>
          <button className="text-btn is-primary" onClick={() => respondReceiveOffer(true)} type="button">
            接收
          </button>
        </div>
      </div>
    );
  }

  if (!transferStatus) return null;
  const phase = transferStatus.phase;
  const passive = phase === "listening" || phase === "idle";
  if (passive) return null;

  const isSend = transferStatus.direction === "send";
  const percent = Math.max(0, Math.min(100, Math.round((transferStatus.progress ?? 0) * 100)));
  const speed = transferMetrics.speedBytesPerSecond;
  const eta = formatEta(transferMetrics.etaSeconds);

  const titleMap: Record<string, string> = {
    awaiting_approval: isSend ? "等待对方确认…" : "等待本机确认",
    connecting: "连接中…",
    negotiating: "协商加密会话…",
    transferring: isSend ? "正在发送" : "正在接收",
    verifying: "校验完整性…",
    done: isSend ? "发送完成" : "接收完成",
    succeeded: isSend ? "发送完成" : "接收完成",
    failed: isSend ? "发送失败" : "接收失败",
    cancelled: "已取消"
  };
  const title = titleMap[phase] ?? (transferStatus.message || phase);
  const name = transferStatus.root_name ?? transferStatus.current_file ?? "";
  const isTerminal = phase === "done" || phase === "succeeded" || phase === "failed" || phase === "cancelled";
  const cls = isTerminal ? (phase === "failed" ? "is-failed" : "is-ok") : "";

  return (
    <div className={`transfer-banner ${cls}`}>
      <Icon name={isSend ? "arrow-up" : "upload"} className="banner-icon" />
      <div className="banner-body">
        <div className="banner-title">
          {title}
          {name ? ` · ${name}` : ""}
        </div>
        <div className="banner-sub">
          {transferStatus.message}
          {transferStatus.total_bytes > 0 && (
            <> · {formatBytes(transferStatus.bytes_transferred)} / {formatBytes(transferStatus.total_bytes)}</>
          )}
          {speed != null && speed > 0 && <> · {formatBytes(speed)}/s{eta ? ` · 剩余 ${eta}` : ""}</>}
        </div>
        {!isTerminal && transferStatus.total_bytes > 0 && (
          <div className="banner-progress">
            <i style={{ width: `${percent}%` }} />
          </div>
        )}
      </div>
      {!isTerminal && (
        <div className="banner-ops">
          <button className="text-btn is-danger" onClick={cancelCurrentTransfer} type="button">
            取消
          </button>
        </div>
      )}
    </div>
  );
}
