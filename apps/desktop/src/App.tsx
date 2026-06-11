import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";

import { invokeCommand } from "./tauri";
import type {
  AppSnapshot,
  DeviceDto,
  DiscoveryStatusDto,
  PendingPairingRequestDto,
  PendingReceiveOfferDto,
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
  | "cancel-transfer"
  | "pair"
  | "forget"
  | "history"
  | "resend"
  | "open";

type ComposerMode = "send" | "devices" | "receive" | "queue" | "history";
type ReceivePolicyMode = "always_ask" | "auto_accept_trusted" | "block_all";

const RECEIVE_POLICY_OPTIONS: Array<{ value: ReceivePolicyMode; label: string }> = [
  { value: "always_ask", label: "询问" },
  { value: "auto_accept_trusted", label: "可信" },
  { value: "block_all", label: "阻止" }
];

export function App() {
  const [snapshot, setSnapshot] = useState<AppSnapshot | null>(null);
  const [selectedPaths, setSelectedPaths] = useState<string[]>([]);
  const [manualPaths, setManualPaths] = useState("");
  const [connectionCode, setConnectionCode] = useState("");
  const [receiveDir, setReceiveDir] = useState("~/Downloads/NekoDrop");
  const [receivePolicy, setReceivePolicy] = useState<ReceivePolicyMode>("always_ask");
  const [bindPort, setBindPort] = useState("45821");
  const [plan, setPlan] = useState<TransferPlanDto | null>(null);
  const [scanStatus, setScanStatus] = useState<TransferScanProgressDto | null>(null);
  const [sendReport, setSendReport] = useState<SendReportDto | null>(null);
  const [nearbyDevices, setNearbyDevices] = useState<DeviceDto[]>([]);
  const [discoveryStatus, setDiscoveryStatus] = useState<DiscoveryStatusDto | null>(null);
  const [receiveSession, setReceiveSession] = useState<ReceiveSessionDto | null>(null);
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
  const [dragActive, setDragActive] = useState(false);
  const [busy, setBusy] = useState<BusyMode | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const previousTransferStatus = useRef<TransferStatusDto | null>(null);
  const autoReceiveStarted = useRef(false);
  const [transferMetrics, setTransferMetrics] = useState<{
    speedBytesPerSecond: number | null;
    etaSeconds: number | null;
  }>({ speedBytesPerSecond: null, etaSeconds: null });

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
  const canSend = transferPaths.length > 0 && !busy && (Boolean(selectedDevice) || trimmedConnectionCode.length > 0);
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
    refreshSnapshot().catch((nextError) => setError(errorMessage(nextError)));
    refreshReceiveState().catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!snapshot || receiveSession || autoReceiveStarted.current) return;
    autoReceiveStarted.current = true;
    startReceive({ receiveDirOverride: snapshot.receive_dir, silent: true }).catch((nextError) =>
      setError(errorMessage(nextError))
    );
  }, [snapshot, receiveSession]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      refreshReceiveState().catch(() => undefined);
    }, 1200);
    return () => window.clearInterval(timer);
  }, []);

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
      setTransferMetrics({ speedBytesPerSecond: null, etaSeconds: null });
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
    setTransferMetrics({
      speedBytesPerSecond,
      etaSeconds: speedBytesPerSecond > 0 ? Math.ceil(remainingBytes / speedBytesPerSecond) : null
    });
  }, [transferStatus]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "enter" || event.payload.type === "over") {
          setDragActive(true);
          return;
        }

        if (event.payload.type === "leave") {
          setDragActive(false);
          return;
        }

        if (event.payload.type === "drop") {
          setDragActive(false);
          applyPickedPaths(event.payload.paths).catch((nextError) =>
            setError(errorMessage(nextError))
          );
        }
      })
      .then((nextUnlisten) => {
        unlisten = nextUnlisten;
      })
      .catch(() => undefined);

    return () => {
      unlisten?.();
    };
  }, [manualPaths, selectedPaths]);

  async function refreshSnapshot() {
    const nextSnapshot = await invokeCommand<AppSnapshot>("get_app_snapshot");
    setSnapshot(nextSnapshot);
    setReceiveDir(nextSnapshot.receive_dir);
    setReceivePolicy(normalizeReceivePolicy(nextSnapshot.receive_policy));
  }

  async function refreshReceiveState() {
    const [
      status,
      session,
      report,
      pendingOffer,
      pairingRequest,
      nextTransferStatus,
      devices,
      trusted,
      discovery,
      nextTransfers
    ] = await Promise.all([
      invokeCommand<string | null>("get_receive_status"),
      invokeCommand<ReceiveSessionDto | null>("get_receive_session"),
      invokeCommand<ReceiveReportDto | null>("get_last_receive_report"),
      invokeCommand<PendingReceiveOfferDto | null>("get_pending_receive_offer"),
      invokeCommand<PendingPairingRequestDto | null>("get_pending_pairing_request"),
      invokeCommand<TransferStatusDto | null>("get_transfer_status"),
      invokeCommand<DeviceDto[]>("list_nearby_devices"),
      invokeCommand<TrustedDeviceDto[]>("list_trusted_devices"),
      invokeCommand<DiscoveryStatusDto>("get_discovery_status"),
      invokeCommand<TransferDto[]>("list_transfers")
    ]);
    setReceiveStatus(status);
    setReceiveSession(session);
    setReceiveReport(report);
    setPendingReceiveOffer(pendingOffer);
    setPendingPairingRequest(pairingRequest);
    setTransferStatus(nextTransferStatus);
    setNearbyDevices(devices);
    setTrustedDevices(trusted);
    setDiscoveryStatus(discovery);
    setTransfers(nextTransfers);
    if (pendingOffer || pairingRequest) setMode("receive");
  }

  async function refreshTransfers() {
    const nextTransfers = await invokeCommand<TransferDto[]>("list_transfers");
    setTransfers(nextTransfers);
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
    await scanPaths(mergedPaths, manualPaths);
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

  async function startReceive(options: { receiveDirOverride?: string; silent?: boolean } = {}) {
    const silent = options.silent ?? false;
    const requestedPort = parseReceivePort(bindPort);
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
      await refreshReceiveState();
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
      await refreshReceiveState();
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
      const actionLabel = transferRetryActionLabel(transfer);
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
      await refreshReceiveState();
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
      await refreshReceiveState();
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
      await invokeCommand<void>("forget_trusted_device", {
        deviceId: device.device_id
      });
      if (selectedDeviceId === device.device_id) {
        setSelectedDeviceId(null);
        setSelectedDeviceSnapshot(null);
      }
      setToast(`已移除：${device.device_name}`);
      await refreshReceiveState();
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

  const discoveryCopy = discoveryStateCopy(discoveryStatus, nearbyDevices.length);
  const targetLabel = selectedDevice
    ? selectedDevice.name
    : trimmedConnectionCode.length > 0
      ? "备用码"
      : "选择目标";
  const pageTitle =
    mode === "receive"
      ? "收件"
      : mode === "devices"
        ? "设备"
        : mode === "queue"
          ? "发送队列"
          : mode === "history"
            ? "传输历史"
            : selectedDevice
              ? `发给 ${selectedDevice.name}`
              : trimmedConnectionCode.length > 0
                ? "使用备用码发送"
                : "把文件发到哪台设备？";
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
  const pageSubtitle =
    mode === "receive"
      ? receiveState
      : mode === "devices"
        ? trustedDevices.length > 0 ? `${trustedDevices.length} 台可信设备` : "暂无可信设备"
        : mode === "history"
          ? transfers.length > 0 ? `${transfers.length} 条真实记录` : "暂无记录"
          : composerSubtitle;

  return (
    <main className="app-shell">
      <aside className="rail" aria-label="NekoDrop">
        <div className="rail-brand" title="NekoDrop">
          N
        </div>

        <nav className="rail-nav" aria-label="主导航">
          <button
            className={mode === "send" ? "rail-item is-active" : "rail-item"}
            onClick={() => setMode("send")}
            title="投递"
            type="button"
          >
            <Icon name="send" />
          </button>
          <button
            className={mode === "receive" ? "rail-item is-active" : "rail-item"}
            onClick={() => setMode("receive")}
            title="收件"
            type="button"
          >
            <Icon name="inbox" />
          </button>
          <button
            className={mode === "devices" ? "rail-item is-active" : "rail-item"}
            onClick={() => setMode("devices")}
            title="设备"
            type="button"
          >
            <Icon name="devices" />
          </button>
          <button
            className={mode === "queue" ? "rail-item is-active" : "rail-item"}
            onClick={() => setMode("queue")}
            title="队列"
            type="button"
          >
            <Icon name="list" />
          </button>
          <button
            className={mode === "history" ? "rail-item is-active" : "rail-item"}
            onClick={() => setMode("history")}
            title="传输"
            type="button"
          >
            <Icon name="clock" />
          </button>
        </nav>

        <button
          className="rail-item rail-bottom"
          disabled={busy === "open"}
          onClick={() => openPath(receiveSession?.receive_dir ?? receiveDir)}
          title="接收目录"
          type="button"
        >
          <Icon name="folder" />
        </button>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div className="title-tab">
            <strong>NekoDrop</strong>
            <span>{snapshot?.device_name ?? "这台电脑"}</span>
          </div>

          <div className="topbar-actions">
            <span className={discoveryCopy.isError ? "status-pill is-error" : "status-pill"}>
              {discoveryCopy.label}
            </span>
            {snapshot ? (
              <span className="device-pill" title={snapshot.device_identity.public_key_fingerprint}>
                {platformLabel(snapshot.device_identity.platform)}
              </span>
            ) : null}
            <button className={receiveSession ? "receive-pill is-on" : "receive-pill"} onClick={() => setMode("receive")} type="button">
              {receiveState}
            </button>
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
              {receiveSession ? "收件" : "打开"}
            </button>
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
          {mode === "send" ? (
            <div className="drop-home">
              <div className="brand-line">
                <span>N</span>
                <strong>NekoDrop</strong>
              </div>

              <div className="home-grid">
                <section className="home-primary">
                  <section className={dragActive ? "composer is-dragging" : "composer"}>
                    <div className="composer-main">
                      <strong>{composerTitle}</strong>
                      <span>{composerSubtitle}</span>
                    </div>
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
                    <div className="composer-actions">
                      <div className="composer-toolset">
                        <button className="tool-button" disabled={busy === "pick-files"} onClick={pickFiles} title="文件" type="button">
                          <Icon name="file" />
                        </button>
                        <button className="tool-button" disabled={busy === "pick-folders"} onClick={pickFolders} title="文件夹" type="button">
                          <Icon name="folder" />
                        </button>
                        <button
                          className={connectionCodeOpen ? "tool-button is-active" : "tool-button"}
                          onClick={() => {
                            setConnectionCodeOpen((open) => !open);
                            setSelectedDeviceId(null);
                            setSelectedDeviceSnapshot(null);
                          }}
                          title="备用码"
                          type="button"
                        >
                          <Icon name="link" />
                        </button>
                        <button className="tool-button" disabled={transferPaths.length === 0} onClick={clearQueue} title="清空" type="button">
                          <Icon name="trash" />
                        </button>
                      </div>
                      <div className="composer-submit">
                        <span>{targetLabel}</span>
                        <button
                          className="composer-send"
                          disabled={!canSend}
                          onClick={sendCurrentTransfer}
                          title={`发送到 ${targetLabel}`}
                          type="button"
                        >
                          <Icon name="arrow-up" />
                        </button>
                      </div>
                    </div>
                  </section>

                  <TargetStrip
                    busy={busy}
                    discoveryStatus={discoveryStatus}
                    devices={nearbyDevices}
                    selectedDeviceId={selectedDeviceId}
                    onSelectDevice={(device) => {
                      setSelectedDeviceId(device.id);
                      setSelectedDeviceSnapshot(device);
                      setConnectionCodeOpen(false);
                      setConnectionCode("");
                      setError(null);
                    }}
                    onTrustDevice={requestPairing}
                  />

                  {selectedPaths.length > 0 ? (
                    <QueuePreview
                      plan={plan}
                      scanStatus={scanStatus}
                      selectedPaths={selectedPaths}
                      onClearQueue={clearQueue}
                      onRemovePath={removePath}
                    />
                  ) : null}
                </section>

                <aside className="home-side">
                  {(transferStatus || sendReport || receiveReport || plan) ? (
                    <StatusLine
                      plan={plan}
                      receiveReport={receiveReport}
                      receiveSession={receiveSession}
                      sendReport={sendReport}
                      transferMetrics={transferMetrics}
                      transferStatus={transferStatus}
                      transferCount={transferPaths.length}
                      busy={busy}
                      onCancelTransfer={cancelCurrentTransfer}
                    />
                  ) : (
                    <HomeStateLine discoveryStatus={discoveryStatus} receiveState={receiveState} transfers={transfers} />
                  )}

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
                      setSelectedTransferId((current) => current === transfer.id ? null : transfer.id)
                    }
                  />
                </aside>
              </div>
            </div>
          ) : (
            <div className={mode === "history" ? "single-workbench is-wide" : "single-workbench"}>
              <div className="pane-head">
                <div>
                  <strong>{pageTitle}</strong>
                  <span>{pageSubtitle}</span>
                </div>
              </div>

              {mode === "receive" ? (
                <>
                  <TargetPanel
                    busy={busy}
                    connectionCode={connectionCode}
                    connectionCodeOpen={connectionCodeOpen}
                    discoveryStatus={discoveryStatus}
                    devices={nearbyDevices}
                    receiveSession={receiveSession}
                    receiveState={receiveState}
                    selectedDeviceId={selectedDeviceId}
                    setConnectionCode={(value) => {
                      setConnectionCode(value);
                      setSelectedDeviceId(null);
                      setSelectedDeviceSnapshot(null);
                    }}
                    setConnectionCodeOpen={setConnectionCodeOpen}
                    onCopyConnectionCode={copyConnectionCode}
                    onOpenReceiveDir={() => openPath(receiveSession?.receive_dir ?? receiveDir)}
                    onSelectDevice={(device) => {
                      setSelectedDeviceId(device.id);
                      setSelectedDeviceSnapshot(device);
                      setConnectionCodeOpen(false);
                      setConnectionCode("");
                      setError(null);
                    }}
                    onStartReceive={startReceive}
                    onStopReceive={stopReceive}
                    onTrustDevice={requestPairing}
                  />
                  <ReceivePanel
                    bindPort={bindPort}
                    busy={busy}
                    receiveDir={receiveDir}
                    receivePolicy={receivePolicy}
                    pendingOffer={pendingReceiveOffer}
                    pendingPairingRequest={pendingPairingRequest}
                    receiveReport={receiveReport}
                    receiveSession={receiveSession}
                    setBindPort={setBindPort}
                    setReceiveDir={setReceiveDir}
                    onChooseReceiveDir={chooseReceiveDir}
                    onCopyConnectionCode={copyConnectionCode}
                    onOpenPath={openPath}
                    onRespondReceiveOffer={respondReceiveOffer}
                    onRespondPairingRequest={respondPairingRequest}
                    onStartReceive={startReceive}
                    onStopReceive={stopReceive}
                    onUpdateReceivePolicy={updateReceivePolicy}
                  />
                </>
              ) : null}

              {mode === "devices" ? (
                <DevicePanel
                  busy={busy}
                  discoveryStatus={discoveryStatus}
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

              {mode === "queue" ? (
                <QueuePanel
                  busy={busy}
                  manualPaths={manualPaths}
                  plan={plan}
                  scanStatus={scanStatus}
                  selectedPaths={selectedPaths}
                  setManualPaths={(value) => {
                    setManualPaths(value);
                    setPlan(null);
                    setScanStatus(null);
                    setSendReport(null);
                  }}
                  onClearQueue={clearQueue}
                  onRemovePath={removePath}
                  onScan={() => scanPaths()}
                />
              ) : null}

              {mode === "history" ? (
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
                />
              ) : null}
            </div>
          )}
        </section>
      </section>
    </main>
  );
}

type IconName =
  | "arrow-up"
  | "clock"
  | "devices"
  | "file"
  | "folder"
  | "inbox"
  | "link"
  | "list"
  | "send"
  | "trash";

function Icon({ name }: { name: IconName }) {
  return (
    <svg aria-hidden="true" className="icon" fill="none" viewBox="0 0 24 24">
      {name === "arrow-up" ? <path d="M12 19V5m0 0 6 6M12 5l-6 6" /> : null}
      {name === "clock" ? <path d="M12 6v6l4 2m5-2a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z" /> : null}
      {name === "devices" ? <path d="M7 8a4 4 0 1 1 8 0 4 4 0 0 1-8 0Zm-3 13a7 7 0 0 1 14 0M17 11a3 3 0 0 1 0 6m3-8a6 6 0 0 1 0 10" /> : null}
      {name === "file" ? <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8l-5-5Zm0 0v5h5" /> : null}
      {name === "folder" ? <path d="M3 7a2 2 0 0 1 2-2h5l2 2h7a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Z" /> : null}
      {name === "inbox" ? <path d="M4 4h16v11l-3 5H7l-3-5V4Zm0 11h5l2 2h2l2-2h5" /> : null}
      {name === "link" ? <path d="M10 13a5 5 0 0 0 7.07 0l2-2A5 5 0 0 0 12 4l-1.2 1.2M14 11a5 5 0 0 0-7.07 0l-2 2A5 5 0 0 0 12 20l1.2-1.2" /> : null}
      {name === "list" ? <path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01" /> : null}
      {name === "send" ? <path d="m4 12 16-8-8 16-2-7-6-1Z" /> : null}
      {name === "trash" ? <path d="M4 7h16M9 7V4h6v3m-8 0 1 14h8l1-14" /> : null}
    </svg>
  );
}

function TargetStrip({
  busy,
  discoveryStatus,
  devices,
  selectedDeviceId,
  onSelectDevice,
  onTrustDevice
}: {
  busy: BusyMode | null;
  discoveryStatus: DiscoveryStatusDto | null;
  devices: DeviceDto[];
  selectedDeviceId: string | null;
  onSelectDevice: (device: DeviceDto) => void;
  onTrustDevice: (device: DeviceDto) => void;
}) {
  const discoveryCopy = discoveryStateCopy(discoveryStatus, devices.length);

  return (
    <section className="target-strip" aria-label="附近设备">
      {devices.length > 0 ? (
        devices.map((device) => {
          const trusted = device.trust_state === "Trusted";
          const selected = device.id === selectedDeviceId;
          return (
            <button
              className={[
                "target-chip",
                trusted ? "is-trusted" : "",
                selected ? "is-selected" : ""
              ]
                .filter(Boolean)
                .join(" ")}
              disabled={!trusted && busy === "pair"}
              key={device.id}
              onClick={() => {
                if (trusted) {
                  onSelectDevice(device);
                  return;
                }
                onTrustDevice(device);
              }}
              type="button"
            >
              <span className="device-dot" />
              <strong>{device.name}</strong>
              <small>{trusted ? (selected ? "已选" : "选择") : "配对"}</small>
            </button>
          );
        })
      ) : (
        <span className={discoveryCopy.isError ? "target-empty is-warning" : "target-empty"}>
          {discoveryCopy.targetLabel}
        </span>
      )}
    </section>
  );
}

function TargetPanel({
  busy,
  connectionCode,
  connectionCodeOpen,
  discoveryStatus,
  devices,
  receiveSession,
  receiveState,
  selectedDeviceId,
  setConnectionCode,
  setConnectionCodeOpen,
  onCopyConnectionCode,
  onOpenReceiveDir,
  onSelectDevice,
  onStartReceive,
  onStopReceive,
  onTrustDevice
}: {
  busy: BusyMode | null;
  connectionCode: string;
  connectionCodeOpen: boolean;
  discoveryStatus: DiscoveryStatusDto | null;
  devices: DeviceDto[];
  receiveSession: ReceiveSessionDto | null;
  receiveState: string;
  selectedDeviceId: string | null;
  setConnectionCode: (value: string) => void;
  setConnectionCodeOpen: (value: boolean) => void;
  onCopyConnectionCode: () => void;
  onOpenReceiveDir: () => void;
  onSelectDevice: (device: DeviceDto) => void;
  onStartReceive: () => void;
  onStopReceive: () => void;
  onTrustDevice: (device: DeviceDto) => void;
}) {
  return (
    <section className="target-panel">
      <NearbyDevices
        busy={busy}
        discoveryStatus={discoveryStatus}
        devices={devices}
        selectedDeviceId={selectedDeviceId}
        onSelectDevice={onSelectDevice}
        onTrustDevice={onTrustDevice}
      />

      <section className="target-block">
        <div className="block-head">
          <strong>备用码</strong>
          <button
            className={connectionCodeOpen ? "text-button is-active" : "text-button"}
            onClick={() => setConnectionCodeOpen(!connectionCodeOpen)}
            type="button"
          >
            {connectionCodeOpen ? "收起" : "使用"}
          </button>
        </div>
        {connectionCodeOpen ? (
          <textarea
            className="target-code"
            value={connectionCode}
            onChange={(event) => setConnectionCode(event.target.value)}
            aria-label="对方连接码或地址"
            placeholder="连接码或 IP:端口"
          />
        ) : null}
      </section>

      <section className="target-block">
        <div className="block-head">
          <strong>本机收件</strong>
          <span>{receiveState}</span>
        </div>
        <div className="receive-actions">
          {receiveSession ? (
            <>
              <button className="tool-button" onClick={onCopyConnectionCode} type="button">
                复制码
              </button>
              <button className="tool-button" onClick={onOpenReceiveDir} type="button">
                目录
              </button>
              <button className="danger-button" disabled={busy === "stop-receive" || busy === "receive"} onClick={onStopReceive} type="button">
                关闭
              </button>
            </>
          ) : (
            <button className="primary-button" disabled={busy === "receive"} onClick={onStartReceive} type="button">
              打开
            </button>
          )}
        </div>
      </section>
    </section>
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

function HomeStateLine({
  discoveryStatus,
  receiveState,
  transfers
}: {
  discoveryStatus: DiscoveryStatusDto | null;
  receiveState: string;
  transfers: TransferDto[];
}) {
  const discoveryCopy = discoveryStateCopy(discoveryStatus, discoveryStatus?.device_count ?? 0);
  const latest = transfers[0];

  return (
    <div className="home-state-line">
      <span>{receiveState}</span>
      <strong>{latest ? transferDirectionLabel(latest) : discoveryCopy.label}</strong>
    </div>
  );
}

function NearbyDevices({
  busy,
  discoveryStatus,
  devices,
  selectedDeviceId,
  onSelectDevice,
  onTrustDevice
}: {
  busy: BusyMode | null;
  discoveryStatus: DiscoveryStatusDto | null;
  devices: DeviceDto[];
  selectedDeviceId: string | null;
  onSelectDevice: (device: DeviceDto) => void;
  onTrustDevice: (device: DeviceDto) => void;
}) {
  const discoveryCopy = discoveryStateCopy(discoveryStatus, devices.length);

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
                    {devicePlatformLabel(device.platform)} · {trustStateLabel(device.trust_state)}
                    {device.pairing_code ? ` · ${device.pairing_code}` : ""}
                  </small>
                </span>
                <span className="device-actions">
                  {trusted ? (
                    <button className="target-button" onClick={() => onSelectDevice(device)} type="button">
                      {selected ? "已选" : "选择"}
                    </button>
                  ) : (
                    <button
                      className="trust-button"
                      disabled={busy === "pair" || !device.public_key_fingerprint}
                      onClick={() => onTrustDevice(device)}
                      type="button"
                    >
                      配对
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
  nearbyDevices: DeviceDto[];
  selectedDeviceId: string | null;
  trustedDevices: TrustedDeviceDto[];
  onForgetTrustedDevice: (device: TrustedDeviceDto) => void;
  onSelectNearbyDevice: (device: DeviceDto) => void;
  onSelectTrustedDevice: (device: TrustedDeviceDto) => void;
  onTrustDevice: (device: DeviceDto) => void;
}) {
  return (
    <section className="device-panel">
      <div className="device-overview">
        <div>
          <strong>{trustedDevices.length}</strong>
          <span>可信设备</span>
        </div>
        <div>
          <strong>{nearbyDevices.length}</strong>
          <span>附近在线</span>
        </div>
      </div>

      <section className="trusted-strip">
        <div className="section-head">
          <strong>已信任</strong>
          <span>{trustedDevices.length > 0 ? "可直接发送" : "未配对"}</span>
        </div>

        {trustedDevices.length > 0 ? (
          <div className="trusted-list">
            {trustedDevices.map((device) => {
              const online = nearbyDevices.some((nearby) => nearby.id === device.device_id);
              const selected = selectedDeviceId === device.device_id;
              return (
                <div
                  className={selected ? "trusted-device is-selected" : "trusted-device"}
                  key={device.device_id}
                >
                  <span className={online ? "device-dot is-online" : "device-dot"} />
                  <span className="trusted-main">
                    <strong>{device.device_name}</strong>
                    <small>
                      {devicePlatformLabel(device.platform)} · {shortDeviceId(device.device_id)}
                    </small>
                  </span>
                  <span className="trusted-meta">
                    {online ? "在线" : formatDeviceSeenTime(device.last_seen_at_ms)}
                  </span>
                  <span className="trusted-actions">
                    <button className="target-button" onClick={() => onSelectTrustedDevice(device)} type="button">
                      {selected ? "已选" : "选择"}
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

      <NearbyDevices
        busy={busy}
        discoveryStatus={discoveryStatus}
        devices={nearbyDevices}
        selectedDeviceId={selectedDeviceId}
        onSelectDevice={onSelectNearbyDevice}
        onTrustDevice={onTrustDevice}
      />
    </section>
  );
}

function ReceivePanel({
  bindPort,
  busy,
  receiveDir,
  receivePolicy,
  pendingOffer,
  pendingPairingRequest,
  receiveReport,
  receiveSession,
  setBindPort,
  setReceiveDir,
  onChooseReceiveDir,
  onCopyConnectionCode,
  onOpenPath,
  onRespondReceiveOffer,
  onRespondPairingRequest,
  onStartReceive,
  onStopReceive,
  onUpdateReceivePolicy
}: {
  bindPort: string;
  busy: BusyMode | null;
  receiveDir: string;
  receivePolicy: ReceivePolicyMode;
  pendingOffer: PendingReceiveOfferDto | null;
  pendingPairingRequest: PendingPairingRequestDto | null;
  receiveReport: ReceiveReportDto | null;
  receiveSession: ReceiveSessionDto | null;
  setBindPort: (value: string) => void;
  setReceiveDir: (value: string) => void;
  onChooseReceiveDir: () => void;
  onCopyConnectionCode: () => void;
  onOpenPath: (path: string) => void;
  onRespondReceiveOffer: (accept: boolean) => void;
  onRespondPairingRequest: (accept: boolean) => void;
  onStartReceive: () => void;
  onStopReceive: () => void;
  onUpdateReceivePolicy: (policy: ReceivePolicyMode) => void;
}) {
  const pendingOfferSender =
    pendingOffer?.sender_device_name?.trim() ||
    pendingOffer?.sender_device_id ||
    null;
  const pendingOfferPreview = pendingOffer ? pendingOfferFilePreview(pendingOffer.files) : null;
  const pendingOfferResumeSummary = pendingOffer
    ? pendingOfferResumeSummaryLabel(pendingOffer.resume_summary)
    : null;
  const receiveReportSender =
    receiveReport?.sender_device_name?.trim() ||
    receiveReport?.sender_device_id ||
    null;

  return (
    <section className="function-panel">
      <div className="panel-head">
        <div>
          <strong>{receiveSession ? "收件开启" : "收件关闭"}</strong>
          <span>{receiveSession?.bind_addr ?? "未监听"}</span>
        </div>
        <div className="panel-actions">
          {receiveSession ? (
            <button className="danger-button" disabled={busy === "stop-receive" || busy === "receive"} onClick={onStopReceive} type="button">
              关闭
            </button>
          ) : (
            <button className="primary-button" disabled={busy === "receive"} onClick={onStartReceive} type="button">
              打开
            </button>
          )}
        </div>
      </div>

      <div className="control-row">
        <label>
          接收目录
          <div className="input-action">
            <input value={receiveDir} onChange={(event) => setReceiveDir(event.target.value)} />
            <button className="tool-button" disabled={busy === "pick-receive" || Boolean(receiveSession)} onClick={onChooseReceiveDir} type="button">
              选择
            </button>
          </div>
        </label>
        <label className="port-field">
          端口
          <input value={bindPort} onChange={(event) => setBindPort(event.target.value)} />
        </label>
      </div>

      <div className="policy-row">
        <span>接收策略</span>
        <div className="policy-segment">
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
      </div>

      {receiveSession ? (
        <div className="code-line">
          <code>{receiveSession.connection_code}</code>
          <button className="tool-button" onClick={onCopyConnectionCode} type="button">
            复制
          </button>
          <button className="tool-button" onClick={() => onOpenPath(receiveSession.receive_dir)} type="button">
            打开目录
          </button>
        </div>
      ) : null}

      {pendingOffer ? (
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
      ) : null}

      {pendingPairingRequest ? (
        <div className="incoming-offer">
          <div className="offer-main">
            <strong>配对请求</strong>
            <span>
              {pendingPairingRequest.device_name} · {devicePlatformLabel(pendingPairingRequest.platform)} · 配对码{" "}
              {pendingPairingRequest.pairing_code}
            </span>
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
      ) : null}

      {receiveReport ? (
        <div className="result-line">
          <strong title={receiveReport.root_name}>
            {receiveReportSender ? `接收完成：来自 ${receiveReportSender}` : `接收完成：${receiveReport.root_name}`}
          </strong>
          <span>
            {receiveReport.files.length} 个 · {receiveReport.files.every((file) => file.verified) ? "已校验" : "检查"}
          </span>
        </div>
      ) : null}
    </section>
  );
}

function QueuePanel({
  busy,
  manualPaths,
  plan,
  scanStatus,
  selectedPaths,
  setManualPaths,
  onClearQueue,
  onRemovePath,
  onScan
}: {
  busy: BusyMode | null;
  manualPaths: string;
  plan: TransferPlanDto | null;
  scanStatus: TransferScanProgressDto | null;
  selectedPaths: string[];
  setManualPaths: (value: string) => void;
  onClearQueue: () => void;
  onRemovePath: (path: string) => void;
  onScan: () => void;
}) {
  return (
    <section className="function-panel">
      <div className="panel-head">
        <div>
          <strong>{plan ? `${plan.file_count} 个文件` : "发送队列"}</strong>
          <span>{plan ? formatBytes(plan.total_bytes) : "未扫描"}</span>
        </div>
        <div className="panel-actions">
          <button className="tool-button" disabled={busy === "scan"} onClick={onScan} type="button">
            扫描
          </button>
          <button className="tool-button" disabled={selectedPaths.length === 0 && !manualPaths.trim()} onClick={onClearQueue} type="button">
            清空
          </button>
        </div>
      </div>

      <TransferScanStatus status={scanStatus} />

      {selectedPaths.length > 0 ? (
        <div className="path-list">
          {selectedPaths.map((path) => (
            <div className="path-row" key={path}>
              <span>{path}</span>
              <button className="text-button" onClick={() => onRemovePath(path)} type="button">
                移除
              </button>
            </div>
          ))}
        </div>
      ) : null}

      <textarea
        className="manual-paths"
        value={manualPaths}
        onChange={(event) => setManualPaths(event.target.value)}
        placeholder="每行一个路径"
      />
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
  onSelectTransfer
}: {
  busy: BusyMode | null;
  selectedTransferId: string | null;
  transferMetrics: {
    speedBytesPerSecond: number | null;
    etaSeconds: number | null;
  };
  transferStatus: TransferStatusDto | null;
  transfers: TransferDto[];
  onCancelTransfer: () => void;
  onClearTransfers: () => void;
  onDeleteTransfer: (transfer: TransferDto) => void;
  onOpenTransfer: (transfer: TransferDto) => void;
  onResendTransfer: (transfer: TransferDto) => void;
  onSelectTransfer: (transfer: TransferDto) => void;
}) {
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

      {transferStatus && transferStatus.phase !== "completed" ? (
        <TransferStatusView
          busy={busy}
          metrics={transferMetrics}
          status={transferStatus}
          onCancel={onCancelTransfer}
        />
      ) : null}

      {transfers.length > 0 ? (
        <div className="history-list">
          {transfers.map((transfer) => {
            const selected = transfer.id === selectedTransferId;
            const paths = transfer.received_paths.length > 0 ? transfer.received_paths : transfer.source_paths;
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
                {selected ? (
                  <div className="history-detail">
                    <div className="history-paths">
                      {paths.slice(0, 6).map((path) => (
                        <span key={path} title={path}>{path}</span>
                      ))}
                      {paths.length > 6 ? <span>还有 {paths.length - 6} 个</span> : null}
                    </div>
                    <div className="history-actions">
                      <button className="text-button" disabled={busy === "open"} onClick={() => onOpenTransfer(transfer)} type="button">
                        打开
                      </button>
                      {transfer.direction === "send" ? (
                        <button className="text-button" disabled={busy === "resend"} onClick={() => onResendTransfer(transfer)} type="button">
                          {transferRetryActionLabel(transfer)}
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
      ) : (
        <div className="history-empty">暂无记录</div>
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
  onCancelTransfer
}: {
  busy: BusyMode | null;
  plan: TransferPlanDto | null;
  receiveReport: ReceiveReportDto | null;
  receiveSession: ReceiveSessionDto | null;
  sendReport: SendReportDto | null;
  transferMetrics: {
    speedBytesPerSecond: number | null;
    etaSeconds: number | null;
  };
  transferStatus: TransferStatusDto | null;
  transferCount: number;
  onCancelTransfer: () => void;
}) {
  if (transferStatus && transferStatus.phase !== "completed") {
    return (
      <TransferStatusView
        busy={busy}
        metrics={transferMetrics}
        status={transferStatus}
        onCancel={onCancelTransfer}
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
    return (
      <div className="status-line">
        <strong>接收完成</strong>
        <span>{receiveReport.files.length} 个文件 · {receiveReport.files.every((file) => file.verified) ? "已校验" : "检查"}</span>
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
  onSelectTransfer
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
                  {paths.slice(0, 3).map((path) => (
                    <span key={path} title={path}>{path}</span>
                  ))}
                  {paths.length > 3 ? <span>还有 {paths.length - 3} 个</span> : null}
                  <div className="recent-actions">
                    <button className="text-button" disabled={busy === "open"} onClick={() => onOpenTransfer(transfer)} type="button">
                      打开
                    </button>
                    {transfer.direction === "send" ? (
                      <button className="text-button" disabled={busy === "resend"} onClick={() => onResendTransfer(transfer)} type="button">
                        {transferRetryActionLabel(transfer)}
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

function TransferStatusView({
  busy,
  metrics,
  status,
  onCancel
}: {
  busy?: BusyMode | null;
  metrics?: {
    speedBytesPerSecond: number | null;
    etaSeconds: number | null;
  };
  status: TransferStatusDto;
  onCancel?: () => void;
}) {
  const canCancel =
    onCancel &&
    !matchesTerminalTransferPhase(status.phase) &&
    (status.direction === "send" ||
      (status.direction === "receive" && isReceiveTransferActivePhase(status.phase)));

  return (
    <div className={status.phase === "failed" ? "transfer-status is-error" : "transfer-status"}>
      <div className="transfer-status-head">
        <strong>{phaseLabel(status.phase)}</strong>
        {status.total_bytes > 0 ? (
          <span>
            {formatBytes(status.bytes_transferred)} / {formatBytes(status.total_bytes)}
          </span>
        ) : null}
        {canCancel ? (
          <button className="text-button" disabled={busy === "cancel-transfer"} onClick={onCancel} type="button">
            取消
          </button>
        ) : null}
      </div>
      {status.total_bytes > 0 ? (
        <div className="progress-track" aria-label="传输进度">
          <span style={{ width: `${Math.round(status.progress * 100)}%` }} />
        </div>
      ) : null}
      <div className="transfer-status-meta">
        <span>{status.message}</span>
        {metrics?.speedBytesPerSecond ? (
          <span>
            {formatBytes(metrics.speedBytesPerSecond)}/s
            {metrics.etaSeconds ? ` · 剩余 ${formatDuration(metrics.etaSeconds)}` : ""}
          </span>
        ) : null}
        {status.current_file ? <span>{status.current_file}</span> : null}
      </div>
    </div>
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

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

function normalizeReceivePolicy(value: string): ReceivePolicyMode {
  if (value === "auto_accept_trusted" || value === "block_all") return value;
  return "always_ask";
}

function parseReceivePort(value: string): number | null {
  const trimmed = value.trim();
  if (!/^\d+$/.test(trimmed)) return null;
  const port = Number(trimmed);
  if (!Number.isInteger(port) || port < 1 || port > 65535) return null;
  return port;
}

function portFromBindAddr(bindAddr: string): number | null {
  const port = bindAddr.trim().split(":").pop();
  return port ? parseReceivePort(port) : null;
}

function pendingOfferFilePreview(files: PendingReceiveOfferDto["files"]) {
  if (files.length === 0) return null;
  const preview = files.slice(0, 3).map((file) => file.manifest_path).join(" · ");
  const rest = files.length > 3 ? ` +${files.length - 3}` : "";
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

function receivePolicyLabel(value: ReceivePolicyMode) {
  if (value === "auto_accept_trusted") return "仅可信设备自动接收";
  if (value === "block_all") return "已阻止外部接收";
  return "接收前询问";
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

function transferRetryActionLabel(transfer: TransferDto) {
  if (isRecoverableSendTransfer(transfer)) return "继续发送";
  if (transfer.status === "failed" || transfer.status === "cancelled") return "重试";
  return "重发";
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

function transferRecoveryLabel(transfer: TransferDto) {
  if (!isRecoverableSendTransfer(transfer)) return null;
  return `已传 ${formatBytes(transfer.transferred_bytes)} / ${formatBytes(transfer.total_bytes)}`;
}

function phaseLabel(phase: string) {
  if (phase === "cancelled") return "已取消";
  if (phase === "connecting") return "连接中";
  if (phase === "listening") return "收件开启";
  if (phase === "awaiting_approval") return "等待确认";
  if (phase === "accepted") return "已接受";
  if (phase === "transferring") return "传输中";
  if (phase === "verifying") return "校验中";
  if (phase === "failed") return "传输失败";
  if (phase === "declined") return "已拒绝";
  if (phase === "expired") return "已超时";
  if (phase === "closed") return "收件关闭";
  if (phase === "completed") return "传输完成";
  return phase;
}

function matchesTerminalTransferPhase(phase: string) {
  return ["completed", "failed", "cancelled", "declined", "expired", "closed", "blocked"].includes(phase);
}

function isReceiveTransferActivePhase(phase: string) {
  return phase === "accepted" || phase === "transferring" || phase === "verifying";
}

function formatDuration(seconds: number) {
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  if (minutes < 60) return rest > 0 ? `${minutes}m ${rest}s` : `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  const minuteRest = minutes % 60;
  return minuteRest > 0 ? `${hours}h ${minuteRest}m` : `${hours}h`;
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

function formatDeviceSeenTime(timestampMs: number) {
  if (timestampMs <= 0) return "未见";
  const elapsedSeconds = Math.max(0, Math.floor((Date.now() - timestampMs) / 1000));
  if (elapsedSeconds < 60) return "刚刚";
  if (elapsedSeconds < 3600) return `${Math.floor(elapsedSeconds / 60)}m 前`;
  if (elapsedSeconds < 86400) return `${Math.floor(elapsedSeconds / 3600)}h 前`;
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false
  }).format(new Date(timestampMs));
}

function discoveryStateCopy(status: DiscoveryStatusDto | null, deviceCount: number) {
  if (!status) {
    return {
      label: "启动中",
      subtitle: "初始化",
      emptyTitle: "启动中",
      emptyBody: "正在准备自动发现",
      targetLabel: "启动中",
      isError: false
    };
  }

  if (status.phase === "unavailable") {
    return {
      label: "发现异常",
      subtitle: status.last_error ? "mDNS 异常" : "不可用",
      emptyTitle: "发现异常",
      emptyBody: "使用备用码，或重启应用后再试",
      targetLabel: "发现异常 · 备用码",
      isError: true
    };
  }

  if (!status.advertised) {
    const hasNetworkError = Boolean(status.last_error);
    return {
      label: hasNetworkError ? "广播异常" : "未广播",
      subtitle: hasNetworkError ? "检查网络" : "收件关闭",
      emptyTitle: hasNetworkError ? "广播异常" : "未广播",
      emptyBody: hasNetworkError ? "检查网络、VPN、代理或虚拟网卡" : "打开收件后会广播本机",
      targetLabel: hasNetworkError ? "广播异常 · 检查网络" : "未广播 · 打开收件",
      isError: hasNetworkError
    };
  }

  if (deviceCount > 0) {
    return {
      label: `${deviceCount} 台在线`,
      subtitle: status.last_seen_seconds_ago == null ? "在线" : `${status.last_seen_seconds_ago}s 前`,
      emptyTitle: "",
      emptyBody: "",
      targetLabel: `${deviceCount} 台在线`,
      isError: false
    };
  }

  return {
    label: "扫描中",
    subtitle: "搜索中",
    emptyTitle: "无设备",
    emptyBody: "确认同一网络、防火墙允许、VPN/代理关闭",
    targetLabel: "扫描中 · 同网段",
    isError: false
  };
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

function devicePlatformLabel(platform: string) {
  if (platform === "MacOS" || platform === "macos") return "macOS";
  if (platform === "Windows" || platform === "windows") return "Windows";
  if (platform === "Linux" || platform === "linux") return "Linux";
  return platform || "Unknown";
}

function trustStateLabel(trustState: string) {
  if (trustState === "Trusted") return "已信任";
  if (trustState === "Pairing") return "配对中";
  if (trustState === "Blocked") return "已阻止";
  if (trustState === "Local") return "本机";
  return "未配对";
}

function shortDeviceId(deviceId: string) {
  if (deviceId.length <= 22) return deviceId;
  return `${deviceId.slice(0, 17)}…${deviceId.slice(-4)}`;
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
