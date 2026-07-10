import React, { useEffect, useState } from "react";
import { AppProvider, useAppContext } from "./context/AppContext";
import { LeftSidebar } from "./components/LeftSidebar";
import { TransferZone } from "./components/TransferZone";
import { DevicesManager } from "./components/DevicesManager";
import { TransfersManager } from "./components/TransfersManager";
import { SettingsManager } from "./components/SettingsManager";
import { ActivityInspector } from "./components/ActivityInspector";
import { InboxDrawer } from "./components/InboxDrawer";

/**
 * 应用内部布局渲染组件（支持全视图路由）
 * Application Inner Content and Layout Component with view routing
 */
function AppContent() {
  const { error, toast, mode } = useAppContext();
  const [inboxOpen, setInboxOpen] = useState(false);
  const [inspectorOpen, setInspectorOpen] = useState(false);

  useEffect(() => {
    if (!inspectorOpen) return;

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setInspectorOpen(false);
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [inspectorOpen]);

  // 根据当前 mode 动态决定中栏渲染的组件 / Dynamically render the middle pane based on mode
  const renderMiddlePane = () => {
    switch (mode) {
      case "send":
        return (
          <TransferZone
            inboxOpen={inboxOpen}
            inspectorOpen={inspectorOpen}
            onToggleInbox={() => setInboxOpen((current) => !current)}
            onToggleInspector={() => setInspectorOpen((current) => !current)}
          />
        );
      case "devices":
        return <DevicesManager />;
      case "transfers":
        return <TransfersManager />;
      case "settings":
        return <SettingsManager />;
      default:
        return <TransferZone />;
    }
  };

  // 返回三栏一体化主布局 / Return the three-column integrated layout
  return (
    <div className="app-container">
      {/* 全局通知提示堆叠区 / Global Alert Notification Overlay */}
      {(error || toast) && (
        <div className="global-notification-overlay">
          {error && (
            <div className="global-alert-card is-error">
              <span className="alert-badge">失败</span>
              <span className="alert-message">{error}</span>
            </div>
          )}
          {toast && (
            <div className="global-alert-card is-toast">
              <span className="alert-badge">喵</span>
              <span className="alert-message">{toast}</span>
            </div>
          )}
        </div>
      )}

      {/* Codex-style two-pane workbench / Codex 风格双栏工作台 */}
      <div className="main-layout">
        <LeftSidebar onToggleInbox={() => setInboxOpen((current) => !current)} />

        {renderMiddlePane()}
      </div>

      {inspectorOpen && (
        <div className="activity-inspector-layer">
          <button
            aria-label="关闭连接详情"
            className="activity-inspector-scrim"
            onClick={() => setInspectorOpen(false)}
            type="button"
          />
          <ActivityInspector
            onClose={() => setInspectorOpen(false)}
            onToggleInbox={() => setInboxOpen((current) => !current)}
          />
        </div>
      )}

      {/* 侧滑通知收件箱抽屉 / Slide-out Inbox Drawer */}
      <InboxDrawer isOpen={inboxOpen} onClose={() => setInboxOpen(false)} />
    </div>
  );
}

/**
 * 客户端主入口组件
 * Main Desktop Client Entry Component
 */
export function App() {
  return (
    <AppProvider>
      <AppContent />
    </AppProvider>
  );
}
