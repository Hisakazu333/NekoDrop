export type BusyMode =
  | "scan" | "send" | "receive" | "pick-files" | "pick-folders" | "pick-receive"
  | "stop-receive" | "receive-policy" | "device-name" | "cancel-transfer" | "pair"
  | "forget" | "history" | "resend" | "bundle-import" | "open";

export type ComposerMode = "overview" | "send" | "receive" | "devices" | "transfers" | "settings";
export type AppearanceMode = "light" | "dark";
export type ReceivePolicyMode = "always_ask" | "block_all";

export type TransferMetrics = {
  speedBytesPerSecond: number | null;
  etaSeconds: number | null;
};

export const EMPTY_TRANSFER_METRICS = Object.freeze<TransferMetrics>({
  speedBytesPerSecond: null,
  etaSeconds: null
});

export const APPEARANCE_STORAGE_KEY = "nekodrop.appearance";

export function resetTransferMetrics(current: TransferMetrics): TransferMetrics {
  return keepIfEqual(current, EMPTY_TRANSFER_METRICS);
}

export function readInitialAppearance(): AppearanceMode {
  if (typeof window === "undefined") return "light";
  return window.localStorage.getItem(APPEARANCE_STORAGE_KEY) === "dark" ? "dark" : "light";
}

export function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return String(error);
}

export function normalizeReceivePolicy(policy: string): ReceivePolicyMode {
  return policy === "block_all" ? "block_all" : "always_ask";
}

export function portFromBindAddr(addr: string): number | null {
  const parts = addr.split(":");
  const portStr = parts[parts.length - 1];
  const parsed = parseInt(portStr, 10);
  return isNaN(parsed) ? null : parsed;
}

export function uniquePaths(paths: string[]): string[] {
  return Array.from(new Set(paths));
}

export function buildPathPayload(selectedPaths: string[], manualPaths: string): string[] {
  const manual = manualPaths
    .split("\n")
    .map((p) => p.trim())
    .filter(Boolean);
  return uniquePaths([...selectedPaths, ...manual]);
}

export function keepIfEqual<T>(current: T, next: T): T {
  if (Object.is(current, next)) return current;
  if (current == null || next == null) return next;
  return stableJson(current) === stableJson(next) ? current : next;
}

export function stableJson(value: unknown) {
  return JSON.stringify(value);
}

export function lastPathSegment(path: string) {
  const normalized = path.replace(/\\/g, "/");
  return normalized.split("/").filter(Boolean).pop() ?? path;
}

export function isReceiveTransferActivePhase(phase: string): boolean {
  return ["connecting", "transferring", "waiting_for_decision"].includes(phase);
}

export function isCancelMessage(msg: string): boolean {
  return msg.includes("cancel") || msg.includes("cancelled") || msg.includes("取消");
}

export async function copyTextToClipboard(text: string): Promise<void> {
  if (navigator.clipboard) {
    await navigator.clipboard.writeText(text);
    return;
  }
  throw new Error("剪贴板不可用");
}
