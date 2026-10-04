// bundle 清理：按 bundle_id 删除单个暂存 bundle、按修改时间批量清理过期条目。
use std::{fs, path::Path, time::SystemTime};

use nekodrop_core::{NekoDropError, NekoDropResult};

use super::fsutil::validate_bundle_id_for_staging;
use super::staging::list_staged_bundles;

pub fn delete_staged_bundle(staging_root: &Path, bundle_id: &str) -> NekoDropResult<bool> {
    validate_bundle_id_for_staging(bundle_id)?;
    let staging_path = staging_root.join(bundle_id);
    if !staging_path.exists() {
        return Ok(false);
    }
    let metadata = fs::symlink_metadata(&staging_path).map_err(|error| {
        NekoDropError::Storage(format!(
            "failed to read staged bundle {}: {error}",
            staging_path.display()
        ))
    })?;
    if !metadata.is_dir() {
        return Err(NekoDropError::Storage(format!(
            "staged bundle is not a directory: {}",
            staging_path.display()
        )));
    }

    fs::remove_dir_all(&staging_path).map_err(|error| {
        NekoDropError::Storage(format!(
            "failed to delete staged bundle {}: {error}",
            staging_path.display()
        ))
    })?;
    Ok(true)
}

pub fn prune_staged_bundles_older_than(
    staging_root: &Path,
    cutoff: SystemTime,
) -> NekoDropResult<Vec<String>> {
    let mut pruned = Vec::new();
    for staged in list_staged_bundles(staging_root)? {
        let modified = fs::symlink_metadata(&staged.staging_path)
            .and_then(|metadata| metadata.modified())
            .map_err(|error| {
                NekoDropError::Storage(format!(
                    "failed to read staged bundle modified time {}: {error}",
                    staged.staging_path.display()
                ))
            })?;
        if modified < cutoff {
            let bundle_id = staged.detected.manifest.bundle_id;
            delete_staged_bundle(staging_root, &bundle_id)?;
            pruned.push(bundle_id);
        }
    }
    Ok(pruned)
}
