import type { TransferDto } from "./types";

export interface TransferHistoryDetailViewModel {
  progressLabel: string | null;
  peerLabel: string | null;
  locationLabel: string | null;
  errorLabel: string | null;
  recoveryLabel: string | null;
  canContinue: boolean;
}

export function buildTransferHistoryDetailViewModel(transfer: TransferDto): TransferHistoryDetailViewModel {
  const canContinue = isRecoverableSendTransfer(transfer);

  return {
    progressLabel:
      transfer.total_bytes > 0
        ? `${formatBytes(transfer.transferred_bytes)} / ${formatBytes(transfer.total_bytes)}`
        : null,
    peerLabel: transfer.peer_name ?? transfer.target_host,
    locationLabel: transfer.receive_dir ?? firstAvailablePath(transfer),
    errorLabel: transfer.error_message,
    recoveryLabel: canContinue ? "可以继续发送" : null,
    canContinue
  };
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

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
}
