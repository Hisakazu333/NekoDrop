import type { ReceivedBundleDto, ReceiveReportDto } from "./types";
import { formatBytes } from "./transferProgress.ts";

export function receiveBundleSummaryLine(report: ReceiveReportDto) {
  const bundle = report.bundle;
  if (!bundle) return null;
  return `${bundle.display_name} · ${bundleTypeLabel(bundle.bundle_type)} · ${bundle.source_app} · ${formatBytes(bundle.total_bytes)}`;
}

export function receiveBundleStatusLabel(report: ReceiveReportDto) {
  const bundle = report.bundle;
  if (!bundle) return null;
  return bundleStatusLabel(bundle);
}

export function bundleStatusLabel(bundle: ReceivedBundleDto) {
  if (bundle.staging_status === "imported") return "已导入";
  if (bundle.staging_status === "deleted") return "已删除";
  if (bundle.staging_status === "import_failed") return "导入失败";
  if (bundle.staging_status === "expired") return "已过期";
  if (bundle.import_conflict || bundle.import_blocking_reason === "destination_exists") return "已存在";
  if (bundle.can_import_now) return "可导入";
  return bundle.import_allowed ? "等待导入" : "仅保存";
}

export function receiveBundleImportHint(bundle: ReceivedBundleDto) {
  if (bundle.staging_status === "imported") {
    return bundle.import_path ? `已导入到 ${bundle.import_path}` : "已导入";
  }
  if (bundle.staging_status === "deleted") return "暂存已删除，历史记录保留";
  if (bundle.staging_status === "import_failed") return "导入没有完成，暂存仍可重试";
  if (bundle.staging_status === "expired") return "暂存已过期清理";
  if (bundle.import_conflict || bundle.import_blocking_reason === "destination_exists") {
    if ((bundle.import_conflict_count ?? 0) > 0) {
      return `有 ${bundle.import_conflict_count} 个目标文件已存在，当前不会覆盖`;
    }
    return "同名资料已存在，当前不会覆盖";
  }
  if (bundle.import_blocking_reason === "destination_file_exists") {
    return "目标位置已有同名文件，当前不会覆盖";
  }
  if (bundle.can_import_now) return "可导入，导入前仍需要确认";
  if (bundle.import_allowed) return "已暂存，等待本机应用申请导入";
  return bundle.import_blocking_reason === "not_importable"
    ? "已暂存，只能保存，不能直接导入"
    : "已暂存，但缺少导入权限或包含敏感内容";
}

export function bundleImportPlanLine(bundle: ReceivedBundleDto) {
  if (bundle.staging_status !== "saved" && bundle.staging_status !== "import_failed") return null;
  if (bundle.import_conflict_count > 0) {
    const conflicted = bundle.import_plan_files
      .filter((file) => file.destination_exists)
      .slice(0, 2)
      .map((file) => file.manifest_path);
    return conflicted.length > 0
      ? `冲突：${conflicted.join("、")}${bundle.import_conflict_count > conflicted.length ? " 等" : ""}`
      : `冲突文件 ${bundle.import_conflict_count} 个`;
  }
  if (bundle.can_import_now && bundle.import_plan_files.length > 0) {
    return `将导入 ${bundle.import_plan_files.length} 个文件`;
  }
  if (bundle.import_destination && bundle.import_allowed) {
    return `目标：${bundle.import_destination}`;
  }
  return null;
}

export function markBundleDeleted(bundle: ReceivedBundleDto): ReceivedBundleDto {
  return {
    ...bundle,
    staging_status: "deleted",
    can_import_now: false
  };
}

export function markBundleImportFailed(bundle: ReceivedBundleDto): ReceivedBundleDto {
  return {
    ...bundle,
    staging_status: "import_failed",
    can_import_now: bundle.import_allowed && !bundle.import_conflict && (bundle.import_conflict_count ?? 0) === 0
  };
}

export function bundleTypeLabel(type: string) {
  switch (type) {
    case "skill":
      return "Skill";
    case "session":
      return "Session";
    case "workspace":
      return "Workspace";
    case "agent_profile":
      return "Agent profile";
    case "config_snapshot":
      return "Config";
    default:
      return type;
  }
}
