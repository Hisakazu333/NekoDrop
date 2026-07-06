import React, { useState, useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";
import { isTauriRuntime } from "../tauri";

interface TitleBarProps {
  onToggleInbox: () => void;
  inboxOpen: boolean;
}

/**
 * 自定义无边框窗口标题栏组件（支持 Mac 交通灯适配）
 * Custom Frameless Window Titlebar Component (supports Mac Traffic Lights)
 */
export function TitleBar({ onToggleInbox, inboxOpen }: TitleBarProps) {
  const {
    localBridgePendingActions,
    stagedBundles,
    appearance,
    setAppearance
  } = useAppContext();

  const [isMaximized, setIsMaximized] = useState(false);
  const appWindow = isTauriRuntime() ? getCurrentWindow() : null;
  const isMac = typeof navigator !== "undefined" && navigator.userAgent.toLowerCase().includes("mac");

  // 待处理动作与资料包总数 / Total count of pending actions and unimported bundles
  const pendingCount =
    localBridgePendingActions.length +
    stagedBundles.filter((b) => b.staging_status === "saved").length;

  // 监听窗口最大化状态以更新图标 / Monitor window maximization state to update the icon
  useEffect(() => {
    if (!appWindow) return;

    const checkMaximized = async () => {
      const maximized = await appWindow.isMaximized();
      setIsMaximized(maximized);
    };

    checkMaximized();

    // 监听窗口缩放事件以实时更新 / Listen to window resize to update in real-time
    const unlisten = appWindow.onResized(() => {
      checkMaximized();
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [appWindow]);

  const handleMinimize = async () => {
    if (appWindow) await appWindow.minimize();
  };

  const handleMaximize = async () => {
    if (appWindow) {
      await appWindow.toggleMaximize();
      const maximized = await appWindow.isMaximized();
      setIsMaximized(maximized);
    }
  };

  const handleClose = async () => {
    if (appWindow) await appWindow.close();
  };

  const toggleTheme = () => {
    setAppearance((curr) => (curr === "dark" ? "light" : "dark"));
  };

  return (
    <header className="app-titlebar" data-tauri-drag-region style={{ WebkitAppRegion: "drag", userSelect: "none" } as any}>
      {/* 针对 Mac 系统的交通灯留白区 / Mac Traffic Lights Spacer */}
      {isMac && <div className="titlebar-mac-spacer" data-tauri-drag-region />}

      {/* 左侧品牌与名称 / Brand and Logo */}
      <div className="titlebar-brand" data-tauri-drag-region>
        <span className="titlebar-logo">
          <Icon name="paw" />
        </span>
        <strong data-tauri-drag-region>NekoDrop</strong>
        <span className="titlebar-subtitle" data-tauri-drag-region>
          局域网安全共享
        </span>
      </div>

      {/* 中间拖拽区域空白 / Middle Drag Space */}
      <div className="titlebar-drag-space" data-tauri-drag-region />

      {/* 右侧操作按钮区 / Actions and Window Controls */}
      <div className="titlebar-actions">
        {/* 收件箱通知按钮 / Notification Inbox Button */}
        <button
          className={`titlebar-btn inbox-trigger ${inboxOpen ? "is-active" : ""} ${
            pendingCount > 0 ? "has-badge" : ""
          }`}
          onClick={onToggleInbox}
          title="收件箱通知"
          type="button"
        >
          <Icon name="inbox" />
          {pendingCount > 0 && (
            <span className="inbox-badge-dot" />
          )}
        </button>

        {/* 主题切换按钮 / Theme Toggle */}
        <button
          className="titlebar-btn"
          onClick={toggleTheme}
          title={appearance === "dark" ? "切换至浅色模式" : "切换至深色模式"}
          type="button"
        >
          <Icon name={appearance === "dark" ? "sun" : "moon"} />
        </button>

        {/* 仅在非 Mac 平台且处于 Tauri 运行时才渲染窗口控制按钮 / Only render controls on non-Mac platforms in Tauri */}
        {appWindow && !isMac && (
          <>
            <span className="titlebar-divider" />
            <button
              className="titlebar-btn window-control btn-minimize"
              onClick={handleMinimize}
              title="最小化"
              type="button"
            >
              <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
                <path d="M2 6h8" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
              </svg>
            </button>
            <button
              className="titlebar-btn window-control btn-maximize"
              onClick={handleMaximize}
              title={isMaximized ? "向下还原" : "最大化"}
              type="button"
            >
              <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
                {isMaximized ? (
                  <path
                    d="M3 4.5h4.5V9H3V4.5z M4.5 3H9v4.5H7.5"
                    stroke="currentColor"
                    strokeWidth="1.2"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  />
                ) : (
                  <rect
                    x="2.5"
                    y="2.5"
                    width="7"
                    height="7"
                    rx="1"
                    stroke="currentColor"
                    strokeWidth="1.2"
                  />
                )}
              </svg>
            </button>
            <button
              className="titlebar-btn window-control btn-close"
              onClick={handleClose}
              title="关闭"
              type="button"
            >
              <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
                <path
                  d="M2.5 2.5l7 7M9.5 2.5l-7 7"
                  stroke="currentColor"
                  strokeWidth="1.2"
                  strokeLinecap="round"
                />
              </svg>
            </button>
          </>
        )}
      </div>
    </header>
  );
}
