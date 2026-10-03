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
export const TEXT_SNIPPET_AUTO_COPY_STORAGE_KEY = "nekodrop.textSnippetAutoCopy";

/** 接收文本片段是否自动进剪贴板（默认开，可在设置关闭） */
export function readTextSnippetAutoCopy(): boolean {
  try {
    const raw = localStorage.getItem(TEXT_SNIPPET_AUTO_COPY_STORAGE_KEY);
    if (raw == null) return true;
    return raw === "1";
  } catch {
    return true;
  }
}

export function writeTextSnippetAutoCopy(enabled: boolean) {
  try {
    localStorage.setItem(TEXT_SNIPPET_AUTO_COPY_STORAGE_KEY, enabled ? "1" : "0");
  } catch {
    /* localStorage 不可用时静默降级为内存态 / ignore storage failures */
  }
}

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

/* ------------------------------------------------------------------ */
/* 发送队列 / send queue（忙线时入队，完成后自动依序发出）               */
/* ------------------------------------------------------------------ */

export type QueuedSendKind = "device" | "code";

export interface QueuedSend {
  id: string;
  kind: QueuedSendKind;
  /** deviceId（device）或连接码（code） */
  target: string;
  pathsText: string;
  label: string;
  enqueuedAtMs: number;
  /** 暂停的条目不自动出队，等"继续"唤醒 / paused entries wait for resume */
  paused?: boolean;
}

export const MAX_SEND_QUEUE = 20;

export function enqueueSend(queue: QueuedSend[], entry: QueuedSend): QueuedSend[] {
  if (queue.length >= MAX_SEND_QUEUE) return queue;
  return [...queue, entry];
}

/** 出队第一条未暂停的条目（保持其余顺序）/ pop first non-paused entry */
export function dequeueSend(queue: QueuedSend[]): { head: QueuedSend | null; rest: QueuedSend[] } {
  const index = queue.findIndex((entry) => !entry.paused);
  if (index === -1) return { head: null, rest: queue };
  const head = queue[index];
  const rest = queue.filter((_, i) => i !== index);
  return { head, rest };
}

export function resumeQueuedSend(queue: QueuedSend[], id: string): QueuedSend[] {
  return queue.map((entry) => (entry.id === id ? { ...entry, paused: false } : entry));
}

/** 队列条目摘要：首个路径名或"文本片段"，多个时附计数 */
export function queuedSendLabel(pathsText: string): string {
  const paths = pathsText.split("\n").map((p) => p.trim()).filter(Boolean);
  if (paths.length === 0) return "空传输";
  const first = lastPathSegment(paths[0]);
  return paths.length > 1 ? `${first} 等 ${paths.length} 项` : first;
}

export async function copyTextToClipboard(text: string): Promise<void> {
  if (navigator.clipboard) {
    await navigator.clipboard.writeText(text);
    return;
  }
  throw new Error("剪贴板不可用");
}
