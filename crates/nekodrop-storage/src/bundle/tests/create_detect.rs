// 行为：手动从源目录创建 bundle、识别合法 bundle 目录、
// 拒绝校验和不符 / 根条目未知 / payload 越界 / 未声明文件 / 符号链接、
// 无权限或含密钥时推导为 save-only。
use std::fs;

use nekolink_protocol::{
    BundlePermissionScope, BundlePermissions, BundleSecretsPolicy, BundleSender, BundleType,
    BundleWriteMode, BundleWritePermission,
};

use crate::bundle::{
    create_manual_bundle_directory, detect_bundle_directory, BundleImportPolicy,
    ManualBundleCreateRequest,
};

use super::fixtures::{create_valid_bundle, unique_temp_dir, valid_bundle_manifest, write_json};

#[test]
fn creates_manual_bundle_from_source_directory() {
    let dir = unique_temp_dir("bundle-create-manual");
    let source = dir.join("workspace");
    fs::create_dir_all(source.join("src")).unwrap();
    fs::write(source.join("README.md"), b"hello workspace").unwrap();
    fs::write(source.join("src").join("main.txt"), b"run agent").unwrap();
    let output_root = dir.join("out");

    let created = create_manual_bundle_directory(ManualBundleCreateRequest {
        source_path: source.clone(),
        output_root: output_root.clone(),
        bundle_id: "bundle_workspace_1".to_string(),
        bundle_type: BundleType::Workspace,
        display_name: "workspace".to_string(),
        source_app: "NekoDrop".to_string(),
        sender: BundleSender {
            device_id: "device-1".to_string(),
            device_name: "MacBook".to_string(),
            fingerprint: "sha256:abc".to_string(),
        },
        created_at: "2026-06-14T00:00:00Z".to_string(),
        permissions: Some(BundlePermissions {
            requested_scopes: vec![BundlePermissionScope::WorkspaceImport],
            writes: vec![BundleWritePermission {
                target: "workspace.import".to_string(),
                mode: BundleWriteMode::ManualImport,
            }],
            secrets: BundleSecretsPolicy {
                contains_secrets: false,
                redacted_fields: vec![],
            },
        }),
    })
    .unwrap();

    assert_eq!(created.staging_path, output_root.join("bundle_workspace_1"));
    assert!(created.staging_path.join("bundle.json").is_file());
    assert!(created.staging_path.join("checksums.json").is_file());
    assert!(created.staging_path.join("permissions.json").is_file());
    assert!(created
        .staging_path
        .join("files")
        .join("README.md")
        .is_file());
    assert!(created
        .staging_path
        .join("files")
        .join("src")
        .join("main.txt")
        .is_file());

    let detected = detect_bundle_directory(&created.staging_path)
        .unwrap()
        .unwrap();
    assert_eq!(detected.manifest.bundle_id, "bundle_workspace_1");
    assert_eq!(detected.manifest.bundle_type, BundleType::Workspace);
    assert_eq!(detected.manifest.summary.file_count, 2);
    assert_eq!(detected.manifest.summary.total_bytes, 24);
    assert_eq!(detected.import_policy, BundleImportPolicy::ImportAllowed);

    fs::remove_dir_all(dir).unwrap();
}

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
