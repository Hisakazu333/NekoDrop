import React from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";
import { formatBytes } from "../transferProgress";
import type { LocalBridgePendingActionDto, ReceivedBundleDto } from "../types";

interface InboxDrawerProps {
  isOpen: boolean;
  onClose: () => void;
}

/**
 * 侧滑式收件箱通知抽屉组件
 * Slide-out Inbox Notification Drawer Component
 */
export function InboxDrawer({ isOpen, onClose }: InboxDrawerProps) {
  const {
    localBridgePendingActions,
    stagedBundles,
    removeLocalBridgePendingAction,
    importCurrentStagedBundle,
    deleteCurrentStagedBundle,
    error,
    setError
  } = useAppContext();

  if (!isOpen) return null;

  // 待处理任务总数 / Total pending notifications count
  const pendingBundles = stagedBundles.filter((b) => b.staging_status === "saved");
  const totalCount = localBridgePendingActions.length + pendingBundles.length;

  const handleAction = async (action: LocalBridgePendingActionDto, accept: boolean) => {
    try {
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
    <div className="drawer-overlay" onClick={onClose}>
      <div className="drawer-container" onClick={(e) => e.stopPropagation()}>
        {/* 抽屉头部 / Drawer Header */}
        <div className="drawer-header">
          <div className="drawer-title-group">
            <h3>系统收件箱 (Inbox)</h3>
            {totalCount > 0 && (
              <span className="drawer-count-badge">{totalCount} 个未处理</span>
            )}
          </div>
          <button className="btn-close-drawer" onClick={onClose} type="button">
            ×
          </button>
        </div>

        {/* 抽屉内容区 / Drawer Body */}
        <div className="drawer-body">
          {totalCount === 0 ? (
            <div className="drawer-empty-state">
              <Icon name="inbox" className="empty-icon" />
              <p>您的收件箱很清爽，没有待处理的任务。</p>
            </div>
          ) : (
            <div className="drawer-items-list">
              {/* 1. 本地桥外部应用请求 / Local Bridge Actions */}
              {localBridgePendingActions.map((action) => (
                <div key={action.request_id} className="drawer-card action-card">
                  <div className="card-tag action-tag">外部应用申请</div>
                  <h4 className="card-title">{action.client_display_name || "本地应用"}</h4>
                  <p className="card-desc">
                    申请获取本机设备列表或发起数据包传输。该权限为临时授权。
                  </p>
                  <div className="card-actions">
                    <button
                      className="btn-pill btn-reject"
                      onClick={() => handleAction(action, false)}
                      type="button"
                    >
                      拒绝
                    </button>
                    <button
                      className="btn-pill btn-accept"
                      onClick={() => handleAction(action, true)}
                      type="button"
                    >
                      允许
                    </button>
                  </div>
                </div>
              ))}

              {/* 2. 未导入的暂存资料包 / Staged Bundles */}
              {pendingBundles.map((bundle) => (
                <div key={bundle.bundle_id} className="drawer-card bundle-card">
                  <div className="card-tag bundle-tag">暂存资料包</div>
                  <h4 className="card-title">{bundle.display_name}</h4>
                  <p className="card-desc">
                    来源应用: {bundle.source_app} · 大小: {formatBytes(bundle.total_bytes)}
                  </p>
                  <div className="card-actions">
                    <button
                      className="btn-pill btn-reject"
                      onClick={() => handleDeleteBundle(bundle)}
                      type="button"
                    >
                      删除
                    </button>
                    <button
                      className="btn-pill btn-accept"
                      onClick={() => handleImportBundle(bundle)}
                      type="button"
                    >
                      导入
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
