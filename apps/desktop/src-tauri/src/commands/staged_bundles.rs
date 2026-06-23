use std::path::Path;
use std::time::SystemTime;

use nekodrop_storage::{
    delete_staged_bundle as delete_staged_bundle_storage,
    import_staged_bundle as import_staged_bundle_storage, import_staged_bundle_with_strategy,
    list_bundle_import_receipts, list_staged_bundles as list_staged_bundles_storage,
    plan_bundle_import_rollback, plan_staged_bundle_import, prune_staged_bundles_older_than,
    rollback_bundle_import, BundleImportConflictStrategy, BundleImportPlan, BundleImportPolicy,
    BundleImportReceipt, StagedBundle,
};

use super::bundle_helpers::bundle_type_label;
use super::dto::{BundleImportPlanFileDto, ReceivedBundleDto};

fn staged_bundle_to_dto(
    staged: &StagedBundle,
    import_root: &Path,
) -> Result<ReceivedBundleDto, String> {
    let manifest = &staged.detected.manifest;
    if let Some(receipt) = find_latest_bundle_import_receipt(import_root, &manifest.bundle_id)? {
        if let Some(mut dto) = receipt_to_imported_bundle_dto(&receipt)? {
            dto.file_count = manifest.summary.file_count;
            dto.total_bytes = manifest.summary.total_bytes;
            dto.staging_path = staged.staging_path.display().to_string();
            return Ok(dto);
        }
    }
    let plan = plan_staged_bundle_import(&staged.staging_path, import_root).ok();
    let plan_files = plan
        .as_ref()
        .map(import_plan_files_to_dto)
        .unwrap_or_default();
    let conflict_count = plan
        .as_ref()
        .map(|plan| plan.conflict_count)
        .unwrap_or_default();
    let strategies = import_conflict_strategies(plan.as_ref());
    Ok(ReceivedBundleDto {
        bundle_id: manifest.bundle_id.clone(),
        bundle_type: bundle_type_label(manifest.bundle_type).to_string(),
        display_name: manifest.display_name.clone(),
        source_app: manifest.source_app.clone(),
        file_count: manifest.summary.file_count,
        total_bytes: manifest.summary.total_bytes,
        staging_path: staged.staging_path.display().to_string(),
        import_allowed: staged.detected.import_policy == BundleImportPolicy::ImportAllowed,
        staging_status: "saved".to_string(),
        can_import_now: plan
            .as_ref()
            .map(|plan| plan.can_import_now)
            .unwrap_or(false),
        import_path: None,
        import_destination: plan
            .as_ref()
            .map(|plan| plan.destination_path.display().to_string()),
        import_conflict: plan
            .as_ref()
            .map(|plan| plan.destination_exists || plan.conflict_count > 0)
            .unwrap_or(false),
        import_blocking_reason: plan.and_then(|plan| plan.blocking_reason),
        import_plan_files: plan_files,
        import_conflict_count: conflict_count,
        import_conflict_strategies: strategies,
        imported_with_strategy: None,
        import_skipped_file_count: 0,
        import_receipt_path: None,
        imported_manifest_paths: Vec::new(),
        skipped_manifest_paths: Vec::new(),
        rollback_file_count: 0,
        can_rollback_now: false,
        rollback_blocking_reason: None,
        rolled_back_file_count: 0,
    })
}

pub(super) fn list_staged_bundle_dtos_at(
    staging_root: &Path,
    import_root: &Path,
) -> Result<Vec<ReceivedBundleDto>, String> {
    list_staged_bundles_storage(staging_root)
        .map_err(|error| error.to_string())
        .map(|bundles| {
            bundles
                .iter()
                .map(|bundle| staged_bundle_to_dto(bundle, import_root))
                .collect::<Result<Vec<_>, _>>()
        })
        .and_then(|result| result)
}

pub(super) fn find_staged_bundle_dto_at(
    staging_root: &Path,
    import_root: &Path,
    bundle_id: &str,
) -> Result<Option<ReceivedBundleDto>, String> {
    Ok(list_staged_bundle_dtos_at(staging_root, import_root)?
        .into_iter()
        .find(|bundle| bundle.bundle_id == bundle_id))
}

pub(super) fn prune_staged_bundle_dtos_at(
    staging_root: &Path,
    cutoff: SystemTime,
) -> Result<Vec<String>, String> {
    prune_staged_bundles_older_than(staging_root, cutoff).map_err(|error| error.to_string())
}

pub(super) fn delete_staged_bundle_at(
    staging_root: &Path,
    bundle_id: &str,
) -> Result<bool, String> {
    validate_safe_bundle_id(bundle_id)?;
    delete_staged_bundle_storage(staging_root, bundle_id).map_err(|error| error.to_string())
}

pub(super) fn import_staged_bundle_at(
    staging_root: &Path,
    import_root: &Path,
    bundle_id: &str,
) -> Result<ReceivedBundleDto, String> {
    import_staged_bundle_with_strategy_at(
        staging_root,
        import_root,
        bundle_id,
        BundleImportConflictStrategy::Reject,
    )
}

pub(super) fn import_staged_bundle_with_strategy_at(
    staging_root: &Path,
    import_root: &Path,
    bundle_id: &str,
    conflict_strategy: BundleImportConflictStrategy,
) -> Result<ReceivedBundleDto, String> {
    validate_safe_bundle_id(bundle_id)?;
    let staged_path = staging_root.join(bundle_id);
    let imported = if conflict_strategy == BundleImportConflictStrategy::Reject {
        import_staged_bundle_storage(&staged_path, import_root)
    } else {
        import_staged_bundle_with_strategy(&staged_path, import_root, conflict_strategy)
    }
    .map_err(|error| error.to_string())?;
    let import_path = imported.destination_path.display().to_string();
    let rollback_plan =
        plan_bundle_import_rollback(&imported.import_receipt).map_err(|error| error.to_string())?;
    Ok(ReceivedBundleDto {
        bundle_id: imported.bundle_id,
        bundle_type: bundle_type_label(imported.bundle_type).to_string(),
        display_name: imported.display_name,
        source_app: imported.source_app,
        file_count: imported.file_count,
        total_bytes: imported.total_bytes,
        staging_path: staged_path.display().to_string(),
        import_allowed: true,
        staging_status: "imported".to_string(),
        can_import_now: false,
        import_path: Some(import_path.clone()),
        import_destination: Some(import_path),
        import_conflict: false,
        import_blocking_reason: None,
        import_plan_files: Vec::new(),
        import_conflict_count: 0,
        import_conflict_strategies: Vec::new(),
        imported_with_strategy: Some(
            import_conflict_strategy_label(imported.conflict_strategy).to_string(),
        ),
        import_skipped_file_count: imported.skipped_file_count,
        import_receipt_path: Some(imported.import_receipt_path.display().to_string()),
        imported_manifest_paths: imported.imported_manifest_paths,
        skipped_manifest_paths: imported.skipped_manifest_paths,
        rollback_file_count: rollback_plan.files.len(),
        can_rollback_now: rollback_plan.can_rollback_now,
        rollback_blocking_reason: rollback_plan.blocking_reason,
        rolled_back_file_count: 0,
    })
}

pub(super) fn rollback_imported_bundle_at(
    import_root: &Path,
    bundle_id: &str,
) -> Result<ReceivedBundleDto, String> {
    validate_safe_bundle_id(bundle_id)?;
    let receipt = find_latest_bundle_import_receipt(import_root, bundle_id)?
        .ok_or_else(|| format!("没有找到资料包导入记录: {bundle_id}"))?;
    let plan = plan_bundle_import_rollback(&receipt).map_err(|error| error.to_string())?;
    if !plan.can_rollback_now {
        return Err(format!(
            "资料包不能撤回: {}",
            plan.blocking_reason
                .as_deref()
                .unwrap_or("rollback_blocked")
        ));
    }
    let rolled_back = rollback_bundle_import(&receipt).map_err(|error| error.to_string())?;
    Ok(ReceivedBundleDto {
        bundle_id: receipt.bundle_id,
        bundle_type: bundle_type_label(receipt.bundle_type).to_string(),
        display_name: receipt.display_name,
        source_app: receipt.source_app,
        file_count: plan.files.len(),
        total_bytes: 0,
        staging_path: String::new(),
        import_allowed: false,
        staging_status: "rolled_back".to_string(),
        can_import_now: false,
        import_path: Some(rolled_back.destination_path.display().to_string()),
        import_destination: Some(rolled_back.destination_path.display().to_string()),
        import_conflict: false,
        import_blocking_reason: None,
        import_plan_files: Vec::new(),
        import_conflict_count: 0,
        import_conflict_strategies: Vec::new(),
        imported_with_strategy: Some(receipt.conflict_strategy),
        import_skipped_file_count: receipt.skipped_manifest_paths.len(),
        import_receipt_path: None,
        imported_manifest_paths: Vec::new(),
        skipped_manifest_paths: receipt.skipped_manifest_paths,
        rollback_file_count: 0,
        can_rollback_now: false,
        rollback_blocking_reason: Some("already_rolled_back".to_string()),
        rolled_back_file_count: rolled_back.removed_file_count,
    })
}

pub(super) fn latest_bundle_import_receipt_dto_at(
    import_root: &Path,
    bundle_id: &str,
) -> Result<Option<ReceivedBundleDto>, String> {
    validate_safe_bundle_id(bundle_id)?;
    let Some(receipt) = find_latest_bundle_import_receipt(import_root, bundle_id)? else {
        return Ok(None);
    };
    receipt_to_imported_bundle_dto(&receipt)
}

fn find_latest_bundle_import_receipt(
    import_root: &Path,
    bundle_id: &str,
) -> Result<Option<BundleImportReceipt>, String> {
    let receipts = list_bundle_import_receipts(import_root).map_err(|error| error.to_string())?;
    Ok(receipts
        .into_iter()
        .find(|receipt| receipt.bundle_id == bundle_id))
}

fn receipt_to_imported_bundle_dto(
    receipt: &BundleImportReceipt,
) -> Result<Option<ReceivedBundleDto>, String> {
    let rollback_plan = plan_bundle_import_rollback(receipt).map_err(|error| error.to_string())?;
    let rolled_back_file_count = rollback_plan
        .files
        .iter()
        .filter(|file| !file.exists)
        .count();
    let staging_status = if rolled_back_file_count > 0 && !rollback_plan.can_rollback_now {
        "rolled_back"
    } else {
        "imported"
    };
    Ok(Some(ReceivedBundleDto {
        bundle_id: receipt.bundle_id.clone(),
        bundle_type: bundle_type_label(receipt.bundle_type).to_string(),
        display_name: receipt.display_name.clone(),
        source_app: receipt.source_app.clone(),
        file_count: receipt.imported_manifest_paths.len() + receipt.skipped_manifest_paths.len(),
        total_bytes: 0,
        staging_path: String::new(),
        import_allowed: false,
        staging_status: staging_status.to_string(),
        can_import_now: false,
        import_path: Some(receipt.destination_path.clone()),
        import_destination: Some(receipt.destination_path.clone()),
        import_conflict: false,
        import_blocking_reason: None,
        import_plan_files: Vec::new(),
        import_conflict_count: 0,
        import_conflict_strategies: Vec::new(),
        imported_with_strategy: Some(receipt.conflict_strategy.clone()),
        import_skipped_file_count: receipt.skipped_manifest_paths.len(),
        import_receipt_path: None,
        imported_manifest_paths: receipt.imported_manifest_paths.clone(),
        skipped_manifest_paths: receipt.skipped_manifest_paths.clone(),
        rollback_file_count: rollback_plan.files.len(),
        can_rollback_now: rollback_plan.can_rollback_now,
        rollback_blocking_reason: rollback_plan.blocking_reason,
        rolled_back_file_count,
    }))
}

fn import_plan_files_to_dto(plan: &BundleImportPlan) -> Vec<BundleImportPlanFileDto> {
    plan.files
        .iter()
        .map(|file| BundleImportPlanFileDto {
            manifest_path: file.manifest_path.clone(),
            size: file.size,
            sha256: file.sha256.clone(),
            destination_path: file.destination_path.display().to_string(),
            destination_exists: file.destination_exists,
        })
        .collect()
}

pub(super) fn parse_import_conflict_strategy(
    value: Option<&str>,
) -> Result<BundleImportConflictStrategy, String> {
    match value.unwrap_or("reject") {
        "reject" => Ok(BundleImportConflictStrategy::Reject),
        "rename" => Ok(BundleImportConflictStrategy::Rename),
        "skip_conflicts" => Ok(BundleImportConflictStrategy::SkipConflicts),
        other => Err(format!("不支持的资料包导入策略：{other}")),
    }
}

pub(super) fn import_conflict_strategy_label(
    strategy: BundleImportConflictStrategy,
) -> &'static str {
    strategy.as_str()
}

fn import_conflict_strategies(plan: Option<&BundleImportPlan>) -> Vec<String> {
    let Some(plan) = plan else {
        return Vec::new();
    };
    if !plan.import_allowed {
        return Vec::new();
    }
    if !plan.destination_exists && plan.conflict_count == 0 {
        return vec!["reject".to_string()];
    }
    vec![
        "reject".to_string(),
        "rename".to_string(),
        "skip_conflicts".to_string(),
    ]
}

pub(super) fn validate_safe_bundle_id(bundle_id: &str) -> Result<(), String> {
    let trimmed = bundle_id.trim();
    if trimmed.is_empty()
        || trimmed != bundle_id
        || bundle_id.contains('/')
        || bundle_id.contains('\\')
        || bundle_id.contains("..")
        || bundle_id.contains(':')
        || bundle_id.contains('\0')
    {
        return Err(format!("bundle_id 不安全: {bundle_id}"));
    }
    Ok(())
}
