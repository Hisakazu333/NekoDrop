use std::path::Path;
use std::time::SystemTime;

use nekodrop_storage::{
    delete_staged_bundle as delete_staged_bundle_storage,
    import_staged_bundle as import_staged_bundle_storage,
    list_staged_bundles as list_staged_bundles_storage, plan_staged_bundle_import,
    prune_staged_bundles_older_than, BundleImportPolicy, StagedBundle,
};

use super::bundle_helpers::bundle_type_label;
use super::dto::ReceivedBundleDto;

fn staged_bundle_to_dto(staged: &StagedBundle, import_root: &Path) -> ReceivedBundleDto {
    let manifest = &staged.detected.manifest;
    let plan = plan_staged_bundle_import(&staged.staging_path, import_root).ok();
    ReceivedBundleDto {
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
            .map(|plan| plan.destination_exists)
            .unwrap_or(false),
        import_blocking_reason: plan.and_then(|plan| plan.blocking_reason),
    }
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
                .collect()
        })
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
    validate_safe_bundle_id(bundle_id)?;
    let staged_path = staging_root.join(bundle_id);
    let imported = import_staged_bundle_storage(&staged_path, import_root)
        .map_err(|error| error.to_string())?;
    let import_path = imported.destination_path.display().to_string();
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
    })
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
