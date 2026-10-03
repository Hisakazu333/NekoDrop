import React, { useEffect, useState } from "react";
import { AppProvider, useAppContext } from "./context/AppContext";
import { Sidebar } from "./components/Sidebar";
import { SendView } from "./components/SendView";
import { DevicesView } from "./components/DevicesView";
import { HistoryView } from "./components/HistoryView";
import { SettingsView } from "./components/SettingsView";
import { TransferBanner } from "./components/TransferBanner";
import { InboxDrawer } from "./components/InboxDrawer";
import { Icon } from "./components/Icon";
import { isPendingInboxBundle } from "./bundleState";

const TABS = [
  { id: "send", icon: "paw", label: "发送" },
  { id: "devices", icon: "devices", label: "设备" },
  { id: "transfers", icon: "clock", label: "历史" },
  { id: "settings", icon: "settings", label: "设置" }
] as const;

/**
 * 顶部标签条（1:1 Notion 窗口签条）：红绿灯留白 + 页签 + [+]
 * Top tab strip with traffic-light inset, page tabs and a [+] action.
 */
function TabStrip() {
  const { mode, setMode } = useAppContext();
  return (
    <div className="tabstrip">
      <div className="tabstrip-tabs">
        {TABS.map((tab) => (
          <button
            key={tab.id}
            className={`tab ${mode === tab.id ? "is-active" : ""}`}
            onClick={() => setMode(tab.id)}
            type="button"
          >
            <Icon name={tab.icon} />
            <span>{tab.label}</span>
          </button>
        ))}
        <button aria-label="新传输" className="tab-new" onClick={() => setMode("send")} title="新传输" type="button">
          <Icon name="plus" />
        </button>
      </div>
    </div>
  );
}

/**
 * 应用骨架：标签条 + 两栏（Notion 式侧栏 + 主区），全局传输横幅与收件箱抽屉
 * App shell: tab strip, two panes, transfer banner and inbox drawer.
 */
function AppContent() {
  const { error, toast, mode, setMode, localBridgePendingActions, stagedBundles } = useAppContext();
  const [inboxOpen, setInboxOpen] = useState(false);

  const inboxCount =
    localBridgePendingActions.length + stagedBundles.filter(isPendingInboxBundle).length;

  // ⌘N / Ctrl+N：开一个新传输
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "n") {
        event.preventDefault();
        setMode("send");
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
        return <SendView />;
    }
  };

  return (
    <div className="app-column">
      <TabStrip />
      <div className="app-shell">
        <Sidebar inboxCount={inboxCount} onToggleInbox={() => setInboxOpen(!inboxOpen)} />
        <main className="main-pane">
          <TransferBanner />
          {renderMain()}
        </main>
      </div>

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
