import React, { useState } from "react";
import { useAppContext } from "../context/AppContext";
import { Icon } from "./Icon";
import {
  localBridgeAdapterImportSummary,
  localBridgeActionResultLifecycleView,
  localBridgeActionResultSummary,
  localBridgePendingActionStateLine,
  localBridgePendingActionSummary,
  localBridgePendingActionTitle,
  localBridgeRuntimeStatusLine,
  localBridgeScopeLabel
} from "../localBridgeState";
import type { LocalBridgePendingActionDto, LocalBridgePendingActionResultDto } from "../types";

const RECEIVE_POLICY_OPTIONS = [
  { value: "always_ask", label: "需要每次询问" },
  { value: "block_all", label: "阻止所有未配对设备" }
];

/**
 * 系统设置与本地桥管理组件
 * System Settings and Local Bridge Manager Component
 */
export function SettingsManager() {
  const {
    snapshot,
    receiveDir,
    setBindPort,
    bindPort,
    setDeviceNameInput,
    deviceNameInput,
    receivePolicy,
    chooseReceiveDir,
    saveReceiveDir,
    saveReceivePort,
    updateReceivePolicy,
    saveDeviceName,
    localBridgeStatus,
    localBridgeAuthorizations,
    localBridgePendingActions,
    localBridgeActionResults,
    localBridgeCheck,
    localBridgeAuthorizationCode,
    setLocalBridgeAuthorizationCode,
    runLocalBridgeSelfCheck,
    confirmLocalBridgeAuthorization,
    removeLocalBridgePendingAction,
    revokeLocalBridgeAuthorization,
    pruneLocalBridgeAuthorizations,
    runNextLocalBridgeAction,
    retryLocalBridgeImportResult,
    rollbackLocalBridgeImportResult,
    busy,
    appearance,
    setAppearance
  } = useAppContext();
  const [expandedActionIds, setExpandedActionIds] = useState<Set<string>>(() => new Set());
  const [expandedResultIds, setExpandedResultIds] = useState<Set<string>>(() => new Set());
  const localBridgeRuntimeLine = localBridgeRuntimeStatusLine(localBridgeStatus);
  const localBridgeImportSummary = localBridgeAdapterImportSummary(localBridgeActionResults);
  const toggleActionDetail = (requestId: string) => {
    setExpandedActionIds((current) => {
      const next = new Set(current);
      if (next.has(requestId)) next.delete(requestId);
      else next.add(requestId);
      return next;
    });
  };
  const toggleResultDetail = (key: string) => {
    setExpandedResultIds((current) => {
      const next = new Set(current);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };
  const canRetryResult = (result: LocalBridgePendingActionResultDto) =>
    result.action_kind === "bundle.import" &&
    Boolean(result.bundle_id) &&
    ["failed", "conflict"].includes(result.lifecycle_status ?? result.status);
  const retryStrategyForResult = (result: LocalBridgePendingActionResultDto) =>
    (result.lifecycle_status ?? result.status) === "conflict" ? "rename" : result.conflict_strategy ?? "reject";

  return (
    <div className="manager-pane settings-manager">
      <div className="manager-header">
        <h2>设置与服务</h2>
        <p>配置这台设备的网络传输属性，并管理本机外部开发工具的受控接入。</p>
      </div>

      <div className="manager-body">
        {/* 0. 外观与界面设置 / Appearance Settings */}
        <div className="settings-section-card">
          <h3>外观</h3>
          <div className="settings-form">
            <div className="form-item">
              <label>界面主题</label>
              <div className="theme-toggle-group">
                <button
                  className={`btn-theme-select ${appearance === "light" ? "is-active" : ""}`}
                  onClick={() => setAppearance("light")}
                  type="button"
                >
                  <Icon name="sun" />
                  <span>浅色模式</span>
                </button>
                <button
                  className={`btn-theme-select ${appearance === "dark" ? "is-active" : ""}`}
                  onClick={() => setAppearance("dark")}
                  type="button"
                >
                  <Icon name="moon" />
                  <span>深色模式</span>
                </button>
              </div>
            </div>
          </div>
        </div>

        {/* 1. 基础局域网文件传输设置 / Base File Sharing Settings */}
        <div className="settings-section-card">
          <h3>文件传输服务</h3>
          
          <div className="settings-form">
            {/* 修改本机设备名 */}
            <div className="form-item">
              <label htmlFor="device-name">本机设备名（局域网显示名称）</label>
              <div className="form-input-group">
                <input
                  id="device-name"
                  type="text"
                  value={deviceNameInput}
                  onChange={(e) => setDeviceNameInput(e.target.value)}
                  placeholder="输入这台电脑的名称..."
                />
                <button
                  className="btn-form-save"
                  onClick={saveDeviceName}
                  disabled={busy === "device-name" || deviceNameInput.trim() === snapshot?.device_name}
                  type="button"
                >
                  保存
                </button>
              </div>
            </div>

            {/* 修改接收存储路径 */}
            <div className="form-item">
              <label htmlFor="receive-dir">默认文件接收目录</label>
              <div className="form-input-group">
                <input
                  id="receive-dir"
                  type="text"
                  value={receiveDir}
                  readOnly
                  placeholder="选择文件保存路径..."
                />
                <button
                  className="btn-form-secondary"
                  onClick={chooseReceiveDir}
                  disabled={busy === "pick-receive"}
                  type="button"
                >
                  选择...
                </button>
                <button
                  className="btn-form-save"
                  onClick={saveReceiveDir}
                  disabled={busy === "pick-receive" || receiveDir.trim() === snapshot?.receive_dir}
                  type="button"
                >
                  保存
                </button>
              </div>
            </div>

            {/* 修改端口与策略 */}
            <div className="form-row-two-columns">
              <div className="form-item">
                <label htmlFor="bind-port">默认监听端口</label>
                <div className="form-input-group">
                  <input
                    id="bind-port"
                    type="text"
                    value={bindPort}
                    onChange={(e) => setBindPort(e.target.value)}
                    placeholder="默认: 45821"
                  />
                  <button
                    className="btn-form-save"
                    onClick={saveReceivePort}
                    disabled={busy === "pick-receive" || bindPort === String(snapshot?.receive_port)}
                    type="button"
                  >
                    修改
                  </button>
                </div>
              </div>

              <div className="form-item">
                <label htmlFor="receive-policy">未配对设备接收策略</label>
                <div className="policy-segment" id="receive-policy">
                  {RECEIVE_POLICY_OPTIONS.map((opt) => (
                    <button
                      key={opt.value}
                      className={receivePolicy === opt.value ? "is-active" : ""}
                      onClick={() => updateReceivePolicy(opt.value as any)}
                      disabled={busy === "receive-policy"}
                      type="button"
                    >
                      {opt.label}
                    </button>
                  ))}
                </div>
              </div>
            </div>
          </div>
        </div>

        {/* 2. 本地桥控制台管理 / Local Bridge Console */}
        <div className="settings-section-card">
          <div className="card-title-header">
            <h3>本机外部接口 Local Bridge</h3>
            <span className={`status-led ${localBridgeStatus?.active ? "is-running" : "is-stopped"}`}>
              {localBridgeStatus?.active ? "服务运行中" : "已停止"}
            </span>
          </div>
          <p className="section-desc">
            Local Bridge 允许本机上运行的其他应用（如开发插件、编辑器助手）通过受控 localhost 接口调用 NekoLink 协议。
          </p>

          <div className="local-bridge-dashboard">
            <div className="local-bridge-status-row">
              <div className="status-metric">
                <span className="metric-label">接口监听端口</span>
                <strong className="metric-val">{localBridgeStatus?.port || "未启用"}</strong>
                <small className="metric-subline" title={localBridgeRuntimeLine}>{localBridgeRuntimeLine}</small>
              </div>
              <div className="status-metric">
                <span className="metric-label">外部已授权应用</span>
                <strong className="metric-val">{localBridgeAuthorizations.length} 个</strong>
              </div>
              <div className="status-metric">
                <span className="metric-label">未决确认请求</span>
                <strong className="metric-val">{localBridgeStatus?.pending_action_count || 0} 个</strong>
              </div>
              <div className="status-metric">
                <span className="metric-label">Adapter 导入</span>
                <strong className="metric-val">{localBridgeActionResults.filter((result) => result.action_kind === "bundle.import").length} 条</strong>
                <small className="metric-subline" title={localBridgeImportSummary}>{localBridgeImportSummary}</small>
              </div>
            </div>

            {/* 本地桥自测与手动授权 */}
            <div className="local-bridge-actions-box">
              <div className="action-col">
                <h4>接口功能自测</h4>
                <div className="btn-group">
                  <button
                    className="btn-form-secondary"
                    onClick={runLocalBridgeSelfCheck}
                    disabled={Boolean(busy)}
                    type="button"
                  >
                    运行 localhost 自测
                  </button>
                  {localBridgeCheck && (
                    <span className="action-result-text">{localBridgeCheck}</span>
                  )}
                </div>
              </div>

              <div className="action-col">
                <h4>输入外部授权码</h4>
                <div className="form-input-group">
                  <input
                    type="text"
                    placeholder="输入编辑器插件生成的授权码..."
                    value={localBridgeAuthorizationCode}
                    onChange={(e) => setLocalBridgeAuthorizationCode(e.target.value)}
                  />
                  <button
                    className="btn-form-save"
                    onClick={confirmLocalBridgeAuthorization}
                    disabled={Boolean(busy) || !localBridgeAuthorizationCode.trim()}
                    type="button"
                  >
                    确认授权
                  </button>
                </div>
              </div>
            </div>

            {/* 待执行动作与最近结果 / Pending actions and recent action results */}
            <div className="local-bridge-work-queue">
              <div className="queue-column">
                <div className="queue-title-row">
                  <h4>待执行</h4>
                  <button
                    className="btn-text-muted"
                    disabled={Boolean(busy) || localBridgePendingActions.length === 0}
                    onClick={runNextLocalBridgeAction}
                    type="button"
                  >
                    确认下一项
                  </button>
                </div>
                {localBridgePendingActions.length > 0 ? (
                  <div className="local-bridge-list">
                    {localBridgePendingActions.map((action) => (
                      <div className="console-row is-stack" key={action.request_id}>
                        <div className="console-row-main">
                          <div className="console-copy">
                            <span title={localBridgePendingActionTitle(action)}>
                              {action.client_display_name} · {localBridgePendingActionSummary(action)}
                            </span>
                            <small>{localBridgePendingActionStateLine(action)}</small>
                          </div>
                          <div className="console-actions">
                            <button
                              className="btn-text-muted"
                              onClick={() => toggleActionDetail(action.request_id)}
                              type="button"
                            >
                              查看详情
                            </button>
                            <button
                              className="btn-text-muted"
                              disabled={busy === "open"}
                              onClick={() => removeLocalBridgePendingAction(action)}
                              type="button"
                            >
                              拒绝
                            </button>
                          </div>
                        </div>
                        {expandedActionIds.has(action.request_id) ? (
                          <LocalBridgeActionDetail action={action} />
                        ) : null}
                      </div>
                    ))}
                  </div>
                ) : (
                  <div className="clients-empty">暂无待执行</div>
                )}
              </div>

              <div className="queue-column">
                <h4>执行结果</h4>
                {localBridgeActionResults.length > 0 ? (
                  <div className="local-bridge-list">
                    {localBridgeActionResults.slice(0, 5).map((result) => {
                      const lifecycle = localBridgeActionResultLifecycleView(result);
                      const resultKey = `${result.request_id}-${result.claimed_at_ms}`;
                      const canRetry = canRetryResult(result);
                      return (
                        <div className="console-row is-stack" key={resultKey}>
                          <div className="console-row-main">
                            <div className="console-copy">
                              <span title={result.message}>
                                {result.client_display_name} · {localBridgeActionResultSummary(result)}
                              </span>
                              <small>{lifecycle.detail}</small>
                            </div>
                            <span className={`local-bridge-result-status is-${lifecycle.tone}`}>
                              {lifecycle.label}
                            </span>
                          </div>
                          <div className="console-actions">
                            <button
                              className="btn-text-muted"
                              onClick={() => toggleResultDetail(resultKey)}
                              type="button"
                            >
                              查看详情
                            </button>
                            {canRetry ? (
                              <button
                                className="btn-text-muted"
                                disabled={Boolean(busy)}
                                onClick={() => retryLocalBridgeImportResult(result, retryStrategyForResult(result))}
                                type="button"
                              >
                                重试
                              </button>
                            ) : null}
                            {result.can_request_rollback ? (
                              <button
                                className="btn-text-muted"
                                disabled={Boolean(busy)}
                                onClick={() => rollbackLocalBridgeImportResult(result)}
                                type="button"
                              >
                                回滚
                              </button>
                            ) : null}
                          </div>
                          {expandedResultIds.has(resultKey) ? (
                            <LocalBridgeResultDetail result={result} />
                          ) : null}
                        </div>
                      );
                    })}
                  </div>
                ) : (
                  <div className="clients-empty">暂无结果</div>
                )}
              </div>
            </div>

            {/* 已授权外部客户端列表 / Authorized Clients */}
            <div className="authorized-clients-section">
              <div className="clients-header">
                <h4>已授权的本地客户端</h4>
                {localBridgeAuthorizations.length > 0 && (
                  <button
                    className="btn-text-muted"
                    onClick={pruneLocalBridgeAuthorizations}
                    type="button"
                  >
                    清理过期授权
                  </button>
                )}
              </div>

              {localBridgeAuthorizations.length > 0 ? (
                <div className="clients-list">
                  {localBridgeAuthorizations.map((auth) => (
                    <div key={auth.client_id} className="client-auth-card">
                      <div className="client-meta">
                        <strong>{auth.display_name}</strong>
                        <span className="client-id-tag">ID: {auth.client_id}</span>
                      </div>
                      <div className="client-scopes">
                        {auth.scopes.map((scope) => (
                          <div key={scope} className="scope-tag">
                            <span>{localBridgeScopeLabel(scope)}</span>
                            <button
                              className="btn-scope-revoke"
                              onClick={() => revokeLocalBridgeAuthorization(auth, scope)}
                              title="撤销该权限"
                              type="button"
                            >
                              ×
                            </button>
                          </div>
                        ))}
                      </div>
                    </div>
                  ))}
                </div>
              ) : (
                <div className="clients-empty">暂无外部客户端授权记录</div>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function LocalBridgeActionDetail({ action }: { action: LocalBridgePendingActionDto }) {
  return (
    <div className="console-detail-grid">
      <span>请求 ID</span>
      <strong>{action.request_id}</strong>
      <span>客户端</span>
      <strong>{action.client_id}</strong>
      <span>动作</span>
      <strong>{action.action_kind}</strong>
      <span>资料包</span>
      <strong>{action.staged_bundle_id ?? action.bundle_type ?? "未公开"}</strong>
      <span>冲突策略</span>
      <strong>{action.conflict_strategy ?? "不需要"}</strong>
    </div>
  );
}

function LocalBridgeResultDetail({ result }: { result: LocalBridgePendingActionResultDto }) {
  return (
    <div className="console-detail-grid">
      <span>请求 ID</span>
      <strong>{result.request_id}</strong>
      <span>客户端</span>
      <strong>{result.client_id}</strong>
      <span>动作</span>
      <strong>{result.action_kind}</strong>
      <span>状态</span>
      <strong>{result.lifecycle_status ?? result.status}</strong>
      <span>资料包</span>
      <strong>{result.bundle_id ?? result.bundle_type ?? "未公开"}</strong>
      <span>原因</span>
      <strong>{result.reason ?? "无"}</strong>
      <span>Receipt</span>
      <strong>{result.has_import_receipt ? "已记录" : "无"}</strong>
      <span>可回滚文件</span>
      <strong>{result.rollback_file_count}</strong>
    </div>
  );
}
