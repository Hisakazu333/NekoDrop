// bundle 导入回滚：依据回执生成回滚计划（目标缺失 / 文件缺失即阻塞），
// 仅删除本次导入的文件并清空随之空掉的目录。
use std::{
    fs,
    path::{Path, PathBuf},
};

use nekodrop_core::{NekoDropError, NekoDropResult};

use super::import::{import_payload_destination, validate_bundle_import_receipt};
use super::types::{
    BundleImportReceipt, BundleImportRollbackPlan, BundleImportRollbackPlanFile,
    RolledBackBundleImport,
};

pub fn plan_bundle_import_rollback(
    receipt: &BundleImportReceipt,
) -> NekoDropResult<BundleImportRollbackPlan> {
    validate_bundle_import_receipt(receipt)?;
    let destination_path = PathBuf::from(&receipt.destination_path);
    let files = receipt
        .imported_manifest_paths
        .iter()
        .map(|manifest_path| {
            let destination_path = import_payload_destination(&destination_path, manifest_path)?;
            Ok(BundleImportRollbackPlanFile {
                manifest_path: manifest_path.clone(),
                exists: destination_path.exists(),
                destination_path,
            })
        })
        .collect::<NekoDropResult<Vec<_>>>()?;
    let missing_count = files.iter().filter(|file| !file.exists).count();
    let blocking_reason = if !destination_path.exists() {
        Some("destination_missing".to_string())
    } else if missing_count > 0 {
        Some("imported_file_missing".to_string())
    } else {
        None
    };

    Ok(BundleImportRollbackPlan {
        bundle_id: receipt.bundle_id.clone(),
        destination_path,
        can_rollback_now: blocking_reason.is_none(),
        blocking_reason,
        files,
    })
}

pub fn rollback_bundle_import(
    receipt: &BundleImportReceipt,
) -> NekoDropResult<RolledBackBundleImport> {
    let plan = plan_bundle_import_rollback(receipt)?;
    if !plan.can_rollback_now {
        return Err(NekoDropError::Storage(format!(
            "bundle import rollback is blocked: {}",
            plan.blocking_reason
                .as_deref()
                .unwrap_or("rollback_blocked")
        )));
    }

    let mut removed_manifest_paths = Vec::new();
    for file in &plan.files {
        let metadata = fs::symlink_metadata(&file.destination_path).map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to read bundle import rollback file {}: {error}",
                file.destination_path.display()
            ))
        })?;
        if metadata.file_type().is_symlink() {
            return Err(NekoDropError::Storage(format!(
                "bundle import rollback refuses symlink: {}",
                file.destination_path.display()
            )));
        }
        if !metadata.is_file() {
            return Err(NekoDropError::Storage(format!(
                "bundle import rollback target is not a file: {}",
                file.destination_path.display()
            )));
        }
    }

    for file in &plan.files {
        fs::remove_file(&file.destination_path).map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to remove bundle import file {}: {error}",
                file.destination_path.display()
            ))
        })?;
        removed_manifest_paths.push(file.manifest_path.clone());
    }

    remove_empty_import_directories(&plan.destination_path, &plan.destination_path);

    Ok(RolledBackBundleImport {
        bundle_id: plan.bundle_id,
        destination_path: plan.destination_path,
        removed_file_count: removed_manifest_paths.len(),
        removed_manifest_paths,
    })
}

fn remove_empty_import_directories(root: &Path, current: &Path) -> bool {
    let Ok(entries) = fs::read_dir(current) else {
        return false;
    };
    let child_dirs = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    for child in child_dirs {
        remove_empty_import_directories(root, &child);
    }
    if current == root {
        return fs::remove_dir(current).is_ok();
    }
    fs::remove_dir(current).is_ok()
}
