import {
  EMPTY_TRANSFER_METRICS, TransferMetrics, BusyMode, ComposerMode, AppearanceMode,
  ReceivePolicyMode, errorMessage, normalizeReceivePolicy, readInitialAppearance,
  portFromBindAddr, buildPathPayload, uniquePaths, keepIfEqual, resetTransferMetrics, APPEARANCE_STORAGE_KEY,
  lastPathSegment, isReceiveTransferActivePhase, isCancelMessage, copyTextToClipboard,
  readTextSnippetAutoCopy, writeTextSnippetAutoCopy,
  deviceIdAtDropPosition,
  QueuedSend, QueuedSendKind, enqueueSend, dequeueSend, queuedSendLabel,
  resumeQueuedSend as resumeQueuedEntry, saveSendQueue, loadSendQueue
} from "./helpers";
import { useSettingsDomain } from "./settings";
import { useComposerDomain } from "./composer";
import { useInboxDomain } from "./inbox";
import { useBridgeDomain } from "./bridge";
import React, { createContext, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { bindWindowDragDrop } from "../dragDrop";
import { checkForUpdate, type UpdateInfo } from "../updateCheck";
import { invokeCommand, isTauriRuntime } from "../tauri";
import {
  bundleImportStrategyLabel,
  markBundleDeleted,
  markBundleImportFailed
} from "../bundleState";
import { localBridgeStatusLabel } from "../localBridgeState";
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


// ---------------------------------------------------------
// Context 接口定义 / Context Interface Definition
// ---------------------------------------------------------

interface AppContextType {
  snapshot: AppSnapshot | null;
  selectedPaths: string[];
  manualPaths: string;
  manualBundleType: string;
  manualBundleSourcePath: string;
  manualBundleDisplayName: string;
  manualBundleSourceApp: string;
  createdManualBundle: ManualBundleCreateDto | null;
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
  selectedDevice: DeviceDto | null;
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
  setManualBundleType: (val: string) => void;
  setManualBundleDisplayName: (val: string) => void;
  setManualBundleSourceApp: (val: string) => void;
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
  chooseManualBundleSourceDir: () => Promise<void>;
  createManualBundleForSend: () => Promise<void>;
  removePath: (path: string) => void;
  clearQueue: () => void;
  chooseReceiveDir: () => Promise<void>;
  saveReceiveDir: () => Promise<void>;
  saveReceivePort: () => Promise<void>;
  updateReceivePolicy: (policy: ReceivePolicyMode) => Promise<void>;
  sendLimitInput: string;
  setSendLimitInput: (value: string) => void;
  saveSendLimit: () => Promise<void>;
  updateOrganizeByDevice: (enabled: boolean) => Promise<void>;
  updateIrohReceiveMode: (mode: "off" | "direct" | "relay") => Promise<void>;
  updateInfo: UpdateInfo | null;
  saveDeviceName: () => Promise<void>;
  openPath: (path: string) => Promise<void>;
  scanPaths: (paths?: string[], manual?: string) => Promise<void>;
  startReceive: (options?: { receiveDirOverride?: string; receivePortOverride?: number; silent?: boolean }) => Promise<void>;
  stopReceive: () => Promise<void>;
  sendFilesToDevice: (device: DeviceDto) => Promise<void>;
  sendCurrentTransfer: (textSnippet?: string) => Promise<void>;
  sendDroppedPathsTo: (device: DeviceDto, paths: string[]) => Promise<void>;
  sendQueue: QueuedSend[];
  pauseCurrentTransfer: () => Promise<void>;
  resumeQueuedSendById: (id: string) => void;
  cancelQueuedSend: (id: string) => void;
  clearSendQueue: () => void;
  cancelCurrentTransfer: () => Promise<void>;
  resendTransfer: (transfer: TransferDto) => Promise<void>;
  openTransferLocation: (transfer: TransferDto) => Promise<void>;
  deleteTransfer: (transfer: TransferDto) => Promise<void>;
  clearTransferHistory: () => Promise<void>;
  requestPairing: (device: DeviceDto) => Promise<void>;
  respondPairingRequest: (accept: boolean) => Promise<void>;
  forgetTrustedDevice: (device: TrustedDeviceDto) => Promise<void>;
  setTrustedDeviceAlias: (deviceId: string, alias: string) => Promise<void>;
  autoCopyTextSnippets: boolean;
  setTextSnippetAutoCopy: (enabled: boolean) => void;
  respondReceiveOffer: (accept: boolean) => Promise<void>;
  copyConnectionCode: () => Promise<void>;

  // 本地桥方法 / Local Bridge Methods
  runLocalBridgeSelfCheck: () => Promise<void>;
  confirmLocalBridgeAuthorization: () => Promise<void>;
  removeLocalBridgePendingAction: (action: LocalBridgePendingActionDto) => Promise<void>;
  respondLocalBridgePendingAction: (action: LocalBridgePendingActionDto, accept: boolean) => Promise<void>;
  revokeLocalBridgeAuthorization: (auth: LocalBridgeAuthorizationDto, scope: string) => Promise<void>;
  pruneLocalBridgeAuthorizations: () => Promise<void>;
  importCurrentStagedBundle: (bundle: ReceivedBundleDto, conflictStrategy?: string) => Promise<void>;
  rollbackCurrentBundle: (bundle: ReceivedBundleDto) => Promise<void>;
  deleteCurrentStagedBundle: (bundle: ReceivedBundleDto) => Promise<void>;
}

const AppContext = createContext<AppContextType | undefined>(undefined);

// ---------------------------------------------------------
// Context 提供者实现 / Context Provider Implementation
// ---------------------------------------------------------

export function AppProvider({ children }: { children: ReactNode }) {
  const [connectionCode, setConnectionCode] = useState("");
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
  const [selectedTransferId, setSelectedTransferId] = useState<string | null>(null);
  const [selectedDeviceId, setSelectedDeviceId] = useState<string | null>(null);
  const [selectedDeviceSnapshot, setSelectedDeviceSnapshot] = useState<DeviceDto | null>(null);
  const [connectionCodeOpen, setConnectionCodeOpen] = useState(false);
  const [mode, setMode] = useState<ComposerMode>("send");
  const [appearance, setAppearance] = useState<AppearanceMode>(() => readInitialAppearance());
  const [autoCopyTextSnippets, setAutoCopyTextSnippetsState] = useState(() =>
    readTextSnippetAutoCopy()
  );
  const seenTextSnippetIds = useRef<Set<string> | null>(null);

  const [dragActive, setDragActive] = useState(false);
  const [dragDropReady, setDragDropReady] = useState(false);
  const [busy, setBusy] = useState<BusyMode | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [transferMetrics, setTransferMetrics] = useState<TransferMetrics>(EMPTY_TRANSFER_METRICS);
  const [sendQueue, setSendQueue] = useState<QueuedSend[]>(() => loadSendQueue());
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const queueFlushInFlight = useRef(false);
  const queueSeq = useRef(0);
  // 正在发送的载荷快照：暂停时原样回队（协议断点续传，继续时接着传）
  const activeSendRef = useRef<{ kind: QueuedSendKind; target: string; pathsText: string } | null>(null);
  const nearbyDevicesRef = useRef<DeviceDto[]>([]);
  nearbyDevicesRef.current = nearbyDevices;

  const desktopRuntime = useMemo(() => isTauriRuntime(), []);
  const previousTransferStatus = useRef<TransferStatusDto | null>(null);
  const autoReceiveStarted = useRef(false);
  const realtimeRefreshInFlight = useRef(false);
  const directoryRefreshInFlight = useRef(false);
  const transfersRefreshSeq = useRef(0);
  const diagnosticsRefreshInFlight = useRef(false);
  const lastDirectoryRefreshAt = useRef(0);
  const lastDiagnosticsRefreshAt = useRef(0);
  const previousMode = useRef<ComposerMode | null>(null);

const {
    snapshot, receiveDir, bindPort, receivePolicy, deviceNameInput,
    sendLimitInput, setSendLimitInput, saveSendLimit, updateOrganizeByDevice, updateIrohReceiveMode,
    setReceiveDir, setBindPort, setReceivePolicy, setDeviceNameInput,
    refreshSnapshot, chooseReceiveDir, saveReceiveDir, saveReceivePort,
    updateReceivePolicy, saveDeviceName, openPath,
  } = useSettingsDomain({ setBusy, setError, setToast, refreshReceiveState, receiveSession, parseReceivePortValue });

  const {
    selectedPaths, setSelectedPaths, manualPaths, setManualPaths,
    manualBundleType, setManualBundleType, manualBundleDisplayName, setManualBundleDisplayName,
    manualBundleSourceApp, setManualBundleSourceApp, manualBundleSourcePath, setManualBundleSourcePath,
    plan, setPlan, scanStatus, setScanStatus,
    createdManualBundle, setCreatedManualBundle,
    pickFiles, pickFolders, chooseManualBundleSourceDir, createManualBundleForSend,
    applyPickedPaths, removePath, clearQueue, scanPaths,
  } = useComposerDomain({ setBusy, setError, setToast, setMode, setSendReport });

  const {
    stagedBundles, setStagedBundles,
    importCurrentStagedBundle, rollbackCurrentBundle, deleteCurrentStagedBundle,
  } = useInboxDomain({ setBusy, setError, setToast, setReceiveReport, refreshDirectoryState, refreshReceiveState });

  const {
    localBridgeStatus, setLocalBridgeStatus,
    localBridgeAuthorizations, setLocalBridgeAuthorizations,
    localBridgePendingActions, setLocalBridgePendingActions,
    localBridgeActionResults, setLocalBridgeActionResults,
    localBridgeCheck, setLocalBridgeCheck,
    localBridgeAuthorizationCode, setLocalBridgeAuthorizationCode,
    refreshLocalBridgeStatus, refreshLocalBridgeAuthorizations,
    refreshLocalBridgePendingActions, refreshLocalBridgeActionResults,
    runLocalBridgeSelfCheck, confirmLocalBridgeAuthorization,
    removeLocalBridgePendingAction, respondLocalBridgePendingAction,
    revokeLocalBridgeAuthorization, pruneLocalBridgeAuthorizations,
  } = useBridgeDomain({ setBusy, setError, setToast, refreshDirectoryState });

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

  // 应用内更新检查：启动后读 GitHub Releases 最新 tag，有新版本才写 updateInfo
  // （设置页提示，不自动下载）；检查失败静默，保持「已是最新」显示。
  useEffect(() => {
    if (!desktopRuntime) return;
    let active = true;
    checkForUpdate()
      .then((info) => {
        if (active) setUpdateInfo(info);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [desktopRuntime]);

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
    if (!latestDevice || latestDevice.trust_state !== "Trusted") {
      // The device went offline or lost trust: drop the stale snapshot so the
      // composer stops offering it as a valid send target.
      setSelectedDeviceSnapshot(null);
      return;
    }
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
      setTransferMetrics((current) => resetTransferMetrics(current));
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

  // 接收文本自动复制：新出现的"接收成功·单 txt"记录直接进剪贴板（可在设置关闭）。
  // 首次运行先把既有历史标记为已见，避免启动时复制旧文本。
  const autoCopyRef = useRef(autoCopyTextSnippets);
  autoCopyRef.current = autoCopyTextSnippets;
  useEffect(() => {
    const snippetRecords = transfers.filter(
      (transfer) =>
        transfer.direction === "receive" &&
        (transfer.status === "succeeded" || transfer.status === "done") &&
        transfer.file_count === 1 &&
        (transfer.root_name ?? "").toLowerCase().endsWith(".txt")
    );
    if (seenTextSnippetIds.current == null) {
      seenTextSnippetIds.current = new Set(snippetRecords.map((transfer) => transfer.id));
      return;
    }
    const fresh = snippetRecords.filter((transfer) => !seenTextSnippetIds.current!.has(transfer.id));
    for (const transfer of fresh) seenTextSnippetIds.current!.add(transfer.id);
    if (fresh.length === 0 || !autoCopyRef.current) return;
    const target = fresh[fresh.length - 1];
    void (async () => {
      try {
        const text = await invokeCommand<string>("read_received_text", { transferId: target.id });
        await copyTextToClipboard(text);
        setToast(`文本已自动复制（${target.root_name}）`);
      } catch {
        // 读取或剪贴板失败时静默：手动复制按钮仍在历史页可用
      }
    })();
  }, [transfers]);

  function setTextSnippetAutoCopy(enabled: boolean) {
    setAutoCopyTextSnippetsState(enabled);
    writeTextSnippetAutoCopy(enabled);
  }

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    if (!desktopRuntime) {
      setDragDropReady(false);
      return;
    }

    void bindWindowDragDrop({
      onActiveChange: setDragActive,
      onDrop: (paths, position) => {
        // 命中侧栏设备行 → 直接发往该设备；否则按普通拖放进队列
        const ratio = window.devicePixelRatio || 1;
        const deviceId = deviceIdAtDropPosition(position, ratio);
        const device = deviceId ? nearbyDevicesRef.current.find((d) => d.id === deviceId) : null;
        if (device) {
          void sendDroppedPathsTo(device, paths).catch((nextError) =>
            setError(errorMessage(nextError))
          );
          return;
        }
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
    const requestId = ++transfersRefreshSeq.current;
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
      if (requestId === transfersRefreshSeq.current) {
        setTransfers((current) => keepIfEqual(current, nextTransfers));
      }
      setStagedBundles((current) => keepIfEqual(current, nextStagedBundles));
      lastDirectoryRefreshAt.current = Date.now();
    } finally {
      directoryRefreshInFlight.current = false;
    }
  }

  async function refreshTransfers() {
    // Monotonic id: a slow stale response (e.g. a poll racing a delete) must
    // never overwrite the result of a newer request.
    const requestId = ++transfersRefreshSeq.current;
    const nextTransfers = await invokeCommand<TransferDto[]>("list_transfers");
    if (requestId !== transfersRefreshSeq.current) return;
    setTransfers((current) => keepIfEqual(current, nextTransfers));
  }






  const applyPickedPathsRef = useRef(applyPickedPaths);
  applyPickedPathsRef.current = applyPickedPaths;






  function parseReceivePortValue(val: string): number | null {
    const parsed = parseInt(val.trim(), 10);
    return isNaN(parsed) || parsed < 1 || parsed > 65535 ? null : parsed;
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

  // 共享发送内核：设备或连接码，直接发送与队列出队共用
  async function runSend(kind: QueuedSendKind, target: string, pathsText: string, successToast: string) {
    activeSendRef.current = { kind, target, pathsText };
    setBusy("send");
    setError(null);
    setSendReport(null);
    try {
      const command = kind === "device" ? "send_paths_to_device" : "send_paths_to_code";
      const args =
        kind === "device"
          ? { deviceId: target, pathsText }
          : { connectionCode: target, pathsText };
      const report = await invokeCommand<SendReportDto>(command, args);
      setSendReport(report);
      setToast(`${successToast}：${report.file_count} 个文件`);
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

  async function sendFiles() {
    const payload = transferPaths;
    if (payload.length === 0) {
      setMode("send");
      setError("未选择文件");
      return;
    }

    await runSend("code", trimmedConnectionCode, payload.join("\n"), "发送完成");
  }

  async function sendFilesToDevice(device: DeviceDto) {
    const payload = transferPaths;
    if (payload.length === 0) {
      setMode("send");
      setError("未选择文件");
      return;
    }

    await runSend("device", device.id, payload.join("\n"), `已发送到 ${device.name}`);
  }

  async function sendCurrentTransfer(textSnippet?: string) {
    let payload = transferPaths;
    if (textSnippet != null && textSnippet.trim().length > 0) {
      try {
        const snippetPath = await invokeCommand<string>("stage_text_snippet", {
          text: textSnippet
        });
        payload = uniquePaths([snippetPath, ...payload]);
      } catch (nextError) {
        setError(errorMessage(nextError));
        return;
      }
    }
    if (payload.length === 0) {
      setMode("send");
      setError("未选择文件");
      return;
    }

    // 解析目标：优先设备，其次连接码
    let kind: QueuedSendKind | null = null;
    let target = "";
    let successToast = "";
    if (selectedDevice) {
      kind = "device";
      target = selectedDevice.id;
      successToast = `已发送到 ${selectedDevice.name}`;
    } else if (trimmedConnectionCode.length > 0) {
      kind = "code";
      target = trimmedConnectionCode;
      successToast = "发送完成";
    } else {
      setMode("send");
      setError("选择目标");
      return;
    }

    await dispatchSend(kind, target, payload.join("\n"), successToast);
  }

  // 统一发送入口：空闲直发，忙线入队（拖拽直发与组合框共用）
  async function dispatchSend(
    kind: QueuedSendKind,
    target: string,
    pathsText: string,
    successToast: string
  ) {
    if (pathsText.trim().length === 0) {
      setMode("send");
      setError("未选择文件");
      return;
    }
    if (busy !== null) {
      queueSeq.current += 1;
      const entry: QueuedSend = {
        id: `queue-${Date.now()}-${queueSeq.current}`,
        kind,
        target,
        pathsText,
        label: queuedSendLabel(pathsText),
        enqueuedAtMs: Date.now()
      };
      const next = enqueueSend(sendQueue, entry);
      if (next === sendQueue) {
        setError(`队列已满（最多 20 项）`);
        return;
      }
      setSendQueue(next);
      setToast(`已加入队列（第 ${next.length} 位）：${entry.label}`);
      return;
    }

    await runSend(kind, target, pathsText, successToast);
  }

  // 拖文件到侧栏设备行：跳过组合框状态，直接发往该设备
  async function sendDroppedPathsTo(device: DeviceDto, paths: string[]) {
    const payload = uniquePaths(paths);
    if (payload.length === 0) return;
    setSelectedDeviceId(device.id);
    setConnectionCodeOpen(false);
    await dispatchSend("device", device.id, payload.join("\n"), `已发送到 ${device.name}`);
  }

  // 队列变更落盘（重启恢复）；暂停条目一并保留
  useEffect(() => {
    saveSendQueue(sendQueue);
  }, [sendQueue]);

  // 空闲时自动出队发送（跳过暂停条目；失败不回队，报错继续下一条）
  useEffect(() => {
    if (busy !== null || queueFlushInFlight.current) return;
    const { head, rest } = dequeueSend(sendQueue);
    if (!head) return;
    queueFlushInFlight.current = true;
    setSendQueue(rest);
    void runSend(head.kind, head.target, head.pathsText, `队列发送完成（${head.label}）`).catch(() => {
      // 失败自动重排队一次：Network 类瞬时错误常见于对端刚上线/拥塞；
      // retryOf 计数防死循环——同一条最多自动重试 1 次，再失败就留给用户手动
      if ((head.retryOf ?? 0) < 1) {
        setSendQueue((current) => enqueueSend(current, { ...head, retryOf: (head.retryOf ?? 0) + 1 }));
      }
    }).finally(() => {
      queueFlushInFlight.current = false;
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [busy, sendQueue]);

  // 暂停：取消当前发送（协议断点续传），原载荷标记为已暂停回队
  async function pauseCurrentTransfer() {
    const snapshot = activeSendRef.current;
    if (!snapshot) return;
    queueSeq.current += 1;
    const entry: QueuedSend = {
      id: `queue-${Date.now()}-${queueSeq.current}`,
      kind: snapshot.kind,
      target: snapshot.target,
      pathsText: snapshot.pathsText,
      label: queuedSendLabel(snapshot.pathsText),
      enqueuedAtMs: Date.now(),
      paused: true
    };
    setSendQueue((current) => enqueueSend(current, entry));
    setToast(`已暂停：${entry.label}（继续时接着传）`);
    await cancelCurrentTransfer();
  }

  function resumeQueuedSendById(id: string) {
    setSendQueue((current) => resumeQueuedEntry(current, id));
  }

  function cancelQueuedSend(id: string) {
    setSendQueue((current) => current.filter((entry) => entry.id !== id));
  }

  function clearSendQueue() {
    setSendQueue([]);
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

  async function setTrustedDeviceAlias(deviceId: string, alias: string) {
    setBusy("forget");
    setError(null);
    try {
      await invokeCommand<TrustedDeviceDto>("set_trusted_device_alias", { deviceId, alias });
      setToast(alias.trim() ? `备注已更新：${alias.trim()}` : "备注已清除");
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














  // ---------------------------------------------------------
  // 暴露的值 / Exposed Values
  // ---------------------------------------------------------

  const value: AppContextType = {
    snapshot,
    updateInfo,
    selectedPaths,
    manualPaths,
    manualBundleType,
    manualBundleSourcePath,
    manualBundleDisplayName,
    manualBundleSourceApp,
    createdManualBundle,
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
    selectedDevice,
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
    setManualBundleType,
    setManualBundleDisplayName,
    setManualBundleSourceApp,
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
    chooseManualBundleSourceDir,
    createManualBundleForSend,
    removePath,
    clearQueue,
    chooseReceiveDir,
    saveReceiveDir,
    saveReceivePort,
    sendLimitInput,
    setSendLimitInput,
    saveSendLimit,
    updateOrganizeByDevice, updateIrohReceiveMode,
    updateReceivePolicy,
    saveDeviceName,
    openPath,
    scanPaths,
    startReceive,
    stopReceive,
    sendFilesToDevice,
    sendCurrentTransfer,
    sendDroppedPathsTo,
    sendQueue,
    pauseCurrentTransfer,
    resumeQueuedSendById,
    cancelQueuedSend,
    clearSendQueue,
    cancelCurrentTransfer,
    resendTransfer,
    openTransferLocation,
    deleteTransfer,
    clearTransferHistory,
    requestPairing,
    respondPairingRequest,
    forgetTrustedDevice,
    setTrustedDeviceAlias,
    autoCopyTextSnippets,
    setTextSnippetAutoCopy,
    respondReceiveOffer,
    copyConnectionCode,

    runLocalBridgeSelfCheck,
    confirmLocalBridgeAuthorization,
    removeLocalBridgePendingAction,
    respondLocalBridgePendingAction,
    revokeLocalBridgeAuthorization,
    pruneLocalBridgeAuthorizations,
    importCurrentStagedBundle,
    rollbackCurrentBundle,
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
