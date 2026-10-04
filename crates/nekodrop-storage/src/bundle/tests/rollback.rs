// 行为：回滚计划在导入文件缺失时阻塞、回滚只删除本次导入的文件、
// 计划不安全时拒绝执行且不动现有文件。
use std::fs;

use crate::bundle::{
    import_staged_bundle, import_staged_bundle_with_strategy, plan_bundle_import_rollback,
    rollback_bundle_import, stage_bundle_directory, BundleImportConflictStrategy,
};

use super::fixtures::{create_valid_bundle, unique_temp_dir};

#[test]
fn rollback_plan_blocks_when_imported_file_is_missing() {
    let dir = unique_temp_dir("bundle-import-rollback-plan-missing");
    let source = create_valid_bundle(&dir, false);
    let staging_root = dir.join("staging");
    let import_root = dir.join("imports");
    let staged = stage_bundle_directory(&source, &staging_root).unwrap();
    let imported = import_staged_bundle(&staged.staging_path, &import_root).unwrap();
    fs::remove_file(imported.destination_path.join("content.bin")).unwrap();

    let rollback = plan_bundle_import_rollback(&imported.import_receipt).unwrap();

    assert!(!rollback.can_rollback_now);
    assert_eq!(
        rollback.blocking_reason.as_deref(),
        Some("imported_file_missing")
    );
    assert!(rollback
        .files
        .iter()
        .any(|file| file.manifest_path == "files/content.bin" && !file.exists));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn rollback_bundle_import_removes_only_imported_files() {
    let dir = unique_temp_dir("bundle-import-rollback-execute");
    let source = create_valid_bundle(&dir, false);
    let staging_root = dir.join("staging");
    let import_root = dir.join("imports");
    let staged = stage_bundle_directory(&source, &staging_root).unwrap();
    fs::create_dir_all(import_root.join("bundle_1234567890")).unwrap();
    fs::write(
        import_root.join("bundle_1234567890").join("content.bin"),
        b"existing",
    )
    .unwrap();
    let imported = import_staged_bundle_with_strategy(
        &staged.staging_path,
        &import_root,
        BundleImportConflictStrategy::SkipConflicts,
    )
    .unwrap();

    let rolled_back = rollback_bundle_import(&imported.import_receipt).unwrap();

    assert_eq!(rolled_back.bundle_id, "bundle_1234567890");
    assert_eq!(rolled_back.removed_file_count, 1);
    assert_eq!(
        rolled_back.removed_manifest_paths,
        vec!["files/manifest.json"]
    );
    assert!(!imported.destination_path.join("manifest.json").exists());
    assert_eq!(
        fs::read(imported.destination_path.join("content.bin")).unwrap(),
        b"existing"
    );
    assert!(
        !plan_bundle_import_rollback(&imported.import_receipt)
            .unwrap()
            .can_rollback_now
    );

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn rollback_bundle_import_blocks_when_plan_is_not_safe() {
    let dir = unique_temp_dir("bundle-import-rollback-blocked");
    let source = create_valid_bundle(&dir, false);
    let staging_root = dir.join("staging");
    let import_root = dir.join("imports");
    let staged = stage_bundle_directory(&source, &staging_root).unwrap();
    let imported = import_staged_bundle(&staged.staging_path, &import_root).unwrap();
    fs::remove_file(imported.destination_path.join("content.bin")).unwrap();

    let error = rollback_bundle_import(&imported.import_receipt).unwrap_err();

    assert!(error.to_string().contains("imported_file_missing"));
    assert!(imported.destination_path.join("manifest.json").exists());

    fs::remove_dir_all(dir).unwrap();
}
