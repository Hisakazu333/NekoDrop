import React, { useState } from "react";
import { AppProvider, useAppContext } from "./context/AppContext";
import { Sidebar } from "./components/Sidebar";
import { SendView } from "./components/SendView";
import { DevicesView } from "./components/DevicesView";
import { HistoryView } from "./components/HistoryView";
import { SettingsView } from "./components/SettingsView";
import { TransferBanner } from "./components/TransferBanner";
import { InboxDrawer } from "./components/InboxDrawer";
import { isPendingInboxBundle } from "./bundleState";

/**
 * 应用骨架：两栏（文字导航 + 主区），全局传输横幅与收件箱抽屉
 * App shell: two panes with a global transfer banner and inbox drawer.
 */
function AppContent() {
  const { error, toast, mode, localBridgePendingActions, stagedBundles } = useAppContext();
  const [inboxOpen, setInboxOpen] = useState(false);

  const inboxCount =
    localBridgePendingActions.length + stagedBundles.filter(isPendingInboxBundle).length;

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
    <div className="app-shell">
      <Sidebar inboxCount={inboxCount} onToggleInbox={() => setInboxOpen(!inboxOpen)} />
      <main className="main-pane">
        <TransferBanner />
        {renderMain()}
      </main>

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
