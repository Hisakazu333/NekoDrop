import type { TransferStatusDto } from "./types";
import { transferFailureAdvice } from "./transferFailureAdvice.ts";

export interface TransferProgressMetrics {
  speedBytesPerSecond: number | null;
  etaSeconds: number | null;
}

export interface TransferProgressViewModel {
  title: string;
  rootName: string;
  progressPercent: number;
  percentLabel: string;
  bytesLabel: string;
  fileIndexLabel: string;
  speedLabel: string | null;
  etaLabel: string | null;
  currentFileLabel: string | null;
  adviceLabel: string | null;
  message: string;
}

export function progressPercent(status: TransferStatusDto) {
  return clampPercent(status.progress);
}

export function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

export function formatDuration(seconds: number) {
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  if (minutes < 60) return rest > 0 ? `${minutes}m ${rest}s` : `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  const minuteRest = minutes % 60;
  return minuteRest > 0 ? `${hours}h ${minuteRest}m` : `${hours}h`;
}

export function formatSpeed(bytesPerSecond: number) {
  return `${formatBytes(bytesPerSecond)}/s`;
}

export function buildTransferProgressViewModel(
  status: TransferStatusDto,
  metrics: TransferProgressMetrics
): TransferProgressViewModel {
  const progress = progressPercent(status);
  const currentFileLabel = status.current_file && status.current_file.trim().length > 0 ? status.current_file : null;
  const fileIndexLabel =
    status.file_count > 0 && status.file_index > 0 ? `${status.file_index} / ${status.file_count}` : `${status.file_count} 个文件`;

  return {
    title: transferTitle(status),
    rootName: status.root_name?.trim() || status.message?.trim() || phaseLabel(status.phase),
    progressPercent: progress,
    percentLabel: `${progress}%`,
    bytesLabel:
      status.total_bytes > 0
        ? `${formatBytes(status.bytes_transferred)} / ${formatBytes(status.total_bytes)}`
        : formatBytes(status.bytes_transferred),
    fileIndexLabel,
    speedLabel: metrics.speedBytesPerSecond ? formatSpeed(metrics.speedBytesPerSecond) : null,
    etaLabel: metrics.etaSeconds != null ? `剩余 ${formatDuration(metrics.etaSeconds)}` : null,
    currentFileLabel,
    adviceLabel: status.phase === "failed" ? transferFailureAdvice(status.message) : null,
    message: status.message
  };
}

function transferTitle(status: TransferStatusDto) {
  if (status.phase === "transferring") {
    return status.direction === "receive" ? "正在接收" : "正在发送";
  }
  return phaseLabel(status.phase);
}

function phaseLabel(phase: string) {
  if (phase === "cancelled") return "已取消";
  if (phase === "connecting") return "连接中";
  if (phase === "listening") return "收件开启";
  if (phase === "awaiting_approval") return "等待确认";
  if (phase === "accepted") return "已接受";
  if (phase === "transferring") return "传输中";
  if (phase === "verifying") return "校验中";
  if (phase === "failed") return "传输失败";
  if (phase === "declined") return "已拒绝";
  if (phase === "expired") return "已超时";
  if (phase === "closed") return "收件关闭";
  if (phase === "completed") return "传输完成";
  return phase;
}

function clampPercent(value: number) {
  if (!Number.isFinite(value)) return 0;
  return Math.round(Math.min(1, Math.max(0, value)) * 100);
}

const HIDDEN_ACTIVE_TRANSFER_PHASES = new Set([
  "completed",
  "closed",
  "listening",
  "declined",
  "expired"
]);

export function shouldShowActiveTransferBar(status: TransferStatusDto) {
  return !HIDDEN_ACTIVE_TRANSFER_PHASES.has(status.phase);
}

export function shouldShowTransferProgressMeter(status: TransferStatusDto) {
  return (
    status.total_bytes > 0 ||
    status.bytes_transferred > 0 ||
    status.phase === "transferring" ||
    status.phase === "verifying"
  );
}
