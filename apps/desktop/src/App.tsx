import { useEffect, useMemo, useState } from "react";

import { invokeCommand } from "./tauri";
import type { AppSnapshot, DeviceDto, PageId, TransferDto } from "./types";

const pages: Array<{ id: PageId; label: string; glyph: string }> = [
  { id: "home", label: "发送", glyph: "传" },
  { id: "devices", label: "设备", glyph: "设" },
  { id: "transfers", label: "记录", glyph: "录" },
  { id: "settings", label: "设置", glyph: "置" }
];

export function App() {
  const [activePage, setActivePage] = useState<PageId>("home");
  const [snapshot, setSnapshot] = useState<AppSnapshot | null>(null);
  const [devices, setDevices] = useState<DeviceDto[]>([]);
  const [transfers, setTransfers] = useState<TransferDto[]>([]);
  const [dragActive, setDragActive] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);

  async function refresh() {
    setLoadError(null);
    const [nextSnapshot, nextDevices, nextTransfers] = await Promise.all([
      invokeCommand<AppSnapshot>("get_app_snapshot"),
      invokeCommand<DeviceDto[]>("list_nearby_devices"),
      invokeCommand<TransferDto[]>("list_transfers")
    ]);
    setSnapshot(nextSnapshot);
    setDevices(nextDevices);
    setTransfers(nextTransfers);
  }

  useEffect(() => {
    refresh().catch((error) => {
      const message = error instanceof Error ? error.message : String(error);
      setLoadError(message);
      console.error(error);
    });
  }, []);

  const trustedDevices = useMemo(
    () => devices.filter((device) => device.trust_state === "Trusted"),
    [devices]
  );

  return (
    <main className="app-shell">
      <aside className="rail">
        <div className="brand-mark">N</div>
        <nav className="rail-nav" aria-label="主导航">
          {pages.map((page) => (
            <button
              className={`rail-item ${activePage === page.id ? "is-active" : ""}`}
              key={page.id}
              onClick={() => setActivePage(page.id)}
              type="button"
            >
              <span className="rail-glyph">{page.glyph}</span>
              <span>{page.label}</span>
            </button>
          ))}
        </nav>
        <div className="rail-status is-paused" title="发现服务未接入" />
      </aside>

      <section className="sidebar">
        <div className="sidebar-header">
          <p className="eyebrow">NekoDrop</p>
          <h1>本地文件互传</h1>
        </div>

        <div className="device-summary">
          <div>
            <span className="summary-label">本机设备</span>
            <strong>{snapshot?.device_name ?? "加载中"}</strong>
          </div>
          <span className="online-dot" />
        </div>

        <div className="sidebar-section">
          <div className="section-heading">
            <span>附近可信设备</span>
            <span>{trustedDevices.length}</span>
          </div>
          <div className="compact-list">
            {trustedDevices.length === 0 ? (
              <p className="empty-note">还没有可信设备。</p>
            ) : (
              trustedDevices.map((device) => (
                <button className="compact-device" key={device.id} type="button">
                  <span className="platform-badge">{platformInitial(device.platform)}</span>
                  <span>
                    <strong>{device.name}</strong>
                    <small>
                      {device.host}:{device.port}
                    </small>
                  </span>
                </button>
              ))
            )}
          </div>
        </div>

        <div className="sidebar-section">
          <div className="section-heading">
            <span>最近传输</span>
            <span>{transfers.length}</span>
          </div>
          <div className="compact-list">
            {transfers.length === 0 ? (
              <p className="empty-note">传输记录会显示在这里。</p>
            ) : (
              transfers.slice(0, 4).map((transfer) => (
                <div className="mini-transfer" key={transfer.id}>
                  <span>{formatBytes(transfer.total_bytes)}</span>
                  <small>{Math.round(transfer.progress * 100)}%</small>
                </div>
              ))
            )}
          </div>
        </div>
      </section>

      <section className="content">
        <header className="topbar">
          <div>
            <p className="eyebrow">{pageEyebrow(activePage)}</p>
            <h2>{pageTitle(activePage)}</h2>
          </div>
          <div className="topbar-actions">
            <span className="status-pill">桌面端</span>
            <span className="status-pill">发现未接入</span>
            <button className="ghost-button" onClick={refresh} type="button">
              刷新
            </button>
          </div>
        </header>

        {loadError ? (
          <div className="empty-state">
            <h3>桌面端状态读取失败</h3>
            <p>{loadError}</p>
          </div>
        ) : activePage === "home" ? (
          <HomePage
            dragActive={dragActive}
            setDragActive={setDragActive}
            trustedDevices={trustedDevices}
          />
        ) : activePage === "devices" ? (
          <DevicesPage devices={devices} />
        ) : activePage === "transfers" ? (
          <TransfersPage transfers={transfers} />
        ) : (
          <SettingsPage snapshot={snapshot} />
        )}
      </section>
    </main>
  );
}

function HomePage({
  dragActive,
  setDragActive,
  trustedDevices
}: {
  dragActive: boolean;
  setDragActive: (active: boolean) => void;
  trustedDevices: DeviceDto[];
}) {
  return (
    <div className="home-layout">
      <section
        className={`drop-zone ${dragActive ? "is-dragging" : ""}`}
        onDragEnter={(event) => {
          event.preventDefault();
          setDragActive(true);
        }}
        onDragOver={(event) => event.preventDefault()}
        onDragLeave={() => setDragActive(false)}
        onDrop={(event) => {
          event.preventDefault();
          setDragActive(false);
        }}
      >
        <div className="drop-icon">+</div>
        <h3>真实传输服务未接入</h3>
        <p>这里会接入系统文件选择、manifest 扫描和局域网发送服务。当前不会创建临时传输记录。</p>
        <div className="drop-actions">
          <button className="primary-button" disabled type="button">
            选择文件
          </button>
          <button className="secondary-button" disabled type="button">
            选择文件夹
          </button>
        </div>
      </section>

      <section className="panel">
        <div className="panel-header">
          <div>
            <p className="eyebrow">附近目标</p>
            <h3>可发送设备</h3>
          </div>
          <span className="count-chip">{trustedDevices.length}</span>
        </div>
        <div className="device-grid">
          {trustedDevices.length === 0 ? (
            <div className="inline-empty">
              <h4>未发现可信设备</h4>
              <p>需要在另一台电脑运行 NekoDrop，并接入真实局域网发现服务。</p>
            </div>
          ) : (
            trustedDevices.map((device) => (
              <article className="device-card" key={device.id}>
                <div className="platform-badge large">{platformInitial(device.platform)}</div>
                <div>
                  <h4>{device.name}</h4>
                  <p>{platformLabel(device.platform)} · 已信任</p>
                </div>
                <button className="secondary-button compact" disabled type="button">
                  发送
                </button>
              </article>
            ))
          )}
        </div>
      </section>
    </div>
  );
}

function DevicesPage({ devices }: { devices: DeviceDto[] }) {
  return (
    <div className="table-panel">
      <div className="panel-header">
        <div>
          <p className="eyebrow">设备发现</p>
          <h3>附近设备</h3>
        </div>
        <button className="secondary-button compact" disabled type="button">
          配对设备
        </button>
      </div>
      {devices.length === 0 ? (
        <div className="empty-state">
          <h3>未发现附近设备</h3>
          <p>设备发现服务未接入前，这里保持真实空状态。</p>
        </div>
      ) : (
        <div className="device-table">
          {devices.map((device) => (
            <div className="table-row" key={device.id}>
              <span className="platform-badge">{platformInitial(device.platform)}</span>
              <strong>{device.name}</strong>
              <span>{platformLabel(device.platform)}</span>
              <span>
                {device.host}:{device.port}
              </span>
              <span className={`trust-state ${device.trust_state.toLowerCase()}`}>
                {trustStateLabel(device.trust_state)}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function TransfersPage({ transfers }: { transfers: TransferDto[] }) {
  return (
    <div className="table-panel">
      <div className="panel-header">
        <div>
          <p className="eyebrow">传输历史</p>
          <h3>传输记录</h3>
        </div>
        <span className="count-chip">{transfers.length}</span>
      </div>
      {transfers.length === 0 ? (
        <div className="empty-state">
          <h3>还没有传输记录</h3>
          <p>传输服务接入前不会生成假记录。</p>
        </div>
      ) : (
        <div className="transfer-list">
          {transfers.map((transfer) => (
            <article className="transfer-card" key={transfer.id}>
              <div>
                <h4>{directionLabel(transfer.direction)}文件包</h4>
                <p>
                  {transfer.file_count} 个文件 · {formatBytes(transfer.total_bytes)}
                </p>
              </div>
              <div className="progress-wrap">
                <span>{Math.round(transfer.progress * 100)}%</span>
                <div className="progress-track">
                  <div
                    className="progress-fill"
                    style={{ width: `${Math.round(transfer.progress * 100)}%` }}
                  />
                </div>
              </div>
              <span className="status-pill">{transferStatusLabel(transfer.status)}</span>
            </article>
          ))}
        </div>
      )}
    </div>
  );
}

function SettingsPage({ snapshot }: { snapshot: AppSnapshot | null }) {
  return (
    <div className="settings-grid">
      <section className="settings-section">
        <p className="eyebrow">本机身份</p>
        <label>
          设备名称
          <input readOnly value={snapshot?.device_name ?? ""} />
        </label>
        <label>
          默认接收目录
          <input readOnly value={snapshot?.receive_dir ?? ""} />
        </label>
      </section>
      <section className="settings-section">
        <p className="eyebrow">运行方式</p>
        <div className="setting-row">
          <span>局域网发现</span>
          <strong>{snapshot?.discovery_enabled ? "开启" : "关闭"}</strong>
        </div>
        <div className="setting-row">
          <span>托盘常驻</span>
          <strong>{snapshot?.tray_enabled ? "开启" : "关闭"}</strong>
        </div>
        <div className="setting-row">
          <span>接收策略</span>
          <strong>每次询问</strong>
        </div>
      </section>
    </div>
  );
}

function pageTitle(page: PageId) {
  if (page === "home") return "发送文件";
  if (page === "devices") return "设备";
  if (page === "transfers") return "传输记录";
  return "设置";
}

function pageEyebrow(page: PageId) {
  if (page === "home") return "最快路径";
  if (page === "devices") return "配对与信任";
  if (page === "transfers") return "进度与校验";
  return "本机偏好";
}

function platformInitial(platform: string) {
  if (platform.toLowerCase().includes("windows")) return "W";
  if (platform.toLowerCase().includes("mac")) return "M";
  return "D";
}

function platformLabel(platform: string) {
  if (platform.toLowerCase().includes("windows")) return "Windows";
  if (platform.toLowerCase().includes("mac")) return "macOS";
  if (platform.toLowerCase().includes("linux")) return "Linux";
  return "未知平台";
}

function trustStateLabel(state: string) {
  if (state === "Local") return "本机";
  if (state === "Trusted") return "已信任";
  if (state === "Pairing") return "配对中";
  if (state === "Blocked") return "已阻止";
  return "未信任";
}

function directionLabel(direction: string) {
  if (direction === "Receive") return "接收";
  return "发送";
}

function transferStatusLabel(status: string) {
  if (status === "Draft") return "草稿";
  if (status === "Offered") return "已发起";
  if (status === "AwaitingApproval") return "等待确认";
  if (status === "Transferring") return "传输中";
  if (status === "Paused") return "已暂停";
  if (status === "Verifying") return "校验中";
  if (status === "Completed") return "已完成";
  if (status === "Failed") return "失败";
  if (status === "Cancelled") return "已取消";
  return status;
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
}
