// bundle 目录识别与完整性校验：根条目白名单、manifest 路径与
// payload 校验和核对、未声明 payload 拒绝、导入策略推导。
use std::{collections::BTreeSet, fs, path::Path};

use nekodrop_core::{NekoDropError, NekoDropResult};
use nekolink_protocol::{BundleChecksums, BundleManifest, BundlePermissions};

use crate::checksum::sha256_file;

use super::fsutil::{
    map_protocol_error, path_to_bundle_manifest_path, protocol_to_storage_error, read_json_file,
};
use super::types::{BundleImportPolicy, DetectedBundle};

pub fn detect_bundle_directory(root: &Path) -> NekoDropResult<Option<DetectedBundle>> {
    let bundle_path = root.join("bundle.json");
    if !bundle_path.exists() {
        return Ok(None);
    }

    reject_unknown_root_entries(root)?;

    let manifest: BundleManifest = read_json_file(&bundle_path)?;
    map_protocol_error(manifest.validate())?;
    validate_manifest_payload_paths(&manifest)?;

    let checksums: BundleChecksums = read_json_file(&root.join("checksums.json"))?;
    map_protocol_error(checksums.validate_against(&manifest))?;

    let permissions_path = root.join("permissions.json");
    let permissions = if permissions_path.exists() {
        let permissions: BundlePermissions = read_json_file(&permissions_path)?;
        map_protocol_error(permissions.validate())?;
        Some(permissions)
    } else {
        None
    };

    verify_payload_files(root, &manifest)?;
    reject_undeclared_payload_files(root, &manifest)?;

    let import_policy = match &permissions {
        Some(permissions)
            if permissions
                .can_import()
                .map_err(protocol_to_storage_error)? =>
        {
            BundleImportPolicy::ImportAllowed
        }
        _ => BundleImportPolicy::SaveOnly,
    };

    Ok(Some(DetectedBundle {
        root_path: root.to_path_buf(),
        manifest,
        checksums,
        permissions,
        import_policy,
    }))
}

fn reject_unknown_root_entries(root: &Path) -> NekoDropResult<()> {
    for entry in fs::read_dir(root).map_err(|error| {
        NekoDropError::Storage(format!(
            "failed to read bundle root {}: {error}",
            root.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            NekoDropError::Storage(format!("failed to read bundle root entry: {error}"))
        })?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let is_allowed = matches!(
            name.as_ref(),
            "bundle.json" | "checksums.json" | "permissions.json" | "files"
        );
        if !is_allowed {
            return Err(NekoDropError::Storage(format!(
                "unknown bundle root entry: {name}"
            )));
        }
    }
    let files_dir = root.join("files");
    if !files_dir.is_dir() {
        return Err(NekoDropError::Storage(
            "bundle files/ directory is required".into(),
        ));
    }
    Ok(())
}

fn validate_manifest_payload_paths(manifest: &BundleManifest) -> NekoDropResult<()> {
    for file in &manifest.files {
        if !file.path.starts_with("files/") {
            return Err(NekoDropError::Storage(format!(
                "bundle payload path must be under files/: {}",
                file.path
            )));
        }
    }
    Ok(())
}

fn verify_payload_files(root: &Path, manifest: &BundleManifest) -> NekoDropResult<()> {
    for file in &manifest.files {
        let payload_path = root.join(&file.path);
        let metadata = fs::symlink_metadata(&payload_path).map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to read bundle file {}: {error}",
                payload_path.display()
            ))
        })?;
        if metadata.file_type().is_symlink() {
            return Err(NekoDropError::Storage(format!(
                "bundle payload symlinks are not supported: {}",
                file.path
            )));
        }
        if !metadata.is_file() {
            return Err(NekoDropError::Storage(format!(
                "bundle payload is not a file: {}",
                file.path
            )));
        }
        if metadata.len() != file.size {
            return Err(NekoDropError::Storage(format!(
                "bundle file size mismatch for {}: {} != {}",
                file.path,
                metadata.len(),
                file.size
            )));
        }
        let checksum = sha256_file(&payload_path)?;
        if checksum.value != file.sha256 {
            return Err(NekoDropError::Storage(format!(
                "checksum mismatch for {}",
                file.path
            )));
        }
    }
    Ok(())
}

fn reject_undeclared_payload_files(root: &Path, manifest: &BundleManifest) -> NekoDropResult<()> {
    let declared = manifest
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<BTreeSet<_>>();
    let files_root = root.join("files");
    reject_undeclared_payload_files_in_dir(&files_root, &files_root, &declared)
}

fn reject_undeclared_payload_files_in_dir(
    files_root: &Path,
    current_dir: &Path,
    declared: &BTreeSet<&str>,
) -> NekoDropResult<()> {
    for entry in fs::read_dir(current_dir).map_err(|error| {
        NekoDropError::Storage(format!(
            "failed to read bundle payload directory {}: {error}",
            current_dir.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            NekoDropError::Storage(format!("failed to read bundle payload entry: {error}"))
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to read bundle payload file type {}: {error}",
                path.display()
            ))
        })?;
        if file_type.is_symlink() {
            return Err(NekoDropError::Storage(format!(
                "bundle payload symlinks are not supported: {}",
                path.display()
            )));
        }
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to read bundle payload metadata {}: {error}",
                path.display()
            ))
        })?;
        if metadata.is_dir() {
            reject_undeclared_payload_files_in_dir(files_root, &path, declared)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(NekoDropError::Storage(format!(
                "unsupported bundle payload entry: {}",
                path.display()
            )));
        }
        let relative_path = path.strip_prefix(files_root).map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to normalize bundle payload path {}: {error}",
                path.display()
            ))
        })?;
        let manifest_path = path_to_bundle_manifest_path(relative_path)?;
        if !declared.contains(manifest_path.as_str()) {
            return Err(NekoDropError::Storage(format!(
                "undeclared bundle payload file: {manifest_path}"
            )));
        }
    }
    Ok(())
}
