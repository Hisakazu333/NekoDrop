// Bundle 暂存 / 导入 / 回滚 / 清理，按职责拆分：
// `types` 领域类型与常量；`fsutil` 共享 JSON 读写、bundle_id 与错误映射工具；
// `detect` bundle 目录识别与完整性校验；`staging` 手动创建与暂存、列表；
// `cleanup` 删除与过期清理；`import` 导入执行、导入计划与回执；
// `rollback` 导入回滚。对外路径保持 `nekodrop_storage::bundle::*` 不变。
mod cleanup;
mod detect;
mod fsutil;
mod import;
mod rollback;
mod staging;
mod types;

pub use cleanup::{delete_staged_bundle, prune_staged_bundles_older_than};
pub use detect::detect_bundle_directory;
pub use import::{
    import_staged_bundle, import_staged_bundle_with_strategy, list_bundle_import_receipts,
    plan_staged_bundle_import,
};
pub use rollback::{plan_bundle_import_rollback, rollback_bundle_import};
pub use staging::{create_manual_bundle_directory, list_staged_bundles, stage_bundle_directory};
pub use types::{
    BundleImportConflictStrategy, BundleImportPlan, BundleImportPlanFile, BundleImportPolicy,
    BundleImportReceipt, BundleImportRollbackPlan, BundleImportRollbackPlanFile, DetectedBundle,
    ImportedBundle, ManualBundleCreateRequest, RolledBackBundleImport, StagedBundle,
    BUNDLE_IMPORT_RECEIPT_SCHEMA_V1,
};

#[cfg(test)]
mod tests;
