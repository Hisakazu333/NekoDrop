import React, { useEffect, useState } from "react";
import { AppProvider, useAppContext } from "./context/AppContext";
import { Rail } from "./components/Rail";
import { HomeView } from "./components/HomeView";
import { DevicesView } from "./components/DevicesView";
import { HistoryView } from "./components/HistoryView";
import { SettingsView } from "./components/SettingsView";
import { TransferPanel } from "./components/TransferPanel";
import { InboxDrawer } from "./components/InboxDrawer";
import { isPendingInboxBundle } from "./bundleState";
import { shouldShowActiveTransferBar } from "./transferProgress";

/**
 * 应用外壳：窄导航栏 + 主区 + 常驻传输面板（右侧）。
 * 布局按文件传输任务组织：发送/接收是主页，传输状态永远可见。
 */
function AppContent() {
  const {
    error,
    toast,
    mode,
    setMode,
    localBridgePendingActions,
    stagedBundles,
    pendingReceiveOffer,
    pendingPairingRequest,
    transferStatus
  } = useAppContext();
  const [inboxOpen, setInboxOpen] = useState(false);
  const [panelOpen, setPanelOpen] = useState(true);

  const inboxCount =
    localBridgePendingActions.length + stagedBundles.filter(isPendingInboxBundle).length;

  // 面板收起时，FAB 用角标补上决策/传输盲区：warn=有待决策，live=传输进行中
  const hasPendingDecision = Boolean(pendingReceiveOffer || pendingPairingRequest);
  const hasLiveTransfer = Boolean(transferStatus && shouldShowActiveTransferBar(transferStatus));
  const fabLabel = hasPendingDecision
    ? "展开传输面板（有待处理的传输请求）"
    : hasLiveTransfer
      ? "展开传输面板（传输进行中）"
      : "展开传输面板";

  // ⌘1..4 切页 / ⌘N 新传输
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey)) return;
      const key = event.key.toLowerCase();
      const modes = ["send", "devices", "transfers", "settings"] as const;
      if (key === "n") {
        event.preventDefault();
        setMode("send");
      } else if (["1", "2", "3", "4"].includes(key)) {
        event.preventDefault();
        setMode(modes[Number(key) - 1]);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [setMode]);

  const renderMain = () => {
    switch (mode) {
      case "devices":
        return <DevicesView />;
      case "transfers":
        return <HistoryView />;
      case "settings":
        return <SettingsView />;
      default:
        return <HomeView />;
    }
  };

  return (
    <div className="app-column">
      <div className="app-body">
        <Rail inboxCount={inboxCount} onToggleInbox={() => setInboxOpen(!inboxOpen)} />
        <main className="main-pane">{renderMain()}</main>
        {panelOpen && (
          <aside className="transfer-panel">
            <TransferPanel onClose={() => setPanelOpen(false)} />
          </aside>
        )}
      </div>
      {!panelOpen && (
        <button
          aria-label={fabLabel}
          className="panel-fab"
          onClick={() => setPanelOpen(true)}
          title={fabLabel}
          type="button"
        >
          ⇄
          {hasPendingDecision && (
            <span aria-hidden="true" className="panel-fab-dot is-warn" />
          )}
          {hasLiveTransfer && (
            <span aria-hidden="true" className="panel-fab-dot is-live" />
          )}
        </button>
      )}

      <InboxDrawer isOpen={inboxOpen} onClose={() => setInboxOpen(false)} />

      {(error || toast) && (
        <div className="toast-overlay">
          {error && (
            <div className="toast-pill is-error" role="alert">
              {error}
            </div>
          )}
          {toast && <div className="toast-pill">{toast}</div>}
        </div>
      )}
    </div>
  );
}

/**
 * 客户端主入口组件 / Main desktop client entry component
 */
export function App() {
  return (
    <AppProvider>
      <AppContent />
    </AppProvider>
  );
}
