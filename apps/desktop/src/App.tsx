import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";

import { bindWindowDragDrop } from "./dragDrop";
import { invokeCommand, isTauriRuntime } from "./tauri";
import {
  buildNearbyDeviceViewModel,
  buildTrustedDeviceViewModel,
  platformLabel as devicePlatformLabel,
  selectedTrustedTargetCopy,
  trustStateLabel
} from "./deviceDisplay";
import {
  currentTransferRecoveryActions,
  findCurrentRecoverableTransfer
} from "./currentTransferRecovery";
import {
  receiveDiagnosticsAdvice
} from "./receiveDiagnostics";
import {
  buildDiscoveryCopy
} from "./networkPermissionHints";
import { pairingFailureAdvice } from "./pairingFailureAdvice";
import {
  buildTransferSecurityViewModel,
  receiveSecuritySummaryLine,
  type TransferSecurityViewModel
} from "./securityState";
import {
  bundleStatusLabel,
  bundleTypeLabel,
  markBundleDeleted,
  markBundleImportFailed,
  receiveBundleImportHint,
  receiveBundleStatusLabel,
  receiveBundleSummaryLine
} from "./bundleState";
import {
  shouldRunDiagnosticsRefresh,
  REALTIME_REFRESH_INTERVAL_MS,
  shouldRefreshDirectoryOnModeActivation,
  shouldRefreshDirectoryForMode,
  shouldRunDirectoryRefresh,
  STARTUP_SLOW_REFRESH_DELAY_MS
} from "./refreshSchedule";
import {
  buildRecentTransferDetailLine,
  buildTransferHistoryDetailViewModel,
  transferFallbackActionLabel,
  transferPrimaryActionLabel
} from "./transferHistoryDetails";
import { buildSettingsViewModel, parseReceivePortValue } from "./settingsView";
import {
  buildTransferProgressViewModel,
  formatBytes,
  shouldShowActiveTransferBar,
  shouldShowSendPageStatusLine,
  shouldShowTransferProgressMeter
} from "./transferProgress";
import type {
  AppSnapshot,
  DesktopRealtimeSnapshotDto,
  DeviceDto,
  DiscoveryStatusDto,
  LocalBridgeAuthorizationListDto,
  LocalBridgeAuthorizationRevokeDto,
  LocalBridgeAuthorizationDto,
  LocalBridgePendingActionDto,
  LocalBridgePendingActionListDto,
  LocalBridgePendingActionRemoveDto,
  LocalBridgePendingActionResultDto,
  LocalBridgePendingActionResultListDto,
  LocalBridgePermissionScope,
  LocalBridgeResponseDto,
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
  TransferStatusDto
} from "./types";

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

type ComposerMode =
  | "overview"
  | "send"
  | "receive"
  | "devices"
  | "transfers"
  | "settings";
type ReceivePolicyMode = "always_ask" | "block_all";
type AppearanceMode = "light" | "dark";
type TransferMetrics = {
  speedBytesPerSecond: number | null;
  etaSeconds: number | null;
};

const RECEIVE_POLICY_OPTIONS: Array<{ value: ReceivePolicyMode; label: string }> = [
  { value: "always_ask", label: "询问" },
  { value: "block_all", label: "阻止" }
];
const APPEARANCE_STORAGE_KEY = "nekodrop.appearance";

const EMPTY_TRANSFER_METRICS = Object.freeze<TransferMetrics>({
  speedBytesPerSecond: null,
  etaSeconds: null
});

type DeviceCapability = "transfer" | "agent" | "state" | "link";

const CAPABILITY_TABS: Array<{ id: DeviceCapability; label: string; lock?: string }> = [
  { id: "transfer", label: "传输" },
  { id: "agent", label: "Agent", lock: "V1.5" },
  { id: "state", label: "状态", lock: "V1.4" },
  { id: "link", label: "联机", lock: "规划" }
];

const CAPABILITY_LOCKED_COPY: Record<
  Exclude<DeviceCapability, "transfer">,
  { title: string; body: string; tag: string }
> = {
  agent: {
    title: "Agent 能力尚未接入",
    body: "未来可让这台已信任设备执行跨设备 Agent 任务：发起指令、回传结果、调用文件流，高风险操作需本机确认。能力由对端协商，对方支持时这里才会出现。",
    tag: "规划中 · V1.5"
  },
  state: {
    title: "状态同步尚未接入",
    body: "未来可与这台设备同步轻量状态（应用设置、任务状态等），离线可追平、冲突有明确策略，不静默覆盖。",
    tag: "规划中 · V1.4 · NekoState"
  },
  link: {
    title: "联机 / 虚拟局域网（规划）",
    body: "这是 1:多 的能力，不是一次性传输——未来会引入「房间」概念：把多台可信设备组进一个虚拟局域网，显示成员、隧道状态与延迟。它会单独成一类对象，依赖 relay / P2P 通道。",
    tag: "规划 · 依赖 relay / P2P"
  }
};

function readInitialAppearance(): AppearanceMode {
  if (typeof window === "undefined") {
    return "light";
  }
  return window.localStorage.getItem(APPEARANCE_STORAGE_KEY) === "dark" ? "dark" : "light";
}

export function App() {
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
  const [manualBundleType, setManualBundleType] = useState("workspace");
  const [manualBundleSourcePath, setManualBundleSourcePath] = useState("");
  const [manualBundleDisplayName, setManualBundleDisplayName] = useState("");
  const [manualBundleSourceApp, setManualBundleSourceApp] = useState("NekoDrop");
  const [createdManualBundle, setCreatedManualBundle] = useState<ManualBundleCreateDto | null>(null);
  const [selectedTransferId, setSelectedTransferId] = useState<string | null>(null);
  const [selectedDeviceId, setSelectedDeviceId] = useState<string | null>(null);
  const [selectedDeviceSnapshot, setSelectedDeviceSnapshot] = useState<DeviceDto | null>(null);
  const [connectionCodeOpen, setConnectionCodeOpen] = useState(false);
  const [capability, setCapability] = useState<DeviceCapability>("transfer");
  const [sendMode, setSendMode] = useState<"file" | "bundle">("file");
  const [localBridgeStatus, setLocalBridgeStatus] = useState<LocalBridgeRuntimeStatusDto | null>(null);
  const [localBridgeAuthorizations, setLocalBridgeAuthorizations] = useState<LocalBridgeAuthorizationDto[]>([]);
  const [localBridgePendingActions, setLocalBridgePendingActions] = useState<LocalBridgePendingActionDto[]>([]);
  const [localBridgeActionResults, setLocalBridgeActionResults] = useState<LocalBridgePendingActionResultDto[]>([]);
  const [localBridgeCheck, setLocalBridgeCheck] = useState<string | null>(null);
  const [localBridgeAuthorizationCode, setLocalBridgeAuthorizationCode] = useState("");
  const [mode, setMode] = useState<ComposerMode>("send");
  const [appearance, setAppearance] = useState<AppearanceMode>(() => readInitialAppearance());
  const [dragActive, setDragActive] = useState(false);
  const [dragDropReady, setDragDropReady] = useState(false);
  const desktopRuntime = useMemo(() => isTauriRuntime(), []);
  const [busy, setBusy] = useState<BusyMode | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const previousTransferStatus = useRef<TransferStatusDto | null>(null);
  const autoReceiveStarted = useRef(false);
  const realtimeRefreshInFlight = useRef(false);
  const directoryRefreshInFlight = useRef(false);
  const diagnosticsRefreshInFlight = useRef(false);
  const lastDirectoryRefreshAt = useRef(0);
  const lastDiagnosticsRefreshAt = useRef(0);
  const previousMode = useRef<ComposerMode | null>(null);
  const [transferMetrics, setTransferMetrics] = useState<TransferMetrics>(EMPTY_TRANSFER_METRICS);

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
  const currentFailedTransfer = useMemo(
    () => findCurrentRecoverableTransfer(transferStatus, transfers),
    [transferStatus, transfers]
  );
  const selectedTrustedRecord = useMemo(
    () => trustedDevices.find((device) => device.device_id === selectedDeviceId) ?? null,
    [selectedDeviceId, trustedDevices]
  );
  const selectedTrustedOnline = Boolean(
    selectedDeviceId && trustedNearbyDevices.some((device) => device.id === selectedDeviceId)
  );
  const selectedTargetCopy = selectedTrustedRecord
    ? selectedTrustedTargetCopy(selectedTrustedRecord, selectedTrustedOnline)
    : null;
  const localPlatform = snapshot?.device_identity.platform ?? null;
  const trimmedConnectionCode = connectionCode.trim();
  const canSend = transferPaths.length > 0 && !busy && (Boolean(selectedDevice) || trimmedConnectionCode.length > 0);
  const hasActiveTransfer = Boolean(transferStatus && shouldShowActiveTransferBar(transferStatus));
  const receiveState = receiveSession
    ? pendingPairingRequest
      ? "等待配对"
      : pendingReceiveOffer
      ? "等待确认"
      : "收件开启"
    : receiveStatus?.startsWith("收件已关闭")
      ? "收件关闭"
      : receiveStatus?.startsWith("接收完成")
      ? "接收完成"
      : receiveStatus?.startsWith("接收失败")
        ? "接收失败"
        : "收件关闭";

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

  useEffect(() => {
    if (!snapshot || receiveSession || autoReceiveStarted.current) return;
    autoReceiveStarted.current = true;
    startReceive({
      receiveDirOverride: snapshot.receive_dir,
      receivePortOverride: snapshot.receive_port,
      silent: true
    }).catch((nextError) =>
      setError(errorMessage(nextError))
    );
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
        void applyPickedPathsRef.current(paths).catch((nextError) =>
          setError(errorMessage(nextError))
        );
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
      setSnapshot((current) =>
        current ? { ...current, receive_dir: nextReceiveDir } : current
      );
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
      setSnapshot((current) =>
        current ? { ...current, receive_port: nextReceivePort } : current
      );
      setToast("默认端口已保存");
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function updateReceivePolicy(nextPolicy: ReceivePolicyMode) {
    if (nextPolicy === receivePolicy) return;
    setBusy("receive-policy");
    setError(null);
    try {
      await invokeCommand<void>("set_receive_policy", { receivePolicy: nextPolicy });
      setReceivePolicy(nextPolicy);
      setSnapshot((current) =>
        current ? { ...current, receive_policy: nextPolicy } : current
      );
      setToast(receivePolicyLabel(nextPolicy));
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
              device_identity: {
                ...current.device_identity,
                device_name: savedName
              }
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
      const nextPlan = await invokeCommand<TransferPlanDto>("create_transfer_plan", {
        paths: payload
      });
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
      const message = deviceSendErrorMessage(errorMessage(nextError));
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
      const actionLabel = transferPrimaryActionLabel(transfer) ?? "重发";
      const report = await invokeCommand<SendReportDto>("resend_transfer", {
        transferId: transfer.id
      });
      setSendReport(report);
      setMode("send");
      setToast(`${actionLabel}完成：${report.file_count} 个文件`);
      await refreshTransfers();
    } catch (nextError) {
      const message = deviceSendErrorMessage(errorMessage(nextError));
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
      await invokeCommand<void>("open_transfer_location", {
        transferId: transfer.id
      });
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
      await invokeCommand<void>("delete_transfer", {
        transferId: transfer.id
      });
      setSelectedTransferId((current) => current === transfer.id ? null : current);
      await refreshTransfers();
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
      await invokeCommand<boolean>("delete_staged_bundle", {
        bundleId: bundle.bundle_id
      });
      setStagedBundles((current) => current.filter((item) => item.bundle_id !== bundle.bundle_id));
      setReceiveReport((current) => {
        if (!current?.bundle || current.bundle.bundle_id !== bundle.bundle_id) return current;
        return { ...current, bundle: markBundleDeleted(current.bundle) };
      });
      setToast("已删除暂存资料包");
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
      const imported = await invokeCommand<ReceivedBundleDto>("import_staged_bundle", {
        bundleId: bundle.bundle_id
      });
      setStagedBundles((current) =>
        current.map((item) => item.bundle_id === imported.bundle_id ? imported : item)
      );
      setReceiveReport((current) => {
        if (!current?.bundle || current.bundle.bundle_id !== bundle.bundle_id) return current;
        return { ...current, bundle: imported };
      });
      setToast(`已导入：${imported.display_name}`);
    } catch (nextError) {
      setStagedBundles((current) =>
        current.map((item) => item.bundle_id === bundle.bundle_id ? markBundleImportFailed(item) : item)
      );
      setReceiveReport((current) => {
        if (!current?.bundle || current.bundle.bundle_id !== bundle.bundle_id) return current;
        return { ...current, bundle: markBundleImportFailed(current.bundle) };
      });
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function runLocalBridgeSelfCheck() {
    setBusy("open");
    setError(null);
    try {
      const response = await invokeCommand<LocalBridgeResponseDto>("handle_local_bridge_request", {
        requestJson: JSON.stringify({
          "kind": "devices.list",
          "payload": {
            request_id: `settings-self-check-${Date.now()}`,
            trusted_only: true,
            client: {
              client_id: "nekodrop.settings",
              display_name: "NekoDrop Settings"
            }
          }
        })
      });
      setLocalBridgeCheck(
        response.authorization_code
          ? `${localBridgeStatusLabel(response.status)} · 授权码 ${response.authorization_code}`
          : `${localBridgeStatusLabel(response.status)} · ${response.devices.length} 台可信设备 · ${response.staged_bundles.length} 个暂存资料包`
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
    const authorizationCode = localBridgeAuthorizationCode.trim();
    if (!authorizationCode) {
      setLocalBridgeCheck("请输入授权码");
      return;
    }
    setBusy("open");
    setError(null);
    try {
      const authorization = await invokeCommand<LocalBridgeAuthorizationDto>("confirm_local_bridge_authorization", {
        authorizationCode
      });
      setLocalBridgeAuthorizationCode("");
      setLocalBridgeCheck(`已授权 ${authorization.display_name} · ${authorization.scopes.join(" · ")}`);
      await refreshLocalBridgeStatus();
      await refreshLocalBridgeAuthorizations();
    } catch (nextError) {
      setLocalBridgeCheck("授权失败");
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function chooseManualBundleSourceDir() {
    setBusy("pick-folders");
    setError(null);
    try {
      const picked = await invokeCommand<string | null>("select_manual_bundle_source_dir");
      if (!picked) return;
      setManualBundleSourcePath(picked);
      if (!manualBundleDisplayName.trim()) {
        setManualBundleDisplayName(lastPathSegment(picked));
      }
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function refreshLocalBridgeStatus() {
    const status = await invokeCommand<LocalBridgeRuntimeStatusDto>("get_local_bridge_runtime_status");
    setLocalBridgeStatus((current) => keepIfEqual(current, status));
  }

  async function refreshLocalBridgeAuthorizations() {
    const response = await invokeCommand<LocalBridgeAuthorizationListDto>("list_local_bridge_authorizations");
    setLocalBridgeAuthorizations((current) => keepIfEqual(current, response.authorizations));
    if (response.pruned_count > 0) {
      await refreshLocalBridgeStatus();
    }
  }

  async function refreshLocalBridgePendingActions() {
    const response = await invokeCommand<LocalBridgePendingActionListDto>("list_local_bridge_pending_actions");
    setLocalBridgePendingActions((current) => keepIfEqual(current, response.actions));
  }

  async function refreshLocalBridgeActionResults() {
    const response = await invokeCommand<LocalBridgePendingActionResultListDto>("list_local_bridge_pending_action_results");
    setLocalBridgeActionResults((current) => keepIfEqual(current, response.results));
  }

  async function removeLocalBridgePendingAction(action: LocalBridgePendingActionDto) {
    setBusy("open");
    setError(null);
    try {
      const response = await invokeCommand<LocalBridgePendingActionRemoveDto>("remove_local_bridge_pending_action", {
        requestId: action.request_id
      });
      setLocalBridgePendingActions((current) => keepIfEqual(current, response.actions));
      setLocalBridgeCheck(response.removed ? `已移除 ${localBridgePendingActionKindLabel(action.action_kind)}` : "没有找到待执行动作");
      await refreshLocalBridgeStatus();
      await refreshLocalBridgeActionResults();
    } catch (nextError) {
      setLocalBridgeCheck("移除失败");
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function revokeLocalBridgeAuthorization(authorization: LocalBridgeAuthorizationDto, scope: LocalBridgePermissionScope) {
    setBusy("open");
    setError(null);
    try {
      const response = await invokeCommand<LocalBridgeAuthorizationRevokeDto>("revoke_local_bridge_authorization", {
        clientId: authorization.client_id,
        scope
      });
      setLocalBridgeAuthorizations((current) => keepIfEqual(current, response.authorizations));
      setLocalBridgeCheck(response.revoked ? `已撤销 ${authorization.display_name} · ${scope}` : "没有找到这条授权");
      await refreshLocalBridgeStatus();
      await refreshLocalBridgeActionResults();
    } catch (nextError) {
      setLocalBridgeCheck("撤销失败");
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
      setLocalBridgeCheck(response.pruned_count > 0 ? `已清理 ${response.pruned_count} 条过期授权` : "没有过期授权");
      await refreshLocalBridgeStatus();
      await refreshLocalBridgeActionResults();
    } catch (nextError) {
      setLocalBridgeCheck("清理失败");
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function createManualBundleForSend() {
    const sourcePath = manualBundleSourcePath.trim();
    if (!sourcePath) {
      setError("选择来源目录");
      return;
    }
    setBusy("scan");
    setError(null);
    setCreatedManualBundle(null);
    try {
      const bundle = await invokeCommand<ManualBundleCreateDto>("create_manual_bundle", {
        request: {
          source_path: sourcePath,
          bundle_type: manualBundleType,
          display_name: manualBundleDisplayName.trim() || lastPathSegment(sourcePath),
          source_app: manualBundleSourceApp.trim() || "NekoDrop"
        }
      });
      setCreatedManualBundle(bundle);
      setToast(`已创建资料包：${bundle.display_name}`);
      await applyPickedPaths([bundle.staging_path]);
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  function openFallbackCode() {
    setMode("send");
    setConnectionCodeOpen(true);
    setSelectedDeviceId(null);
    setSelectedDeviceSnapshot(null);
    setError(null);
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
      const trusted = await invokeCommand<TrustedDeviceDto>("request_device_pairing", {
        deviceId: device.id
      });
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
      const message = errorMessage(nextError);
      setError(pairingFailureAdvice(message) ?? message);
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
      const message = errorMessage(nextError);
      setError(pairingFailureAdvice(message) ?? message);
    } finally {
      setBusy(null);
    }
  }

  async function forgetTrustedDevice(device: TrustedDeviceDto) {
    setBusy("forget");
    setError(null);
    try {
      await invokeCommand<void>("forget_trusted_device", {
        deviceId: device.device_id
      });
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

  function clearQueue() {
    setSelectedPaths([]);
    setManualPaths("");
    setPlan(null);
    setScanStatus(null);
    setSendReport(null);
  }

  function removePath(path: string) {
    const nextPaths = selectedPaths.filter((item) => item !== path);
    setSelectedPaths(nextPaths);
    setPlan(null);
    setScanStatus(null);
    setSendReport(null);
  }

  const discoveryCopy = buildDiscoveryCopy(discoveryStatus, nearbyDevices.length);
  const targetLabel = selectedTargetCopy
    ? selectedTargetCopy.targetLabel
    : selectedDevice
      ? selectedDevice.name
    : trimmedConnectionCode.length > 0
      ? "备用码"
      : "选择目标";
  const composerTitle = plan
    ? plan.root_name
    : transferPaths.length > 0
      ? `${transferPaths.length} 个路径已加入`
      : "拖入文件";
  const composerSubtitle = plan
    ? `${plan.file_count} 个文件 · ${formatBytes(plan.total_bytes)}`
    : transferPaths.length > 0
      ? transferPaths[0]
      : "文件 / 文件夹";
  const pageTitle = buildPageTitle({
    connectionCode: trimmedConnectionCode,
    mode,
    selectedDeviceName: selectedDevice?.name ?? null,
    selectedTargetLabel: selectedTargetCopy?.targetLabel ?? null
  });
  const pageSubtitle = buildPageSubtitle({
    composerSubtitle,
    discoveryLabel: discoveryCopy.label,
    mode,
    nearbyDeviceCount: nearbyDevices.length,
    receiveSessionBindAddr: receiveSession?.bind_addr ?? null,
    receiveState,
    selectedTargetSubtitle: selectedTargetCopy?.subtitle ?? null,
    snapshotDeviceName: snapshot?.device_name ?? null,
    transferCount: transfers.length
  });

  return (
    <main className="app-shell">
      <header className="app-titlebar" data-tauri-drag-region>
        <div className="titlebar-traffic-space" aria-hidden="true" />
        <div className="titlebar-brand" data-tauri-drag-region>
          <span className="titlebar-mark">
            <Icon name="package" />
          </span>
          <strong>NekoDrop</strong>
        </div>
        <div className="titlebar-actions">
          <button
            className={receiveSession ? "titlebar-receive is-on" : "titlebar-receive"}
            disabled={busy === "receive"}
            onClick={() => {
              if (receiveSession) {
                setMode("receive");
                return;
              }
              startReceive();
            }}
            type="button"
          >
            <span className="titlebar-receive-led" />
            {receiveSession ? "收件开启" : "开启收件"}
          </button>
          <button
            className={mode === "transfers" ? "titlebar-icon is-active" : "titlebar-icon"}
            onClick={() => setMode("transfers")}
            title="传输历史"
            type="button"
          >
            <Icon name="clock" />
          </button>
          <button
            className="titlebar-icon"
            onClick={() => setAppearance((current) => (current === "dark" ? "light" : "dark"))}
            title={appearance === "dark" ? "切换浅色" : "切换深色"}
            type="button"
          >
            <Icon name="appearance" />
          </button>
          <button
            className={mode === "settings" ? "titlebar-icon is-active" : "titlebar-icon"}
            onClick={() => setMode("settings")}
            title="设置"
            type="button"
          >
            <Icon name="settings" />
          </button>
        </div>
      </header>

      <div className="shell-body">
      <aside className="device-rail" aria-label="设备">
        <div className="rail-scroll">
          <div className="rail-label is-first">
            <span>这台电脑</span>
          </div>
          <div className="rail-self">
            <span className="rail-self-dot" />
            <div className="rail-self-main">
              <strong>{snapshot?.device_name ?? "这台电脑"}</strong>
              <small>{snapshot ? `${platformLabel(snapshot.device_identity.platform)} · 已验证身份` : "本机"}</small>
            </div>
          </div>

          <div className="rail-label">
            <span>可信设备</span>
            <span className="rail-count">{trustedDevices.length}</span>
          </div>
          {trustedDevices.length > 0 ? (
            trustedDevices.map((device) => {
              const online = nearbyDevices.some((nearby) => nearby.id === device.device_id);
              const selected = selectedDeviceId === device.device_id;
              return (
                <button
                  className={selected ? "rail-dev is-selected" : "rail-dev"}
                  key={device.device_id}
                  onClick={() => {
                    const target = trustedDeviceToDeviceDto(device);
                    setSelectedDeviceId(target.id);
                    setSelectedDeviceSnapshot(target);
                    setConnectionCodeOpen(false);
                    setConnectionCode("");
                    setMode("send");
                    setError(null);
                  }}
                  type="button"
                >
                  <span className={online ? "rail-dev-dot is-online" : "rail-dev-dot"} />
                  <span className="rail-dev-main">
                    <strong>{device.device_name}</strong>
                    <small>{`${platformLabel(device.platform)} · ${online ? "在线" : "离线"}`}</small>
                  </span>
                  <Icon name="shield" className="rail-dev-trust" />
                </button>
              );
            })
          ) : (
            <div className="rail-empty">暂无可信设备</div>
          )}

          <div className="rail-label">
            <span>附近设备</span>
            <span className="rail-count">{nearbyDevices.length}</span>
          </div>
          {nearbyDevices.length > 0 ? (
            nearbyDevices.map((device) => {
              const trusted = isTrustedDeviceState(device.trust_state);
              const selected = selectedDeviceId === device.id;
              return (
                <button
                  className={selected ? "rail-dev is-selected" : "rail-dev"}
                  key={device.id}
                  onClick={() => {
                    if (!trusted) {
                      requestPairing(device);
                      return;
                    }
                    setSelectedDeviceId(device.id);
                    setSelectedDeviceSnapshot(device);
                    setConnectionCodeOpen(false);
                    setConnectionCode("");
                    setMode("send");
                    setError(null);
                  }}
                  type="button"
                >
                  <span className="rail-dev-dot is-online" />
                  <span className="rail-dev-main">
                    <strong>{device.name}</strong>
                    <small>
                      {`${devicePlatformLabel(device.platform)} · ${trusted ? "已信任" : "待配对"}`}
                      {device.pairing_code ? ` · ${device.pairing_code}` : ""}
                    </small>
                  </span>
                  {trusted ? null : <span className="rail-dev-pair">配对</span>}
                </button>
              );
            })
          ) : (
            <div className="rail-empty">{discoveryCopy.label}</div>
          )}
        </div>

        <div className="rail-foot">
          <button
            className={connectionCodeOpen ? "rail-code is-active" : "rail-code"}
            onClick={openFallbackCode}
            type="button"
          >
            <Icon name="link" />
            <span>用连接码连接</span>
          </button>
        </div>
      </aside>

      <section className="workspace">
        <div className="page-frame">
        <header className="topbar">
          {mode !== "send" ? (
            <div className="page-heading">
              <strong>{pageTitle}</strong>
              <span>{pageSubtitle}</span>
            </div>
          ) : (
            <div className="topbar-spacer" aria-hidden="true" />
          )}

          <div className="topbar-actions">
            {mode === "receive" && receiveSession ? (
              <button
                className="danger-button"
                disabled={busy === "stop-receive" || busy === "receive"}
                onClick={stopReceive}
                type="button"
              >
                关闭收件
              </button>
            ) : (
              <button
                className={receiveSession ? "primary-button is-muted" : "primary-button"}
                disabled={busy === "receive"}
                onClick={() => {
                  if (receiveSession) {
                    setMode("receive");
                    return;
                  }
                  startReceive();
                }}
                type="button"
              >
                {receiveSession ? "查看收件" : "开启收件"}
              </button>
            )}
          </div>
        </header>

        {(error || toast) ? (
          <div className="notice-stack">
            {error ? (
              <section className="notice is-error">
                <strong>失败</strong>
                <span>{error}</span>
              </section>
            ) : null}
            {toast ? (
              <section className="notice is-info">
                <strong>完成</strong>
                <span>{toast}</span>
              </section>
            ) : null}
          </div>
        ) : null}

        <section className={mode === "send" ? "work-surface" : "work-surface is-single"}>
          {transferStatus && shouldShowActiveTransferBar(transferStatus) ? (
            <ActiveTransferBar
              busy={busy}
              metrics={transferMetrics}
              status={transferStatus}
              recoveryTransfer={currentFailedTransfer}
              onCancel={cancelCurrentTransfer}
              onRecover={resendTransfer}
              onUseFallbackCode={openFallbackCode}
            />
          ) : null}

          {mode === "send" ? (
            <div className="send-page">
              <header className="dw-header">
                <div className="dw-avatar">
                  <Icon name="devices" />
                  <span className={selectedTrustedOnline || selectedDevice ? "dw-led is-online" : "dw-led"} />
                </div>
                <div className="dw-id">
                  <div className="dw-name">
                    <strong>
                      {selectedDevice?.name ??
                        selectedTrustedRecord?.device_name ??
                        (trimmedConnectionCode ? "备用码连接" : "发送文件")}
                    </strong>
                    {selectedTrustedRecord ? (
                      <span className="dw-badge is-ok">{selectedTrustedOnline ? "在线 · 已信任" : "离线 · 已信任"}</span>
                    ) : selectedDevice ? (
                      <span className="dw-badge is-ok">在线</span>
                    ) : trimmedConnectionCode ? (
                      <span className="dw-badge">连接码</span>
                    ) : (
                      <span className="dw-badge">从左侧选择设备，或拖入文件</span>
                    )}
                  </div>
                  <div className="dw-meta">
                    {(selectedDevice?.platform ?? selectedTrustedRecord?.platform) ? (
                      <span>{platformLabel((selectedDevice?.platform ?? selectedTrustedRecord?.platform)!)}</span>
                    ) : null}
                    {(selectedDevice?.host ?? selectedTrustedRecord?.host) ? (
                      <span className="is-mono">
                        {(selectedDevice?.host ?? selectedTrustedRecord?.host)}
                        {(selectedDevice?.port ?? selectedTrustedRecord?.port) ? `:${selectedDevice?.port ?? selectedTrustedRecord?.port}` : ""}
                      </span>
                    ) : null}
                    {(selectedDevice?.public_key_fingerprint ?? selectedTrustedRecord?.public_key_fingerprint) ? (
                      <span className="is-mono" title={(selectedDevice?.public_key_fingerprint ?? selectedTrustedRecord?.public_key_fingerprint)!}>
                        指纹 {(selectedDevice?.public_key_fingerprint ?? selectedTrustedRecord?.public_key_fingerprint)!.slice(0, 17)}…
                      </span>
                    ) : null}
                  </div>
                </div>
                {selectedTrustedRecord ? (
                  <button className="dw-forget" disabled={busy === "forget"} onClick={() => forgetTrustedDevice(selectedTrustedRecord)} type="button">
                    忘记设备
                  </button>
                ) : null}
              </header>

              <nav className="dw-tabs" role="tablist" aria-label="设备能力">
                {CAPABILITY_TABS.map((item) => (
                  <button
                    aria-selected={capability === item.id}
                    className={capability === item.id ? "dw-tab is-active" : "dw-tab"}
                    key={item.id}
                    onClick={() => setCapability(item.id)}
                    role="tab"
                    type="button"
                  >
                    {item.label}
                    {item.lock ? <span className="dw-tab-lock">{item.lock}</span> : null}
                  </button>
                ))}
              </nav>

              {capability !== "transfer" ? (
                <CapabilityLocked capability={capability} />
              ) : (
              <>
              <div className="send-modes" role="tablist" aria-label="发送类型">
                <button
                  className={sendMode === "file" ? "send-mode is-active" : "send-mode"}
                  onClick={() => setSendMode("file")}
                  type="button"
                >
                  <Icon name="file" />
                  文件 / 文件夹
                </button>
                <button
                  className={sendMode === "bundle" ? "send-mode is-active is-bundle" : "send-mode"}
                  onClick={() => setSendMode("bundle")}
                  type="button"
                >
                  <Icon name="package" />
                  资料包
                </button>
              </div>

              {sendMode === "file" ? (
                <>
                  <section className={dragActive ? "composer is-dragging" : "composer"}>
                    <button
                      className="composer-dropzone"
                      disabled={!desktopRuntime || busy === "pick-files" || busy === "pick-folders" || busy === "scan"}
                      onClick={() => {
                        void pickFiles();
                      }}
                      type="button"
                    >
                      <Icon className="icon-drop" name="upload" />
                      <div className="composer-copy">
                        <strong>{composerTitle}</strong>
                        <span>
                          {plan
                            ? `${plan.file_count} 个文件 · ${formatBytes(plan.total_bytes)}`
                            : transferPaths.length > 0
                              ? composerSubtitle
                              : busy === "scan"
                                ? "正在扫描文件…"
                                : desktopRuntime
                                  ? "点击选择，或拖入文件 / 文件夹"
                                  : "浏览器预览不支持拖放"}
                        </span>
                      </div>
                    </button>
                    {connectionCodeOpen ? (
                      <textarea
                        className="composer-code"
                        value={connectionCode}
                        onChange={(event) => {
                          setConnectionCode(event.target.value);
                          setSelectedDeviceId(null);
                          setSelectedDeviceSnapshot(null);
                        }}
                        aria-label="对方连接码或地址"
                        placeholder="连接码或 IP:端口"
                      />
                    ) : null}
                  </section>

                  <div className="composer-pills">
                    <button className="action-pill" disabled={busy === "pick-files"} onClick={pickFiles} type="button">
                      选文件
                    </button>
                    <button className="action-pill" disabled={busy === "pick-folders"} onClick={pickFolders} type="button">
                      选文件夹
                    </button>
                    <button
                      className={connectionCodeOpen ? "action-pill is-active" : "action-pill"}
                      onClick={() => {
                        setConnectionCodeOpen((open) => !open);
                        setSelectedDeviceId(null);
                        setSelectedDeviceSnapshot(null);
                      }}
                      type="button"
                    >
                      连接码
                    </button>
                    <button className="action-pill" disabled={transferPaths.length === 0} onClick={clearQueue} type="button">
                      清空
                    </button>
                  </div>
                </>
              ) : (
                <ManualBundleComposer
                  busy={busy}
                  createdManualBundle={createdManualBundle}
                  manualBundleDisplayName={manualBundleDisplayName}
                  manualBundleSourceApp={manualBundleSourceApp}
                  manualBundleSourcePath={manualBundleSourcePath}
                  manualBundleType={manualBundleType}
                  setManualBundleDisplayName={setManualBundleDisplayName}
                  setManualBundleSourceApp={setManualBundleSourceApp}
                  setManualBundleType={setManualBundleType}
                  onChooseManualBundleSourceDir={chooseManualBundleSourceDir}
                  onCreateManualBundle={createManualBundleForSend}
                />
              )}

              <div className="send-bar">
                <div className="send-bar-target">
                  <span className="send-bar-label">发送到</span>
                  <strong title={targetLabel}>{targetLabel}</strong>
                </div>
                <button
                  className="send-bar-action"
                  disabled={!canSend}
                  onClick={sendCurrentTransfer}
                  title={`发送到 ${targetLabel}`}
                  type="button"
                >
                  发送
                  <Icon name="arrow-up" />
                </button>
              </div>

              {shouldShowSendPageStatusLine(
                transferStatus,
                sendReport,
                receiveReport,
                plan,
                transferPaths.length
              ) ? (
                <StatusLine
                  plan={plan}
                  receiveReport={receiveReport}
                  receiveSession={receiveSession}
                  sendReport={sendReport}
                  transferMetrics={transferMetrics}
                  transferStatus={transferStatus}
                  transferCount={transferPaths.length}
                  busy={busy}
                  recoveryTransfer={currentFailedTransfer}
                  showActiveTransfer={false}
                  onCancelTransfer={cancelCurrentTransfer}
                  onRecoverTransfer={resendTransfer}
                  onUseFallbackCode={openFallbackCode}
                />
              ) : null}

              {selectedPaths.length > 0 ? (
                <QueuePreview
                  plan={plan}
                  scanStatus={scanStatus}
                  selectedPaths={selectedPaths}
                  onClearQueue={clearQueue}
                  onRemovePath={removePath}
                />
              ) : null}

              <div className="dw-section-head">最近传输</div>
              <RecentActivity
                busy={busy}
                compact
                selectedTransferId={selectedTransferId}
                transfers={transfers}
                onClearTransfers={clearTransferHistory}
                onDeleteTransfer={deleteTransfer}
                onOpenTransfer={openTransferLocation}
                onResendTransfer={resendTransfer}
                onSelectTransfer={(transfer) =>
                  setSelectedTransferId((current) => (current === transfer.id ? null : transfer.id))
                }
                onUseFallbackCode={openFallbackCode}
              />

              </>
              )}
            </div>
          ) : (
            <div className="page-stack">
              {mode === "overview" ? (
                <OverviewPanel
                  busy={busy}
                  currentFailedTransfer={currentFailedTransfer}
                  discoveryStatus={discoveryStatus}
                  localPlatform={localPlatform}
                  nearbyDevices={nearbyDevices}
                  pendingOffer={pendingReceiveOffer}
                  receiveReport={receiveReport}
                  receiveSession={receiveSession}
                  receiveState={receiveState}
                  selectedTransferId={selectedTransferId}
                  stagedBundles={stagedBundles}
                  transferMetrics={transferMetrics}
                  transferStatus={transferStatus}
                  transfers={transfers}
                  trustedDevices={trustedDevices}
                  onCancelTransfer={cancelCurrentTransfer}
                  onClearTransfers={clearTransferHistory}
                  onDeleteTransfer={deleteTransfer}
                  onOpenTransfer={openTransferLocation}
                  onRecoverTransfer={resendTransfer}
                  onSelectMode={setMode}
                  onSelectTransfer={(transfer) =>
                    setSelectedTransferId((current) => current === transfer.id ? null : transfer.id)
                  }
                  onUseFallbackCode={openFallbackCode}
                />
              ) : null}

              {mode === "receive" ? (
                <ReceivePanel
                  busy={busy}
                  diagnostics={receiveDiagnostics}
                  pendingOffer={pendingReceiveOffer}
                  pendingPairingRequest={pendingPairingRequest}
                  receiveReport={receiveReport}
                  receiveSession={receiveSession}
                  stagedBundles={stagedBundles}
                  onCopyConnectionCode={copyConnectionCode}
                  onOpenPath={openPath}
                  onRespondReceiveOffer={respondReceiveOffer}
                  onRespondPairingRequest={respondPairingRequest}
                  onStartReceive={startReceive}
                  onStopReceive={stopReceive}
                  onDeleteStagedBundle={deleteCurrentStagedBundle}
                  onImportStagedBundle={importCurrentStagedBundle}
                />
              ) : null}

              {mode === "devices" ? (
                <DevicePanel
                  busy={busy}
                  discoveryStatus={discoveryStatus}
                  localPlatform={localPlatform}
                  nearbyDevices={nearbyDevices}
                  selectedDeviceId={selectedDeviceId}
                  trustedDevices={trustedDevices}
                  onForgetTrustedDevice={forgetTrustedDevice}
                  onSelectNearbyDevice={(device) => {
                    setSelectedDeviceId(device.id);
                    setSelectedDeviceSnapshot(device);
                    setConnectionCodeOpen(false);
                    setConnectionCode("");
                    setMode("send");
                    setError(null);
                  }}
                  onSelectTrustedDevice={(device) => {
                    const target = trustedDeviceToDeviceDto(device);
                    setSelectedDeviceId(target.id);
                    setSelectedDeviceSnapshot(target);
                    setConnectionCodeOpen(false);
                    setConnectionCode("");
                    setMode("send");
                    setError(null);
                  }}
                  onTrustDevice={requestPairing}
                />
              ) : null}

              {mode === "transfers" ? (
                <HistoryPanel
                  busy={busy}
                  selectedTransferId={selectedTransferId}
                  transferMetrics={transferMetrics}
                  transferStatus={transferStatus}
                  transfers={transfers}
                  onCancelTransfer={cancelCurrentTransfer}
                  onClearTransfers={clearTransferHistory}
                  onDeleteTransfer={deleteTransfer}
                  onOpenTransfer={openTransferLocation}
                  onResendTransfer={resendTransfer}
                  onSelectTransfer={(transfer) =>
                    setSelectedTransferId((current) => current === transfer.id ? null : transfer.id)
                  }
                  onUseFallbackCode={openFallbackCode}
                />
              ) : null}

              {mode === "settings" ? (
                <SettingsPanel
                  bindPort={bindPort}
                  busy={busy}
                  deviceNameInput={deviceNameInput}
                  discoveryStatus={discoveryStatus}
                  localBridgeAuthorizationCode={localBridgeAuthorizationCode}
                  localBridgeAuthorizations={localBridgeAuthorizations}
                  localBridgeActionResults={localBridgeActionResults}
                  localBridgeCheck={localBridgeCheck}
                  localBridgePendingActions={localBridgePendingActions}
                  localBridgeStatus={localBridgeStatus}
                  receiveDir={receiveDir}
                  receivePolicy={receivePolicy}
                  receiveSession={receiveSession}
                  setBindPort={setBindPort}
                  setDeviceNameInput={setDeviceNameInput}
                  setLocalBridgeAuthorizationCode={setLocalBridgeAuthorizationCode}
                  setReceiveDir={setReceiveDir}
                  snapshot={snapshot}
                  onChooseReceiveDir={chooseReceiveDir}
                  onConfirmLocalBridgeAuthorization={confirmLocalBridgeAuthorization}
                  onOpenReceiveDir={() => openPath(receiveSession?.receive_dir ?? receiveDir)}
                  onPruneLocalBridgeAuthorizations={pruneLocalBridgeAuthorizations}
                  onRemoveLocalBridgePendingAction={removeLocalBridgePendingAction}
                  onRevokeLocalBridgeAuthorization={revokeLocalBridgeAuthorization}
                  onRunLocalBridgeSelfCheck={runLocalBridgeSelfCheck}
                  onSaveReceiveDir={saveReceiveDir}
                  onSaveReceivePort={saveReceivePort}
                  onSaveDeviceName={saveDeviceName}
                  onUpdateReceivePolicy={updateReceivePolicy}
                />
              ) : null}
            </div>
          )}
        </section>
        </div>
      </section>
      </div>
    </main>
  );
}

type IconName =
  | "appearance"
  | "arrow-up"
  | "clock"
  | "devices"
  | "file"
  | "folder"
  | "inbox"
  | "link"
  | "list"
  | "overview"
  | "package"
  | "plug"
  | "settings"
  | "send"
  | "shield"
  | "trash"
  | "upload";

function CapabilityLocked({ capability }: { capability: Exclude<DeviceCapability, "transfer"> }) {
  const copy = CAPABILITY_LOCKED_COPY[capability];
  const icon: IconName = capability === "agent" ? "plug" : capability === "state" ? "package" : "link";
  return (
    <div className="capability-locked">
      <div className="capability-locked-icon">
        <Icon name={icon} />
      </div>
      <strong>{copy.title}</strong>
      <p>{copy.body}</p>
      <span className="capability-locked-tag">{copy.tag}</span>
    </div>
  );
}

function Icon({ className, name }: { className?: string; name: IconName }) {
  return (
    <svg aria-hidden="true" className={className ? `icon ${className}` : "icon"} fill="none" viewBox="0 0 24 24">
      {name === "appearance" ? <path d="M12 8a4 4 0 1 1 0 8 4 4 0 0 1 0-8Zm0-5v3m0 12v3M4.9 4.9 7 7m10 10 2.1 2.1M3 12h3m12 0h3M4.9 19.1 7 17m10-10 2.1-2.1" /> : null}
      {name === "arrow-up" ? <path d="M12 19V5m0 0 6 6M12 5l-6 6" /> : null}
      {name === "clock" ? <path d="M12 6v6l4 2m5-2a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z" /> : null}
      {name === "devices" ? <path d="M7 8a4 4 0 1 1 8 0 4 4 0 0 1-8 0Zm-3 13a7 7 0 0 1 14 0M17 11a3 3 0 0 1 0 6m3-8a6 6 0 0 1 0 10" /> : null}
      {name === "file" ? <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8l-5-5Zm0 0v5h5" /> : null}
      {name === "folder" ? <path d="M3 7a2 2 0 0 1 2-2h5l2 2h7a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Z" /> : null}
      {name === "inbox" ? <path d="M4 4h16v11l-3 5H7l-3-5V4Zm0 11h5l2 2h2l2-2h5" /> : null}
      {name === "link" ? <path d="M10 13a5 5 0 0 0 7.07 0l2-2A5 5 0 0 0 12 4l-1.2 1.2M14 11a5 5 0 0 0-7.07 0l-2 2A5 5 0 0 0 12 20l1.2-1.2" /> : null}
      {name === "list" ? <path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01" /> : null}
      {name === "overview" ? <path d="M4 5h7v6H4V5Zm9 0h7v4h-7V5ZM4 13h7v6H4v-6Zm9-2h7v8h-7v-8Z" /> : null}
      {name === "package" ? <path d="m4 7 8-4 8 4-8 4-8-4Zm0 0v10l8 4m0-10v10m0-10 8-4m0 0v10l-8 4" /> : null}
      {name === "plug" ? <path d="M9 7V3m6 4V3M7 7h10v5a5 5 0 0 1-10 0V7Zm5 10v4" /> : null}
      {name === "settings" ? (
        <>
          <path d="M12 15.2a3.2 3.2 0 1 0 0-6.4 3.2 3.2 0 0 0 0 6.4Z" />
          <path d="M19.4 15a1.7 1.7 0 0 0 .34 1.88l.06.06a2.05 2.05 0 1 1-2.9 2.9l-.06-.06A1.7 1.7 0 0 0 15 19.4a1.7 1.7 0 0 0-1 .6 1.7 1.7 0 0 0-.4 1.1V21a2.05 2.05 0 0 1-4.1 0v-.1A1.7 1.7 0 0 0 8.1 19.2a1.7 1.7 0 0 0-1.88.34l-.06.06a2.05 2.05 0 1 1-2.9-2.9l.06-.06A1.7 1.7 0 0 0 3.6 15a1.7 1.7 0 0 0-.6-1 1.7 1.7 0 0 0-1.1-.4H2a2.05 2.05 0 0 1 0-4.1h.1A1.7 1.7 0 0 0 3.8 8.1a1.7 1.7 0 0 0-.34-1.88l-.06-.06a2.05 2.05 0 1 1 2.9-2.9l.06.06A1.7 1.7 0 0 0 8.1 3.8a1.7 1.7 0 0 0 1-.6 1.7 1.7 0 0 0 .4-1.1V2a2.05 2.05 0 0 1 4.1 0v.1A1.7 1.7 0 0 0 15 3.8a1.7 1.7 0 0 0 1.88-.34l.06-.06a2.05 2.05 0 1 1 2.9 2.9l-.06.06A1.7 1.7 0 0 0 19.4 8c.08.36.28.7.6 1 .3.27.7.43 1.1.43h.1a2.05 2.05 0 0 1 0 4.1h-.1a1.7 1.7 0 0 0-1.7 1.47Z" />
        </>
      ) : null}
      {name === "send" ? <path d="m4 12 16-8-8 16-2-7-6-1Z" /> : null}
      {name === "shield" ? <path d="M12 3l8 3v6c0 5-8 9-8 9s-8-4-8-9V6l8-3Zm-3 9 2 2 4-4" /> : null}
      {name === "trash" ? <path d="M4 7h16M9 7V4h6v3m-8 0 1 14h8l1-14" /> : null}
      {name === "upload" ? <path d="M12 16V6m0 0 5 5m-5-5-5 5M4 18h16" /> : null}
    </svg>
  );
}

function QueuePreview({
  plan,
  scanStatus,
  selectedPaths,
  onClearQueue,
  onRemovePath
}: {
  plan: TransferPlanDto | null;
  scanStatus: TransferScanProgressDto | null;
  selectedPaths: string[];
  onClearQueue: () => void;
  onRemovePath: (path: string) => void;
}) {
  const previewPaths = selectedPaths.slice(0, 4);

  return (
    <section className="queue-preview">
      <div className="block-head">
        <strong>待发送</strong>
        <span>{plan ? `${plan.file_count} 个文件 · ${formatBytes(plan.total_bytes)}` : `${selectedPaths.length} 个路径`}</span>
      </div>

      <TransferScanStatus status={scanStatus} />

      {previewPaths.length > 0 ? (
        <div className="queue-preview-list">
          {previewPaths.map((path) => (
            <div className="queue-preview-row" key={path}>
              <span>{path}</span>
              <button className="text-button" onClick={() => onRemovePath(path)} type="button">
                移除
              </button>
            </div>
          ))}
          {selectedPaths.length > previewPaths.length ? (
            <div className="queue-preview-row is-muted">
              <span>还有 {selectedPaths.length - previewPaths.length} 个路径</span>
              <button className="text-button" onClick={onClearQueue} type="button">
                清空
              </button>
            </div>
          ) : null}
        </div>
      ) : (
        <div className="queue-preview-empty">未选择文件</div>
      )}
    </section>
  );
}

function TransferScanStatus({ status }: { status: TransferScanProgressDto | null }) {
  if (!status || status.phase === "completed") return null;

  const title = status.phase === "hashing" ? "正在校验文件" : "正在准备传输";
  const discovered = [
    `${status.files_found} 个文件`,
    `${status.directories_found} 个文件夹`,
    formatBytes(status.bytes_found)
  ].join(" · ");

  return (
    <div className="transfer-status">
      <div className="transfer-status-head">
        <strong>{title}</strong>
        <span>{discovered}</span>
      </div>
      {status.current_path ? (
        <div className="transfer-status-meta">{status.current_path}</div>
      ) : null}
    </div>
  );
}

function NearbyDevices({
  busy,
  discoveryStatus,
  devices,
  localPlatform,
  selectedDeviceId,
  onSelectDevice,
  onTrustDevice
}: {
  busy: BusyMode | null;
  discoveryStatus: DiscoveryStatusDto | null;
  devices: DeviceDto[];
  localPlatform: string | null;
  selectedDeviceId: string | null;
  onSelectDevice: (device: DeviceDto) => void;
  onTrustDevice: (device: DeviceDto) => void;
}) {
  const discoveryCopy = buildDiscoveryCopy(discoveryStatus, devices.length, localPlatform);

  return (
    <section className="nearby-strip">
      <div className="nearby-head">
        <div>
          <strong>附近设备</strong>
          <span>{discoveryCopy.subtitle}</span>
        </div>
        <span className={discoveryCopy.isError ? "discovery-badge is-error" : "discovery-badge"}>
          {discoveryCopy.label}
        </span>
      </div>

      {devices.length > 0 ? (
        <div className="nearby-list">
          {devices.map((device) => {
            const trusted = device.trust_state === "Trusted";
            const selected = device.id === selectedDeviceId;
            const model = buildNearbyDeviceViewModel(device, selected);
            return (
              <div
                className={[
                  "nearby-device",
                  trusted ? "is-trusted" : "",
                  selected ? "is-selected" : ""
                ]
                  .filter(Boolean)
                  .join(" ")}
                key={device.id}
              >
                <span className="device-dot" />
                <span className="device-main">
                  <strong>{device.name}</strong>
                  <small>
                    {devicePlatformLabel(device.platform)} · {model.statusLabel}
                    {device.pairing_code ? ` · ${device.pairing_code}` : ""}
                  </small>
                </span>
                <span className="device-actions">
                  {trusted ? (
                    <button className="target-button" onClick={() => onSelectDevice(device)} type="button">
                      {model.actionLabel}
                    </button>
                  ) : (
                    <button
                      className="trust-button"
                      disabled={busy === "pair" || !model.canPair}
                      onClick={() => onTrustDevice(device)}
                      type="button"
                    >
                      {model.actionLabel}
                    </button>
                  )}
                </span>
              </div>
            );
          })}
        </div>
      ) : (
        <div className={discoveryCopy.isError ? "nearby-empty is-warning" : "nearby-empty"}>
          <strong>{discoveryCopy.emptyTitle}</strong>
          <span>{discoveryCopy.emptyBody}</span>
        </div>
      )}
    </section>
  );
}

function DevicePanel({
  busy,
  discoveryStatus,
  localPlatform,
  nearbyDevices,
  selectedDeviceId,
  trustedDevices,
  onForgetTrustedDevice,
  onSelectNearbyDevice,
  onSelectTrustedDevice,
  onTrustDevice
}: {
  busy: BusyMode | null;
  discoveryStatus: DiscoveryStatusDto | null;
  localPlatform: string | null;
  nearbyDevices: DeviceDto[];
  selectedDeviceId: string | null;
  trustedDevices: TrustedDeviceDto[];
  onForgetTrustedDevice: (device: TrustedDeviceDto) => void;
  onSelectNearbyDevice: (device: DeviceDto) => void;
  onSelectTrustedDevice: (device: TrustedDeviceDto) => void;
  onTrustDevice: (device: DeviceDto) => void;
}) {
  const selectedNearby = nearbyDevices.find((device) => device.id === selectedDeviceId) ?? null;
  const selectedTrusted =
    trustedDevices.find((device) => device.device_id === selectedDeviceId) ?? null;
  const hasSelection = Boolean(selectedNearby || selectedTrusted);
  const selectedOnline = selectedTrusted
    ? nearbyDevices.some((nearby) => nearby.id === selectedTrusted.device_id)
    : true;
  const selectedName = selectedTrusted?.device_name ?? selectedNearby?.name ?? "";
  const selectedPlatform = selectedTrusted?.platform ?? selectedNearby?.platform ?? null;
  const selectedFingerprint =
    selectedTrusted?.public_key_fingerprint ?? selectedNearby?.public_key_fingerprint ?? null;
  const selectedHost = selectedTrusted?.host ?? selectedNearby?.host ?? null;
  const selectedPort = selectedTrusted?.port ?? selectedNearby?.port ?? null;
  const selectedTrustLabel = selectedTrusted
    ? "已信任"
    : selectedNearby
      ? trustStateLabel(selectedNearby.trust_state)
      : "—";

  return (
    <section className="device-panel">
      <NearbyDevices
        busy={busy}
        discoveryStatus={discoveryStatus}
        devices={nearbyDevices}
        localPlatform={localPlatform}
        selectedDeviceId={selectedDeviceId}
        onSelectDevice={onSelectNearbyDevice}
        onTrustDevice={onTrustDevice}
      />

      <section className="trusted-strip">
        <div className="section-head">
          <strong>已信任设备</strong>
          <span>{trustedDevices.length > 0 ? "可直接发送" : "未配对"}</span>
        </div>

        {trustedDevices.length > 0 ? (
          <div className="trusted-list">
            {trustedDevices.map((device) => {
              const online = nearbyDevices.some((nearby) => nearby.id === device.device_id);
              const selected = selectedDeviceId === device.device_id;
              const model = buildTrustedDeviceViewModel(device, Date.now(), online);
              return (
                <div
                  className={selected ? "trusted-device is-selected" : "trusted-device"}
                  key={device.device_id}
                >
                  <span className={online ? "device-dot is-online" : "device-dot"} />
                  <span className="trusted-main">
                    <strong>{device.device_name}</strong>
                    <small>{model.detailLabel}</small>
                  </span>
                  <span className="trusted-meta">{model.presenceLabel}</span>
                  <span className="trusted-actions">
                    <button className="target-button" onClick={() => onSelectTrustedDevice(device)} type="button">
                      {selected ? "已选" : model.actionLabel}
                    </button>
                    <button className="text-button" disabled={busy === "forget"} onClick={() => onForgetTrustedDevice(device)} type="button">
                      移除
                    </button>
                  </span>
                </div>
              );
            })}
          </div>
        ) : (
          <div className="device-empty">暂无可信设备</div>
        )}
      </section>

      {hasSelection ? (
        <section className="device-detail">
          <div className="section-head">
            <strong title={selectedName}>{selectedName}</strong>
            <span>{selectedOnline ? "在线" : "离线"} · {selectedTrustLabel}</span>
          </div>
          <div className="device-detail-rows">
            <div className="device-detail-row">
              <span>平台</span>
              <strong>{selectedPlatform ? devicePlatformLabel(selectedPlatform) : "—"}</strong>
            </div>
            <div className="device-detail-row">
              <span>地址</span>
              <strong className="is-mono">{selectedHost && selectedPort ? `${selectedHost}:${selectedPort}` : "—"}</strong>
            </div>
            <div className="device-detail-row">
              <span>指纹</span>
              <strong className="is-mono" title={selectedFingerprint ?? undefined}>{selectedFingerprint ?? "—"}</strong>
            </div>
            <div className="device-detail-row">
              <span>能力</span>
              <strong>文件传输 · 资料包</strong>
            </div>
          </div>
          <div className="device-detail-actions">
            {selectedTrusted ? (
              <>
                <button className="primary-button" onClick={() => onSelectTrustedDevice(selectedTrusted)} type="button">
                  发送到此设备
                </button>
                <button className="text-button" disabled={busy === "forget"} onClick={() => onForgetTrustedDevice(selectedTrusted)} type="button">
                  忘记设备
                </button>
              </>
            ) : selectedNearby ? (
              <>
                <button className="primary-button" onClick={() => onSelectNearbyDevice(selectedNearby)} type="button">
                  发送到此设备
                </button>
                <button className="tool-button" onClick={() => onTrustDevice(selectedNearby)} type="button">
                  配对
                </button>
              </>
            ) : null}
          </div>
        </section>
      ) : null}
    </section>
  );
}

function OverviewPanel({
  busy,
  currentFailedTransfer,
  discoveryStatus,
  localPlatform,
  nearbyDevices,
  pendingOffer,
  receiveReport,
  receiveSession,
  receiveState,
  selectedTransferId,
  stagedBundles,
  transferMetrics,
  transferStatus,
  transfers,
  trustedDevices,
  onCancelTransfer,
  onClearTransfers,
  onDeleteTransfer,
  onOpenTransfer,
  onRecoverTransfer,
  onSelectMode,
  onSelectTransfer,
  onUseFallbackCode
}: {
  busy: BusyMode | null;
  currentFailedTransfer: TransferDto | null;
  discoveryStatus: DiscoveryStatusDto | null;
  localPlatform: string | null;
  nearbyDevices: DeviceDto[];
  pendingOffer: PendingReceiveOfferDto | null;
  receiveReport: ReceiveReportDto | null;
  receiveSession: ReceiveSessionDto | null;
  receiveState: string;
  selectedTransferId: string | null;
  stagedBundles: ReceivedBundleDto[];
  transferMetrics: TransferMetrics;
  transferStatus: TransferStatusDto | null;
  transfers: TransferDto[];
  trustedDevices: TrustedDeviceDto[];
  onCancelTransfer: () => void;
  onClearTransfers: () => void;
  onDeleteTransfer: (transfer: TransferDto) => void;
  onOpenTransfer: (transfer: TransferDto) => void;
  onRecoverTransfer: (transfer: TransferDto) => void;
  onSelectMode: (mode: ComposerMode) => void;
  onSelectTransfer: (transfer: TransferDto) => void;
  onUseFallbackCode: () => void;
}) {
  const latestBundle = receiveReport?.bundle ?? null;
  const pendingBundleCount = stagedBundles.filter((bundle) => bundle.can_import_now).length;
  const activeTransfer = transferStatus && shouldShowActiveTransferBar(transferStatus);
  const discoveryCopy = buildDiscoveryCopy(discoveryStatus, nearbyDevices.length, localPlatform);

  return (
    <section className="overview-page">
      <div className="overview-status">
        <button className="overview-status-item" type="button" onClick={() => onSelectMode("receive")}>
          <strong>{receiveState}</strong>
          <span>{receiveSession?.bind_addr ?? "收件未监听"}</span>
        </button>
        <button
          className={discoveryCopy.isError ? "overview-status-item is-warning" : "overview-status-item"}
          type="button"
          onClick={() => onSelectMode("devices")}
          title={discoveryCopy.emptyBody || discoveryCopy.subtitle}
        >
          <strong>{nearbyDevices.length > 0 ? nearbyDevices.length : discoveryCopy.label}</strong>
          <span>{nearbyDevices.length > 0 ? "附近在线" : discoveryCopy.targetLabel}</span>
        </button>
        <button className="overview-status-item" type="button" onClick={() => onSelectMode("devices")}>
          <strong>{trustedDevices.length}</strong>
          <span>可信设备</span>
        </button>
        <button className="overview-status-item" type="button" onClick={() => onSelectMode("receive")}>
          <strong>{pendingBundleCount > 0 ? pendingBundleCount : latestBundle ? "1" : "0"}</strong>
          <span>待处理资料包</span>
        </button>
      </div>

      <section className="console-section">
        <div className="console-section-head">
          <strong>当前</strong>
          <span>{pendingOffer ? "等待确认" : activeTransfer ? "传输中" : "空闲"}</span>
        </div>
        {pendingOffer ? (
          <div className="console-row is-attention">
            <span>
              {pendingOffer.root_name} · {pendingOffer.file_count} 个文件 · {formatBytes(pendingOffer.total_bytes)}
            </span>
            <button className="text-button" type="button" onClick={() => onSelectMode("receive")}>
              处理
            </button>
          </div>
        ) : activeTransfer && transferStatus ? (
          <TransferStatusView
            busy={busy}
            metrics={transferMetrics}
            status={transferStatus}
            recoveryTransfer={currentFailedTransfer}
            onCancel={onCancelTransfer}
            onRecover={onRecoverTransfer}
            onUseFallbackCode={onUseFallbackCode}
          />
        ) : (
          <div className="console-empty">没有正在进行的传输</div>
        )}
      </section>

      <section className="console-section">
        <div className="console-section-head">
          <strong>下一步</strong>
        </div>
        <div className="overview-actions">
          <button className="primary-button" type="button" onClick={() => onSelectMode("send")}>
            发送文件
          </button>
          <button className="tool-button" type="button" onClick={() => onSelectMode("receive")}>
            查看收件
          </button>
        </div>
      </section>

      {latestBundle ? (
        <section className="console-section">
          <div className="console-section-head">
            <strong>最近资料包</strong>
            <span>{receiveBundleStatusLabel(receiveReport!)}</span>
          </div>
          <div className="console-row">
            <span>{receiveBundleSummaryLine(receiveReport!)}</span>
            <button className="text-button" type="button" onClick={() => onSelectMode("receive")}>
              查看
            </button>
          </div>
        </section>
      ) : null}

      <RecentActivity
        busy={busy}
        compact
        selectedTransferId={selectedTransferId}
        transfers={transfers}
        onClearTransfers={onClearTransfers}
        onDeleteTransfer={onDeleteTransfer}
        onOpenTransfer={onOpenTransfer}
        onResendTransfer={onRecoverTransfer}
        onSelectTransfer={onSelectTransfer}
        onUseFallbackCode={onUseFallbackCode}
      />
    </section>
  );
}

function ManualBundleComposer({
  busy,
  createdManualBundle,
  manualBundleDisplayName,
  manualBundleSourceApp,
  manualBundleSourcePath,
  manualBundleType,
  setManualBundleDisplayName,
  setManualBundleSourceApp,
  setManualBundleType,
  onChooseManualBundleSourceDir,
  onCreateManualBundle
}: {
  busy: BusyMode | null;
  createdManualBundle: ManualBundleCreateDto | null;
  manualBundleDisplayName: string;
  manualBundleSourceApp: string;
  manualBundleSourcePath: string;
  manualBundleType: string;
  setManualBundleDisplayName: (value: string) => void;
  setManualBundleSourceApp: (value: string) => void;
  setManualBundleType: (value: string) => void;
  onChooseManualBundleSourceDir: () => void;
  onCreateManualBundle: () => void;
}) {
  return (
    <section className="bundle-create-section">
      <div className="section-head">
        <strong>资料包目录</strong>
        <span>{manualBundleSourcePath ? lastPathSegment(manualBundleSourcePath) : "把一个目录打包后发送"}</span>
      </div>
      <div className="bundle-create">
        <label>
          <span>类型</span>
          <select
            value={manualBundleType}
            onChange={(event) => setManualBundleType(event.target.value)}
          >
            <option value="workspace">Workspace</option>
            <option value="session">Session</option>
            <option value="skill">Skill</option>
            <option value="agent_profile">Agent profile</option>
            <option value="config_snapshot">Config</option>
          </select>
        </label>
        <label>
          <span>名称</span>
          <input
            value={manualBundleDisplayName}
            onChange={(event) => setManualBundleDisplayName(event.target.value)}
            placeholder="资料包名称"
          />
        </label>
        <label>
          <span>来源</span>
          <input
            value={manualBundleSourceApp}
            onChange={(event) => setManualBundleSourceApp(event.target.value)}
            placeholder="NekoDrop"
          />
        </label>
        <button className="tool-button" disabled={busy === "pick-folders"} type="button" onClick={onChooseManualBundleSourceDir}>
          选目录
        </button>
        <button className="primary-button" disabled={!manualBundleSourcePath || busy === "scan"} type="button" onClick={onCreateManualBundle}>
          加入发送
        </button>
      </div>
      {createdManualBundle ? (
        <div className="console-row">
          <span>
            {createdManualBundle.display_name} · {bundleTypeLabel(createdManualBundle.bundle_type)} · {createdManualBundle.file_count} 个文件 · {formatBytes(createdManualBundle.total_bytes)}
          </span>
        </div>
      ) : null}
    </section>
  );
}

function IntegrationSettings({
  busy,
  localBridgeAuthorizationCode,
  localBridgeAuthorizations,
  localBridgeActionResults,
  localBridgeCheck,
  localBridgePendingActions,
  localBridgeStatus,
  setLocalBridgeAuthorizationCode,
  onConfirmLocalBridgeAuthorization,
  onPruneLocalBridgeAuthorizations,
  onRemoveLocalBridgePendingAction,
  onRevokeLocalBridgeAuthorization,
  onRunLocalBridgeSelfCheck
}: {
  busy: BusyMode | null;
  localBridgeAuthorizationCode: string;
  localBridgeAuthorizations: LocalBridgeAuthorizationDto[];
  localBridgeActionResults: LocalBridgePendingActionResultDto[];
  localBridgeCheck: string | null;
  localBridgePendingActions: LocalBridgePendingActionDto[];
  localBridgeStatus: LocalBridgeRuntimeStatusDto | null;
  setLocalBridgeAuthorizationCode: (value: string) => void;
  onConfirmLocalBridgeAuthorization: () => void;
  onPruneLocalBridgeAuthorizations: () => void;
  onRemoveLocalBridgePendingAction: (action: LocalBridgePendingActionDto) => void;
  onRevokeLocalBridgeAuthorization: (authorization: LocalBridgeAuthorizationDto, scope: LocalBridgePermissionScope) => void;
  onRunLocalBridgeSelfCheck: () => void;
}) {
  const localBridgeRuntimeLine = localBridgeRuntimeStatusLine(localBridgeStatus);

  return (
    <SettingsGroup title="本机接入" note="本机应用只能通过受控请求读取设备、发送资料包或申请导入">
      <SettingsRow label="桥接状态">
        <SettingsValue mono={Boolean(localBridgeStatus?.active)} title={localBridgeRuntimeLine}>
          {localBridgeRuntimeLine}
        </SettingsValue>
      </SettingsRow>
      <SettingsRow label="只读自测">
        <SettingsValue>{localBridgeCheck ?? "尚未运行"}</SettingsValue>
        <button className="text-button" disabled={busy === "open"} onClick={onRunLocalBridgeSelfCheck} type="button">
          运行
        </button>
      </SettingsRow>
      <SettingsRow label="授权请求">
        <div className="settings-inline-field">
          <input
            aria-label="本机接入授权码"
            placeholder="授权码"
            value={localBridgeAuthorizationCode}
            onChange={(event) => setLocalBridgeAuthorizationCode(event.target.value)}
          />
          <button
            className="tool-button"
            disabled={busy === "open" || localBridgeAuthorizationCode.trim().length === 0}
            onClick={onConfirmLocalBridgeAuthorization}
            type="button"
          >
            确认
          </button>
        </div>
      </SettingsRow>
      <SettingsRow label="权限范围">
        <SettingsValue>读取设备、发送资料包、查看传输、申请导入</SettingsValue>
      </SettingsRow>
      <SettingsRow label="待授权">
        <SettingsValue>
          {localBridgeStatus?.pending_authorization_client
            ? `${localBridgeStatus.pending_authorization_client} 正在等待授权码`
            : "暂无待授权"}
        </SettingsValue>
      </SettingsRow>
      <SettingsRow label="待执行">
        <div className="local-bridge-auth-list">
          {localBridgePendingActions.length > 0 ? (
            localBridgePendingActions.map((action) => (
              <div className="console-row" key={action.request_id}>
                <span title={localBridgePendingActionTitle(action)}>
                  {action.client_display_name} · {localBridgePendingActionSummary(action)}
                </span>
                <div className="settings-inline-actions">
                  <button
                    className="text-button"
                    disabled={busy === "open"}
                    onClick={() => onRemoveLocalBridgePendingAction(action)}
                    type="button"
                  >
                    移除
                  </button>
                </div>
              </div>
            ))
          ) : (
            <div className="console-empty">暂无待执行</div>
          )}
        </div>
      </SettingsRow>
      <SettingsRow label="执行结果">
        <div className="local-bridge-auth-list">
          {localBridgeActionResults.length > 0 ? (
            localBridgeActionResults.slice(0, 5).map((result) => (
              <div className="console-row" key={`${result.request_id}-${result.claimed_at_ms}`}>
                <span title={result.message}>
                  {result.client_display_name} · {localBridgeActionResultSummary(result)}
                </span>
              </div>
            ))
          ) : (
            <div className="console-empty">暂无结果</div>
          )}
        </div>
      </SettingsRow>
      <SettingsRow label="已授权">
        <div className="local-bridge-auth-list">
          {localBridgeAuthorizations.length > 0 ? (
            localBridgeAuthorizations.map((authorization) => (
              <div className="console-row" key={`${authorization.client_id}-${authorization.scopes.join("-")}`}>
                <span title={`${authorization.display_name} · ${authorization.scopes.join(" · ")}`}>
                  {authorization.display_name} · {authorization.scopes.map(localBridgeScopeLabel).join(" · ")}
                </span>
                <div className="settings-inline-actions">
                  {authorization.scopes.map((scope) => (
                    <button
                      className="text-button"
                      disabled={busy === "open"}
                      key={scope}
                      onClick={() => onRevokeLocalBridgeAuthorization(authorization, scope)}
                      type="button"
                    >
                      撤销
                    </button>
                  ))}
                </div>
              </div>
            ))
          ) : (
            <div className="console-empty">暂无授权</div>
          )}
        </div>
      </SettingsRow>
      <SettingsRow label="过期授权">
        <button className="text-button" disabled={busy === "open"} onClick={onPruneLocalBridgeAuthorizations} type="button">
          清理
        </button>
      </SettingsRow>
    </SettingsGroup>
  );
}

function SettingsGroup({
  title,
  note,
  children
}: {
  title: string;
  note?: string;
  children: ReactNode;
}) {
  return (
    <section className="settings-group">
      <header className="settings-group-head">
        <h2>{title}</h2>
        {note ? <p>{note}</p> : null}
      </header>
      <div className="settings-rows">{children}</div>
    </section>
  );
}

function SettingsRow({
  label,
  hint,
  children
}: {
  label: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <div className="settings-row">
      <div className="settings-row-label">
        <span>{label}</span>
        {hint ? <small>{hint}</small> : null}
      </div>
      <div className="settings-row-control">{children}</div>
    </div>
  );
}

function SettingsValue({
  children,
  mono,
  title
}: {
  children: ReactNode;
  mono?: boolean;
  title?: string;
}) {
  return (
    <span className={mono ? "settings-value is-mono" : "settings-value"} title={title}>
      {children}
    </span>
  );
}

function SettingsFacts({
  items
}: {
  items: Array<{ label: string; value: string | null; mono?: boolean }>;
}) {
  const visible = items.filter((item) => item.value);
  if (visible.length === 0) return null;

  return (
    <div className="settings-facts">
      {visible.map((item) => (
        <div className="settings-fact" key={item.label}>
          <span>{item.label}</span>
          <strong className={item.mono ? "is-mono" : undefined} title={item.value ?? undefined}>
            {item.value}
          </strong>
        </div>
      ))}
    </div>
  );
}

function ReceivePanel({
  busy,
  diagnostics,
  pendingOffer,
  pendingPairingRequest,
  receiveReport,
  receiveSession,
  stagedBundles,
  onCopyConnectionCode,
  onOpenPath,
  onDeleteStagedBundle,
  onImportStagedBundle,
  onRespondReceiveOffer,
  onRespondPairingRequest,
  onStartReceive,
  onStopReceive
}: {
  busy: BusyMode | null;
  diagnostics: ReceivePortDiagnosticsDto | null;
  pendingOffer: PendingReceiveOfferDto | null;
  pendingPairingRequest: PendingPairingRequestDto | null;
  receiveReport: ReceiveReportDto | null;
  receiveSession: ReceiveSessionDto | null;
  stagedBundles: ReceivedBundleDto[];
  onCopyConnectionCode: () => void;
  onOpenPath: (path: string) => void;
  onDeleteStagedBundle: (bundle: ReceivedBundleDto) => void;
  onImportStagedBundle: (bundle: ReceivedBundleDto) => void;
  onRespondReceiveOffer: (accept: boolean) => void;
  onRespondPairingRequest: (accept: boolean) => void;
  onStartReceive: () => void;
  onStopReceive: () => void;
}) {
  const pendingOfferSender =
    pendingOffer?.sender_device_name?.trim() ||
    pendingOffer?.sender_device_id ||
    null;
  const pendingOfferPreview = pendingOffer ? pendingOfferFilePreview(pendingOffer) : null;
  const pendingOfferResumeSummary = pendingOffer
    ? pendingOfferResumeSummaryLabel(pendingOffer.resume_summary)
    : null;
  const receiveReportSender =
    receiveReport?.sender_device_name?.trim() ||
    receiveReport?.sender_device_id ||
    null;
  const receivedBundle = receiveReport?.bundle ?? null;
  const receivedBundleSummary = receiveReport ? receiveBundleSummaryLine(receiveReport) : null;
  const receivedBundleStatus = receiveReport ? receiveBundleStatusLabel(receiveReport) : null;
  const receivedBundleHint = receivedBundle ? receiveBundleImportHint(receivedBundle) : null;
  const receiveSecurity = receiveReport
    ? buildTransferSecurityViewModel(receiveReport.security_mode)
    : null;
  const receiveSecuritySummary = receiveReport ? receiveSecuritySummaryLine(receiveReport) : null;
  const diagnosticsAdvice = receiveDiagnosticsAdvice(diagnostics);
  const visibleStagedBundles = stagedBundles.filter((bundle) => bundle.staging_status !== "imported");

  return (
    <section className="receive-page">
      {pendingOffer ? (
        <section className="receive-section">
          <h2 className="receive-section-title">待确认传输</h2>
          <div className="incoming-offer">
            <div className="offer-main">
              <strong>{pendingOfferSender ? `来自 ${pendingOfferSender}` : "传输请求"}</strong>
              <span>
                {pendingOffer.root_name} · {pendingOffer.file_count} 个文件 · {formatBytes(pendingOffer.total_bytes)}
              </span>
              {pendingOfferResumeSummary ? <small>{pendingOfferResumeSummary}</small> : null}
              {pendingOfferPreview ? <small>{pendingOfferPreview}</small> : null}
            </div>
            <div className="offer-actions">
              <button className="tool-button" disabled={busy === "receive"} onClick={() => onRespondReceiveOffer(false)} type="button">
                拒绝
              </button>
              <button className="primary-button" disabled={busy === "receive"} onClick={() => onRespondReceiveOffer(true)} type="button">
                接受
              </button>
            </div>
          </div>
        </section>
      ) : null}

      {pendingPairingRequest ? (
        <section className="receive-section">
          <h2 className="receive-section-title">配对请求</h2>
          <div className="incoming-offer">
            <div className="offer-main">
              <strong>{pendingPairingRequest.device_name}</strong>
              <span>
                {devicePlatformLabel(pendingPairingRequest.platform)} · 配对码 {pendingPairingRequest.pairing_code}
              </span>
              <small>确认两端配对码一致后再接受</small>
            </div>
            <div className="offer-actions">
              <button className="tool-button" disabled={busy === "pair"} onClick={() => onRespondPairingRequest(false)} type="button">
                拒绝
              </button>
              <button className="primary-button" disabled={busy === "pair"} onClick={() => onRespondPairingRequest(true)} type="button">
                接受
              </button>
            </div>
          </div>
        </section>
      ) : null}

      {visibleStagedBundles.length > 0 ? (
        <section className="receive-section">
          <h2 className="receive-section-title">待处理资料</h2>
          <div className="bundle-list">
            {visibleStagedBundles.map((bundle) => (
              <div className="bundle-line" key={bundle.bundle_id}>
                <div className="bundle-copy">
                  <span>
                    {bundle.display_name} · {bundleTypeLabel(bundle.bundle_type)} · {bundle.source_app} · {formatBytes(bundle.total_bytes)}
                  </span>
                  <small>{receiveBundleImportHint(bundle)}</small>
                </div>
                <div className="bundle-actions">
                  <strong>{bundleStatusLabel(bundle)}</strong>
                  {bundle.can_import_now ? (
                    <button
                      className="primary-button"
                      disabled={busy === "bundle-import"}
                      onClick={() => onImportStagedBundle(bundle)}
                      type="button"
                    >
                      导入
                    </button>
                  ) : null}
                  <button
                    className="text-button"
                    disabled={busy === "receive"}
                    onClick={() => onDeleteStagedBundle(bundle)}
                    type="button"
                  >
                    删除
                  </button>
                </div>
              </div>
            ))}
          </div>
        </section>
      ) : null}

      {receiveSession ? (
        <section className="receive-section">
          <h2 className="receive-section-title">等待接收</h2>
          <p className="receive-section-note">
            {diagnosticsAdvice ?? "发送端可粘贴此连接码，或输入你的局域网 IP:端口"}
          </p>
          <div className="code-highlight">
            <code title={receiveSession.connection_code}>{receiveSession.connection_code}</code>
            <div className="code-highlight-actions">
              <button className="primary-button" onClick={onCopyConnectionCode} type="button">
                复制连接码
              </button>
              <button className="tool-button" onClick={() => onOpenPath(receiveSession.receive_dir)} type="button">
                打开目录
              </button>
              <button className="text-button" disabled={busy === "receive"} onClick={onStopReceive} type="button">
                停止收件
              </button>
            </div>
          </div>
          {diagnostics && diagnostics.checks.length > 0 ? (
            <ul className="settings-checks">
              {diagnostics.checks.map((check) => (
                <li key={check}>{check}</li>
              ))}
            </ul>
          ) : null}
        </section>
      ) : (
        <section className="receive-section receive-empty">
          <p className="receive-section-note">
            开启收件后，附近设备可直接发送文件，或让对方粘贴连接码。
          </p>
          <button className="primary-button" disabled={busy === "receive"} onClick={onStartReceive} type="button">
            开启收件
          </button>
        </section>
      )}

      {receiveReport ? (
        <section className="receive-section">
          <h2 className="receive-section-title">最近完成</h2>
          <div className="result-line">
            <strong title={receiveReport.root_name}>
              {receiveReportSender ? `来自 ${receiveReportSender}` : receiveReport.root_name}
            </strong>
            <span>
              {receiveReport.file_count} 个 · {receiveReport.files.every((file) => file.verified) ? "已校验" : "检查中"}
            </span>
          </div>
          {receiveSecurity ? (
            <div className="security-line">
              <SecurityBadge model={receiveSecurity} />
              {receiveSecuritySummary ? <span>{receiveSecuritySummary}</span> : null}
            </div>
          ) : null}
          {receivedBundleSummary ? (
            <div className="bundle-line">
              <span>{receivedBundleSummary}</span>
              {receivedBundleHint ? <small>{receivedBundleHint}</small> : null}
              <div className="bundle-actions">
                {receivedBundleStatus ? <strong>{receivedBundleStatus}</strong> : null}
                {receivedBundle ? (
                  <>
                    {receivedBundle.can_import_now ? (
                      <button
                        className="primary-button"
                        disabled={busy === "bundle-import"}
                        onClick={() => onImportStagedBundle(receivedBundle)}
                        type="button"
                      >
                        导入
                      </button>
                    ) : null}
                    <button
                      className="text-button"
                      disabled={busy === "receive"}
                      onClick={() => onDeleteStagedBundle(receivedBundle)}
                      type="button"
                    >
                      删除暂存
                    </button>
                  </>
                ) : null}
              </div>
            </div>
          ) : null}
        </section>
      ) : null}
    </section>
  );
}

type SettingsTab = "general" | "receive" | "security" | "access" | "advanced" | "about";

const SETTINGS_TABS: Array<{ id: SettingsTab; label: string }> = [
  { id: "general", label: "通用" },
  { id: "receive", label: "收件" },
  { id: "security", label: "安全" },
  { id: "access", label: "接入" },
  { id: "advanced", label: "高级" },
  { id: "about", label: "关于" }
];

const EXPERIMENTAL_TRANSPORTS: Array<{ id: string; label: string }> = [
  { id: "iroh", label: "iroh" },
  { id: "relay", label: "relay" },
  { id: "p2p", label: "P2P" }
];

function SettingsPanel({
  bindPort,
  busy,
  deviceNameInput,
  discoveryStatus,
  localBridgeAuthorizationCode,
  localBridgeAuthorizations,
  localBridgeActionResults,
  localBridgeCheck,
  localBridgePendingActions,
  localBridgeStatus,
  receiveDir,
  receivePolicy,
  receiveSession,
  setBindPort,
  setDeviceNameInput,
  setLocalBridgeAuthorizationCode,
  setReceiveDir,
  snapshot,
  onChooseReceiveDir,
  onConfirmLocalBridgeAuthorization,
  onOpenReceiveDir,
  onPruneLocalBridgeAuthorizations,
  onRemoveLocalBridgePendingAction,
  onRevokeLocalBridgeAuthorization,
  onRunLocalBridgeSelfCheck,
  onSaveDeviceName,
  onSaveReceiveDir,
  onSaveReceivePort,
  onUpdateReceivePolicy
}: {
  bindPort: string;
  busy: BusyMode | null;
  deviceNameInput: string;
  discoveryStatus: DiscoveryStatusDto | null;
  localBridgeAuthorizationCode: string;
  localBridgeAuthorizations: LocalBridgeAuthorizationDto[];
  localBridgeActionResults: LocalBridgePendingActionResultDto[];
  localBridgeCheck: string | null;
  localBridgePendingActions: LocalBridgePendingActionDto[];
  localBridgeStatus: LocalBridgeRuntimeStatusDto | null;
  receiveDir: string;
  receivePolicy: ReceivePolicyMode;
  receiveSession: ReceiveSessionDto | null;
  setBindPort: (value: string) => void;
  setDeviceNameInput: (value: string) => void;
  setLocalBridgeAuthorizationCode: (value: string) => void;
  setReceiveDir: (value: string) => void;
  snapshot: AppSnapshot | null;
  onChooseReceiveDir: () => void;
  onConfirmLocalBridgeAuthorization: () => void;
  onOpenReceiveDir: () => void;
  onPruneLocalBridgeAuthorizations: () => void;
  onRemoveLocalBridgePendingAction: (action: LocalBridgePendingActionDto) => void;
  onRevokeLocalBridgeAuthorization: (authorization: LocalBridgeAuthorizationDto, scope: LocalBridgePermissionScope) => void;
  onRunLocalBridgeSelfCheck: () => void;
  onSaveDeviceName: () => void;
  onSaveReceiveDir: () => void;
  onSaveReceivePort: () => void;
  onUpdateReceivePolicy: (policy: ReceivePolicyMode) => void;
}) {
  const model = buildSettingsViewModel({
    snapshot,
    deviceNameInput,
    discoveryStatus,
    receiveSession,
    receiveDir,
    receivePolicy,
    bindPort
  });

  const [tab, setTab] = useState<SettingsTab>("general");

  return (
    <section className="settings-page">
      <div className="settings-tabs" role="tablist" aria-label="设置分区">
        {SETTINGS_TABS.map((item) => (
          <button
            aria-selected={tab === item.id}
            className={tab === item.id ? "settings-tab is-active" : "settings-tab"}
            key={item.id}
            onClick={() => setTab(item.id)}
            role="tab"
            type="button"
          >
            {item.label}
          </button>
        ))}
      </div>

      {tab === "general" ? (
        <SettingsGroup title="通用" note="附近设备会看到这里的名称">
          <SettingsRow label="设备名称">
            <div className="settings-inline-field">
              <input value={deviceNameInput} onChange={(event) => setDeviceNameInput(event.target.value)} />
              <button className="tool-button" disabled={busy === "device-name" || !model.canSaveDeviceName} onClick={onSaveDeviceName} type="button">
                保存
              </button>
            </div>
          </SettingsRow>
          <SettingsRow label="平台">
            <SettingsValue>{model.platformLabel}</SettingsValue>
          </SettingsRow>
        </SettingsGroup>
      ) : null}

      {tab === "receive" ? (
        <SettingsGroup
          title="收件"
          note={model.receiveConfigLocked ? "收件开启时，目录和端口需先关闭收件再改" : "默认保存位置和连接端口"}
        >
          <SettingsRow label="保存位置">
            <div className="settings-inline-field is-wide">
              <input disabled={model.receiveConfigLocked} value={model.receiveDir} onChange={(event) => setReceiveDir(event.target.value)} />
              <button className="tool-button" disabled={busy === "pick-receive" || model.receiveConfigLocked} onClick={onChooseReceiveDir} type="button">
                选择
              </button>
              <button className="tool-button" disabled={busy === "pick-receive" || !model.canSaveReceiveDir} onClick={onSaveReceiveDir} type="button">
                保存
              </button>
            </div>
          </SettingsRow>
          <SettingsRow label="默认端口">
            <div className="settings-inline-field">
              <input className="is-port" disabled={model.receiveConfigLocked} value={model.bindPort} onChange={(event) => setBindPort(event.target.value)} />
              <button className="tool-button" disabled={busy === "pick-receive" || !model.canSaveReceivePort} onClick={onSaveReceivePort} type="button">
                保存
              </button>
            </div>
          </SettingsRow>
          <SettingsRow label="收到文件时">
            <div className="policy-segment is-settings">
              {RECEIVE_POLICY_OPTIONS.map((option) => (
                <button
                  className={receivePolicy === option.value ? "policy-button is-active" : "policy-button"}
                  disabled={busy === "receive-policy"}
                  key={option.value}
                  onClick={() => onUpdateReceivePolicy(option.value)}
                  type="button"
                >
                  {option.label}
                </button>
              ))}
            </div>
          </SettingsRow>
          {receiveSession ? (
            <SettingsRow label="当前地址" hint="连接码请在收件页查看">
              <SettingsValue>{model.receiveAddressLabel}</SettingsValue>
            </SettingsRow>
          ) : null}
          <SettingsRow label="文件夹">
            <button className="text-button" disabled={busy === "open"} onClick={onOpenReceiveDir} type="button">
              打开接收文件夹
            </button>
          </SettingsRow>
        </SettingsGroup>
      ) : null}

      {tab === "security" ? (
        <SettingsGroup title="安全" note="用于配对与排查">
          <SettingsRow label="设备 ID">
            <SettingsValue mono title={model.deviceIdLabel ?? undefined}>
              {model.deviceIdLabel ?? "—"}
            </SettingsValue>
          </SettingsRow>
          <SettingsRow label="指纹">
            <SettingsValue mono title={model.fingerprintLabel ?? undefined}>
              {model.fingerprintLabel ?? "—"}
            </SettingsValue>
          </SettingsRow>
          <SettingsRow label="能力">
            <SettingsValue>{model.capabilitiesLabel ?? "—"}</SettingsValue>
          </SettingsRow>
        </SettingsGroup>
      ) : null}

      {tab === "access" ? (
        <IntegrationSettings
          busy={busy}
          localBridgeAuthorizationCode={localBridgeAuthorizationCode}
          localBridgeAuthorizations={localBridgeAuthorizations}
          localBridgeActionResults={localBridgeActionResults}
          localBridgeCheck={localBridgeCheck}
          localBridgePendingActions={localBridgePendingActions}
          localBridgeStatus={localBridgeStatus}
          setLocalBridgeAuthorizationCode={setLocalBridgeAuthorizationCode}
          onConfirmLocalBridgeAuthorization={onConfirmLocalBridgeAuthorization}
          onPruneLocalBridgeAuthorizations={onPruneLocalBridgeAuthorizations}
          onRemoveLocalBridgePendingAction={onRemoveLocalBridgePendingAction}
          onRevokeLocalBridgeAuthorization={onRevokeLocalBridgeAuthorization}
          onRunLocalBridgeSelfCheck={onRunLocalBridgeSelfCheck}
        />
      ) : null}

      {tab === "advanced" ? (
        <>
          <SettingsGroup title="网络" note="发现与连接状态为只读">
            <SettingsRow label="局域网发现">
              <SettingsValue>{model.discoveryLabel}</SettingsValue>
            </SettingsRow>
            <SettingsRow label="本机地址">
              <SettingsValue mono title={model.lanIpLabel ?? undefined}>
                {model.lanIpLabel ?? "—"}
              </SettingsValue>
            </SettingsRow>
            <SettingsRow label="托盘菜单">
              <SettingsValue>{model.trayLabel}</SettingsValue>
            </SettingsRow>
          </SettingsGroup>

          <SettingsGroup title="实验传输" note="跨网络通道尚未接入，当前仅同局域网 TCP">
            {EXPERIMENTAL_TRANSPORTS.map((transport) => (
              <SettingsRow key={transport.id} label={transport.label}>
                <SettingsValue>未启用</SettingsValue>
              </SettingsRow>
            ))}
          </SettingsGroup>
        </>
      ) : null}

      {tab === "about" ? (
        <SettingsGroup title="关于" note="本机文件传输完成后会做完整性校验">
          <SettingsRow label="应用">
            <SettingsValue>NekoDrop</SettingsValue>
          </SettingsRow>
          <SettingsRow label="文件校验">
            <SettingsValue>传输完成后 SHA-256</SettingsValue>
          </SettingsRow>
          <SettingsRow label="项目">
            <SettingsValue mono>github.com/Hisakazu333/NekoDrop</SettingsValue>
          </SettingsRow>
        </SettingsGroup>
      ) : null}
    </section>
  );
}

function HistoryPanel({
  busy,
  selectedTransferId,
  transferMetrics,
  transferStatus,
  transfers,
  onCancelTransfer,
  onClearTransfers,
  onDeleteTransfer,
  onOpenTransfer,
  onResendTransfer,
  onSelectTransfer,
  onUseFallbackCode
}: {
  busy: BusyMode | null;
  selectedTransferId: string | null;
  transferMetrics: TransferMetrics;
  transferStatus: TransferStatusDto | null;
  transfers: TransferDto[];
  onCancelTransfer: () => void;
  onClearTransfers: () => void;
  onDeleteTransfer: (transfer: TransferDto) => void;
  onOpenTransfer: (transfer: TransferDto) => void;
  onResendTransfer: (transfer: TransferDto) => void;
  onSelectTransfer: (transfer: TransferDto) => void;
  onUseFallbackCode: () => void;
}) {
  const needsAttention = transfers.filter((transfer) => isTransferNeedsAttention(transfer));
  const inProgress = transfers.filter((transfer) => isTransferInProgress(transfer));
  const done = transfers.filter(
    (transfer) => !isTransferNeedsAttention(transfer) && !isTransferInProgress(transfer)
  );

  const renderItem = (transfer: TransferDto, quickActions: boolean) => {
    const selected = transfer.id === selectedTransferId;
    const paths = transfer.received_paths.length > 0 ? transfer.received_paths : transfer.source_paths;
    const detail = buildTransferHistoryDetailViewModel(transfer);
    const fallbackActionLabel = transferFallbackActionLabel(transfer);
    const security = buildTransferSecurityViewModel(transfer.security_mode);
    return (
      <div
        className={[
          "history-item",
          selected ? "is-selected" : "",
          transfer.status === "failed" ? "is-failed" : ""
        ]
          .filter(Boolean)
          .join(" ")}
        key={transfer.id}
      >
        <button className="history-row" onClick={() => onSelectTransfer(transfer)} type="button">
          <span className="history-kind">{transferDirectionLabel(transfer)}</span>
          <span className="history-main">
            <strong title={transfer.root_name}>{transfer.root_name}</strong>
            <small title={transfer.error_message ?? transfer.peer_name ?? transfer.target_host ?? undefined}>
              {transferMetaLabel(transfer)}
            </small>
          </span>
          <span className="history-size">
            {transfer.file_count} 个 · {formatBytes(transfer.total_bytes)}
          </span>
          <time>{formatTransferTime(transfer.updated_at_ms)}</time>
        </button>
        {shouldShowHistoryProgress(transfer) ? (
          <div className="history-progress" aria-label="历史传输进度">
            <span style={{ width: `${Math.round(transfer.progress * 100)}%` }} />
          </div>
        ) : null}
        {quickActions && !selected ? (
          <div className="history-actions is-quick">
            {detail.primaryActionLabel ? (
              <button className="text-button" disabled={busy === "resend"} onClick={() => onResendTransfer(transfer)} type="button">
                {detail.primaryActionLabel}
              </button>
            ) : null}
            {fallbackActionLabel ? (
              <button className="text-button" onClick={onUseFallbackCode} type="button">
                {fallbackActionLabel}
              </button>
            ) : null}
            <button className="text-button" disabled={busy === "history"} onClick={() => onDeleteTransfer(transfer)} type="button">
              删除
            </button>
          </div>
        ) : null}
        {selected ? (
          <div className="history-detail">
            <div className="history-detail-main">
              <div className="history-detail-grid">
                {detail.progressLabel ? (
                  <span><strong>进度</strong>{detail.progressLabel}</span>
                ) : null}
                {detail.peerLabel ? (
                  <span><strong>对方</strong>{detail.peerLabel}</span>
                ) : null}
                {detail.locationLabel ? (
                  <span><strong>位置</strong>{detail.locationLabel}</span>
                ) : null}
                {security ? (
                  <span>
                    <strong>安全</strong>
                    <SecurityBadge model={security} />
                  </span>
                ) : null}
                {detail.recoveryLabel ? (
                  <span><strong>恢复</strong>{detail.recoveryLabel}</span>
                ) : null}
                {detail.errorLabel ? (
                  <span className="is-error"><strong>原因</strong>{detail.errorLabel}</span>
                ) : null}
                {detail.adviceLabel ? (
                  <span><strong>建议</strong>{detail.adviceLabel}</span>
                ) : null}
              </div>
              <div className="history-paths">
                {paths.slice(0, 6).map((path) => (
                  <span key={path} title={path}>{path}</span>
                ))}
                {paths.length > 6 ? <span>还有 {paths.length - 6} 个</span> : null}
              </div>
            </div>
            <div className="history-actions">
              <button className="text-button" disabled={busy === "open"} onClick={() => onOpenTransfer(transfer)} type="button">
                打开
              </button>
              {detail.primaryActionLabel ? (
                <button className="text-button" disabled={busy === "resend"} onClick={() => onResendTransfer(transfer)} type="button">
                  {detail.primaryActionLabel}
                </button>
              ) : null}
              {fallbackActionLabel ? (
                <button className="text-button" onClick={onUseFallbackCode} type="button">
                  {fallbackActionLabel}
                </button>
              ) : null}
              <button className="text-button" disabled={busy === "history"} onClick={() => onDeleteTransfer(transfer)} type="button">
                删除
              </button>
            </div>
          </div>
        ) : null}
      </div>
    );
  };

  return (
    <section className="history-panel">
      <div className="history-toolbar">
        <div>
          <strong>{transfers.length}</strong>
          <span>条记录</span>
        </div>
        <button className="text-button" disabled={busy === "history" || transfers.length === 0} onClick={onClearTransfers} type="button">
          清空
        </button>
      </div>

      {transferStatus && shouldShowActiveTransferBar(transferStatus) ? (
        <TransferStatusView
          busy={busy}
          metrics={transferMetrics}
          status={transferStatus}
          onCancel={onCancelTransfer}
          onUseFallbackCode={onUseFallbackCode}
        />
      ) : null}

      {transfers.length === 0 ? (
        <div className="history-empty">暂无记录</div>
      ) : (
        <>
          {needsAttention.length > 0 ? (
            <section className="history-group">
              <h2 className="history-group-title">需要处理</h2>
              <div className="history-list">
                {needsAttention.map((transfer) => renderItem(transfer, true))}
              </div>
            </section>
          ) : null}

          {inProgress.length > 0 ? (
            <section className="history-group">
              <h2 className="history-group-title">进行中</h2>
              <div className="history-list">
                {inProgress.map((transfer) => renderItem(transfer, false))}
              </div>
            </section>
          ) : null}

          {done.length > 0 ? (
            <section className="history-group">
              <h2 className="history-group-title">历史</h2>
              <div className="history-list">
                {done.map((transfer) => renderItem(transfer, false))}
              </div>
            </section>
          ) : null}
        </>
      )}
    </section>
  );
}

function StatusLine({
  busy,
  plan,
  receiveReport,
  receiveSession,
  sendReport,
  transferMetrics,
  transferStatus,
  transferCount,
  recoveryTransfer,
  showActiveTransfer = true,
  onCancelTransfer,
  onRecoverTransfer,
  onUseFallbackCode
}: {
  busy: BusyMode | null;
  plan: TransferPlanDto | null;
  receiveReport: ReceiveReportDto | null;
  receiveSession: ReceiveSessionDto | null;
  sendReport: SendReportDto | null;
  transferMetrics: TransferMetrics;
  transferStatus: TransferStatusDto | null;
  transferCount: number;
  recoveryTransfer: TransferDto | null;
  showActiveTransfer?: boolean;
  onCancelTransfer: () => void;
  onRecoverTransfer: (transfer: TransferDto) => void;
  onUseFallbackCode: () => void;
}) {
  if (
    showActiveTransfer &&
    transferStatus &&
    shouldShowActiveTransferBar(transferStatus)
  ) {
    return (
      <TransferStatusView
        busy={busy}
        metrics={transferMetrics}
        status={transferStatus}
        recoveryTransfer={recoveryTransfer}
        onCancel={onCancelTransfer}
        onRecover={onRecoverTransfer}
        onUseFallbackCode={onUseFallbackCode}
      />
    );
  }

  if (sendReport) {
    return (
      <div className="status-line">
        <strong>发送完成</strong>
        <span>{sendReport.root_name} · {sendReport.file_count} 个文件 · {formatBytes(sendReport.total_bytes)}</span>
      </div>
    );
  }

  if (receiveReport) {
    const receivedBundleSummary = receiveBundleSummaryLine(receiveReport);
    const receivedBundleStatus = receiveBundleStatusLabel(receiveReport);
    return (
      <div className="status-line">
        <strong>接收完成</strong>
        <span>
          {receivedBundleSummary
            ? [receivedBundleSummary, receivedBundleStatus, "已保存"].filter(Boolean).join(" · ")
            : `${receiveReport.file_count} 个文件 · ${receiveReport.files.every((file) => file.verified) ? "已校验" : "检查"}`}
        </span>
      </div>
    );
  }

  if (plan) {
    return (
      <div className="status-line">
        <strong>已选择</strong>
        <span>{plan.file_count} 个文件 · {formatBytes(plan.total_bytes)}</span>
      </div>
    );
  }

  if (transferCount > 0) {
    return (
      <div className="status-line">
        <strong>队列</strong>
        <span>{transferCount} 个路径待扫描</span>
      </div>
    );
  }

  return null;
}

function RecentActivity({
  busy,
  compact = false,
  selectedTransferId,
  transfers,
  onClearTransfers,
  onDeleteTransfer,
  onOpenTransfer,
  onResendTransfer,
  onSelectTransfer,
  onUseFallbackCode
}: {
  busy: BusyMode | null;
  compact?: boolean;
  selectedTransferId: string | null;
  transfers: TransferDto[];
  onClearTransfers: () => void;
  onDeleteTransfer: (transfer: TransferDto) => void;
  onOpenTransfer: (transfer: TransferDto) => void;
  onResendTransfer: (transfer: TransferDto) => void;
  onSelectTransfer: (transfer: TransferDto) => void;
  onUseFallbackCode: () => void;
}) {
  const recentTransfers = transfers.slice(0, compact ? 5 : 3);
  if (recentTransfers.length === 0) return null;

  return (
    <section className={compact ? "recent-block is-compact" : "recent-block"}>
      <div className="section-head">
        <strong>最近</strong>
        <button className="text-button" disabled={busy === "history"} onClick={onClearTransfers} type="button">
          清空
        </button>
      </div>
      <div className="recent-list">
        {recentTransfers.map((transfer) => {
          const selected = transfer.id === selectedTransferId;
          const paths = transfer.received_paths.length > 0 ? transfer.received_paths : transfer.source_paths;
          const actionLabel = transferPrimaryActionLabel(transfer);
          const fallbackActionLabel = transferFallbackActionLabel(transfer);
          const detailLine = buildRecentTransferDetailLine(transfer);
          const security = buildTransferSecurityViewModel(transfer.security_mode);
          return (
            <div
              className={[
                "recent-item",
                selected ? "is-selected" : "",
                transfer.status === "failed" ? "is-failed" : ""
              ]
                .filter(Boolean)
                .join(" ")}
              key={transfer.id}
            >
              <button className="recent-row" onClick={() => onSelectTransfer(transfer)} type="button">
                <span>{transferDirectionLabel(transfer)}</span>
                <strong title={transfer.root_name}>{transfer.root_name}</strong>
                <small title={transfer.error_message ?? transfer.peer_name ?? transfer.target_host ?? undefined}>
                  {transferMetaLabel(transfer)}
                </small>
              </button>
              {selected ? (
                <div className="recent-detail">
                  {detailLine ? <strong>{detailLine}</strong> : null}
                  {security ? <SecurityBadge model={security} /> : null}
                  {paths.slice(0, 3).map((path) => (
                    <span key={path} title={path}>{path}</span>
                  ))}
                  {paths.length > 3 ? <span>还有 {paths.length - 3} 个</span> : null}
                  <div className="recent-actions">
                    <button className="text-button" disabled={busy === "open"} onClick={() => onOpenTransfer(transfer)} type="button">
                      打开
                    </button>
                    {actionLabel ? (
                      <button className="text-button" disabled={busy === "resend"} onClick={() => onResendTransfer(transfer)} type="button">
                        {actionLabel}
                      </button>
                    ) : null}
                    {fallbackActionLabel ? (
                      <button className="text-button" onClick={onUseFallbackCode} type="button">
                        {fallbackActionLabel}
                      </button>
                    ) : null}
                    <button className="text-button" disabled={busy === "history"} onClick={() => onDeleteTransfer(transfer)} type="button">
                      删除
                    </button>
                  </div>
                </div>
              ) : null}
            </div>
          );
        })}
      </div>
    </section>
  );
}

function ActiveTransferBar({
  busy,
  metrics,
  status,
  recoveryTransfer,
  onCancel,
  onRecover,
  onUseFallbackCode
}: {
  busy: BusyMode | null;
  metrics: TransferMetrics;
  status: TransferStatusDto;
  recoveryTransfer: TransferDto | null;
  onCancel: () => void;
  onRecover: (transfer: TransferDto) => void;
  onUseFallbackCode: () => void;
}) {
  const model = buildTransferProgressViewModel(status, metrics);
  const recoveryActions = currentTransferRecoveryActions(status, recoveryTransfer);
  const canCancel =
    !matchesTerminalTransferPhase(status.phase) &&
    (status.direction === "send" ||
      (status.direction === "receive" && isReceiveTransferActivePhase(status.phase)));

  return (
    <section className={isRecoverableCurrentStatus(status.phase) ? "active-transfer is-error" : "active-transfer"}>
      <div className="active-transfer-main">
        <div className="active-transfer-title">
          <strong>{model.title}</strong>
          <span title={model.rootName}>{model.rootName}</span>
        </div>
        {shouldShowTransferProgressMeter(status) ? (
          <div className="active-transfer-meter" aria-label="当前传输进度">
            <span style={{ width: `${model.progressPercent}%` }} />
          </div>
        ) : null}
        {shouldShowTransferProgressMeter(status) ? (
          <div className="active-transfer-meta">
            <span>{model.percentLabel}</span>
            <span>{model.bytesLabel}</span>
            <span>{model.fileIndexLabel}</span>
            {model.speedLabel ? <span>{model.speedLabel}</span> : null}
            {model.etaLabel ? <span>{model.etaLabel}</span> : null}
            {model.currentFileLabel ? <span title={model.currentFileLabel}>{model.currentFileLabel}</span> : null}
          </div>
        ) : (
          <div className="active-transfer-meta">
            <span>{model.message}</span>
          </div>
        )}
      </div>
      {canCancel ? (
        <button className="text-button" disabled={busy === "cancel-transfer"} onClick={onCancel} type="button">
          取消
        </button>
      ) : isRecoverableCurrentStatus(status.phase) ? (
        <div className="transfer-status-actions">
          {recoveryActions.primaryLabel && recoveryTransfer ? (
            <button className="text-button" disabled={busy === "resend"} onClick={() => onRecover(recoveryTransfer)} type="button">
              {recoveryActions.primaryLabel}
            </button>
          ) : null}
          {recoveryActions.fallbackLabel ? (
            <button className="text-button" onClick={onUseFallbackCode} type="button">
              {recoveryActions.fallbackLabel}
            </button>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}

function TransferStatusView({
  busy,
  metrics,
  status,
  recoveryTransfer,
  onCancel,
  onRecover,
  onUseFallbackCode
}: {
  busy?: BusyMode | null;
  metrics?: TransferMetrics;
  status: TransferStatusDto;
  recoveryTransfer?: TransferDto | null;
  onCancel?: () => void;
  onRecover?: (transfer: TransferDto) => void;
  onUseFallbackCode?: () => void;
}) {
  const model = buildTransferProgressViewModel(status, metrics ?? EMPTY_TRANSFER_METRICS);
  const recoveryActions = currentTransferRecoveryActions(status, recoveryTransfer ?? null);
  const canCancel =
    onCancel &&
    !matchesTerminalTransferPhase(status.phase) &&
    (status.direction === "send" ||
      (status.direction === "receive" && isReceiveTransferActivePhase(status.phase)));

  return (
    <div className={isRecoverableCurrentStatus(status.phase) ? "transfer-status is-error" : "transfer-status"}>
      <div className="transfer-status-head">
        <strong>{model.title}</strong>
        {status.total_bytes > 0 ? (
          <span>{model.percentLabel} · {model.bytesLabel}</span>
        ) : null}
        {canCancel ? (
          <button className="text-button" disabled={busy === "cancel-transfer"} onClick={onCancel} type="button">
            取消
          </button>
        ) : isRecoverableCurrentStatus(status.phase) ? (
          <span className="transfer-status-actions">
            {recoveryActions.primaryLabel && recoveryTransfer && onRecover ? (
              <button className="text-button" disabled={busy === "resend"} onClick={() => onRecover(recoveryTransfer)} type="button">
                {recoveryActions.primaryLabel}
              </button>
            ) : null}
            {recoveryActions.fallbackLabel && onUseFallbackCode ? (
              <button className="text-button" onClick={onUseFallbackCode} type="button">
                {recoveryActions.fallbackLabel}
              </button>
            ) : null}
          </span>
        ) : null}
      </div>
      {status.total_bytes > 0 ? (
        <div className="progress-track" aria-label="传输进度">
          <span style={{ width: `${model.progressPercent}%` }} />
        </div>
      ) : null}
      <div className="transfer-status-meta">
        <span>{model.message}</span>
        <span>{model.fileIndexLabel}</span>
        {model.speedLabel ? <span>{model.speedLabel}{model.etaLabel ? ` · ${model.etaLabel}` : ""}</span> : null}
        {model.adviceLabel ? <span>{model.adviceLabel}</span> : null}
        {model.currentFileLabel ? <span title={model.currentFileLabel}>{model.currentFileLabel}</span> : null}
      </div>
    </div>
  );
}

function SecurityBadge({ model }: { model: TransferSecurityViewModel }) {
  return (
    <span className={`security-badge is-${model.tone}`} title={model.detail}>
      {model.label}
    </span>
  );
}

function buildPathPayload(selectedPaths: string[], manualPaths: string) {
  const manual = manualPaths
    .split(/\r?\n/)
    .map((line) => normalizeInputPath(line))
    .filter((line): line is string => Boolean(line));

  return uniquePaths([...selectedPaths, ...manual]);
}

function normalizeInputPath(path: string) {
  const trimmed = path.trim().replace(/\\ /g, " ");
  if (!trimmed) return null;
  return trimmed.replace(/^["']|["']$/g, "");
}

function uniquePaths(paths: string[]) {
  return Array.from(new Set(paths.map((path) => path.trim()).filter(Boolean)));
}

function keepIfEqual<T>(current: T, next: T): T {
  if (Object.is(current, next)) return current;
  if (current == null || next == null) return next;
  return stableJson(current) === stableJson(next) ? current : next;
}

function resetTransferMetrics(
  setTransferMetrics: (updater: (current: TransferMetrics) => TransferMetrics) => void
) {
  setTransferMetrics((current) => keepIfEqual(current, EMPTY_TRANSFER_METRICS));
}

function stableJson(value: unknown) {
  return JSON.stringify(value);
}

function trustedDeviceToDeviceDto(device: TrustedDeviceDto): DeviceDto {
  return {
    id: device.device_id,
    name: device.device_name,
    platform: device.platform,
    host: device.host,
    port: device.port,
    trust_state: "Trusted",
    public_key_fingerprint: device.public_key_fingerprint,
    pairing_code: device.pairing_code
  };
}

function buildPageTitle({
  connectionCode,
  mode,
  selectedDeviceName,
  selectedTargetLabel
}: {
  connectionCode: string;
  mode: ComposerMode;
  selectedDeviceName: string | null;
  selectedTargetLabel: string | null;
}) {
  if (mode === "overview") return "概览";
  if (mode === "send") {
    if (selectedTargetLabel) return `发给 ${selectedTargetLabel}`;
    if (selectedDeviceName) return `发给 ${selectedDeviceName}`;
    if (connectionCode.length > 0) return "使用备用码发送";
    return "发送";
  }
  if (mode === "receive") return "收件";
  if (mode === "devices") return "设备";
  if (mode === "transfers") return "传输";
  return "设置";
}

function buildPageSubtitle({
  composerSubtitle,
  discoveryLabel,
  mode,
  nearbyDeviceCount,
  receiveSessionBindAddr,
  receiveState,
  selectedTargetSubtitle,
  snapshotDeviceName,
  transferCount
}: {
  composerSubtitle: string;
  discoveryLabel: string;
  mode: ComposerMode;
  nearbyDeviceCount: number;
  receiveSessionBindAddr: string | null;
  receiveState: string;
  selectedTargetSubtitle: string | null;
  snapshotDeviceName: string | null;
  transferCount: number;
}) {
  if (mode === "overview") return `${receiveState} · ${nearbyDeviceCount} 台附近设备`;
  if (mode === "send") return selectedTargetSubtitle ?? composerSubtitle;
  if (mode === "receive") return receiveSessionBindAddr ?? receiveState;
  if (mode === "devices") {
    return nearbyDeviceCount > 0 ? `${nearbyDeviceCount} 台附近设备` : discoveryLabel;
  }
  if (mode === "transfers") {
    return transferCount > 0 ? `${transferCount} 条真实记录` : "暂无记录";
  }
  return snapshotDeviceName ?? "这台电脑";
}

function normalizeReceivePolicy(value: string): ReceivePolicyMode {
  if (value === "block_all") return value;
  return "always_ask";
}

function portFromBindAddr(bindAddr: string): number | null {
  const port = bindAddr.trim().split(":").pop();
  return port ? parseReceivePortValue(port) : null;
}

function pendingOfferFilePreview(offer: PendingReceiveOfferDto) {
  if (offer.files.length === 0) return null;
  const preview = offer.files.slice(0, 3).map((file) => file.manifest_path).join(" · ");
  const restCount = Math.max(0, offer.file_count - Math.min(3, offer.files.length));
  const rest = restCount > 0 ? ` +${restCount}` : "";
  return `${preview}${rest}`;
}

function pendingOfferResumeSummaryLabel(summary: PendingReceiveOfferDto["resume_summary"]) {
  if (!summary || summary.resumable_file_count <= 0) return null;

  const parts: string[] = [];
  if (summary.partial_file_count > 0) {
    parts.push(`可继续 ${summary.partial_file_count} 个文件`);
  }
  if (summary.completed_file_count > 0) {
    parts.push(`可跳过 ${summary.completed_file_count} 个已完成文件`);
  }
  if (parts.length === 0) {
    parts.push(`可继续 ${summary.resumable_file_count} 个文件`);
  }

  return `${parts.join(" · ")} · 已接收 ${formatBytes(summary.received_bytes)}`;
}

function lastPathSegment(path: string) {
  const normalized = path.trim().replace(/\\/g, "/").replace(/\/+$/, "");
  if (!normalized) return "资料包";
  return normalized.split("/").pop() || "资料包";
}

function localBridgeStatusLabel(status: string) {
  if (status === "ok") return "可读取";
  if (status === "pending_auth") return "等待确认";
  if (status === "unsupported") return "不支持";
  return status;
}

function localBridgeRuntimeStatusLine(status: LocalBridgeRuntimeStatusDto | null) {
  if (!status) return "读取中";
  if (!status.active) {
    return status.last_error ? `未监听 · ${status.last_error}` : "未监听";
  }
  const auth = status.pending_authorization_client
    ? ` · 等待 ${status.pending_authorization_client}`
    : status.authorization_count > 0
      ? ` · 已授权 ${status.authorization_count}`
      : "";
  const actions = status.pending_action_count > 0 ? ` · 待执行 ${status.pending_action_count}` : "";
  return `${status.bind_host}:${status.port}${status.request_path}${auth}${actions}`;
}

function localBridgeScopeLabel(scope: LocalBridgePermissionScope) {
  if (scope === "device.read") return "设备";
  if (scope === "transfer.status.read") return "传输";
  if (scope === "bundle.read") return "资料包";
  if (scope === "bundle.send") return "发送";
  if (scope === "bundle.import.request") return "导入";
  return scope;
}

function localBridgePendingActionKindLabel(actionKind: string) {
  if (actionKind === "bundle.send") return "发送资料包";
  if (actionKind === "bundle.import") return "导入资料包";
  return actionKind;
}

function localBridgePendingActionSummary(action: LocalBridgePendingActionDto) {
  if (action.action_kind === "bundle.send") {
    const type = action.bundle_type ? bundleTypeLabel(action.bundle_type) : "资料包";
    const target = action.target_device_id ? ` -> ${action.target_device_id}` : "";
    return `${localBridgePendingActionKindLabel(action.action_kind)} · ${type}${target}`;
  }
  if (action.action_kind === "bundle.import") {
    const type = action.expected_bundle_type ? bundleTypeLabel(action.expected_bundle_type) : "资料包";
    return `${localBridgePendingActionKindLabel(action.action_kind)} · ${type}`;
  }
  return localBridgePendingActionKindLabel(action.action_kind);
}

function localBridgePendingActionTitle(action: LocalBridgePendingActionDto) {
  const target = action.target_device_id ?? action.staged_bundle_id ?? action.request_id;
  return `${action.client_display_name} · ${localBridgePendingActionKindLabel(action.action_kind)} · ${target}`;
}

function localBridgeActionResultSummary(result: LocalBridgePendingActionResultDto) {
  const kind = localBridgePendingActionKindLabel(result.action_kind);
  const status = localBridgeActionResultStatusLabel(result.status);
  const target = result.bundle_id ?? result.target_device_id ?? result.request_id;
  const reason = result.reason ? ` · ${localBridgeActionResultReasonLabel(result.reason)}` : "";
  return `${kind} · ${status}${reason} · ${target}`;
}

function localBridgeActionResultStatusLabel(status: string) {
  if (status === "completed") return "完成";
  if (status === "failed") return "失败";
  if (status === "ready") return "可执行";
  if (status === "failed_preflight") return "预检失败";
  return status;
}

function localBridgeActionResultReasonLabel(reason: string) {
  if (reason === "bundle_root_missing") return "来源不存在";
  if (reason === "bundle_manifest_missing") return "缺少清单";
  if (reason === "bundle_invalid") return "校验失败";
  if (reason === "bundle_type_mismatch") return "类型不匹配";
  if (reason === "trusted_target_required") return "需要可信设备";
  if (reason === "trusted_target_missing") return "目标未配对";
  if (reason === "target_device_required") return "缺少目标设备";
  if (reason === "bundle_send_failed") return "发送失败";
  if (reason === "bundle_import_failed") return "导入失败";
  if (reason === "bundle_import_conflict") return "已存在同名导入";
  return reason;
}

function receivePolicyLabel(value: ReceivePolicyMode) {
  if (value === "block_all") return "已阻止外部接收";
  return "接收前询问";
}

function receiveDiagnosticsLabel(diagnostics: ReceivePortDiagnosticsDto | null) {
  if (!diagnostics) return "";
  if (!diagnostics.listening) return "未监听";
  if (diagnostics.phase === "no_lan_ip") return "无局域网地址";
  if (diagnostics.phase === "invalid_bind_addr") return "监听地址异常";
  if (diagnostics.advertised_host && diagnostics.port) {
    return `${diagnostics.advertised_host}:${diagnostics.port}`;
  }
  return diagnostics.port ? `端口 ${diagnostics.port}` : "";
}

function transferDirectionLabel(transfer: TransferDto) {
  if (transfer.status === "cancelled") return "取消";
  if (transfer.status === "failed") return "失败";
  if (transfer.direction === "receive") return "接收";
  return "发送";
}

function transferMetaLabel(transfer: TransferDto) {
  const recovery = transferRecoveryLabel(transfer);
  if (transfer.status === "failed") {
    const message = transfer.error_message ?? "失败";
    return recovery ? `${message} · ${recovery}` : message;
  }

  if (transfer.status === "cancelled" && recovery) {
    return `已取消 · ${recovery}`;
  }

  const size = formatBytes(transfer.total_bytes);
  const count = `${transfer.file_count} 个`;
  const peer = transfer.peer_name ?? transfer.target_host;
  return peer ? `${count} · ${size} · ${peer}` : `${count} · ${size}`;
}

function transferRecoveryLabel(transfer: TransferDto) {
  if (transferPrimaryActionLabel(transfer) !== "继续发送") return null;
  return `已传 ${formatBytes(transfer.transferred_bytes)} / ${formatBytes(transfer.total_bytes)}`;
}

function matchesTerminalTransferPhase(phase: string) {
  return ["completed", "failed", "cancelled", "declined", "expired", "closed", "blocked"].includes(phase);
}

function isRecoverableCurrentStatus(phase: string) {
  return phase === "failed" || phase === "cancelled";
}

function isReceiveTransferActivePhase(phase: string) {
  return phase === "accepted" || phase === "transferring" || phase === "verifying";
}

function shouldShowHistoryProgress(transfer: TransferDto) {
  return (
    transfer.total_bytes > 0 &&
    transfer.transferred_bytes > 0 &&
    transfer.transferred_bytes < transfer.total_bytes &&
    transfer.status !== "completed"
  );
}

const TRANSFER_ATTENTION_STATUSES = new Set(["failed", "cancelled"]);
const TRANSFER_ACTIVE_STATUSES = new Set([
  "connecting",
  "accepted",
  "awaiting_approval",
  "transferring",
  "verifying"
]);

function isTransferNeedsAttention(transfer: TransferDto) {
  return TRANSFER_ATTENTION_STATUSES.has(transfer.status);
}

function isTransferInProgress(transfer: TransferDto) {
  return TRANSFER_ACTIVE_STATUSES.has(transfer.status);
}

function formatTransferTime(timestampMs: number) {
  const date = new Date(timestampMs);
  if (Number.isNaN(date.getTime())) return "";
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false
  }).format(date);
}

function isTrustedDeviceState(trustState: string) {
  return trustState.toLowerCase() === "trusted";
}

function platformLabel(platform: string) {
  if (platform === "macos") return "macOS";
  if (platform === "windows") return "Windows";
  if (platform === "linux") return "Linux";
  if (platform === "ios") return "iOS";
  if (platform === "android") return "Android";
  if (platform === "openharmony") return "OpenHarmony";
  if (platform === "web") return "Web";
  return "Unknown";
}

function errorMessage(error: unknown) {
  if (error instanceof Error) return error.message;
  return String(error);
}

async function copyTextToClipboard(text: string) {
  if (navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return;
    } catch {
      // Some WebViews expose clipboard but reject writes depending on permissions.
    }
  }

  const textarea = document.createElement("textarea");
  textarea.value = text;
  textarea.setAttribute("readonly", "true");
  textarea.style.position = "fixed";
  textarea.style.left = "-9999px";
  textarea.style.top = "0";
  document.body.appendChild(textarea);
  textarea.select();

  try {
    const copied = document.execCommand("copy");
    if (!copied) {
      throw new Error("无法写入剪贴板，请手动选择连接码复制。");
    }
  } finally {
    document.body.removeChild(textarea);
  }
}

function deviceSendErrorMessage(message: string) {
  if (
    message.includes("failed to connect") ||
    message.includes("Connection refused") ||
    message.includes("连接") ||
    message.includes("不在线")
  ) {
    return `${message} 备用码重试`;
  }
  return message;
}

function isCancelMessage(message: string) {
  return message.includes("取消") || message.includes("cancelled") || message.includes("canceled");
}
