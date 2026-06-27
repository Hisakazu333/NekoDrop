import React, { createContext, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { bindWindowDragDrop } from "../dragDrop";
import { invokeCommand, isTauriRuntime } from "../tauri";
import { findCurrentRecoverableTransfer } from "../currentTransferRecovery";
import {
  shouldRunDiagnosticsRefresh,
  REALTIME_REFRESH_INTERVAL_MS,
  shouldRefreshDirectoryOnModeActivation,
  shouldRefreshDirectoryForMode,
  shouldRunDirectoryRefresh,
  STARTUP_SLOW_REFRESH_DELAY_MS
} from "../refreshSchedule";
import { shouldShowActiveTransferBar } from "../transferProgress";
import type {
  AppSnapshot,
  DesktopRealtimeSnapshotDto,
  DeviceDto,
  DiscoveryStatusDto,
  LocalBridgeAuthorizationDto,
  LocalBridgePendingActionDto,
  LocalBridgePendingActionResultDto,
  LocalBridgeRuntimeStatusDto,
  ManualBundleCreateDto,
  PendingPairingRequestDto,
  PendingReceiveOfferDto,
  ReceivePortDiagnosticsDto,
  ReceivedBundleDto,
  ReceiveReportDto,
  ReceiveSessionDto,
  SendReportDto,
  TrustedDeviceDto,
  TransferDto,
  TransferPlanDto,
  TransferScanProgressDto,
  TransferStatusDto,
  LocalBridgeAuthorizationListDto,
  LocalBridgePendingActionListDto,
  LocalBridgePendingActionResultListDto,
  LocalBridgeResponseDto
} from "../types";

// ---------------------------------------------------------
// 声明和工具函数 / Helper Declarations and Utility Functions
// ---------------------------------------------------------

type BusyMode =
  | "scan"
  | "send"
  | "receive"
  | "pick-files"
  | "pick-folders"
  | "pick-receive"
  | "stop-receive"
  | "receive-policy"
  | "device-name"
  | "cancel-transfer"
  | "pair"
  | "forget"
  | "history"
  | "resend"
  | "bundle-import"
  | "open";

type ComposerMode = "overview" | "send" | "receive" | "devices" | "transfers" | "settings";
type ReceivePolicyMode = "always_ask" | "block_all";
type AppearanceMode = "light" | "dark";
type TransferMetrics = {
  speedBytesPerSecond: number | null;
  etaSeconds: number | null;
};

const APPEARANCE_STORAGE_KEY = "nekodrop.appearance";
const EMPTY_TRANSFER_METRICS = Object.freeze<TransferMetrics>({
  speedBytesPerSecond: null,
  etaSeconds: null
});

function readInitialAppearance(): AppearanceMode {
  if (typeof window === "undefined") return "light";
  return window.localStorage.getItem(APPEARANCE_STORAGE_KEY) === "dark" ? "dark" : "light";
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return String(error);
}

function normalizeReceivePolicy(policy: string): ReceivePolicyMode {
  return policy === "block_all" ? "block_all" : "always_ask";
}

function portFromBindAddr(addr: string): number | null {
  const parts = addr.split(":");
  const portStr = parts[parts.length - 1];
  const parsed = parseInt(portStr, 10);
  return isNaN(parsed) ? null : parsed;
}

function uniquePaths(paths: string[]): string[] {
  return Array.from(new Set(paths));
}

function buildPathPayload(selectedPaths: string[], manualPaths: string): string[] {
  const manual = manualPaths
    .split("\n")
    .map((p) => p.trim())
    .filter(Boolean);
  return uniquePaths([...selectedPaths, ...manual]);
}

function keepIfEqual<T>(current: T, next: T): T {
  if (JSON.stringify(current) === JSON.stringify(next)) return current;
  return next;
}

function resetTransferMetrics(setter: React.Dispatch<React.SetStateAction<TransferMetrics>>) {
  setter(EMPTY_TRANSFER_METRICS);
}

function isReceiveTransferActivePhase(phase: string): boolean {
  return ["connecting", "transferring", "waiting_for_decision"].includes(phase);
}

function isCancelMessage(msg: string): boolean {
  return msg.includes("cancel") || msg.includes("cancelled") || msg.includes("取消");
}

function isTrustedDeviceState(state: string): boolean {
  return state === "Trusted";
}

function trustedDeviceToDeviceDto(device: TrustedDeviceDto): DeviceDto {
  return {
    id: device.device_id,
    name: device.device_name,
    platform: device.platform,
    trust_state: "Trusted",
    pairing_code: device.pairing_code,
    host: device.host,
    port: device.port,
    public_key_fingerprint: device.public_key_fingerprint
  };
}

async function copyTextToClipboard(text: string): Promise<void> {
  if (navigator.clipboard) {
    await navigator.clipboard.writeText(text);
    return;
  }
  throw new Error("剪贴板不可用");
}

// ---------------------------------------------------------
// Context 接口定义 / Context Interface Definition
// ---------------------------------------------------------

interface AppContextType {
  snapshot: AppSnapshot | null;
  selectedPaths: string[];
  manualPaths: string;
  connectionCode: string;
  receiveDir: string;
  receivePolicy: ReceivePolicyMode;
  bindPort: string;
  deviceNameInput: string;
  plan: TransferPlanDto | null;
  scanStatus: TransferScanProgressDto | null;
  sendReport: SendReportDto | null;
  nearbyDevices: DeviceDto[];
  discoveryStatus: DiscoveryStatusDto | null;
  receiveSession: ReceiveSessionDto | null;
  receiveDiagnostics: ReceivePortDiagnosticsDto | null;
  receiveStatus: string | null;
  receiveReport: ReceiveReportDto | null;
  pendingReceiveOffer: PendingReceiveOfferDto | null;
  pendingPairingRequest: PendingPairingRequestDto | null;
  transferStatus: TransferStatusDto | null;
  transfers: TransferDto[];
  trustedDevices: TrustedDeviceDto[];
  stagedBundles: ReceivedBundleDto[];
  selectedTransferId: string | null;
  selectedDeviceId: string | null;
  selectedDeviceSnapshot: DeviceDto | null;
  connectionCodeOpen: boolean;
  localBridgeStatus: LocalBridgeRuntimeStatusDto | null;
  localBridgeAuthorizations: LocalBridgeAuthorizationDto[];
  localBridgePendingActions: LocalBridgePendingActionDto[];
  localBridgeActionResults: LocalBridgePendingActionResultDto[];
  localBridgeCheck: string | null;
  localBridgeAuthorizationCode: string;
  mode: ComposerMode;
  appearance: AppearanceMode;
  dragActive: boolean;
  dragDropReady: boolean;
  busy: BusyMode | null;
  error: string | null;
  toast: string | null;
  transferMetrics: TransferMetrics;

  setManualPaths: (val: string) => void;
  setConnectionCode: (val: string) => void;
  setBindPort: (val: string) => void;
  setDeviceNameInput: (val: string) => void;
  setSelectedTransferId: (val: string | null) => void;
  setSelectedDeviceId: (val: string | null) => void;
  setConnectionCodeOpen: (val: boolean) => void;
  setLocalBridgeAuthorizationCode: (val: string) => void;
  setMode: (mode: ComposerMode) => void;
  setAppearance: (updater: AppearanceMode | ((curr: AppearanceMode) => AppearanceMode)) => void;
  setError: (val: string | null) => void;
  setToast: (val: string | null) => void;

  refreshSnapshot: () => Promise<void>;
  refreshReceiveState: (options?: { includeDiagnostics?: boolean; includeDirectoryState?: boolean }) => Promise<void>;
  pickFiles: () => Promise<void>;
  pickFolders: () => Promise<void>;
  removePath: (path: string) => void;
  clearQueue: () => void;
  chooseReceiveDir: () => Promise<void>;
  saveReceiveDir: () => Promise<void>;
  saveReceivePort: () => Promise<void>;
  updateReceivePolicy: (policy: ReceivePolicyMode) => Promise<void>;
  saveDeviceName: () => Promise<void>;
  openPath: (path: string) => Promise<void>;
  scanPaths: (paths?: string[], manual?: string) => Promise<void>;
  startReceive: (options?: { receiveDirOverride?: string; receivePortOverride?: number; silent?: boolean }) => Promise<void>;
  stopReceive: () => Promise<void>;
  sendFilesToDevice: (device: DeviceDto) => Promise<void>;
  sendCurrentTransfer: () => Promise<void>;
  cancelCurrentTransfer: () => Promise<void>;
  resendTransfer: (transfer: TransferDto) => Promise<void>;
  openTransferLocation: (transfer: TransferDto) => Promise<void>;
  deleteTransfer: (transfer: TransferDto) => Promise<void>;
  clearTransferHistory: () => Promise<void>;
  requestPairing: (device: DeviceDto) => Promise<void>;
  respondPairingRequest: (accept: boolean) => Promise<void>;
  forgetTrustedDevice: (device: TrustedDeviceDto) => Promise<void>;
  respondReceiveOffer: (accept: boolean) => Promise<void>;
  copyConnectionCode: () => Promise<void>;

  // 本地桥方法 / Local Bridge Methods
  runLocalBridgeSelfCheck: () => Promise<void>;
  confirmLocalBridgeAuthorization: () => Promise<void>;
  removeLocalBridgePendingAction: (action: LocalBridgePendingActionDto) => Promise<void>;
  revokeLocalBridgeAuthorization: (auth: LocalBridgeAuthorizationDto, scope: string) => Promise<void>;
  pruneLocalBridgeAuthorizations: () => Promise<void>;
  importCurrentStagedBundle: (bundle: ReceivedBundleDto) => Promise<void>;
  deleteCurrentStagedBundle: (bundle: ReceivedBundleDto) => Promise<void>;
}

const AppContext = createContext<AppContextType | undefined>(undefined);

// ---------------------------------------------------------
// Context 提供者实现 / Context Provider Implementation
// ---------------------------------------------------------

export function AppProvider({ children }: { children: ReactNode }) {
  const [snapshot, setSnapshot] = useState<AppSnapshot | null>(null);
  const [selectedPaths, setSelectedPaths] = useState<string[]>([]);
  const [manualPaths, setManualPaths] = useState("");
  const [connectionCode, setConnectionCode] = useState("");
  const [receiveDir, setReceiveDir] = useState("~/Downloads/NekoDrop");
  const [receivePolicy, setReceivePolicy] = useState<ReceivePolicyMode>("always_ask");
  const [bindPort, setBindPort] = useState("45821");
  const [deviceNameInput, setDeviceNameInput] = useState("这台电脑");
  const [plan, setPlan] = useState<TransferPlanDto | null>(null);
  const [scanStatus, setScanStatus] = useState<TransferScanProgressDto | null>(null);
  const [sendReport, setSendReport] = useState<SendReportDto | null>(null);
  const [nearbyDevices, setNearbyDevices] = useState<DeviceDto[]>([]);
  const [discoveryStatus, setDiscoveryStatus] = useState<DiscoveryStatusDto | null>(null);
  const [receiveSession, setReceiveSession] = useState<ReceiveSessionDto | null>(null);
  const [receiveDiagnostics, setReceiveDiagnostics] = useState<ReceivePortDiagnosticsDto | null>(null);
  const [receiveStatus, setReceiveStatus] = useState<string | null>(null);
  const [receiveReport, setReceiveReport] = useState<ReceiveReportDto | null>(null);
  const [pendingReceiveOffer, setPendingReceiveOffer] = useState<PendingReceiveOfferDto | null>(null);
  const [pendingPairingRequest, setPendingPairingRequest] = useState<PendingPairingRequestDto | null>(null);
  const [transferStatus, setTransferStatus] = useState<TransferStatusDto | null>(null);
  const [transfers, setTransfers] = useState<TransferDto[]>([]);
  const [trustedDevices, setTrustedDevices] = useState<TrustedDeviceDto[]>([]);
  const [stagedBundles, setStagedBundles] = useState<ReceivedBundleDto[]>([]);
  const [selectedTransferId, setSelectedTransferId] = useState<string | null>(null);
  const [selectedDeviceId, setSelectedDeviceId] = useState<string | null>(null);
  const [selectedDeviceSnapshot, setSelectedDeviceSnapshot] = useState<DeviceDto | null>(null);
  const [connectionCodeOpen, setConnectionCodeOpen] = useState(false);
  const [localBridgeStatus, setLocalBridgeStatus] = useState<LocalBridgeRuntimeStatusDto | null>(null);
  const [localBridgeAuthorizations, setLocalBridgeAuthorizations] = useState<LocalBridgeAuthorizationDto[]>([]);
  const [localBridgePendingActions, setLocalBridgePendingActions] = useState<LocalBridgePendingActionDto[]>([]);
  const [localBridgeActionResults, setLocalBridgeActionResults] = useState<LocalBridgePendingActionResultDto[]>([]);
  const [localBridgeCheck, setLocalBridgeCheck] = useState<string | null>(null);
  const [localBridgeAuthorizationCode, setLocalBridgeAuthorizationCode] = useState("");
  const [mode, setMode] = useState<ComposerMode>("send");
  const [appearance, setAppearance] = useState<AppearanceMode>(() => readInitialAppearance());

  // 监听 appearance 状态并同步到 DOM 和 LocalStorage
  // Sync appearance state to DOM and LocalStorage
  useEffect(() => {
    if (typeof document !== "undefined") {
      document.documentElement.setAttribute("data-theme", appearance);
      window.localStorage.setItem(APPEARANCE_STORAGE_KEY, appearance);
    }
  }, [appearance]);
  const [dragActive, setDragActive] = useState(false);
  const [dragDropReady, setDragDropReady] = useState(false);
  const [busy, setBusy] = useState<BusyMode | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [transferMetrics, setTransferMetrics] = useState<TransferMetrics>(EMPTY_TRANSFER_METRICS);

  const desktopRuntime = useMemo(() => isTauriRuntime(), []);
  const previousTransferStatus = useRef<TransferStatusDto | null>(null);
  const autoReceiveStarted = useRef(false);
  const realtimeRefreshInFlight = useRef(false);
  const directoryRefreshInFlight = useRef(false);
  const diagnosticsRefreshInFlight = useRef(false);
  const lastDirectoryRefreshAt = useRef(0);
  const lastDiagnosticsRefreshAt = useRef(0);
  const previousMode = useRef<ComposerMode | null>(null);

  const transferPaths = useMemo(
    () => buildPathPayload(selectedPaths, manualPaths),
    [manualPaths, selectedPaths]
  );

  const trustedNearbyDevices = useMemo(
    () => nearbyDevices.filter((device) => device.trust_state === "Trusted"),
    [nearbyDevices]
  );

  const selectedDevice = useMemo(
    () =>
      trustedNearbyDevices.find((device) => device.id === selectedDeviceId) ??
      (selectedDeviceSnapshot?.id === selectedDeviceId ? selectedDeviceSnapshot : null) ??
      null,
    [selectedDeviceId, selectedDeviceSnapshot, trustedNearbyDevices]
  );

  const trimmedConnectionCode = connectionCode.trim();

  // ---------------------------------------------------------
  // 数据更新与事件监听 / Data Update and Event Listening
  // ---------------------------------------------------------

  useEffect(() => {
    document.documentElement.dataset.theme = appearance;
    window.localStorage.setItem(APPEARANCE_STORAGE_KEY, appearance);
  }, [appearance]);

  useEffect(() => {
    refreshSnapshot().catch((nextError) => setError(errorMessage(nextError)));
    refreshLocalBridgeStatus().catch(() => undefined);
    refreshLocalBridgeAuthorizations().catch(() => undefined);
    refreshLocalBridgePendingActions().catch(() => undefined);
    refreshLocalBridgeActionResults().catch(() => undefined);
    const slowRefreshTimer = window.setTimeout(() => {
      refreshReceiveState({ includeDiagnostics: true, includeDirectoryState: true }).catch(() => undefined);
    }, STARTUP_SLOW_REFRESH_DELAY_MS);
    return () => window.clearTimeout(slowRefreshTimer);
  }, []);

  const hasActiveTransfer = Boolean(transferStatus && shouldShowActiveTransferBar(transferStatus));

  useEffect(() => {
    if (!snapshot || receiveSession || autoReceiveStarted.current) return;
    autoReceiveStarted.current = true;
    startReceive({
      receiveDirOverride: snapshot.receive_dir,
      receivePortOverride: snapshot.receive_port,
      silent: true
    }).catch((nextError) => setError(errorMessage(nextError)));
  }, [snapshot, receiveSession]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      const shouldRefreshDirectory =
        shouldRefreshDirectoryForMode(mode, hasActiveTransfer) &&
        shouldRunDirectoryRefresh(Date.now(), lastDirectoryRefreshAt.current);
      refreshReceiveState({
        includeDiagnostics: shouldRunDiagnosticsRefresh(Date.now(), lastDiagnosticsRefreshAt.current),
        includeDirectoryState: shouldRefreshDirectory
      }).catch(() => undefined);
      if (mode === "settings") {
        refreshLocalBridgeStatus().catch(() => undefined);
        refreshLocalBridgePendingActions().catch(() => undefined);
        refreshLocalBridgeActionResults().catch(() => undefined);
      }
    }, REALTIME_REFRESH_INTERVAL_MS);
    return () => window.clearInterval(timer);
  }, [hasActiveTransfer, mode]);

  useEffect(() => {
    const lastMode = previousMode.current;
    previousMode.current = mode;
    if (!shouldRefreshDirectoryOnModeActivation(mode, lastMode, hasActiveTransfer)) return;
    refreshDirectoryState().catch(() => undefined);
  }, [hasActiveTransfer, mode]);

  useEffect(() => {
    let active = true;
    const unlistenPromise = listen<TransferScanProgressDto>("transfer_scan_progress", (event) => {
      if (!active) return;
      setScanStatus(event.payload);
    });

    return () => {
      active = false;
      unlistenPromise.then((unlisten) => unlisten()).catch(() => undefined);
    };
  }, []);

  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(null), 2200);
    return () => window.clearTimeout(timer);
  }, [toast]);

  useEffect(() => {
    if (!selectedDeviceId) return;
    const latestDevice = nearbyDevices.find((device) => device.id === selectedDeviceId);
    if (!latestDevice || latestDevice.trust_state !== "Trusted") return;
    setSelectedDeviceSnapshot(latestDevice);
  }, [nearbyDevices, selectedDeviceId]);

  useEffect(() => {
    if (mode !== "send") return;
    if (selectedDeviceId || connectionCodeOpen || trimmedConnectionCode.length > 0) return;
    if (trustedNearbyDevices.length !== 1) return;
    setSelectedDeviceId(trustedNearbyDevices[0].id);
    setSelectedDeviceSnapshot(trustedNearbyDevices[0]);
  }, [connectionCodeOpen, mode, selectedDeviceId, trimmedConnectionCode.length, trustedNearbyDevices]);

  useEffect(() => {
    if (!transferStatus || transferStatus.phase !== "transferring") {
      previousTransferStatus.current = transferStatus;
      resetTransferMetrics(setTransferMetrics);
      return;
    }

    const previous = previousTransferStatus.current;
    previousTransferStatus.current = transferStatus;

    if (
      !previous ||
      previous.direction !== transferStatus.direction ||
      previous.updated_at_ms >= transferStatus.updated_at_ms ||
      transferStatus.bytes_transferred < previous.bytes_transferred
    ) {
      return;
    }

    const elapsedSeconds = (transferStatus.updated_at_ms - previous.updated_at_ms) / 1000;
    if (elapsedSeconds <= 0) return;

    const speedBytesPerSecond =
      (transferStatus.bytes_transferred - previous.bytes_transferred) / elapsedSeconds;
    const remainingBytes = Math.max(0, transferStatus.total_bytes - transferStatus.bytes_transferred);
    setTransferMetrics((current) =>
      keepIfEqual(current, {
        speedBytesPerSecond,
        etaSeconds: speedBytesPerSecond > 0 ? Math.ceil(remainingBytes / speedBytesPerSecond) : null
      })
    );
  }, [transferStatus]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    if (!desktopRuntime) {
      setDragDropReady(false);
      return;
    }

    void bindWindowDragDrop({
      onActiveChange: setDragActive,
      onDrop: (paths) => {
        void applyPickedPathsRef.current(paths).catch((nextError) => setError(errorMessage(nextError)));
      },
      onError: (message) => {
        setDragDropReady(false);
        setError(`拖放初始化失败：${message}`);
      }
    }).then((nextUnlisten) => {
      if (cancelled) {
        nextUnlisten();
        return;
      }
      unlisten = nextUnlisten;
      setDragDropReady(true);
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [desktopRuntime]);

  // ---------------------------------------------------------
  // 核心 API 调用 / Core API Invocations
  // ---------------------------------------------------------

  async function refreshSnapshot() {
    const nextSnapshot = await invokeCommand<AppSnapshot>("get_app_snapshot");
    setSnapshot(nextSnapshot);
    setDeviceNameInput(nextSnapshot.device_name);
    setReceiveDir(nextSnapshot.receive_dir);
    setBindPort(String(nextSnapshot.receive_port));
    setReceivePolicy(normalizeReceivePolicy(nextSnapshot.receive_policy));
  }

  async function refreshReceiveState(options: { includeDiagnostics?: boolean; includeDirectoryState?: boolean } = {}) {
    await Promise.all([
      refreshRealtimeState(),
      options.includeDiagnostics ? refreshDiagnosticsState() : Promise.resolve(),
      options.includeDirectoryState ? refreshDirectoryState() : Promise.resolve()
    ]);
  }

  async function refreshRealtimeState() {
    if (realtimeRefreshInFlight.current) return;
    realtimeRefreshInFlight.current = true;
    try {
      const nextSnapshot = await invokeCommand<DesktopRealtimeSnapshotDto>("get_desktop_realtime_snapshot");
      setReceiveStatus((current) => keepIfEqual(current, nextSnapshot.receive_status));
      setReceiveSession((current) => keepIfEqual(current, nextSnapshot.receive_session));
      setReceiveReport((current) => keepIfEqual(current, nextSnapshot.receive_report));
      setPendingReceiveOffer((current) => keepIfEqual(current, nextSnapshot.pending_receive_offer));
      setPendingPairingRequest((current) => keepIfEqual(current, nextSnapshot.pending_pairing_request));
      setTransferStatus((current) => keepIfEqual(current, nextSnapshot.transfer_status));
      setDiscoveryStatus((current) => keepIfEqual(current, nextSnapshot.discovery_status));
      if (nextSnapshot.pending_receive_offer || nextSnapshot.pending_pairing_request) setMode("receive");
    } finally {
      realtimeRefreshInFlight.current = false;
    }
  }

  async function refreshDiagnosticsState() {
    if (diagnosticsRefreshInFlight.current) return;
    diagnosticsRefreshInFlight.current = true;
    try {
      const diagnostics = await invokeCommand<ReceivePortDiagnosticsDto>("get_receive_port_diagnostics");
      setReceiveDiagnostics((current) => keepIfEqual(current, diagnostics));
      lastDiagnosticsRefreshAt.current = Date.now();
    } finally {
      diagnosticsRefreshInFlight.current = false;
    }
  }

  async function refreshDirectoryState() {
    if (directoryRefreshInFlight.current) return;
    directoryRefreshInFlight.current = true;
    try {
      await invokeCommand<string[]>("prune_staged_bundles");
      const [devices, trusted, nextTransfers, nextStagedBundles] = await Promise.all([
        invokeCommand<DeviceDto[]>("list_nearby_devices"),
        invokeCommand<TrustedDeviceDto[]>("list_trusted_devices"),
        invokeCommand<TransferDto[]>("list_transfers"),
        invokeCommand<ReceivedBundleDto[]>("list_staged_bundles")
      ]);
      setNearbyDevices((current) => keepIfEqual(current, devices));
      setTrustedDevices((current) => keepIfEqual(current, trusted));
      setTransfers((current) => keepIfEqual(current, nextTransfers));
      setStagedBundles((current) => keepIfEqual(current, nextStagedBundles));
      lastDirectoryRefreshAt.current = Date.now();
    } finally {
      directoryRefreshInFlight.current = false;
    }
  }

  async function refreshTransfers() {
    const nextTransfers = await invokeCommand<TransferDto[]>("list_transfers");
    setTransfers((current) => keepIfEqual(current, nextTransfers));
  }

  async function pickFiles() {
    setBusy("pick-files");
    setError(null);
    try {
      const paths = await invokeCommand<string[]>("select_send_files");
      await applyPickedPaths(paths);
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function pickFolders() {
    setBusy("pick-folders");
    setError(null);
    try {
      const paths = await invokeCommand<string[]>("select_send_folders");
      await applyPickedPaths(paths);
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function applyPickedPaths(paths: string[]) {
    if (paths.length === 0) return;
    const mergedPaths = uniquePaths([...selectedPaths, ...paths]);
    setSelectedPaths(mergedPaths);
    setSendReport(null);
    setMode("send");
    setToast(`已加入 ${paths.length} 个路径`);
    await scanPaths(mergedPaths, manualPaths);
  }

  const applyPickedPathsRef = useRef(applyPickedPaths);
  applyPickedPathsRef.current = applyPickedPaths;

  function removePath(path: string) {
    const nextPaths = selectedPaths.filter((item) => item !== path);
    setSelectedPaths(nextPaths);
    setPlan(null);
    setScanStatus(null);
    setSendReport(null);
  }

  function clearQueue() {
    setSelectedPaths([]);
    setManualPaths("");
    setPlan(null);
    setScanStatus(null);
    setSendReport(null);
  }

  async function chooseReceiveDir() {
    setBusy("pick-receive");
    setError(null);
    try {
      const pickedDir = await invokeCommand<string | null>("select_receive_dir");
      if (pickedDir) {
        await invokeCommand<void>("set_receive_dir", { receiveDir: pickedDir });
        setReceiveDir(pickedDir);
        await refreshSnapshot();
        setToast("接收目录已更新");
      }
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function saveReceiveDir() {
    if (receiveSession) return;
    const nextReceiveDir = receiveDir.trim();
    if (!nextReceiveDir || nextReceiveDir === snapshot?.receive_dir) return;
    setBusy("pick-receive");
    setError(null);
    try {
      await invokeCommand<void>("set_receive_dir", { receiveDir: nextReceiveDir });
      setReceiveDir(nextReceiveDir);
      setSnapshot((current) => (current ? { ...current, receive_dir: nextReceiveDir } : current));
      await refreshSnapshot();
      setToast("接收目录已保存");
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function saveReceivePort() {
    if (receiveSession) return;
    const nextReceivePort = parseReceivePortValue(bindPort);
    if (nextReceivePort === null || nextReceivePort === snapshot?.receive_port) return;
    setBusy("pick-receive");
    setError(null);
    try {
      await invokeCommand<void>("set_receive_port", { receivePort: nextReceivePort });
      setBindPort(String(nextReceivePort));
      setSnapshot((current) => (current ? { ...current, receive_port: nextReceivePort } : current));
      setToast("默认端口已保存");
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  function parseReceivePortValue(val: string): number | null {
    const parsed = parseInt(val.trim(), 10);
    return isNaN(parsed) || parsed < 1 || parsed > 65535 ? null : parsed;
  }

  async function updateReceivePolicy(nextPolicy: ReceivePolicyMode) {
    if (nextPolicy === receivePolicy) return;
    setBusy("receive-policy");
    setError(null);
    try {
      await invokeCommand<void>("set_receive_policy", { receivePolicy: nextPolicy });
      setReceivePolicy(nextPolicy);
      setSnapshot((current) => (current ? { ...current, receive_policy: nextPolicy } : current));
      setToast(nextPolicy === "block_all" ? "接收策略：阻止" : "接收策略：询问");
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function saveDeviceName() {
    const nextName = deviceNameInput.trim();
    if (!nextName || nextName === snapshot?.device_name) return;
    setBusy("device-name");
    setError(null);
    try {
      const savedName = await invokeCommand<string>("set_device_name", { deviceName: nextName });
      setSnapshot((current) =>
        current
          ? {
              ...current,
              device_name: savedName,
              device_identity: { ...current.device_identity, device_name: savedName }
            }
          : current
      );
      setDeviceNameInput(savedName);
      setToast("设备名已保存");
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function openPath(path: string) {
    setBusy("open");
    setError(null);
    try {
      await invokeCommand<void>("open_path", { path });
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function scanPaths(paths = selectedPaths, manual = manualPaths) {
    const payload = buildPathPayload(paths, manual);
    if (payload.length === 0) return;

    setBusy("scan");
    setError(null);
    setScanStatus(null);
    setSendReport(null);
    try {
      const nextPlan = await invokeCommand<TransferPlanDto>("create_transfer_plan", { paths: payload });
      setPlan(nextPlan);
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setScanStatus(null);
      setBusy(null);
    }
  }

  async function startReceive(options: { receiveDirOverride?: string; receivePortOverride?: number; silent?: boolean } = {}) {
    const silent = options.silent ?? false;
    const requestedPort = options.receivePortOverride ?? parseReceivePortValue(bindPort);
    if (requestedPort === null) {
      setError("端口必须是 1-65535");
      return;
    }

    if (!silent) setBusy("receive");
    setError(null);
    setReceiveReport(null);
    if (!silent) setMode("receive");
    try {
      const session = await invokeCommand<ReceiveSessionDto>("start_receive_once", {
        bindHost: "0.0.0.0",
        port: requestedPort,
        receiveDir: options.receiveDirOverride ?? receiveDir
      });
      setReceiveSession(session);
      setBindPort(String(portFromBindAddr(session.bind_addr) ?? requestedPort));
      setReceiveStatus("等待接收中");
      setToast(silent ? "已自动打开收件" : "收件已打开");
      await refreshDiagnosticsState();
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      if (!silent) setBusy(null);
    }
  }

  async function stopReceive() {
    setBusy("stop-receive");
    setError(null);
    try {
      await invokeCommand<void>("stop_receive_once");
      setReceiveSession(null);
      setPendingReceiveOffer(null);
      setReceiveStatus("收件已关闭");
      setToast("收件已关闭");
      await refreshReceiveState({ includeDiagnostics: true, includeDirectoryState: true });
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function sendFiles() {
    const payload = transferPaths;
    if (payload.length === 0) {
      setMode("send");
      setError("未选择文件");
      return;
    }

    setBusy("send");
    setError(null);
    setSendReport(null);
    try {
      const report = await invokeCommand<SendReportDto>("send_paths_to_code", {
        connectionCode,
        pathsText: payload.join("\n")
      });
      setSendReport(report);
      setToast(`发送完成：${report.file_count} 个文件`);
      await refreshTransfers();
    } catch (nextError) {
      const message = errorMessage(nextError);
      if (isCancelMessage(message)) {
        setToast("传输已取消");
      } else {
        setError(message);
      }
      await refreshTransfers().catch(() => undefined);
    } finally {
      setBusy(null);
    }
  }

  async function sendFilesToDevice(device: DeviceDto) {
    const payload = transferPaths;
    if (payload.length === 0) {
      setMode("send");
      setError("未选择文件");
      return;
    }

    setBusy("send");
    setError(null);
    setSendReport(null);
    try {
      const report = await invokeCommand<SendReportDto>("send_paths_to_device", {
        deviceId: device.id,
        pathsText: payload.join("\n")
      });
      setSendReport(report);
      setToast(`已发送到 ${device.name}`);
      await refreshTransfers();
    } catch (nextError) {
      setMode("send");
      const message = errorMessage(nextError);
      if (isCancelMessage(message)) {
        setToast("传输已取消");
      } else {
        setError(message);
      }
      await refreshTransfers().catch(() => undefined);
    } finally {
      setBusy(null);
    }
  }

  async function sendCurrentTransfer() {
    if (transferPaths.length === 0) {
      setMode("send");
      setError("未选择文件");
      return;
    }

    if (selectedDevice) {
      await sendFilesToDevice(selectedDevice);
      return;
    }

    if (trimmedConnectionCode.length > 0) {
      await sendFiles();
      return;
    }

    setMode("send");
    setError("选择目标");
  }

  async function cancelCurrentTransfer() {
    setBusy("cancel-transfer");
    setError(null);
    try {
      if (transferStatus?.direction === "receive" && isReceiveTransferActivePhase(transferStatus.phase)) {
        await invokeCommand<void>("stop_receive_once");
        setReceiveSession(null);
        setPendingReceiveOffer(null);
        setReceiveStatus("正在取消接收");
        setToast("正在取消接收");
      } else {
        await invokeCommand<void>("cancel_current_transfer");
        setToast("正在取消发送");
      }
      await refreshReceiveState({ includeDiagnostics: true, includeDirectoryState: true });
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function resendTransfer(transfer: TransferDto) {
    setBusy("resend");
    setError(null);
    setSendReport(null);
    try {
      const report = await invokeCommand<SendReportDto>("resend_transfer", { transferId: transfer.id });
      setSendReport(report);
      setMode("send");
      setToast(`重发完成：${report.file_count} 个文件`);
      await refreshTransfers();
    } catch (nextError) {
      const message = errorMessage(nextError);
      if (isCancelMessage(message)) {
        setToast("传输已取消");
      } else {
        setError(message);
      }
      await refreshTransfers().catch(() => undefined);
    } finally {
      setBusy(null);
    }
  }

  async function openTransferLocation(transfer: TransferDto) {
    setBusy("open");
    setError(null);
    try {
      await invokeCommand<void>("open_transfer_location", { transferId: transfer.id });
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function deleteTransfer(transfer: TransferDto) {
    setBusy("history");
    setError(null);
    try {
      await invokeCommand<void>("delete_transfer", { transferId: transfer.id });
      setSelectedTransferId((current) => (current === transfer.id ? null : current));
      await refreshTransfers();
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function clearTransferHistory() {
    if (transfers.length === 0) return;
    setBusy("history");
    setError(null);
    try {
      await invokeCommand<void>("clear_transfer_history");
      setSelectedTransferId(null);
      await refreshTransfers();
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function requestPairing(device: DeviceDto) {
    setBusy("pair");
    setError(null);
    try {
      const trusted = await invokeCommand<TrustedDeviceDto>("request_device_pairing", { deviceId: device.id });
      setSelectedDeviceId(trusted.device_id);
      setSelectedDeviceSnapshot({
        ...device,
        trust_state: "Trusted",
        pairing_code: trusted.pairing_code
      });
      setConnectionCodeOpen(false);
      setToast(`配对完成：${trusted.device_name} · ${trusted.pairing_code}`);
      await refreshReceiveState({ includeDirectoryState: true });
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function respondPairingRequest(accept: boolean) {
    setBusy("pair");
    setError(null);
    try {
      await invokeCommand<void>("respond_pairing_request", { accept });
      setPendingPairingRequest(null);
      setToast(accept ? "已接受配对" : "已拒绝配对");
      await refreshReceiveState({ includeDirectoryState: true });
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function forgetTrustedDevice(device: TrustedDeviceDto) {
    setBusy("forget");
    setError(null);
    try {
      await invokeCommand<void>("forget_trusted_device", { deviceId: device.device_id });
      if (selectedDeviceId === device.device_id) {
        setSelectedDeviceId(null);
        setSelectedDeviceSnapshot(null);
      }
      setToast(`已移除：${device.device_name}`);
      await refreshReceiveState({ includeDirectoryState: true });
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function respondReceiveOffer(accept: boolean) {
    setBusy("receive");
    setError(null);
    try {
      await invokeCommand<void>("respond_receive_offer", { accept });
      setPendingReceiveOffer(null);
      setToast(accept ? "已接受传输" : "已拒绝传输");
      await refreshReceiveState();
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function copyConnectionCode() {
    if (!receiveSession?.connection_code) return;
    setError(null);
    try {
      await copyTextToClipboard(receiveSession.connection_code);
      setToast("连接码已复制");
    } catch (nextError) {
      setError(errorMessage(nextError));
    }
  }

  // ---------------------------------------------------------
  // 本地桥与资料包功能 / Local Bridge & Bundle Functions
  // ---------------------------------------------------------

  async function refreshLocalBridgeStatus() {
    const status = await invokeCommand<LocalBridgeRuntimeStatusDto>("get_local_bridge_runtime_status");
    setLocalBridgeStatus((current) => keepIfEqual(current, status));
  }

  async function refreshLocalBridgeAuthorizations() {
    const response = await invokeCommand<LocalBridgeAuthorizationListDto>("list_local_bridge_authorizations");
    setLocalBridgeAuthorizations((current) => keepIfEqual(current, response.authorizations));
  }

  async function refreshLocalBridgePendingActions() {
    const response = await invokeCommand<LocalBridgePendingActionListDto>("list_local_bridge_pending_actions");
    setLocalBridgePendingActions((current) => keepIfEqual(current, response.actions));
  }

  async function refreshLocalBridgeActionResults() {
    const response = await invokeCommand<LocalBridgePendingActionResultListDto>("list_local_bridge_pending_action_results");
    setLocalBridgeActionResults((current) => keepIfEqual(current, response.results));
  }

  async function runLocalBridgeSelfCheck() {
    setBusy("open");
    setError(null);
    try {
      const response = await invokeCommand<LocalBridgeResponseDto>("handle_local_bridge_request", {
        requestJson: JSON.stringify({
          kind: "devices.list",
          payload: {
            request_id: `settings-self-check-${Date.now()}`,
            trusted_only: true,
            client: { client_id: "nekodrop.settings", display_name: "NekoDrop Settings" }
          }
        })
      });
      setLocalBridgeCheck(
        response.authorization_code
          ? `授权就绪 · 授权码 ${response.authorization_code}`
          : `自测成功 · ${response.devices.length} 台可信设备`
      );
      await refreshLocalBridgeStatus();
    } catch (nextError) {
      setLocalBridgeCheck("自测失败");
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function confirmLocalBridgeAuthorization() {
    const code = localBridgeAuthorizationCode.trim();
    if (!code) {
      setLocalBridgeCheck("请输入授权码");
      return;
    }
    setBusy("open");
    setError(null);
    try {
      const authorization = await invokeCommand<LocalBridgeAuthorizationDto>("confirm_local_bridge_authorization", {
        authorizationCode: code
      });
      setLocalBridgeAuthorizationCode("");
      setLocalBridgeCheck(`已授权 ${authorization.display_name}`);
      await refreshLocalBridgeStatus();
      await refreshLocalBridgeAuthorizations();
    } catch (nextError) {
      setLocalBridgeCheck("授权失败");
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function removeLocalBridgePendingAction(action: LocalBridgePendingActionDto) {
    setBusy("open");
    setError(null);
    try {
      const response = await invokeCommand<{ actions: LocalBridgePendingActionDto[]; removed: boolean }>("remove_local_bridge_pending_action", {
        requestId: action.request_id
      });
      setLocalBridgePendingActions((current) => keepIfEqual(current, response.actions));
      setToast("已处理该请求");
      await refreshLocalBridgeStatus();
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function revokeLocalBridgeAuthorization(authorization: LocalBridgeAuthorizationDto, scope: string) {
    setBusy("open");
    setError(null);
    try {
      const response = await invokeCommand<{ authorizations: LocalBridgeAuthorizationDto[]; revoked: boolean }>("revoke_local_bridge_authorization", {
        clientId: authorization.client_id,
        scope
      });
      setLocalBridgeAuthorizations((current) => keepIfEqual(current, response.authorizations));
      setToast("已撤销授权");
      await refreshLocalBridgeStatus();
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function pruneLocalBridgeAuthorizations() {
    setBusy("open");
    setError(null);
    try {
      const response = await invokeCommand<LocalBridgeAuthorizationListDto>("prune_local_bridge_authorizations");
      setLocalBridgeAuthorizations((current) => keepIfEqual(current, response.authorizations));
      setToast("已清理过期授权");
      await refreshLocalBridgeStatus();
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function importCurrentStagedBundle(bundle: ReceivedBundleDto) {
    setBusy("bundle-import");
    setError(null);
    try {
      const imported = await invokeCommand<ReceivedBundleDto>("import_staged_bundle", { bundleId: bundle.bundle_id });
      setStagedBundles((current) => current.map((item) => (item.bundle_id === imported.bundle_id ? imported : item)));
      setToast(`已导入：${imported.display_name}`);
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function deleteCurrentStagedBundle(bundle: ReceivedBundleDto) {
    setBusy("receive");
    setError(null);
    try {
      await invokeCommand<boolean>("delete_staged_bundle", { bundleId: bundle.bundle_id });
      setStagedBundles((current) => current.filter((item) => item.bundle_id !== bundle.bundle_id));
      setToast("已删除暂存资料包");
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  // ---------------------------------------------------------
  // 暴露的值 / Exposed Values
  // ---------------------------------------------------------

  const value: AppContextType = {
    snapshot,
    selectedPaths,
    manualPaths,
    connectionCode,
    receiveDir,
    receivePolicy,
    bindPort,
    deviceNameInput,
    plan,
    scanStatus,
    sendReport,
    nearbyDevices,
    discoveryStatus,
    receiveSession,
    receiveDiagnostics,
    receiveStatus,
    receiveReport,
    pendingReceiveOffer,
    pendingPairingRequest,
    transferStatus,
    transfers,
    trustedDevices,
    stagedBundles,
    selectedTransferId,
    selectedDeviceId,
    selectedDeviceSnapshot,
    connectionCodeOpen,
    localBridgeStatus,
    localBridgeAuthorizations,
    localBridgePendingActions,
    localBridgeActionResults,
    localBridgeCheck,
    localBridgeAuthorizationCode,
    mode,
    appearance,
    dragActive,
    dragDropReady,
    busy,
    error,
    toast,
    transferMetrics,

    setManualPaths,
    setConnectionCode,
    setBindPort,
    setDeviceNameInput,
    setSelectedTransferId,
    setSelectedDeviceId,
    setConnectionCodeOpen,
    setLocalBridgeAuthorizationCode,
    setMode,
    setAppearance,
    setError,
    setToast,

    refreshSnapshot,
    refreshReceiveState,
    pickFiles,
    pickFolders,
    removePath,
    clearQueue,
    chooseReceiveDir,
    saveReceiveDir,
    saveReceivePort,
    updateReceivePolicy,
    saveDeviceName,
    openPath,
    scanPaths,
    startReceive,
    stopReceive,
    sendFilesToDevice,
    sendCurrentTransfer,
    cancelCurrentTransfer,
    resendTransfer,
    openTransferLocation,
    deleteTransfer,
    clearTransferHistory,
    requestPairing,
    respondPairingRequest,
    forgetTrustedDevice,
    respondReceiveOffer,
    copyConnectionCode,

    runLocalBridgeSelfCheck,
    confirmLocalBridgeAuthorization,
    removeLocalBridgePendingAction,
    revokeLocalBridgeAuthorization,
    pruneLocalBridgeAuthorizations,
    importCurrentStagedBundle,
    deleteCurrentStagedBundle
  };

  return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
}

export function useAppContext() {
  const context = useContext(AppContext);
  if (context === undefined) {
    throw new Error("useAppContext 必须在 AppProvider 中使用");
  }
  return context;
}
