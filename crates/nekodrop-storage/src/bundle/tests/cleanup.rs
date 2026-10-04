// 行为：只删除指定 bundle、不安全 id 拒绝、缺失返回 false、
// 只清理过期条目、过期清理在存在损坏条目时仍然生效。
use std::{
    fs,
    time::{Duration, SystemTime},
};

use crate::bundle::{
    delete_staged_bundle, prune_staged_bundles_older_than, stage_bundle_directory,
};

use super::fixtures::{create_valid_bundle_with_id, set_modified_time, unique_temp_dir};

#[test]
fn prune_staged_bundles_older_than_survives_malformed_entries() {
    let dir = unique_temp_dir("bundle-prune-malformed");
    let source_old = create_valid_bundle_with_id(&dir, "source-old", "bundle_old");
    let staging_root = dir.join("staging");
    stage_bundle_directory(&source_old, &staging_root).unwrap();
    set_modified_time(&staging_root.join("bundle_old"), SystemTime::UNIX_EPOCH);

    // 过期清理同样依赖 list_staged_bundles：损坏条目不能让清理失效
    fs::create_dir_all(staging_root.join("bad:id")).unwrap();
    fs::create_dir_all(staging_root.join("no_manifest")).unwrap();

    let cutoff = SystemTime::UNIX_EPOCH + Duration::from_secs(60);
    let pruned = prune_staged_bundles_older_than(&staging_root, cutoff).unwrap();

    assert_eq!(pruned, vec!["bundle_old"]);
    assert!(!staging_root.join("bundle_old").exists());
    assert!(staging_root.join("bad:id").is_dir());
    assert!(staging_root.join("no_manifest").is_dir());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn delete_staged_bundle_removes_only_requested_bundle() {
    let dir = unique_temp_dir("bundle-delete-one");
    let source_a = create_valid_bundle_with_id(&dir, "source-a", "bundle_a");
    let source_b = create_valid_bundle_with_id(&dir, "source-b", "bundle_b");
    let staging_root = dir.join("staging");
    stage_bundle_directory(&source_a, &staging_root).unwrap();
    stage_bundle_directory(&source_b, &staging_root).unwrap();

    let removed = delete_staged_bundle(&staging_root, "bundle_a").unwrap();

    assert!(removed);
    assert!(!staging_root.join("bundle_a").exists());
    assert!(staging_root.join("bundle_b").is_dir());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn delete_staged_bundle_rejects_unsafe_bundle_id() {
    let dir = unique_temp_dir("bundle-delete-unsafe");

    let error = delete_staged_bundle(&dir.join("staging"), "../bundle").unwrap_err();

    assert!(error.to_string().contains("bundle_id"));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn delete_staged_bundle_returns_false_for_missing_safe_id() {
    let dir = unique_temp_dir("bundle-delete-missing");

    let removed = delete_staged_bundle(&dir.join("staging"), "bundle_missing").unwrap();

    assert!(!removed);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn prune_staged_bundles_older_than_removes_only_expired_bundles() {
    let dir = unique_temp_dir("bundle-prune-expired");
    let source_old_b = create_valid_bundle_with_id(&dir, "source-old-b", "bundle_old_b");
    let source_old_a = create_valid_bundle_with_id(&dir, "source-old-a", "bundle_old_a");
    let source_new = create_valid_bundle_with_id(&dir, "source-new", "bundle_new");
    let source_cutoff = create_valid_bundle_with_id(&dir, "source-cutoff", "bundle_cutoff");
    let staging_root = dir.join("staging");
    stage_bundle_directory(&source_old_b, &staging_root).unwrap();
    stage_bundle_directory(&source_old_a, &staging_root).unwrap();
    stage_bundle_directory(&source_new, &staging_root).unwrap();
    stage_bundle_directory(&source_cutoff, &staging_root).unwrap();

    let base = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);
    let old = base - Duration::from_secs(60);
    let cutoff = base;
    let new = base + Duration::from_secs(60);
    set_modified_time(&staging_root.join("bundle_old_b"), old);
    set_modified_time(&staging_root.join("bundle_old_a"), old);
    set_modified_time(&staging_root.join("bundle_cutoff"), cutoff);
    set_modified_time(&staging_root.join("bundle_new"), new);

    let pruned = prune_staged_bundles_older_than(&staging_root, cutoff).unwrap();

    assert_eq!(pruned, vec!["bundle_old_a", "bundle_old_b"]);
    assert!(!staging_root.join("bundle_old_a").exists());
    assert!(!staging_root.join("bundle_old_b").exists());
    assert!(staging_root.join("bundle_cutoff").is_dir());
    assert!(staging_root.join("bundle_new").is_dir());

    fs::remove_dir_all(dir).unwrap();
}
