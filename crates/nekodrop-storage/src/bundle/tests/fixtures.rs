// bundle 测试共享夹具：合法 bundle 目录构造、manifest / checksums /
// permissions 样例、JSON 写入与修改时间设置、独立临时目录。
use std::{collections::BTreeMap, fs, path::PathBuf, time::SystemTime};

use filetime::FileTime;

use nekolink_protocol::{
    BundleChecksums, BundleCompatibility, BundleFile, BundleManifest, BundlePermissionScope,
    BundlePermissions, BundleSecretsPolicy, BundleSender, BundleSummary, BundleType,
    BundleWriteMode, BundleWritePermission, Capability, BUNDLE_CHECKSUM_SHA256, BUNDLE_SCHEMA_V1,
    PROTOCOL_VERSION,
};

pub(crate) fn create_valid_bundle(dir: &std::path::Path, contains_secrets: bool) -> PathBuf {
    create_valid_bundle_with_id_and_secrets(dir, "bundle", "bundle_1234567890", contains_secrets)
}

pub(crate) fn create_valid_bundle_with_id(
    dir: &std::path::Path,
    directory_name: &str,
    bundle_id: &str,
) -> PathBuf {
    create_valid_bundle_with_id_and_secrets(dir, directory_name, bundle_id, false)
}

pub(crate) fn create_valid_bundle_with_id_and_secrets(
    dir: &std::path::Path,
    directory_name: &str,
    bundle_id: &str,
    contains_secrets: bool,
) -> PathBuf {
    let root = dir.join(directory_name);
    fs::create_dir_all(root.join("files")).unwrap();
    fs::write(
        root.join("files").join("manifest.json"),
        b"{\"kind\":\"skill\"}",
    )
    .unwrap();
    fs::write(root.join("files").join("content.bin"), b"hello bundle").unwrap();
    let mut manifest = valid_bundle_manifest();
    manifest.bundle_id = bundle_id.to_string();
    write_json(root.join("bundle.json"), &manifest);
    write_json(root.join("checksums.json"), &valid_bundle_checksums());
    write_json(
        root.join("permissions.json"),
        &valid_bundle_permissions(contains_secrets),
    );
    root
}

pub(crate) fn valid_bundle_manifest() -> BundleManifest {
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

pub(crate) fn valid_bundle_checksums() -> BundleChecksums {
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

pub(crate) fn valid_bundle_permissions(contains_secrets: bool) -> BundlePermissions {
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

pub(crate) fn write_json(path: impl AsRef<std::path::Path>, value: &impl serde::Serialize) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

pub(crate) fn set_modified_time(path: &std::path::Path, time: SystemTime) {
    let file_time = FileTime::from_system_time(time);
    filetime::set_file_mtime(path, file_time).unwrap();
}

pub(crate) fn unique_temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "nekodrop-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
