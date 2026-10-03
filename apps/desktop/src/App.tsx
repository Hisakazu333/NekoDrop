import React, { useEffect, useRef, useState } from "react";
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

type Mode = "send" | "devices" | "transfers" | "settings";

const TABS: Array<{ id: Mode; icon: Parameters<typeof Icon>[0]["name"]; label: string }> = [
  { id: "send", icon: "paw", label: "发送" },
  { id: "devices", icon: "devices", label: "设备" },
  { id: "transfers", icon: "clock", label: "历史" },
  { id: "settings", icon: "settings", label: "设置" }
];

interface TabStripProps {
  collapsed: boolean;
  onToggleCollapse: () => void;
  onBack: () => void;
  onForward: () => void;
  canBack: boolean;
  canForward: boolean;
}

/**
 * 顶部标签条（1:1 Notion 窗口签条）：红绿灯留白 + 工作区页签
 * + 侧栏开合/前后导航 + 页签 + [+]
 */
function TabStrip({ collapsed, onToggleCollapse, onBack, onForward, canBack, canForward }: TabStripProps) {
  const { mode, setMode } = useAppContext();
  return (
    <div className="tabstrip">
      <button
        className="tab tab-workspace"
        onClick={() => setMode("send")}
        title="NekoDrop 工作区"
        type="button"
      >
        <Icon name="paw" />
        <span>NekoDrop</span>
      </button>
      <div className="tabstrip-nav">
        <button
          aria-label="收起或展开侧栏"
          className={`strip-icon ${collapsed ? "is-active" : ""}`}
          onClick={onToggleCollapse}
          title="收起/展开侧栏"
          type="button"
        >
          <Icon name="panel" />
        </button>
        <button
          aria-label="上一页"
          className="strip-icon"
          disabled={!canBack}
          onClick={onBack}
          title="上一页"
          type="button"
        >
          <Icon name="chevron-left" />
        </button>
        <button
          aria-label="下一页"
          className="strip-icon"
          disabled={!canForward}
          onClick={onForward}
          title="下一页"
          type="button"
        >
          <Icon name="chevron-right" />
        </button>
      </div>
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
  const {
    error,
    toast,
    mode,
    setMode,
    localBridgePendingActions,
    stagedBundles,
    clearQueue,
    clearSendQueue,
    setConnectionCode,
    setConnectionCodeOpen
  } = useAppContext();
  const [inboxOpen, setInboxOpen] = useState(false);
  const [collapsed, setCollapsed] = useState(false);

  // 页面历史（供 < > 翻页）/ mode history for back/forward navigation
  const historyRef = useRef<Mode[]>(["send"]);
  const pointerRef = useRef(0);
  const [navState, setNavState] = useState({ back: false, forward: false });

  useEffect(() => {
    const current = historyRef.current[pointerRef.current];
    if (current === mode) return;
    historyRef.current = historyRef.current.slice(0, pointerRef.current + 1);
    historyRef.current.push(mode as Mode);
    pointerRef.current = historyRef.current.length - 1;
    setNavState({
      back: pointerRef.current > 0,
      forward: false
    });
  }, [mode]);

  const go = (delta: number) => {
    const next = pointerRef.current + delta;
    if (next < 0 || next >= historyRef.current.length) return;
    pointerRef.current = next;
    setMode(historyRef.current[next]);
    setNavState({ back: next > 0, forward: next < historyRef.current.length - 1 });
  };

  const inboxCount =
    localBridgePendingActions.length + stagedBundles.filter(isPendingInboxBundle).length;

  // ⌘N / Ctrl+N：新传输 = 清空文件队列与待发队列、复位连接码、回到发送页
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "n") {
        event.preventDefault();
        clearQueue();
        clearSendQueue();
        setConnectionCode("");
        setConnectionCodeOpen(false);
        setMode("send");
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [clearQueue, clearSendQueue, setConnectionCode, setConnectionCodeOpen, setMode]);

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
      <TabStrip
        canBack={navState.back}
        canForward={navState.forward}
        collapsed={collapsed}
        onBack={() => go(-1)}
        onForward={() => go(1)}
        onToggleCollapse={() => setCollapsed(!collapsed)}
      />
      <div className="app-shell">
        <Sidebar
          collapsed={collapsed}
          inboxCount={inboxCount}
          onToggleInbox={() => setInboxOpen(!inboxOpen)}
        />
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
