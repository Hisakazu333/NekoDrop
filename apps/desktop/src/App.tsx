import { useEffect, useMemo, useRef, useState } from "react";
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
  TransferPlanDto,
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
  | "pair"
  | "open";

type ComposerMode = "send" | "receive" | "queue";

export function App() {
  const [snapshot, setSnapshot] = useState<AppSnapshot | null>(null);
  const [selectedPaths, setSelectedPaths] = useState<string[]>([]);
  const [manualPaths, setManualPaths] = useState("");
  const [connectionCode, setConnectionCode] = useState("");
  const [receiveDir, setReceiveDir] = useState("~/Downloads/NekoDrop");
  const [bindPort, setBindPort] = useState("45821");
  const [plan, setPlan] = useState<TransferPlanDto | null>(null);
  const [sendReport, setSendReport] = useState<SendReportDto | null>(null);
  const [nearbyDevices, setNearbyDevices] = useState<DeviceDto[]>([]);
  const [discoveryStatus, setDiscoveryStatus] = useState<DiscoveryStatusDto | null>(null);
  const [receiveSession, setReceiveSession] = useState<ReceiveSessionDto | null>(null);
  const [receiveStatus, setReceiveStatus] = useState<string | null>(null);
  const [receiveReport, setReceiveReport] = useState<ReceiveReportDto | null>(null);
  const [pendingReceiveOffer, setPendingReceiveOffer] = useState<PendingReceiveOfferDto | null>(null);
  const [pendingPairingRequest, setPendingPairingRequest] = useState<PendingPairingRequestDto | null>(null);
  const [transferStatus, setTransferStatus] = useState<TransferStatusDto | null>(null);
  const [selectedDeviceId, setSelectedDeviceId] = useState<string | null>(null);
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
      null,
    [selectedDeviceId, trustedNearbyDevices]
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
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(null), 2200);
    return () => window.clearTimeout(timer);
  }, [toast]);

  useEffect(() => {
    if (!selectedDeviceId) return;
    if (nearbyDevices.some((device) => device.id === selectedDeviceId)) return;
    setSelectedDeviceId(null);
  }, [nearbyDevices, selectedDeviceId]);

  useEffect(() => {
    if (mode !== "send") return;
    if (selectedDeviceId || connectionCodeOpen || trimmedConnectionCode.length > 0) return;
    if (trustedNearbyDevices.length !== 1) return;
    setSelectedDeviceId(trustedNearbyDevices[0].id);
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
  }

  async function refreshReceiveState() {
    const [status, session, report, pendingOffer, pairingRequest, nextTransferStatus, devices, discovery] = await Promise.all([
      invokeCommand<string | null>("get_receive_status"),
      invokeCommand<ReceiveSessionDto | null>("get_receive_session"),
      invokeCommand<ReceiveReportDto | null>("get_last_receive_report"),
      invokeCommand<PendingReceiveOfferDto | null>("get_pending_receive_offer"),
      invokeCommand<PendingPairingRequestDto | null>("get_pending_pairing_request"),
      invokeCommand<TransferStatusDto | null>("get_transfer_status"),
      invokeCommand<DeviceDto[]>("list_nearby_devices"),
      invokeCommand<DiscoveryStatusDto>("get_discovery_status")
    ]);
    setReceiveStatus(status);
    setReceiveSession(session);
    setReceiveReport(report);
    setPendingReceiveOffer(pendingOffer);
    setPendingPairingRequest(pairingRequest);
    setTransferStatus(nextTransferStatus);
    setNearbyDevices(devices);
    setDiscoveryStatus(discovery);
    if (pendingOffer || pairingRequest) setMode("receive");
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
      if (pickedDir) setReceiveDir(pickedDir);
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
    setSendReport(null);
    try {
      const nextPlan = await invokeCommand<TransferPlanDto>("create_transfer_plan", {
        paths: payload
      });
      setPlan(nextPlan);
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
    }
  }

  async function startReceive(options: { receiveDirOverride?: string; silent?: boolean } = {}) {
    const silent = options.silent ?? false;
    if (!silent) setBusy("receive");
    setError(null);
    setReceiveReport(null);
    if (!silent) setMode("receive");
    try {
      const session = await invokeCommand<ReceiveSessionDto>("start_receive_once", {
        bindHost: "0.0.0.0",
        port: Number(bindPort),
        receiveDir: options.receiveDirOverride ?? receiveDir
      });
      setReceiveSession(session);
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
    } catch (nextError) {
      setError(errorMessage(nextError));
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
    } catch (nextError) {
      setMode("send");
      setError(deviceSendErrorMessage(errorMessage(nextError)));
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

  async function requestPairing(device: DeviceDto) {
    setBusy("pair");
    setError(null);
    try {
      const trusted = await invokeCommand<TrustedDeviceDto>("request_device_pairing", {
        deviceId: device.id
      });
      setSelectedDeviceId(trusted.device_id);
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
    await navigator.clipboard.writeText(receiveSession.connection_code);
    setToast("连接码已复制");
  }

  function clearQueue() {
    setSelectedPaths([]);
    setManualPaths("");
    setPlan(null);
    setSendReport(null);
  }

  function removePath(path: string) {
    const nextPaths = selectedPaths.filter((item) => item !== path);
    setSelectedPaths(nextPaths);
    setPlan(null);
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
      : mode === "queue"
        ? "发送队列"
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
            className={mode === "queue" ? "rail-item is-active" : "rail-item"}
            onClick={() => setMode("queue")}
            title="队列"
            type="button"
          >
            <Icon name="list" />
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
                    }}
                    aria-label="对方连接码"
                    placeholder="连接码"
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
                  setConnectionCodeOpen(false);
                  setConnectionCode("");
                  setError(null);
                }}
                onTrustDevice={requestPairing}
              />

              {selectedPaths.length > 0 ? (
                <QueuePreview
                  plan={plan}
                  selectedPaths={selectedPaths}
                  onClearQueue={clearQueue}
                  onRemovePath={removePath}
                />
              ) : null}

              {(transferStatus || sendReport || receiveReport || plan) ? (
                <StatusLine
                  plan={plan}
                  receiveReport={receiveReport}
                  receiveSession={receiveSession}
                  sendReport={sendReport}
                  transferMetrics={transferMetrics}
                  transferStatus={transferStatus}
                  transferCount={transferPaths.length}
                />
              ) : null}

              <RecentActivity sendReport={sendReport} receiveReport={receiveReport} />
            </div>
          ) : (
            <div className="single-workbench">
              <div className="pane-head">
                <div>
                  <strong>{pageTitle}</strong>
                  <span>{mode === "receive" ? receiveState : composerSubtitle}</span>
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
                  }}
                  setConnectionCodeOpen={setConnectionCodeOpen}
                  onCopyConnectionCode={copyConnectionCode}
                  onOpenReceiveDir={() => openPath(receiveSession?.receive_dir ?? receiveDir)}
                  onSelectDevice={(device) => {
                    setSelectedDeviceId(device.id);
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
                  />
                </>
              ) : null}

              {mode === "queue" ? (
              <QueuePanel
                busy={busy}
                manualPaths={manualPaths}
                plan={plan}
                selectedPaths={selectedPaths}
                setManualPaths={(value) => {
                  setManualPaths(value);
                  setPlan(null);
                  setSendReport(null);
                }}
                onClearQueue={clearQueue}
                onRemovePath={removePath}
                onScan={() => scanPaths()}
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
        <span className="target-empty">{discoveryCopy.label}</span>
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
            aria-label="对方连接码"
            placeholder="粘贴连接码"
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
  selectedPaths,
  onClearQueue,
  onRemovePath
}: {
  plan: TransferPlanDto | null;
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

function ReceivePanel({
  bindPort,
  busy,
  receiveDir,
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
  onStopReceive
}: {
  bindPort: string;
  busy: BusyMode | null;
  receiveDir: string;
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
}) {
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
            <strong>传输请求</strong>
            <span>
              {pendingOffer.root_name} · {pendingOffer.file_count} 个文件 · {formatBytes(pendingOffer.total_bytes)}
            </span>
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
          <strong>接收完成：{receiveReport.files.length} 个文件</strong>
          <span>{receiveReport.files.every((file) => file.verified) ? "已校验" : "检查"}</span>
        </div>
      ) : null}
    </section>
  );
}

function QueuePanel({
  busy,
  manualPaths,
  plan,
  selectedPaths,
  setManualPaths,
  onClearQueue,
  onRemovePath,
  onScan
}: {
  busy: BusyMode | null;
  manualPaths: string;
  plan: TransferPlanDto | null;
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

function StatusLine({
  plan,
  receiveReport,
  receiveSession,
  sendReport,
  transferMetrics,
  transferStatus,
  transferCount
}: {
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
}) {
  if (transferStatus && transferStatus.phase !== "completed") {
    return <TransferStatusView metrics={transferMetrics} status={transferStatus} />;
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
  receiveReport,
  sendReport
}: {
  receiveReport: ReceiveReportDto | null;
  sendReport: SendReportDto | null;
}) {
  if (!sendReport && !receiveReport) return null;

  return (
    <section className="recent-block">
      <div className="section-head">
        <strong>最近</strong>
      </div>
      <div className="recent-list">
        {sendReport ? (
          <div className="recent-row">
            <span>已发送</span>
            <strong>{sendReport.root_name}</strong>
            <small>
              {sendReport.file_count} 个文件 · {formatBytes(sendReport.total_bytes)}
            </small>
          </div>
        ) : null}
        {receiveReport ? (
          <div className="recent-row">
            <span>已接收</span>
            <strong>{receiveReport.files.length} 个文件</strong>
            <small>{receiveReport.files.every((file) => file.verified) ? "已校验" : "检查"}</small>
          </div>
        ) : null}
      </div>
    </section>
  );
}

function TransferStatusView({
  metrics,
  status
}: {
  metrics?: {
    speedBytesPerSecond: number | null;
    etaSeconds: number | null;
  };
  status: TransferStatusDto;
}) {
  return (
    <div className={status.phase === "failed" ? "transfer-status is-error" : "transfer-status"}>
      <div className="transfer-status-head">
        <strong>{phaseLabel(status.phase)}</strong>
        {status.total_bytes > 0 ? (
          <span>
            {formatBytes(status.bytes_transferred)} / {formatBytes(status.total_bytes)}
          </span>
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

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

function phaseLabel(phase: string) {
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

function formatDuration(seconds: number) {
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  if (minutes < 60) return rest > 0 ? `${minutes}m ${rest}s` : `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  const minuteRest = minutes % 60;
  return minuteRest > 0 ? `${hours}h ${minuteRest}m` : `${hours}h`;
}

function discoveryStateCopy(status: DiscoveryStatusDto | null, deviceCount: number) {
  if (!status) {
    return {
      label: "启动中",
      subtitle: "初始化",
      emptyTitle: "启动中",
      emptyBody: "无设备",
      isError: false
    };
  }

  if (status.phase === "unavailable") {
    return {
      label: "发现不可用",
      subtitle: status.last_error ? "异常" : "不可用",
      emptyTitle: "发现不可用",
      emptyBody: "备用码",
      isError: true
    };
  }

  if (!status.advertised) {
    return {
      label: "未广播",
      subtitle: status.last_error ? "异常" : "未开启",
      emptyTitle: "未广播",
      emptyBody: "收件关闭",
      isError: Boolean(status.last_error)
    };
  }

  if (deviceCount > 0) {
    return {
      label: `${deviceCount} 台在线`,
      subtitle: status.last_seen_seconds_ago == null ? "在线" : `${status.last_seen_seconds_ago}s 前`,
      emptyTitle: "",
      emptyBody: "",
      isError: false
    };
  }

  return {
    label: "扫描中",
    subtitle: "搜索中",
    emptyTitle: "无设备",
    emptyBody: "扫描中",
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
