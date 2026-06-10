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
  const canSend = connectionCode.trim().length > 0 && transferPaths.length > 0 && !busy;
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
  const stageCopy = useMemo(() => {
    if (mode === "receive") {
      return receiveSession
        ? {
            eyebrow: "收件",
            title: pendingPairingRequest
              ? "配对请求"
              : pendingReceiveOffer
                ? "传输请求"
                : "收件开启"
          }
        : {
            eyebrow: "收件",
            title: "收件关闭"
          };
    }

    if (mode === "queue") {
      return plan
        ? {
            eyebrow: "队列",
            title: `${plan.file_count} 个文件，${formatBytes(plan.total_bytes)}`
          }
        : {
            eyebrow: "队列",
            title: transferPaths.length > 0 ? "待扫描" : "空队列"
          };
    }

    return {
      eyebrow: "投递",
      title: "选择文件"
    };
  }, [mode, pendingPairingRequest, pendingReceiveOffer, plan, receiveSession, transferPaths.length]);

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
    setMode("queue");
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
      setMode("queue");
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
      setMode("queue");
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

  async function requestPairing(device: DeviceDto) {
    setBusy("pair");
    setError(null);
    try {
      const trusted = await invokeCommand<TrustedDeviceDto>("request_device_pairing", {
        deviceId: device.id
      });
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

  return (
    <main className="app-shell">
      <aside className="sidebar" aria-label="NekoDrop">
        <div className="sidebar-brand">
          <strong>NekoDrop</strong>
          <span>桌面互传</span>
        </div>

        <nav className="nav-list" aria-label="主导航">
          <span className="nav-label">功能</span>
          <button className={mode === "send" ? "nav-item is-active" : "nav-item"} onClick={() => setMode("send")} type="button">
            <span>01</span>
            新投递
          </button>
          <button className={mode === "receive" ? "nav-item is-active" : "nav-item"} onClick={() => setMode("receive")} type="button">
            <span>02</span>
            收件
          </button>
          <button className={mode === "queue" ? "nav-item is-active" : "nav-item"} onClick={() => setMode("queue")} type="button">
            <span>03</span>
            队列
          </button>
          <span className="nav-item is-roadmap is-ready">
            <span>04</span>
            <strong>设备身份</strong>
            <small>已接入</small>
          </span>
          <span className="nav-item is-roadmap is-ready">
            <span>05</span>
            <strong>本机信任</strong>
            <small>已接入</small>
          </span>
          <span className="nav-label">计划</span>
          <span className="nav-item is-roadmap is-ready">
            <span>06</span>
            <strong>配对握手</strong>
            <small>已接入</small>
          </span>
          <span className="nav-item is-roadmap">
            <span>07</span>
            <strong>历史</strong>
            <small>待接入</small>
          </span>
          <span className="nav-item is-roadmap">
            <span>08</span>
            <strong>OpenNeko 支撑</strong>
            <small>待接入</small>
          </span>
        </nav>

        <span className="nav-item is-roadmap sidebar-settings">
          <span>09</span>
          <strong>设置</strong>
          <small>待接入</small>
        </span>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div className="breadcrumb">
            <strong>设备投递</strong>
            <span>{snapshot?.device_name ?? "这台电脑"}</span>
          </div>

          {snapshot ? (
            <div className="identity-strip" title={snapshot.device_identity.public_key_fingerprint}>
              <span>{platformLabel(snapshot.device_identity.platform)}</span>
              <code>{shortDeviceId(snapshot.device_identity.device_id)}</code>
            </div>
          ) : null}

          <div className="topbar-actions">
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
              {receiveSession ? "连接码" : "打开收件"}
            </button>
            <button className="ghost-pill" onClick={() => openPath(receiveSession?.receive_dir ?? receiveDir)} type="button">
              接收目录
            </button>
          </div>
        </header>

        {error ? (
          <section className="notice is-error">
            <strong>操作失败</strong>
            <span>{error}</span>
          </section>
        ) : null}

        {toast ? (
          <section className="notice is-info">
            <strong>已处理</strong>
            <span>{toast}</span>
          </section>
        ) : null}

        <section className="stage">
          <div className="hero-copy">
            <p>{stageCopy.eyebrow}</p>
            <h1>{stageCopy.title}</h1>
          </div>

          <section className={dragActive ? "composer file-composer is-dragging" : "composer file-composer"}>
            <div className="composer-header">
              <div>
                <strong>发送内容</strong>
                <span>{plan ? `${plan.file_count} 个文件` : `${transferPaths.length} 个路径`}</span>
              </div>
              <span>{transferPaths.length} 个路径</span>
            </div>
            <div className="drop-target">
              <strong>{transferPaths.length > 0 ? `${transferPaths.length} 个路径已加入` : "拖入文件"}</strong>
              <span>
                {plan
                  ? `${plan.file_count} 个文件 · ${formatBytes(plan.total_bytes)}`
                  : transferPaths.length > 0
                    ? transferPaths[0]
                    : "拖拽到此处"}
              </span>
            </div>
            <div className="composer-bottom">
              <div className="composer-tools">
                <button className="tool-button" disabled={busy === "pick-files"} onClick={pickFiles} type="button">
                  + 文件
                </button>
                <button className="tool-button" disabled={busy === "pick-folders"} onClick={pickFolders} type="button">
                  文件夹
                </button>
                <button className="tool-button" disabled={transferPaths.length === 0 || busy === "scan"} onClick={() => scanPaths()} type="button">
                  扫描
                </button>
                <button className="tool-button" onClick={() => setMode("queue")} type="button">
                  队列 {transferPaths.length}
                </button>
              </div>
              <span className="composer-hint">{nearbyDevices.length > 0 ? "设备在线" : "扫描中"}</span>
            </div>
          </section>

          <NearbyDevices
            busy={busy}
            discoveryStatus={discoveryStatus}
            devices={nearbyDevices}
            transferCount={transferPaths.length}
            onSendToDevice={sendFilesToDevice}
            onTrustDevice={requestPairing}
          />

          <section className="fallback-strip">
            <div className="fallback-copy">
              <strong>备用码</strong>
              <span>手动投递</span>
            </div>
            <textarea
              className="fallback-code"
              value={connectionCode}
              onChange={(event) => {
                setConnectionCode(event.target.value);
                setMode("send");
              }}
              aria-label="对方连接码"
              placeholder="连接码"
            />
            <button className="send-button" disabled={!canSend} onClick={sendFiles} type="button">
              发送
            </button>
          </section>

          <StatusLine
            plan={plan}
            receiveReport={receiveReport}
            receiveSession={receiveSession}
            sendReport={sendReport}
            transferMetrics={transferMetrics}
            transferStatus={transferStatus}
            transferCount={transferPaths.length}
          />

          {mode === "receive" ? (
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
        </section>
      </section>
    </main>
  );
}

function NearbyDevices({
  busy,
  discoveryStatus,
  devices,
  transferCount,
  onSendToDevice,
  onTrustDevice
}: {
  busy: BusyMode | null;
  discoveryStatus: DiscoveryStatusDto | null;
  devices: DeviceDto[];
  transferCount: number;
  onSendToDevice: (device: DeviceDto) => void;
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
            return (
              <div className={trusted ? "nearby-device is-trusted" : "nearby-device"} key={device.id}>
                <span className="device-dot" />
                <span className="device-main">
                  <strong>{device.name}</strong>
                  <small>
                    {devicePlatformLabel(device.platform)} · {trustStateLabel(device.trust_state)}
                    {device.pairing_code ? ` · 配对码 ${device.pairing_code}` : ""} · {device.host}:{device.port}
                  </small>
                </span>
                <span className="device-actions">
                  {trusted ? (
                    <span className="trust-badge">已信任</span>
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
                  <button
                    className="inline-send-button"
                    disabled={busy === "send" || transferCount === 0}
                    onClick={() => onSendToDevice(device)}
                    type="button"
                  >
                    {transferCount > 0 ? "发送" : "先选文件"}
                  </button>
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

  if (receiveSession) {
    return (
      <div className="status-line">
        <strong>收件中</strong>
        <span>{receiveSession.bind_addr}</span>
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
