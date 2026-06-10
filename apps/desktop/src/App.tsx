import { useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";

import { invokeCommand } from "./tauri";
import type {
  AppSnapshot,
  PendingReceiveOfferDto,
  ReceiveReportDto,
  ReceiveSessionDto,
  SendReportDto,
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
  const [receiveSession, setReceiveSession] = useState<ReceiveSessionDto | null>(null);
  const [receiveStatus, setReceiveStatus] = useState<string | null>(null);
  const [receiveReport, setReceiveReport] = useState<ReceiveReportDto | null>(null);
  const [pendingReceiveOffer, setPendingReceiveOffer] = useState<PendingReceiveOfferDto | null>(null);
  const [transferStatus, setTransferStatus] = useState<TransferStatusDto | null>(null);
  const [mode, setMode] = useState<ComposerMode>("send");
  const [dragActive, setDragActive] = useState(false);
  const [busy, setBusy] = useState<BusyMode | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const previousTransferStatus = useRef<TransferStatusDto | null>(null);
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
    ? pendingReceiveOffer
      ? "等待确认"
      : "收件已打开"
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
    const [status, session, report, pendingOffer, nextTransferStatus] = await Promise.all([
      invokeCommand<string | null>("get_receive_status"),
      invokeCommand<ReceiveSessionDto | null>("get_receive_session"),
      invokeCommand<ReceiveReportDto | null>("get_last_receive_report"),
      invokeCommand<PendingReceiveOfferDto | null>("get_pending_receive_offer"),
      invokeCommand<TransferStatusDto | null>("get_transfer_status")
    ]);
    setReceiveStatus(status);
    setReceiveSession(session);
    setReceiveReport(report);
    setPendingReceiveOffer(pendingOffer);
    setTransferStatus(nextTransferStatus);
    if (pendingOffer) setMode("receive");
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

  async function startReceive() {
    setBusy("receive");
    setError(null);
    setReceiveReport(null);
    setMode("receive");
    try {
      const session = await invokeCommand<ReceiveSessionDto>("start_receive_once", {
        bindHost: "0.0.0.0",
        port: Number(bindPort),
        receiveDir
      });
      setReceiveSession(session);
      setReceiveStatus("等待接收中");
      setToast("收件已打开");
    } catch (nextError) {
      setError(errorMessage(nextError));
    } finally {
      setBusy(null);
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
      setError("请先选择或拖入文件。");
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
          <span className="nav-label">当前可用</span>
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
          <span className="nav-label">后续迭代</span>
          <span className="nav-item is-roadmap">
            <span>04</span>
            <strong>设备配对</strong>
            <small>待接入</small>
          </span>
          <span className="nav-item is-roadmap">
            <span>05</span>
            <strong>历史</strong>
            <small>待接入</small>
          </span>
          <span className="nav-item is-roadmap">
            <span>06</span>
            <strong>OpenNeko 支撑</strong>
            <small>待接入</small>
          </span>
        </nav>

        <span className="nav-item is-roadmap sidebar-settings">
          <span>07</span>
          <strong>设置</strong>
          <small>待接入</small>
        </span>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div className="breadcrumb">
            <strong>连接码投递</strong>
            <span>{snapshot?.device_name ?? "这台电脑"}</span>
          </div>

          {snapshot ? (
            <div className="identity-strip" title={snapshot.device_identity.public_key_fingerprint}>
              <span>{platformLabel(snapshot.device_identity.platform)}</span>
              <code>{shortDeviceId(snapshot.device_identity.device_id)}</code>
              <span>{shortFingerprint(snapshot.device_identity.public_key_fingerprint)}</span>
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
              {receiveSession ? "查看连接码" : "打开收件"}
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
            <p>连接码投递</p>
            <h1>选择文件，粘贴连接码，发送</h1>
          </div>

          <section className={dragActive ? "composer is-dragging" : "composer"}>
            <div className="composer-header">
              <div>
                <strong>对方连接码</strong>
                <span>连接码以 nekodrop-v1 开头</span>
              </div>
              <span>{transferPaths.length} 个路径</span>
            </div>
            <textarea
              value={connectionCode}
              onChange={(event) => {
                setConnectionCode(event.target.value);
                setMode("send");
              }}
              aria-label="对方连接码"
              placeholder="粘贴对方连接码"
            />
            <div className="composer-bottom">
              <div className="composer-tools">
                <button className="tool-button" disabled={busy === "pick-files"} onClick={pickFiles} type="button">
                  + 文件
                </button>
                <button className="tool-button" disabled={busy === "pick-folders"} onClick={pickFolders} type="button">
                  文件夹
                </button>
                <button className="tool-button" onClick={() => setMode("receive")} type="button">
                  收件
                </button>
                <button className="tool-button" onClick={() => setMode("queue")} type="button">
                  队列 {transferPaths.length}
                </button>
              </div>
              <button className="send-button" disabled={!canSend} onClick={sendFiles} type="button">
                发送
              </button>
            </div>
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
              receiveReport={receiveReport}
              receiveSession={receiveSession}
              setBindPort={setBindPort}
              setReceiveDir={setReceiveDir}
              onChooseReceiveDir={chooseReceiveDir}
              onCopyConnectionCode={copyConnectionCode}
              onOpenPath={openPath}
              onRespondReceiveOffer={respondReceiveOffer}
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

function ReceivePanel({
  bindPort,
  busy,
  receiveDir,
  pendingOffer,
  receiveReport,
  receiveSession,
  setBindPort,
  setReceiveDir,
  onChooseReceiveDir,
  onCopyConnectionCode,
  onOpenPath,
  onRespondReceiveOffer,
  onStartReceive,
  onStopReceive
}: {
  bindPort: string;
  busy: BusyMode | null;
  receiveDir: string;
  pendingOffer: PendingReceiveOfferDto | null;
  receiveReport: ReceiveReportDto | null;
  receiveSession: ReceiveSessionDto | null;
  setBindPort: (value: string) => void;
  setReceiveDir: (value: string) => void;
  onChooseReceiveDir: () => void;
  onCopyConnectionCode: () => void;
  onOpenPath: (path: string) => void;
  onRespondReceiveOffer: (accept: boolean) => void;
  onStartReceive: () => void;
  onStopReceive: () => void;
}) {
  return (
    <section className="function-panel">
      <div className="panel-head">
        <div>
          <strong>{receiveSession ? "这台电脑正在收件" : "打开这台电脑收件"}</strong>
          <span>{receiveSession?.bind_addr ?? "生成连接码后给另一台电脑"}</span>
        </div>
        <div className="panel-actions">
          {receiveSession ? (
            <button className="danger-button" disabled={busy === "stop-receive" || busy === "receive"} onClick={onStopReceive} type="button">
              关闭收件
            </button>
          ) : (
            <button className="primary-button" disabled={busy === "receive"} onClick={onStartReceive} type="button">
              打开收件
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
            <strong>收到传输请求</strong>
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

      {receiveReport ? (
        <div className="result-line">
          <strong>接收完成：{receiveReport.files.length} 个文件</strong>
          <span>{receiveReport.files.every((file) => file.verified) ? "校验通过" : "需要检查"}</span>
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
          <span>{plan ? formatBytes(plan.total_bytes) : "文件会经过真实扫描和 SHA-256 校验"}</span>
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
        placeholder="手动补充路径，每行一个..."
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
        <span>{receiveReport.files.length} 个文件 · {receiveReport.files.every((file) => file.verified) ? "校验通过" : "需要检查"}</span>
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
  if (phase === "listening") return "收件已打开";
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

function shortDeviceId(deviceId: string) {
  if (deviceId.length <= 22) return deviceId;
  return `${deviceId.slice(0, 17)}…${deviceId.slice(-4)}`;
}

function shortFingerprint(fingerprint: string) {
  const value = fingerprint.replace(/^sha256:/, "");
  if (value.length <= 16) return `sha256:${value}`;
  return `sha256:${value.slice(0, 8)}…${value.slice(-6)}`;
}

function errorMessage(error: unknown) {
  if (error instanceof Error) return error.message;
  return String(error);
}
