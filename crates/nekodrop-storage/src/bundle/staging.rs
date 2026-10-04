// bundle 创建与暂存：从源目录手动打包、校验后复制进 staging 根、
// 按 bundle_id 列出已暂存 bundle（条目级隔离，损坏条目跳过）。
use std::{collections::BTreeMap, fs, path::Path};

use nekodrop_core::{NekoDropError, NekoDropResult};
use nekolink_protocol::{
    BundleChecksums, BundleCompatibility, BundleFile, BundleManifest, BundleSummary, Capability,
    BUNDLE_CHECKSUM_SHA256, BUNDLE_SCHEMA_V1,
};
use walkdir::WalkDir;

use crate::checksum::sha256_file;

use super::detect::detect_bundle_directory;
use super::fsutil::{
    path_to_bundle_manifest_path, validate_bundle_id_for_staging, write_json_file,
};
use super::types::{ManualBundleCreateRequest, StagedBundle};

pub fn create_manual_bundle_directory(
    request: ManualBundleCreateRequest,
) -> NekoDropResult<StagedBundle> {
    validate_bundle_id_for_staging(&request.bundle_id)?;
    if request.display_name.trim().is_empty() {
        return Err(NekoDropError::Storage(
            "bundle display_name is required".into(),
        ));
    }
    if request.source_app.trim().is_empty() {
        return Err(NekoDropError::Storage(
            "bundle source_app is required".into(),
        ));
    }
    if !request.source_path.is_dir() {
        return Err(NekoDropError::Storage(format!(
            "bundle source must be a directory: {}",
            request.source_path.display()
        )));
    }

    let bundle_root = request.output_root.join(&request.bundle_id);
    if bundle_root.exists() {
        fs::remove_dir_all(&bundle_root).map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to replace bundle directory {}: {error}",
                bundle_root.display()
            ))
        })?;
    }
    fs::create_dir_all(bundle_root.join("files")).map_err(|error| {
        NekoDropError::Storage(format!(
            "failed to create bundle directory {}: {error}",
            bundle_root.display()
        ))
    })?;

    let mut bundle_files = Vec::new();
    let mut checksums = BTreeMap::new();
    for entry in WalkDir::new(&request.source_path)
        .follow_links(false)
        .sort_by_file_name()
    {
        let entry = entry.map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to scan bundle source {}: {error}",
                request.source_path.display()
            ))
        })?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to read bundle source {}: {error}",
                path.display()
            ))
        })?;
        if metadata.file_type().is_symlink() {
            return Err(NekoDropError::Storage(format!(
                "bundle source symlinks are not supported: {}",
                path.display()
            )));
        }
        if metadata.is_dir() {
            continue;
        }
        if !metadata.is_file() {
            return Err(NekoDropError::Storage(format!(
                "unsupported bundle source entry: {}",
                path.display()
            )));
        }

        let relative = path.strip_prefix(&request.source_path).map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to normalize bundle source {}: {error}",
                path.display()
            ))
        })?;
        let bundle_path = path_to_bundle_manifest_path(relative)?;
        let destination = bundle_root.join(&bundle_path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                NekoDropError::Storage(format!(
                    "failed to create bundle payload directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        fs::copy(path, &destination).map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to copy bundle payload {}: {error}",
                path.display()
            ))
        })?;
        let checksum = sha256_file(&destination)?;
        bundle_files.push(BundleFile {
            path: bundle_path.clone(),
            size: metadata.len(),
            sha256: checksum.value.clone(),
            role: "payload".to_string(),
        });
        checksums.insert(bundle_path, checksum.value);
    }

    if bundle_files.is_empty() {
        return Err(NekoDropError::Storage(
            "bundle source must contain at least one file".into(),
        ));
    }

    let manifest = BundleManifest {
        schema: BUNDLE_SCHEMA_V1.to_string(),
        bundle_id: request.bundle_id,
        bundle_type: request.bundle_type,
        display_name: request.display_name,
        source_app: request.source_app,
        created_at: request.created_at,
        sender: request.sender,
        compatibility: BundleCompatibility {
            min_nekolink_version: 1,
            required_capabilities: vec![Capability::BundleTransfer],
        },
        summary: BundleSummary {
            file_count: bundle_files.len(),
            total_bytes: bundle_files.iter().map(|file| file.size).sum(),
        },
        files: bundle_files,
    };
    let checksum_index = BundleChecksums {
        algorithm: BUNDLE_CHECKSUM_SHA256.to_string(),
        files: checksums,
    };

    write_json_file(&bundle_root.join("bundle.json"), &manifest)?;
    write_json_file(&bundle_root.join("checksums.json"), &checksum_index)?;
    if let Some(permissions) = request.permissions {
        write_json_file(&bundle_root.join("permissions.json"), &permissions)?;
    }

    let detected = detect_bundle_directory(&bundle_root)?.ok_or_else(|| {
        NekoDropError::Storage(format!(
            "created bundle is not detectable: {}",
            bundle_root.display()
        ))
    })?;
    Ok(StagedBundle {
        staging_path: bundle_root,
        detected,
    })
}

pub fn stage_bundle_directory(
    source_root: &Path,
    staging_root: &Path,
) -> NekoDropResult<StagedBundle> {
    let detected = detect_bundle_directory(source_root)?
        .ok_or_else(|| NekoDropError::Storage("bundle.json is required for staging".into()))?;
    validate_bundle_id_for_staging(&detected.manifest.bundle_id)?;

    let staging_path = staging_root.join(&detected.manifest.bundle_id);
    if staging_path.exists() {
        fs::remove_dir_all(&staging_path).map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to replace staged bundle {}: {error}",
                staging_path.display()
            ))
        })?;
    }
    fs::create_dir_all(staging_path.join("files")).map_err(|error| {
        NekoDropError::Storage(format!(
            "failed to create staged bundle {}: {error}",
            staging_path.display()
        ))
    })?;

    copy_required_root_file(source_root, &staging_path, "bundle.json")?;
    copy_required_root_file(source_root, &staging_path, "checksums.json")?;
    if detected.permissions.is_some() {
        copy_required_root_file(source_root, &staging_path, "permissions.json")?;
    }
    for file in &detected.manifest.files {
        copy_bundle_file(source_root, &staging_path, &file.path)?;
    }

    let detected = detect_bundle_directory(&staging_path)?.ok_or_else(|| {
        NekoDropError::Storage(format!(
            "staged bundle is not detectable: {}",
            staging_path.display()
        ))
    })?;

    Ok(StagedBundle {
        staging_path,
        detected,
    })
}

pub fn list_staged_bundles(staging_root: &Path) -> NekoDropResult<Vec<StagedBundle>> {
    if !staging_root.exists() {
        return Ok(Vec::new());
    }
    let metadata = fs::symlink_metadata(staging_root).map_err(|error| {
        NekoDropError::Storage(format!(
            "failed to read bundle staging root {}: {error}",
            staging_root.display()
        ))
    })?;
    if !metadata.is_dir() {
        return Err(NekoDropError::Storage(format!(
            "bundle staging root is not a directory: {}",
            staging_root.display()
        )));
    }

    let mut staged_bundles = Vec::new();
    for entry in fs::read_dir(staging_root).map_err(|error| {
        NekoDropError::Storage(format!(
            "failed to read bundle staging root {}: {error}",
            staging_root.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            NekoDropError::Storage(format!("failed to read staged bundle entry: {error}"))
        })?;
        let file_type = entry.file_type().map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to read staged bundle file type {}: {error}",
                entry.path().display()
            ))
        })?;
        if !file_type.is_dir() {
            continue;
        }

        let bundle_id = entry.file_name().to_string_lossy().into_owned();
        // 条目级隔离：单个损坏或外来目录不能让整个 staging 列表失败，
        // 否则其余完好 bundle 全部不可见，过期清理也会失效。
        let detected = match validate_bundle_id_for_staging(&bundle_id)
            .and_then(|()| detect_bundle_directory(&entry.path()))
        {
            Ok(Some(detected)) => detected,
            Ok(None) => {
                eprintln!(
                    "nekodrop-storage: skipping staged entry without bundle.json: {}",
                    entry.path().display()
                );
                continue;
            }
            Err(error) => {
                eprintln!(
                    "nekodrop-storage: skipping unreadable staged bundle {} ({error})",
                    entry.path().display()
                );
                continue;
            }
        };
        staged_bundles.push(StagedBundle {
            staging_path: entry.path(),
            detected,
        });
    }

    staged_bundles.sort_by(|left, right| {
        left.detected
            .manifest
            .bundle_id
            .cmp(&right.detected.manifest.bundle_id)
    });
    Ok(staged_bundles)
}

fn copy_required_root_file(
    source_root: &Path,
    staging_path: &Path,
    name: &str,
) -> NekoDropResult<()> {
    fs::copy(source_root.join(name), staging_path.join(name)).map_err(|error| {
        NekoDropError::Storage(format!("failed to copy bundle root file {name}: {error}"))
    })?;
    Ok(())
}

fn copy_bundle_file(
    source_root: &Path,
    staging_path: &Path,
    relative_path: &str,
) -> NekoDropResult<()> {
    let source = source_root.join(relative_path);
    let destination = staging_path.join(relative_path);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to create staged bundle directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    fs::copy(&source, &destination).map_err(|error| {
        NekoDropError::Storage(format!(
            "failed to copy bundle file {}: {error}",
            relative_path
        ))
    })?;
    Ok(())
}
