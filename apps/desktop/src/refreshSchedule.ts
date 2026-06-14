export const REALTIME_REFRESH_INTERVAL_MS = 1200;
export const DIRECTORY_REFRESH_INTERVAL_MS = 5000;
export const DIAGNOSTICS_REFRESH_INTERVAL_MS = 10000;

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
