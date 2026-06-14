use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use nekodrop_core::{NekoDropError, NekoDropResult};
use nekolink_protocol::{BundleChecksums, BundleManifest, BundlePermissions, ProtocolError};

use crate::checksum::sha256_file;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleImportPolicy {
    ImportAllowed,
    SaveOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedBundle {
    pub root_path: PathBuf,
    pub manifest: BundleManifest,
    pub checksums: BundleChecksums,
    pub permissions: Option<BundlePermissions>,
    pub import_policy: BundleImportPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedBundle {
    pub staging_path: PathBuf,
    pub detected: DetectedBundle,
}

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

fn read_json_file<T: for<'de> serde::Deserialize<'de>>(path: &Path) -> NekoDropResult<T> {
    let bytes = fs::read(path).map_err(|error| {
        NekoDropError::Storage(format!("failed to read {}: {error}", path.display()))
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        NekoDropError::Storage(format!("failed to parse {}: {error}", path.display()))
    })
}

fn map_protocol_error(result: Result<(), ProtocolError>) -> NekoDropResult<()> {
    result.map_err(protocol_to_storage_error)
}

fn protocol_to_storage_error(error: ProtocolError) -> NekoDropError {
    NekoDropError::Storage(format!("invalid bundle: {}", error.message))
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

fn path_to_bundle_manifest_path(path: &Path) -> NekoDropResult<String> {
    let path = path.to_str().ok_or_else(|| {
        NekoDropError::Storage(format!("bundle path is not UTF-8: {}", path.display()))
    })?;
    Ok(format!("files/{}", path.replace('\\', "/")))
}

fn validate_bundle_id_for_staging(bundle_id: &str) -> NekoDropResult<()> {
    let trimmed = bundle_id.trim();
    if trimmed.is_empty()
        || trimmed != bundle_id
        || bundle_id.contains('/')
        || bundle_id.contains('\\')
        || bundle_id.contains("..")
        || bundle_id.contains(':')
        || bundle_id.contains('\0')
    {
        return Err(NekoDropError::Storage(format!(
            "bundle_id is not safe for staging: {bundle_id}"
        )));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, fs, path::PathBuf};

    use nekolink_protocol::{
        BundleChecksums, BundleCompatibility, BundleFile, BundleManifest, BundlePermissionScope,
        BundlePermissions, BundleSecretsPolicy, BundleSender, BundleSummary, BundleType,
        BundleWriteMode, BundleWritePermission, Capability, BUNDLE_CHECKSUM_SHA256,
        BUNDLE_SCHEMA_V1, PROTOCOL_VERSION,
    };

    use super::*;

    #[test]
    fn detects_valid_bundle_directory() {
        let dir = unique_temp_dir("bundle-detect-valid");
        let root = create_valid_bundle(&dir, false);

        let detected = detect_bundle_directory(&root).unwrap().unwrap();

        assert_eq!(detected.root_path, root);
        assert_eq!(detected.manifest.bundle_id, "bundle_1234567890");
        assert_eq!(detected.manifest.bundle_type, BundleType::Skill);
        assert_eq!(detected.import_policy, BundleImportPolicy::ImportAllowed);
        assert!(detected.permissions.is_some());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn returns_none_when_bundle_json_is_missing() {
        let dir = unique_temp_dir("bundle-detect-none");
        let root = dir.join("ordinary");
        fs::create_dir_all(root.join("files")).unwrap();
        fs::write(root.join("files").join("sample.txt"), b"ordinary").unwrap();

        let detected = detect_bundle_directory(&root).unwrap();

        assert!(detected.is_none());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_bundle_with_mismatched_payload_checksum() {
        let dir = unique_temp_dir("bundle-detect-bad-checksum");
        let root = create_valid_bundle(&dir, false);
        fs::write(root.join("files").join("content.bin"), b"jello bundle").unwrap();

        let error = detect_bundle_directory(&root).unwrap_err();

        assert!(error.to_string().contains("checksum mismatch"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn detects_bundle_without_permissions_as_save_only() {
        let dir = unique_temp_dir("bundle-detect-save-only");
        let root = create_valid_bundle(&dir, false);
        fs::remove_file(root.join("permissions.json")).unwrap();

        let detected = detect_bundle_directory(&root).unwrap().unwrap();

        assert_eq!(detected.import_policy, BundleImportPolicy::SaveOnly);
        assert!(detected.permissions.is_none());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn detects_bundle_with_secrets_as_save_only() {
        let dir = unique_temp_dir("bundle-detect-secrets");
        let root = create_valid_bundle(&dir, true);

        let detected = detect_bundle_directory(&root).unwrap().unwrap();

        assert_eq!(detected.import_policy, BundleImportPolicy::SaveOnly);
        assert!(detected.permissions.is_some());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_bundle_with_unknown_root_file() {
        let dir = unique_temp_dir("bundle-detect-unknown-root");
        let root = create_valid_bundle(&dir, false);
        fs::write(root.join("notes.txt"), b"not allowed").unwrap();

        let error = detect_bundle_directory(&root).unwrap_err();

        assert!(error.to_string().contains("unknown bundle root entry"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_bundle_manifest_payloads_outside_files_directory() {
        let dir = unique_temp_dir("bundle-detect-root-payload");
        let root = create_valid_bundle(&dir, false);
        let mut manifest = valid_bundle_manifest();
        manifest.files[0].path = "bundle.json".to_string();
        write_json(root.join("bundle.json"), &manifest);

        let error = detect_bundle_directory(&root).unwrap_err();

        assert!(error.to_string().contains("under files"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_bundle_with_undeclared_files_payload_entries() {
        let dir = unique_temp_dir("bundle-detect-extra-file");
        let root = create_valid_bundle(&dir, false);
        fs::write(root.join("files").join("extra.bin"), b"extra").unwrap();

        let error = detect_bundle_directory(&root).unwrap_err();

        assert!(error.to_string().contains("undeclared bundle payload"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn rejects_bundle_payload_symlinks() {
        let dir = unique_temp_dir("bundle-detect-symlink");
        let root = create_valid_bundle(&dir, false);
        fs::remove_file(root.join("files").join("content.bin")).unwrap();
        std::os::unix::fs::symlink("/etc/passwd", root.join("files").join("content.bin")).unwrap();

        let error = detect_bundle_directory(&root).unwrap_err();

        assert!(error.to_string().contains("symlinks are not supported"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stages_valid_bundle_into_bundle_id_directory() {
        let dir = unique_temp_dir("bundle-stage-valid");
        let root = create_valid_bundle(&dir, false);
        let staging_root = dir.join("staging");

        let staged = stage_bundle_directory(&root, &staging_root).unwrap();

        assert_eq!(staged.staging_path, staging_root.join("bundle_1234567890"));
        assert!(staged.staging_path.join("bundle.json").is_file());
        assert!(staged.staging_path.join("checksums.json").is_file());
        assert!(staged.staging_path.join("permissions.json").is_file());
        assert!(staged
            .staging_path
            .join("files")
            .join("content.bin")
            .is_file());
        assert_eq!(
            staged.detected.import_policy,
            BundleImportPolicy::ImportAllowed
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn staging_replaces_existing_bundle_id_directory() {
        let dir = unique_temp_dir("bundle-stage-replace");
        let root = create_valid_bundle(&dir, false);
        let staging_root = dir.join("staging");
        let stale_file = staging_root.join("bundle_1234567890").join("stale.txt");
        fs::create_dir_all(stale_file.parent().unwrap()).unwrap();
        fs::write(&stale_file, b"old").unwrap();

        let staged = stage_bundle_directory(&root, &staging_root).unwrap();

        assert!(!stale_file.exists());
        assert!(staged.staging_path.join("bundle.json").is_file());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_staging_when_bundle_id_would_escape_staging_root() {
        let dir = unique_temp_dir("bundle-stage-bad-id");
        let root = create_valid_bundle(&dir, false);
        let mut manifest = valid_bundle_manifest();
        manifest.bundle_id = "../escaped".to_string();
        write_json(root.join("bundle.json"), &manifest);

        let error = stage_bundle_directory(&root, &dir.join("staging")).unwrap_err();

        assert!(error.to_string().contains("bundle_id"));

        fs::remove_dir_all(dir).unwrap();
    }

    fn create_valid_bundle(dir: &std::path::Path, contains_secrets: bool) -> PathBuf {
        let root = dir.join("bundle");
        fs::create_dir_all(root.join("files")).unwrap();
        fs::write(
            root.join("files").join("manifest.json"),
            b"{\"kind\":\"skill\"}",
        )
        .unwrap();
        fs::write(root.join("files").join("content.bin"), b"hello bundle").unwrap();
        write_json(root.join("bundle.json"), &valid_bundle_manifest());
        write_json(root.join("checksums.json"), &valid_bundle_checksums());
        write_json(
            root.join("permissions.json"),
            &valid_bundle_permissions(contains_secrets),
        );
        root
    }

    fn valid_bundle_manifest() -> BundleManifest {
        BundleManifest {
            schema: BUNDLE_SCHEMA_V1.to_string(),
            bundle_id: "bundle_1234567890".to_string(),
            bundle_type: BundleType::Skill,
            display_name: "voice_transcribe".to_string(),
            source_app: "OpenNeko".to_string(),
            created_at: "2026-06-14T10:30:00Z".to_string(),
            sender: BundleSender {
                device_id: "neko-device-1234567890".to_string(),
                device_name: "MacBook".to_string(),
                fingerprint: "sha256:0123456789abcdef".to_string(),
            },
            compatibility: BundleCompatibility {
                min_nekolink_version: PROTOCOL_VERSION,
                required_capabilities: vec![Capability::BundleTransfer],
            },
            summary: BundleSummary {
                file_count: 2,
                total_bytes: 28,
            },
            files: vec![
                BundleFile {
                    path: "files/manifest.json".to_string(),
                    size: 16,
                    sha256: "0bc3f835203da0c2bbb44658e66c6bc0449e7f00bd9bd8fecd5d12283baaf5c9"
                        .to_string(),
                    role: "manifest".to_string(),
                },
                BundleFile {
                    path: "files/content.bin".to_string(),
                    size: 12,
                    sha256: "04cfecf64270c52b81da10bf6890b24fa73ee79715c44d1bc443dd9dd1de04d0"
                        .to_string(),
                    role: "payload".to_string(),
                },
            ],
        }
    }

    fn valid_bundle_checksums() -> BundleChecksums {
        let mut files = BTreeMap::new();
        files.insert(
            "files/manifest.json".to_string(),
            "0bc3f835203da0c2bbb44658e66c6bc0449e7f00bd9bd8fecd5d12283baaf5c9".to_string(),
        );
        files.insert(
            "files/content.bin".to_string(),
            "04cfecf64270c52b81da10bf6890b24fa73ee79715c44d1bc443dd9dd1de04d0".to_string(),
        );
        BundleChecksums {
            algorithm: BUNDLE_CHECKSUM_SHA256.to_string(),
            files,
        }
    }

    fn valid_bundle_permissions(contains_secrets: bool) -> BundlePermissions {
        BundlePermissions {
            requested_scopes: vec![BundlePermissionScope::SkillInstall],
            writes: vec![BundleWritePermission {
                target: "openneko.skills".to_string(),
                mode: BundleWriteMode::CreateOnly,
            }],
            secrets: BundleSecretsPolicy {
                contains_secrets,
                redacted_fields: Vec::new(),
            },
        }
    }

    fn write_json(path: impl AsRef<std::path::Path>, value: &impl serde::Serialize) {
        fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "nekodrop-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
