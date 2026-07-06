import React, { useState } from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";
import {
  bundleCanUseImportStrategy,
  bundleImportPlanLine,
  bundleImportStatusView,
  bundleTypeLabel
} from "../bundleState";
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
    rollbackCurrentBundle,
    deleteCurrentStagedBundle,
    setError
  } = useAppContext();
  const [expandedBundleIds, setExpandedBundleIds] = useState<Set<string>>(() => new Set());

  if (!isOpen) return null;

  // 待处理任务总数 / Total pending notifications count
  const pendingBundles = stagedBundles.filter((b) => b.staging_status !== "deleted" && b.staging_status !== "expired");
  const totalCount = localBridgePendingActions.length + pendingBundles.length;

  const handleCancelAction = async (action: LocalBridgePendingActionDto) => {
    try {
      await removeLocalBridgePendingAction(action);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleImportBundle = async (bundle: ReceivedBundleDto, conflictStrategy?: string) => {
    try {
      await importCurrentStagedBundle(bundle, conflictStrategy);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleRollbackBundle = async (bundle: ReceivedBundleDto) => {
    try {
      await rollbackCurrentBundle(bundle);
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
  const toggleBundleDetail = (bundleId: string) => {
    setExpandedBundleIds((current) => {
      const next = new Set(current);
      if (next.has(bundleId)) next.delete(bundleId);
      else next.add(bundleId);
      return next;
    });
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
                  <div className="card-tag action-tag">待执行动作</div>
                  <h4 className="card-title">{action.client_display_name || "本地应用"}</h4>
                  <p className="card-desc">
                    已授权的本机动作正在队列中等待执行。拒绝会从队列移除。
                  </p>
                  <div className="card-actions">
                    <button
                      className="btn-pill btn-reject"
                      onClick={() => handleCancelAction(action)}
                      type="button"
                    >
                      拒绝
                    </button>
                  </div>
                </div>
              ))}

              {/* 2. 未导入的暂存资料包 / Staged Bundles */}
              {pendingBundles.map((bundle) => {
                const importStatus = bundleImportStatusView(bundle);
                const importPlanLine = bundleImportPlanLine(bundle);
                return (
                  <div key={bundle.bundle_id} className="drawer-card bundle-card">
                    <div className="card-tag bundle-tag">暂存资料包</div>
                    <h4 className="card-title">{bundle.display_name}</h4>
                    <p className="card-desc">
                      {bundleTypeLabel(bundle.bundle_type)} · 来源应用: {bundle.source_app} · 大小:{" "}
                      {formatBytes(bundle.total_bytes)}
                    </p>
                    <p className="card-desc">{importStatus.detail}</p>
                    {importPlanLine ? <p className="card-desc">{importPlanLine}</p> : null}
                    <div className="card-actions">
                      <button
                        className="btn-pill btn-reject"
                        onClick={() => toggleBundleDetail(bundle.bundle_id)}
                        type="button"
                      >
                        查看详情
                      </button>
                      <button
                        className="btn-pill btn-reject"
                        onClick={() => handleDeleteBundle(bundle)}
                        type="button"
                      >
                        拒绝
                      </button>
                      {bundle.can_import_now ? (
                        <button
                          className="btn-pill btn-accept"
                          onClick={() => handleImportBundle(bundle)}
                          type="button"
                        >
                          {bundle.staging_status === "import_failed" ? "重试" : "确认导入"}
                        </button>
                      ) : null}
                      {!bundle.can_import_now && bundleCanUseImportStrategy(bundle, "rename") ? (
                        <button
                          className="btn-pill btn-accept"
                          onClick={() => handleImportBundle(bundle, "rename")}
                          type="button"
                        >
                          重命名
                        </button>
                      ) : null}
                      {!bundle.can_import_now && bundleCanUseImportStrategy(bundle, "skip_conflicts") ? (
                        <button
                          className="btn-pill btn-accept"
                          onClick={() => handleImportBundle(bundle, "skip_conflicts")}
                          type="button"
                        >
                          跳过冲突
                        </button>
                      ) : null}
                      {bundle.staging_status === "imported" && bundle.can_rollback_now ? (
                        <button
                          className="btn-pill btn-accept"
                          onClick={() => handleRollbackBundle(bundle)}
                          type="button"
                        >
                          回滚
                        </button>
                      ) : null}
                    </div>
                    {expandedBundleIds.has(bundle.bundle_id) ? (
                      <div className="bundle-detail-grid">
                        <span>Bundle ID</span>
                        <strong>{bundle.bundle_id}</strong>
                        <span>类型</span>
                        <strong>{bundleTypeLabel(bundle.bundle_type)}</strong>
                        <span>文件</span>
                        <strong>{bundle.file_count} 个</strong>
                        <span>导入计划</span>
                        <strong>{importPlanLine ?? importStatus.detail}</strong>
                        <span>Receipt</span>
                        <strong>{bundle.has_import_receipt ? "已记录" : "无"}</strong>
                        <span>回滚</span>
                        <strong>{bundle.can_request_rollback ? `可回滚 ${bundle.rollback_file_count} 个文件` : "不可回滚"}</strong>
                      </div>
                    ) : null}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
