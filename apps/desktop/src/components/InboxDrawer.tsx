import React from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";
import { bundleStatusLabel, isPendingInboxBundle } from "../bundleState";
import { formatBytes } from "../transferProgress";

interface InboxDrawerProps {
  isOpen: boolean;
  onClose: () => void;
}

/**
 * 收件箱抽屉：白底浮层，处理外部请求与收到的资料包
 * Inbox drawer: pending bridge actions + staged bundles.
 */
export function InboxDrawer({ isOpen, onClose }: InboxDrawerProps) {
  const {
    localBridgePendingActions,
    stagedBundles,
    respondLocalBridgePendingAction,
    importCurrentStagedBundle,
    rollbackCurrentBundle,
    deleteCurrentStagedBundle,
    busy
  } = useAppContext();

  if (!isOpen) return null;

  const pendingBundles = stagedBundles.filter(isPendingInboxBundle);
  const historyBundles = stagedBundles.filter((bundle) => !isPendingInboxBundle(bundle)).slice(0, 5);

  return (
    <>
      <div className="drawer-overlay" onClick={onClose} role="presentation" />
      <aside className="drawer">
        <div className="drawer-head">
          <h3>收件箱</h3>
          <button aria-label="关闭" className="icon-btn" onClick={onClose} type="button">
            <Icon name="x" />
          </button>
        </div>
        <div className="drawer-body">
          <div className="drawer-section-label">待确认请求 · {localBridgePendingActions.length}</div>
          {localBridgePendingActions.length === 0 && (
            <div className="inline-note">没有等待确认的外部请求。</div>
          )}
          {localBridgePendingActions.map((action) => (
            <div className="drawer-row" key={action.request_id}>
              <div className="drawer-row-main">
                <div className="drawer-row-title">
                  {action.client_display_name} · {action.action_kind === "bundle.send" ? "发送资料包" : action.action_kind === "bundle.import" ? "导入资料包" : "撤回导入"}
                </div>
                <div className="drawer-row-sub">
                  {action.bundle_type ? `类型 ${action.bundle_type} · ` : ""}
                  <span className="mono">{action.client_id}</span>
                </div>
              </div>
              <div className="drawer-row-ops">
                <button
                  className="text-btn"
                  disabled={busy === "open"}
                  onClick={() => respondLocalBridgePendingAction(action, false)}
                  type="button"
                >
                  拒绝
                </button>
                <button
                  className="text-btn is-primary"
                  disabled={busy === "open"}
                  onClick={() => respondLocalBridgePendingAction(action, true)}
                  type="button"
                >
                  允许
                </button>
              </div>
            </div>
          ))}

          <div className="drawer-section-label">收到的资料包 · {pendingBundles.length}</div>
          {pendingBundles.length === 0 && (
            <div className="inline-note">没有待处理的资料包。</div>
          )}
          {pendingBundles.map((bundle) => (
            <div className="drawer-row" key={bundle.bundle_id}>
              <div className="drawer-row-main">
                <div className="drawer-row-title">{bundle.display_name}</div>
                <div className="drawer-row-sub">
                  {bundle.bundle_type} · {bundle.file_count} 个文件 · {formatBytes(bundle.total_bytes)} · 来自{" "}
                  {bundle.source_app || "未知应用"} · {bundleStatusLabel(bundle)}
                </div>
              </div>
              <div className="drawer-row-ops">
                {bundle.can_import_now ? (
                  <button
                    className="text-btn is-primary"
                    disabled={busy === "bundle-import"}
                    onClick={() => importCurrentStagedBundle(bundle, "reject")}
                    type="button"
                  >
                    导入
                  </button>
                ) : (
                  <span className="state-tag is-muted">
                    {bundle.import_blocking_reason === "not_importable" ? "仅暂存" : "不可导入"}
                  </span>
                )}
                <button
                  className="text-btn is-danger"
                  disabled={busy === "open"}
                  onClick={() => deleteCurrentStagedBundle(bundle)}
                  type="button"
                >
                  删除
                </button>
              </div>
            </div>
          ))}

          {historyBundles.length > 0 && (
            <>
              <div className="drawer-section-label">已处理</div>
              {historyBundles.map((bundle) => (
                <div className="drawer-row" key={bundle.bundle_id}>
                  <div className="drawer-row-main">
                    <div className="drawer-row-title">{bundle.display_name}</div>
                    <div className="drawer-row-sub">{bundleStatusLabel(bundle)}</div>
                  </div>
                  <div className="drawer-row-ops">
                    {bundle.staging_status === "imported" && (
                      <button
                        className="text-btn"
                        disabled={busy === "bundle-import"}
                        onClick={() => rollbackCurrentBundle(bundle)}
                        type="button"
                      >
                        撤回导入
                      </button>
                    )}
                  </div>
                </div>
              ))}
            </>
          )}
        </div>
      </aside>
    </>
  );
}
