import React, { useState } from "react";
import { AppProvider, useAppContext } from "./context/AppContext";
import { TitleBar } from "./components/TitleBar";
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

  // 根据当前 mode 动态决定中栏渲染的组件 / Dynamically render the middle pane based on mode
  const renderMiddlePane = () => {
    switch (mode) {
      case "send":
        return <TransferZone />;
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

  return (
    <div className="app-container">
      {/* 自定义标题栏（适配 Mac 交通灯） / Custom Titlebar */}
      <TitleBar onToggleInbox={() => setInboxOpen(!inboxOpen)} inboxOpen={inboxOpen} />

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
              <span className="alert-badge">提示</span>
              <span className="alert-message">{toast}</span>
            </div>
          )}
        </div>
      )}

      {/* 三栏式工具工作台核心布局 / Three-Column Workbench Layout */}
      <div className="main-layout">
        {/* 1. 左栏：导航与设备树列表 */}
        <LeftSidebar />

        {/* 2. 中栏：工作台与能力页签（动态路由渲染） */}
        {renderMiddlePane()}

        {/* 3. 右栏：活跃传输与历史面板 */}
        <ActivityInspector />
      </div>

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
