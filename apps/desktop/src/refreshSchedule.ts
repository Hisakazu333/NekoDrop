export const REALTIME_REFRESH_INTERVAL_MS = 1200;
export const DIRECTORY_REFRESH_INTERVAL_MS = 5000;
export const DIAGNOSTICS_REFRESH_INTERVAL_MS = 10000;
export const STARTUP_SLOW_REFRESH_DELAY_MS = 600;

export function shouldRefreshDirectoryForMode(
  mode: string,
  hasActiveTransfer: boolean
) {
  return (
    hasActiveTransfer ||
    mode === "overview" ||
    mode === "send" ||
    mode === "devices" ||
    mode === "transfers"
  );
}

export function shouldRefreshDirectoryOnModeActivation(
  mode: string,
  previousMode: string | null,
  hasActiveTransfer: boolean
) {
  return (
    previousMode !== null &&
    mode !== previousMode &&
    shouldRefreshDirectoryForMode(mode, hasActiveTransfer)
  );
}

export function shouldRunDirectoryRefresh(
  nowMs: number,
  lastRefreshMs: number,
  intervalMs = DIRECTORY_REFRESH_INTERVAL_MS
) {
  return lastRefreshMs <= 0 || nowMs - lastRefreshMs >= intervalMs;
}

export function shouldRunDiagnosticsRefresh(
  nowMs: number,
  lastRefreshMs: number,
  intervalMs = DIAGNOSTICS_REFRESH_INTERVAL_MS
) {
  return lastRefreshMs <= 0 || nowMs - lastRefreshMs >= intervalMs;
}
