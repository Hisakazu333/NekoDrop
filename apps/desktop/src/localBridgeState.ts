import { bundleTypeLabel } from "./bundleState.ts";
import type {
  LocalBridgePendingActionDto,
  LocalBridgePendingActionResultDto,
  LocalBridgePermissionScope,
  LocalBridgeRuntimeStatusDto
} from "./types";

export function localBridgeStatusLabel(status: string) {
  if (status === "ok") return "可读取";
  if (status === "pending_auth") return "等待确认";
  if (status === "unsupported") return "不支持";
  return status;
}

export function localBridgeRuntimeStatusLine(status: LocalBridgeRuntimeStatusDto | null) {
  if (!status) return "读取中";
  if (!status.active) {
    return status.last_error ? `未监听 · ${status.last_error}` : "未监听";
  }
  const auth = status.pending_authorization_client
    ? ` · 待授权 ${status.pending_authorization_client}`
    : status.authorization_count > 0
      ? ` · 已授权 ${status.authorization_count}`
      : "";
  const actions = status.pending_action_count > 0 ? ` · 待执行 ${status.pending_action_count}` : "";
  return `${status.bind_host}:${status.port}${status.request_path}${auth}${actions}`;
}

export function localBridgeScopeLabel(scope: LocalBridgePermissionScope) {
  if (scope === "device.read") return "设备";
  if (scope === "transfer.status.read") return "传输";
  if (scope === "bundle.read") return "资料包";
  if (scope === "bundle.send") return "发送";
  if (scope === "bundle.import.request") return "导入";
  return scope;
}

export function localBridgePendingActionKindLabel(actionKind: string) {
  if (actionKind === "bundle.send") return "发送资料包";
  if (actionKind === "bundle.import") return "导入资料包";
  return actionKind;
}

export function localBridgePendingActionSummary(action: LocalBridgePendingActionDto) {
  if (action.action_kind === "bundle.send") {
    const type = action.bundle_type ? bundleTypeLabel(action.bundle_type) : "资料包";
    const target = action.target_device_id ? ` -> ${action.target_device_id}` : "";
    return `${localBridgePendingActionKindLabel(action.action_kind)} · ${type}${target}`;
  }
  if (action.action_kind === "bundle.import") {
    const type = action.expected_bundle_type ? bundleTypeLabel(action.expected_bundle_type) : "资料包";
    return `${localBridgePendingActionKindLabel(action.action_kind)} · ${type}`;
  }
  return localBridgePendingActionKindLabel(action.action_kind);
}

export function localBridgePendingActionStateLine(action: LocalBridgePendingActionDto) {
  if (action.action_kind === "bundle.send") {
    if (!action.bundle_root) return "等待执行 · 缺少资料包目录";
    if (action.require_trusted_device && !action.target_device_id) return "等待执行 · 需要选择可信设备";
    return action.target_device_id ? `等待执行 · 发送到 ${action.target_device_id}` : "等待执行 · 发送资料包";
  }
  if (action.action_kind === "bundle.import") {
    if (!action.staged_bundle_id) return "等待执行 · 缺少暂存资料";
    return `等待执行 · 导入 ${action.staged_bundle_id}`;
  }
  return "等待执行";
}

export function localBridgePendingActionTitle(action: LocalBridgePendingActionDto) {
  const target = action.target_device_id ?? action.staged_bundle_id ?? action.request_id;
  return `${action.client_display_name} · ${localBridgePendingActionKindLabel(action.action_kind)} · ${target}`;
}

export function localBridgeActionResultSummary(result: LocalBridgePendingActionResultDto) {
  const kind = localBridgePendingActionKindLabel(result.action_kind);
  const status = localBridgeActionResultStatusLabel(result.lifecycle_status ?? result.status);
  const target = result.bundle_id ?? result.target_device_id ?? result.request_id;
  const reason = result.reason ? ` · ${localBridgeActionResultReasonLabel(result.reason)}` : "";
  return `${kind} · ${status}${reason} · ${target}`;
}

export function localBridgeActionResultDetailLine(result: LocalBridgePendingActionResultDto) {
  const status = result.lifecycle_status ?? result.status;
  const reason = result.reason ? localBridgeActionResultReasonLabel(result.reason) : null;
  if (status === "queued") return "排队中";
  if (status === "running") return "执行中";
  if (status === "succeeded" || status === "completed") return "已完成";
  if (status === "conflict") return reason ? `冲突：${reason}` : "冲突";
  if (status === "cancelled") return reason ? `已取消：${reason}` : "已取消";
  if (status === "failed_preflight") return reason ? `预检失败：${reason}` : "预检失败";
  if (status === "failed") return reason ? `失败：${reason}` : result.message || "失败";
  return reason ? `${localBridgeActionResultStatusLabel(status)}：${reason}` : result.message;
}

export function localBridgeActionResultStatusLabel(status: string) {
  if (status === "queued") return "排队中";
  if (status === "running") return "执行中";
  if (status === "succeeded") return "完成";
  if (status === "conflict") return "有冲突";
  if (status === "cancelled") return "已取消";
  if (status === "completed") return "完成";
  if (status === "failed") return "失败";
  if (status === "ready") return "可执行";
  if (status === "failed_preflight") return "预检失败";
  return status;
}

export function localBridgeActionResultReasonLabel(reason: string) {
  if (reason === "bundle_root_missing") return "来源不存在";
  if (reason === "bundle_manifest_missing") return "缺少清单";
  if (reason === "bundle_invalid") return "校验失败";
  if (reason === "bundle_type_mismatch") return "类型不匹配";
  if (reason === "trusted_target_required") return "需要可信设备";
  if (reason === "trusted_target_missing") return "目标未配对";
  if (reason === "target_device_required") return "缺少目标设备";
  if (reason === "bundle_send_failed") return "发送失败";
  if (reason === "bundle_import_failed") return "导入失败";
  if (reason === "bundle_import_conflict") return "已存在同名导入";
  return reason;
}
