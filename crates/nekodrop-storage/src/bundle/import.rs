// bundle 导入：导入计划（不改盘）、按冲突策略执行导入、导入回执
// 写入 / 读取 / 校验与回执列表。
use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use nekodrop_core::{NekoDropError, NekoDropResult};

use super::detect::detect_bundle_directory;
use super::fsutil::{read_json_file, validate_bundle_id_for_staging, write_json_file};
use super::types::{
    BundleImportConflictStrategy, BundleImportPlan, BundleImportPlanFile, BundleImportPolicy,
    BundleImportReceipt, ImportedBundle, BUNDLE_IMPORT_RECEIPT_SCHEMA_V1,
};

const BUNDLE_IMPORT_RECEIPTS_DIR: &str = ".nekodrop_import_receipts";

pub fn import_staged_bundle(
    staged_bundle_root: &Path,
    import_root: &Path,
) -> NekoDropResult<ImportedBundle> {
    import_staged_bundle_with_strategy(
        staged_bundle_root,
        import_root,
        BundleImportConflictStrategy::Reject,
    )
}

pub fn import_staged_bundle_with_strategy(
    staged_bundle_root: &Path,
    import_root: &Path,
    conflict_strategy: BundleImportConflictStrategy,
) -> NekoDropResult<ImportedBundle> {
    let plan = plan_staged_bundle_import(staged_bundle_root, import_root)?;
    if !plan.import_allowed {
        return Err(NekoDropError::Storage(format!(
            "bundle is not importable: {}",
            plan.bundle_id
        )));
    }
    if plan.destination_exists && conflict_strategy == BundleImportConflictStrategy::Reject {
        return Err(NekoDropError::Storage(format!(
            "bundle import destination already exists: {}",
            plan.destination_path.display()
        )));
    }

    fs::create_dir_all(import_root).map_err(|error| {
        NekoDropError::Storage(format!(
            "failed to create bundle import root {}: {error}",
            import_root.display()
        ))
    })?;

    let detected = detect_bundle_directory(staged_bundle_root)?.ok_or_else(|| {
        NekoDropError::Storage(format!(
            "staged bundle is missing bundle.json: {}",
            staged_bundle_root.display()
        ))
    })?;

    let destination_path = match conflict_strategy {
        BundleImportConflictStrategy::Reject | BundleImportConflictStrategy::SkipConflicts => {
            plan.destination_path.clone()
        }
        BundleImportConflictStrategy::Rename => unique_bundle_import_destination(
            import_root,
            &plan.bundle_id,
            plan.destination_exists || plan.conflict_count > 0,
        ),
    };
    if destination_path.exists() && conflict_strategy == BundleImportConflictStrategy::Reject {
        return Err(NekoDropError::Storage(format!(
            "bundle import destination already exists: {}",
            destination_path.display()
        )));
    }

    let temp_path = import_root.join(format!("{}.importing", plan.bundle_id));
    if temp_path.exists() {
        fs::remove_dir_all(&temp_path).map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to clean stale bundle import temp {}: {error}",
                temp_path.display()
            ))
        })?;
    }

    let write_root = if conflict_strategy == BundleImportConflictStrategy::SkipConflicts {
        fs::create_dir_all(&destination_path).map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to create bundle import destination {}: {error}",
                destination_path.display()
            ))
        })?;
        destination_path.clone()
    } else {
        fs::create_dir_all(&temp_path).map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to create bundle import temp {}: {error}",
                temp_path.display()
            ))
        })?;
        temp_path.clone()
    };

    let mut imported_manifest_paths = Vec::new();
    let mut skipped_manifest_paths = Vec::new();
    let copy_result = detected.manifest.files.iter().try_for_each(|file| {
        if conflict_strategy == BundleImportConflictStrategy::SkipConflicts
            && import_payload_destination(&write_root, &file.path)?.exists()
        {
            skipped_manifest_paths.push(file.path.clone());
            return Ok(());
        }
        copy_import_payload_file(staged_bundle_root, &write_root, &file.path)?;
        imported_manifest_paths.push(file.path.clone());
        Ok(())
    });
    if let Err(error) = copy_result {
        if conflict_strategy != BundleImportConflictStrategy::SkipConflicts {
            let _ = fs::remove_dir_all(&temp_path);
        }
        return Err(error);
    }

    if conflict_strategy != BundleImportConflictStrategy::SkipConflicts {
        fs::rename(&temp_path, &destination_path).map_err(|error| {
            let _ = fs::remove_dir_all(&temp_path);
            NekoDropError::Storage(format!(
                "failed to finalize bundle import {}: {error}",
                destination_path.display()
            ))
        })?;
    }

    let imported_at_ms = unique_suffix();
    let import_receipt = BundleImportReceipt {
        schema: BUNDLE_IMPORT_RECEIPT_SCHEMA_V1.to_string(),
        bundle_id: plan.bundle_id.clone(),
        bundle_type: plan.bundle_type,
        display_name: plan.display_name.clone(),
        source_app: plan.source_app.clone(),
        destination_path: destination_path.display().to_string(),
        conflict_strategy: conflict_strategy.as_str().to_string(),
        imported_manifest_paths: imported_manifest_paths.clone(),
        skipped_manifest_paths: skipped_manifest_paths.clone(),
        imported_at_ms,
    };
    let import_receipt_path = write_bundle_import_receipt(
        import_root,
        &plan.bundle_id,
        imported_at_ms,
        &import_receipt,
    )?;

    Ok(ImportedBundle {
        bundle_id: plan.bundle_id,
        bundle_type: plan.bundle_type,
        display_name: plan.display_name,
        source_app: plan.source_app,
        destination_path,
        file_count: plan.file_count,
        total_bytes: plan.total_bytes,
        conflict_strategy,
        skipped_file_count: skipped_manifest_paths.len(),
        imported_manifest_paths,
        skipped_manifest_paths,
        import_receipt_path,
        import_receipt,
    })
}

pub fn plan_staged_bundle_import(
    staged_bundle_root: &Path,
    import_root: &Path,
) -> NekoDropResult<BundleImportPlan> {
    let detected = detect_bundle_directory(staged_bundle_root)?.ok_or_else(|| {
        NekoDropError::Storage(format!(
            "staged bundle is missing bundle.json: {}",
            staged_bundle_root.display()
        ))
    })?;
    validate_bundle_id_for_staging(&detected.manifest.bundle_id)?;

    let destination_path = import_root.join(&detected.manifest.bundle_id);
    let import_allowed = detected.import_policy == BundleImportPolicy::ImportAllowed;
    let destination_exists = destination_path.exists();
    let files: Vec<BundleImportPlanFile> = detected
        .manifest
        .files
        .iter()
        .map(|file| {
            let destination_path = import_payload_destination(&destination_path, &file.path)?;
            Ok(BundleImportPlanFile {
                manifest_path: file.path.clone(),
                size: file.size,
                sha256: file.sha256.clone(),
                destination_exists: destination_path.exists(),
                destination_path,
            })
        })
        .collect::<NekoDropResult<Vec<_>>>()?;
    let conflict_count = files.iter().filter(|file| file.destination_exists).count();
    let blocking_reason = if !import_allowed {
        Some("not_importable".to_string())
    } else if destination_exists {
        Some("destination_exists".to_string())
    } else if conflict_count > 0 {
        Some("destination_file_exists".to_string())
    } else {
        None
    };

    Ok(BundleImportPlan {
        bundle_id: detected.manifest.bundle_id,
        bundle_type: detected.manifest.bundle_type,
        display_name: detected.manifest.display_name,
        source_app: detected.manifest.source_app,
        destination_path,
        file_count: detected.manifest.summary.file_count,
        total_bytes: detected.manifest.summary.total_bytes,
        import_allowed,
        can_import_now: import_allowed && !destination_exists && conflict_count == 0,
        destination_exists,
        blocking_reason,
        files,
        conflict_count,
    })
}

pub fn list_bundle_import_receipts(import_root: &Path) -> NekoDropResult<Vec<BundleImportReceipt>> {
    let receipts_root = bundle_import_receipts_root(import_root);
    if !receipts_root.exists() {
        return Ok(Vec::new());
    }
    let metadata = fs::symlink_metadata(&receipts_root).map_err(|error| {
        NekoDropError::Storage(format!(
            "failed to read bundle import receipts root {}: {error}",
            receipts_root.display()
        ))
    })?;
    if !metadata.is_dir() {
        return Err(NekoDropError::Storage(format!(
            "bundle import receipts root is not a directory: {}",
            receipts_root.display()
        )));
    }

    let mut receipts = Vec::new();
    for entry in fs::read_dir(&receipts_root).map_err(|error| {
        NekoDropError::Storage(format!(
            "failed to read bundle import receipts root {}: {error}",
            receipts_root.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to read bundle import receipt entry: {error}"
            ))
        })?;
        let path = entry.path();
        if entry
            .file_type()
            .map_err(|error| {
                NekoDropError::Storage(format!(
                    "failed to read bundle import receipt file type {}: {error}",
                    path.display()
                ))
            })?
            .is_file()
            && path.extension().and_then(|extension| extension.to_str()) == Some("json")
        {
            receipts.push(read_bundle_import_receipt(&path)?);
        }
    }
    receipts.sort_by(|left, right| {
        right
            .imported_at_ms
            .cmp(&left.imported_at_ms)
            .then_with(|| left.bundle_id.cmp(&right.bundle_id))
    });
    Ok(receipts)
}

fn write_bundle_import_receipt(
    import_root: &Path,
    bundle_id: &str,
    imported_at_ms: u128,
    receipt: &BundleImportReceipt,
) -> NekoDropResult<PathBuf> {
    let receipts_root = bundle_import_receipts_root(import_root);
    fs::create_dir_all(&receipts_root).map_err(|error| {
        NekoDropError::Storage(format!(
            "failed to create bundle import receipts root {}: {error}",
            receipts_root.display()
        ))
    })?;
    let path = receipts_root.join(format!("{bundle_id}-{imported_at_ms}.json"));
    write_json_file(&path, receipt)?;
    Ok(path)
}

fn read_bundle_import_receipt(path: &Path) -> NekoDropResult<BundleImportReceipt> {
    let receipt: BundleImportReceipt = read_json_file(path)?;
    validate_bundle_import_receipt(&receipt)?;
    Ok(receipt)
}

pub(crate) fn validate_bundle_import_receipt(receipt: &BundleImportReceipt) -> NekoDropResult<()> {
    if receipt.schema != BUNDLE_IMPORT_RECEIPT_SCHEMA_V1 {
        return Err(NekoDropError::Storage(format!(
            "unsupported bundle import receipt schema: {}",
            receipt.schema
        )));
    }
    validate_bundle_id_for_staging(&receipt.bundle_id)?;
    if receipt.destination_path.trim().is_empty() {
        return Err(NekoDropError::Storage(
            "bundle import receipt destination_path is required".into(),
        ));
    }
    for manifest_path in receipt
        .imported_manifest_paths
        .iter()
        .chain(receipt.skipped_manifest_paths.iter())
    {
        let _ = import_payload_destination(Path::new("."), manifest_path)?;
    }
    Ok(())
}

fn bundle_import_receipts_root(import_root: &Path) -> PathBuf {
    import_root.join(BUNDLE_IMPORT_RECEIPTS_DIR)
}

fn unique_bundle_import_destination(
    import_root: &Path,
    bundle_id: &str,
    force_suffix: bool,
) -> PathBuf {
    let base = import_root.join(bundle_id);
    if !force_suffix && !base.exists() {
        return base;
    }
    for index in 2..=9999 {
        let candidate = import_root.join(format!("{bundle_id}-{index}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    import_root.join(format!("{bundle_id}-{}", unique_suffix()))
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn copy_import_payload_file(
    staged_bundle_root: &Path,
    import_temp_path: &Path,
    bundle_manifest_path: &str,
) -> NekoDropResult<()> {
    let source = staged_bundle_root.join(bundle_manifest_path);
    let destination = import_payload_destination(import_temp_path, bundle_manifest_path)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to create bundle import directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    fs::copy(&source, &destination).map_err(|error| {
        NekoDropError::Storage(format!(
            "failed to import bundle payload {}: {error}",
            bundle_manifest_path
        ))
    })?;
    Ok(())
}

pub(crate) fn import_payload_destination(
    import_root: &Path,
    bundle_manifest_path: &str,
) -> NekoDropResult<PathBuf> {
    let payload_path = bundle_manifest_path.strip_prefix("files/").ok_or_else(|| {
        NekoDropError::Storage(format!(
            "bundle import payload path must be under files/: {bundle_manifest_path}"
        ))
    })?;
    Ok(import_root.join(payload_path))
}
