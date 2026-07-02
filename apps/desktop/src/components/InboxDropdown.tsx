import React, { useState, useRef, useEffect } from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";
import { formatBytes } from "../transferProgress";
import type { LocalBridgePendingActionDto, ReceivedBundleDto } from "../types";

/**
 * 极简系统收件箱下拉组件
 * Minimalist System Inbox Dropdown Component
 */
export function InboxDropdown() {
  const {
    localBridgePendingActions,
    stagedBundles,
    removeLocalBridgePendingAction,
    importCurrentStagedBundle,
    deleteCurrentStagedBundle,
    error,
    setError
  } = useAppContext();

  const [isOpen, setIsOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  // 待处理动作和未导入资料包的数量总和 / Total count of pending actions and unimported bundles
  const totalCount = localBridgePendingActions.length + stagedBundles.filter(b => b.staging_status === "saved").length;

  // 点击外部关闭下拉菜单 / Click outside to close the dropdown
  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  const handleAction = async (action: LocalBridgePendingActionDto, accept: boolean) => {
    try {
      // 确认或拒绝本地桥动作 / Approve or deny the local bridge action
      await removeLocalBridgePendingAction(action);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleImportBundle = async (bundle: ReceivedBundleDto) => {
    try {
      await importCurrentStagedBundle(bundle);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleDeleteBundle = async (bundle: ReceivedBundleDto) => {
    try {
      await deleteCurrentStagedBundle(bundle);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  return (
    <div className="inbox-container" ref={dropdownRef}>
      {/* 收件箱触发按钮 / Inbox Trigger Button */}
      <button
        className={`inbox-trigger ${isOpen ? "is-active" : ""} ${totalCount > 0 ? "has-badge" : ""}`}
        onClick={() => setIsOpen(!isOpen)}
        title="收件箱"
        type="button"
      >
        <Icon name="inbox" />
        {totalCount > 0 && (
          <span className="inbox-badge">{totalCount}</span>
        )}
      </button>

      {/* 下拉面板 / Dropdown Panel */}
      {isOpen && (
        <div className="inbox-dropdown">
          <div className="inbox-header">
            <strong>收件箱 (Inbox)</strong>
            {totalCount > 0 && <span className="inbox-status-tag">{totalCount} 个待处理</span>}
          </div>

          <div className="inbox-body">
            {totalCount === 0 ? (
              <div className="inbox-empty">
                <Icon name="inbox" className="inbox-empty-icon" />
                <p>暂无待处理通知</p>
              </div>
            ) : (
              <>
                {/* 1. 本地桥外部应用授权请求 / Local Bridge Actions */}
                {localBridgePendingActions.map((action) => (
                  <div key={action.request_id} className="inbox-item action-item">
                    <div className="inbox-item-meta">
                      <span className="item-tag action-tag">应用授权</span>
                      <strong className="item-title">{action.client_display_name || "外部应用"}</strong>
                      <p className="item-desc">请求读取您的设备或发起文件流</p>
                    </div>
                    <div className="inbox-item-ops">
                      <button
                        className="btn-pill btn-reject"
                        onClick={() => handleAction(action, false)}
                      >
                        拒绝
                      </button>
                      <button
                        className="btn-pill btn-accept"
                        onClick={() => handleAction(action, true)}
                      >
                        允许
                      </button>
                    </div>
                  </div>
                ))}

                {/* 2. 暂存未导入的资料包 / Staged Bundles */}
                {stagedBundles
                  .filter((b) => b.staging_status === "saved")
                  .map((bundle) => (
                    <div key={bundle.bundle_id} className="inbox-item bundle-item">
                      <div className="inbox-item-meta">
                        <span className="item-tag bundle-tag">收到资料包</span>
                        <strong className="item-title">{bundle.display_name}</strong>
                        <p className="item-desc">
                          来自: {bundle.source_app} · {formatBytes(bundle.total_bytes)}
                        </p>
                      </div>
                      <div className="inbox-item-ops">
                        <button
                          className="btn-pill btn-reject"
                          onClick={() => handleDeleteBundle(bundle)}
                        >
                          删除
                        </button>
                        <button
                          className="btn-pill btn-accept"
                          onClick={() => handleImportBundle(bundle)}
                        >
                          导入
                        </button>
                      </div>
                    </div>
                  ))}
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
