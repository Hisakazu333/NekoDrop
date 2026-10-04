// 行为：暂存进 bundle_id 目录并覆盖旧目录、列表按 id 排序、
// 损坏 / 外来条目被跳过而不影响其余条目、不安全 bundle_id 拒绝暂存。
use std::fs;

use crate::bundle::{list_staged_bundles, stage_bundle_directory, BundleImportPolicy};

use super::fixtures::{
    create_valid_bundle, create_valid_bundle_with_id, unique_temp_dir, valid_bundle_manifest,
    write_json,
};

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
fn list_staged_bundles_returns_empty_for_missing_root() {
    let dir = unique_temp_dir("bundle-list-missing");

    let bundles = list_staged_bundles(&dir.join("staging")).unwrap();

    assert!(bundles.is_empty());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn list_staged_bundles_returns_detected_bundles_sorted_by_id() {
    let dir = unique_temp_dir("bundle-list-sorted");
    let source_a = create_valid_bundle_with_id(&dir, "source-a", "bundle_b");
    let source_b = create_valid_bundle_with_id(&dir, "source-b", "bundle_a");
    let staging_root = dir.join("staging");
    stage_bundle_directory(&source_a, &staging_root).unwrap();
    stage_bundle_directory(&source_b, &staging_root).unwrap();
    fs::write(staging_root.join("note.txt"), b"ignored").unwrap();

    let bundles = list_staged_bundles(&staging_root).unwrap();

    let ids = bundles
        .iter()
        .map(|bundle| bundle.detected.manifest.bundle_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["bundle_a", "bundle_b"]);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn list_staged_bundles_skips_malformed_entries() {
    let dir = unique_temp_dir("bundle-list-malformed");
    let source = create_valid_bundle_with_id(&dir, "source-a", "bundle_a");
    let staging_root = dir.join("staging");
    stage_bundle_directory(&source, &staging_root).unwrap();

    // 目录名不安全（含 ':'）
    fs::create_dir_all(staging_root.join("bad:id")).unwrap();
    // 外来目录：没有 bundle.json
    fs::create_dir_all(staging_root.join("no_manifest")).unwrap();
    // 损坏目录：bundle.json 不是合法 JSON
    let corrupt = staging_root.join("corrupt_id");
    fs::create_dir_all(&corrupt).unwrap();
    fs::write(corrupt.join("bundle.json"), b"{not json").unwrap();

    let bundles = list_staged_bundles(&staging_root).unwrap();

    let ids = bundles
        .iter()
        .map(|bundle| bundle.detected.manifest.bundle_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["bundle_a"]);
    assert!(corrupt.is_dir());

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
