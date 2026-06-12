import type { TransferDto, TransferStatusDto } from "./types";
import { transferPrimaryActionLabel } from "./transferHistoryDetails.ts";

export interface CurrentTransferRecoveryActions {
  primaryLabel: string | null;
  fallbackLabel: string | null;
}

export function findCurrentFailedTransfer(
  status: TransferStatusDto | null,
  transfers: TransferDto[]
): TransferDto | null {
  if (!status || !isRecoverableCurrentPhase(status.phase) || status.direction !== "send") return null;
  const rootName = status.root_name?.trim();
  if (!rootName) return null;

  return transfers
    .filter((transfer) => transfer.direction === "send")
    .filter((transfer) => transfer.status === "failed" || transfer.status === "cancelled")
    .filter((transfer) => transfer.root_name === rootName)
    .filter((transfer) => transfer.total_bytes === status.total_bytes)
    .sort((left, right) => right.updated_at_ms - left.updated_at_ms)[0] ?? null;
}

export function currentTransferRecoveryActions(
  status: TransferStatusDto,
  transfer: TransferDto | null
): CurrentTransferRecoveryActions {
  if (!isRecoverableCurrentPhase(status.phase)) {
    return {
      primaryLabel: null,
      fallbackLabel: null
    };
  }

  return {
    primaryLabel: transfer ? transferPrimaryActionLabel(transfer) : null,
    fallbackLabel: "备用码"
  };
}

function isRecoverableCurrentPhase(phase: string) {
  return phase === "failed" || phase === "cancelled";
}
