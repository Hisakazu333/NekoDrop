// 行为：导入落盘并写回执、导入计划不改盘、目标 / 文件级冲突阻塞、
// rename 与 skip_conflicts 策略、save-only 拒绝导入、失败不留半成品。
use std::fs;

use nekolink_protocol::BundleType;

use crate::bundle::{
    import_staged_bundle, import_staged_bundle_with_strategy, list_bundle_import_receipts,
    plan_bundle_import_rollback, plan_staged_bundle_import, stage_bundle_directory,
    BundleImportConflictStrategy, BUNDLE_IMPORT_RECEIPT_SCHEMA_V1,
};

use super::fixtures::{create_valid_bundle, unique_temp_dir};

#[test]
fn imports_staged_bundle_payload_into_destination_directory() {
    let dir = unique_temp_dir("bundle-import-confirm");
    let source = create_valid_bundle(&dir, false);
    let staging_root = dir.join("staging");
    let import_root = dir.join("imports");
    let staged = stage_bundle_directory(&source, &staging_root).unwrap();

    let imported = import_staged_bundle(&staged.staging_path, &import_root).unwrap();

    assert_eq!(imported.bundle_id, "bundle_1234567890");
    assert_eq!(
        imported.destination_path,
        import_root.join("bundle_1234567890")
    );
    assert_eq!(imported.file_count, 2);
    assert_eq!(imported.total_bytes, 28);
    assert_eq!(
        fs::read(imported.destination_path.join("content.bin")).unwrap(),
        b"hello bundle"
    );
    assert!(imported.import_receipt_path.is_file());
    assert_eq!(
        imported.import_receipt.schema,
        BUNDLE_IMPORT_RECEIPT_SCHEMA_V1
    );
    assert_eq!(imported.import_receipt.bundle_id, "bundle_1234567890");
    assert_eq!(imported.import_receipt.conflict_strategy, "reject");
    assert_eq!(
        imported.import_receipt.destination_path,
        imported.destination_path.display().to_string()
    );
    assert_eq!(
        imported.import_receipt.imported_manifest_paths,
        vec!["files/manifest.json", "files/content.bin"]
    );
    assert!(imported.import_receipt.skipped_manifest_paths.is_empty());
    let receipts = list_bundle_import_receipts(&import_root).unwrap();
    assert_eq!(receipts, vec![imported.import_receipt.clone()]);
    let rollback = plan_bundle_import_rollback(&imported.import_receipt).unwrap();
    assert!(rollback.can_rollback_now);
    assert_eq!(rollback.blocking_reason, None);
    assert_eq!(
        rollback.destination_path,
        import_root.join("bundle_1234567890")
    );
    assert_eq!(rollback.files.len(), 2);
    assert!(rollback.files.iter().all(|file| file.exists));
    assert!(rollback
        .files
        .iter()
        .any(|file| file.manifest_path == "files/content.bin"
            && file.destination_path == imported.destination_path.join("content.bin")));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn plans_importable_staged_bundle_without_mutating_disk() {
    let dir = unique_temp_dir("bundle-import-plan-ready");
    let source = create_valid_bundle(&dir, false);
    let staging_root = dir.join("staging");
    let import_root = dir.join("imports");
    let staged = stage_bundle_directory(&source, &staging_root).unwrap();

    let plan = plan_staged_bundle_import(&staged.staging_path, &import_root).unwrap();

    assert_eq!(plan.bundle_id, "bundle_1234567890");
    assert_eq!(plan.bundle_type, BundleType::Skill);
    assert_eq!(plan.display_name, "voice_transcribe");
    assert_eq!(plan.destination_path, import_root.join("bundle_1234567890"));
    assert_eq!(plan.file_count, 2);
    assert_eq!(plan.total_bytes, 28);
    assert!(plan.import_allowed);
    assert!(plan.can_import_now);
    assert!(!plan.destination_exists);
    assert_eq!(plan.blocking_reason, None);
    assert_eq!(plan.conflict_count, 0);
    assert_eq!(plan.files.len(), 2);
    assert_eq!(plan.files[0].manifest_path, "files/manifest.json");
    assert_eq!(
        plan.files[0].destination_path,
        import_root.join("bundle_1234567890").join("manifest.json")
    );
    assert!(!plan.files[0].destination_exists);
    assert!(!import_root.exists());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn plans_import_conflict_without_creating_temp_directory() {
    let dir = unique_temp_dir("bundle-import-plan-conflict");
    let source = create_valid_bundle(&dir, false);
    let staging_root = dir.join("staging");
    let import_root = dir.join("imports");
    let staged = stage_bundle_directory(&source, &staging_root).unwrap();
    fs::create_dir_all(import_root.join("bundle_1234567890")).unwrap();

    let plan = plan_staged_bundle_import(&staged.staging_path, &import_root).unwrap();

    assert!(plan.import_allowed);
    assert!(!plan.can_import_now);
    assert!(plan.destination_exists);
    assert_eq!(plan.blocking_reason.as_deref(), Some("destination_exists"));
    assert!(!import_root.join("bundle_1234567890.importing").exists());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn plans_file_level_import_conflicts_without_overwriting() {
    let dir = unique_temp_dir("bundle-import-plan-file-conflict");
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

    let plan = plan_staged_bundle_import(&staged.staging_path, &import_root).unwrap();

    assert!(plan.import_allowed);
    assert!(!plan.can_import_now);
    assert!(plan.destination_exists);
    assert_eq!(plan.conflict_count, 1);
    assert_eq!(plan.blocking_reason.as_deref(), Some("destination_exists"));
    let conflicted = plan
        .files
        .iter()
        .find(|file| file.destination_exists)
        .expect("one bundle payload file should conflict");
    assert_eq!(conflicted.manifest_path, "files/content.bin");
    assert_eq!(
        fs::read(import_root.join("bundle_1234567890").join("content.bin")).unwrap(),
        b"existing"
    );

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn rename_import_keeps_existing_destination_and_uses_next_safe_directory() {
    let dir = unique_temp_dir("bundle-import-rename");
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
        BundleImportConflictStrategy::Rename,
    )
    .unwrap();

    assert_eq!(
        imported.destination_path,
        import_root.join("bundle_1234567890-2")
    );
    assert_eq!(
        imported.conflict_strategy,
        BundleImportConflictStrategy::Rename
    );
    assert_eq!(imported.skipped_file_count, 0);
    assert_eq!(
        fs::read(import_root.join("bundle_1234567890").join("content.bin")).unwrap(),
        b"existing"
    );
    assert_eq!(
        fs::read(imported.destination_path.join("content.bin")).unwrap(),
        b"hello bundle"
    );

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn skip_conflicts_imports_missing_files_without_overwriting_existing_payload() {
    let dir = unique_temp_dir("bundle-import-skip-conflicts");
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

    assert_eq!(
        imported.destination_path,
        import_root.join("bundle_1234567890")
    );
    assert_eq!(
        imported.conflict_strategy,
        BundleImportConflictStrategy::SkipConflicts
    );
    assert_eq!(imported.skipped_file_count, 1);
    assert_eq!(
        fs::read(imported.destination_path.join("content.bin")).unwrap(),
        b"existing"
    );
    assert!(imported.destination_path.join("manifest.json").is_file());
    assert_eq!(
        imported.imported_manifest_paths,
        vec!["files/manifest.json".to_string()]
    );
    assert_eq!(
        imported.skipped_manifest_paths,
        vec!["files/content.bin".to_string()]
    );
    let receipts = list_bundle_import_receipts(&import_root).unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].bundle_id, "bundle_1234567890");
    assert_eq!(receipts[0].conflict_strategy, "skip_conflicts");
    assert_eq!(
        receipts[0].imported_manifest_paths,
        vec!["files/manifest.json"]
    );
    assert_eq!(
        receipts[0].skipped_manifest_paths,
        vec!["files/content.bin"]
    );
    let rollback = plan_bundle_import_rollback(&receipts[0]).unwrap();
    assert!(rollback.can_rollback_now);
    assert_eq!(rollback.files.len(), 1);
    assert_eq!(rollback.files[0].manifest_path, "files/manifest.json");
    assert_eq!(
        rollback.files[0].destination_path,
        imported.destination_path.join("manifest.json")
    );

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn plans_save_only_bundle_as_not_importable() {
    let dir = unique_temp_dir("bundle-import-plan-save-only");
    let source = create_valid_bundle(&dir, false);
    fs::remove_file(source.join("permissions.json")).unwrap();
    let staging_root = dir.join("staging");
    let import_root = dir.join("imports");
    let staged = stage_bundle_directory(&source, &staging_root).unwrap();

    let plan = plan_staged_bundle_import(&staged.staging_path, &import_root).unwrap();

    assert!(!plan.import_allowed);
    assert!(!plan.can_import_now);
    assert!(!plan.destination_exists);
    assert_eq!(plan.blocking_reason.as_deref(), Some("not_importable"));
    assert!(!import_root.exists());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn import_rejects_save_only_bundle() {
    let dir = unique_temp_dir("bundle-import-save-only");
    let source = create_valid_bundle(&dir, false);
    fs::remove_file(source.join("permissions.json")).unwrap();
    let staging_root = dir.join("staging");
    let import_root = dir.join("imports");
    let staged = stage_bundle_directory(&source, &staging_root).unwrap();

    let error = import_staged_bundle(&staged.staging_path, &import_root).unwrap_err();

    assert!(error.to_string().contains("not importable"));
    assert!(!import_root.join("bundle_1234567890").exists());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn failed_import_does_not_leave_partial_destination() {
    let dir = unique_temp_dir("bundle-import-rollback");
    let source = create_valid_bundle(&dir, false);
    let staging_root = dir.join("staging");
    let import_root = dir.join("imports");
    let staged = stage_bundle_directory(&source, &staging_root).unwrap();
    fs::create_dir_all(import_root.join("bundle_1234567890")).unwrap();

    let error = import_staged_bundle(&staged.staging_path, &import_root).unwrap_err();

    assert!(error.to_string().contains("already exists"));
    assert!(!import_root.join("bundle_1234567890.importing").exists());

    fs::remove_dir_all(dir).unwrap();
}
