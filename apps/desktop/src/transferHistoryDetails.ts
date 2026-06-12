import type { TransferDto } from "./types";
import { transferFailureAdvice } from "./transferFailureAdvice.ts";

export interface TransferHistoryDetailViewModel {
  progressLabel: string | null;
  peerLabel: string | null;
  locationLabel: string | null;
  errorLabel: string | null;
  adviceLabel: string | null;
  recoveryLabel: string | null;
  primaryActionLabel: string | null;
  canContinue: boolean;
}

export function buildTransferHistoryDetailViewModel(transfer: TransferDto): TransferHistoryDetailViewModel {
  const canContinue = isRecoverableSendTransfer(transfer);
  const primaryActionLabel = transferPrimaryActionLabel(transfer);

  return {
    progressLabel:
      transfer.total_bytes > 0
        ? `${formatBytes(transfer.transferred_bytes)} / ${formatBytes(transfer.total_bytes)}`
        : null,
    peerLabel: transfer.peer_name ?? transfer.target_host,
    locationLabel: transfer.receive_dir ?? firstAvailablePath(transfer),
    errorLabel: transfer.error_message,
    adviceLabel: transferFailureAdvice(transfer.error_message),
    recoveryLabel: transferRecoveryLabel(transfer, canContinue, primaryActionLabel),
    primaryActionLabel,
    canContinue
  };
}

export function transferPrimaryActionLabel(transfer: TransferDto) {
  if (transfer.direction !== "send") return null;
  if (isRecoverableSendTransfer(transfer)) return "继续发送";
  if (transfer.status === "failed" || transfer.status === "cancelled") return "重试";
  return "重发";
}

export function buildRecentTransferDetailLine(transfer: TransferDto) {
  const detail = buildTransferHistoryDetailViewModel(transfer);
  return detail.recoveryLabel ?? detail.adviceLabel ?? detail.errorLabel ?? null;
}

function firstAvailablePath(transfer: TransferDto) {
  return transfer.received_paths[0] ?? transfer.source_paths[0] ?? null;
}

function isRecoverableSendTransfer(transfer: TransferDto) {
  return (
    transfer.direction === "send" &&
    (transfer.status === "failed" || transfer.status === "cancelled") &&
    transfer.total_bytes > 0 &&
    transfer.transferred_bytes > 0 &&
    transfer.transferred_bytes < transfer.total_bytes
  );
}

function transferRecoveryLabel(
  transfer: TransferDto,
  canContinue: boolean,
  primaryActionLabel: string | null
) {
  if (transfer.status === "cancelled" && !canContinue && primaryActionLabel === "重试") {
    return "已取消，可重试";
  }

  if (!canContinue) return null;

  const remainingBytes = Math.max(0, transfer.total_bytes - transfer.transferred_bytes);
  const label = `已传 ${formatBytes(transfer.transferred_bytes)}，剩余 ${formatBytes(remainingBytes)}，可继续发送`;
  return transfer.status === "cancelled" ? `已取消，${label}` : label;
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
}
