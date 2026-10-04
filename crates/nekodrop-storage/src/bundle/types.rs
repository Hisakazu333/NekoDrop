// Bundle 领域类型与常量：检测、暂存、导入、回执、回滚各流程共享的 DTO。
use std::path::PathBuf;

use nekolink_protocol::{
    BundleChecksums, BundleManifest, BundlePermissions, BundleSender, BundleType,
};
use serde::{Deserialize, Serialize};

pub const BUNDLE_IMPORT_RECEIPT_SCHEMA_V1: &str = "nekodrop.bundle.import_receipt.v1";

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedBundle {
    pub bundle_id: String,
    pub bundle_type: BundleType,
    pub display_name: String,
    pub source_app: String,
    pub destination_path: PathBuf,
    pub file_count: usize,
    pub total_bytes: u64,
    pub conflict_strategy: BundleImportConflictStrategy,
    pub skipped_file_count: usize,
    pub imported_manifest_paths: Vec<String>,
    pub skipped_manifest_paths: Vec<String>,
    pub import_receipt_path: PathBuf,
    pub import_receipt: BundleImportReceipt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleImportConflictStrategy {
    Reject,
    Rename,
    SkipConflicts,
}

impl BundleImportConflictStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            BundleImportConflictStrategy::Reject => "reject",
            BundleImportConflictStrategy::Rename => "rename",
            BundleImportConflictStrategy::SkipConflicts => "skip_conflicts",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleImportReceipt {
    pub schema: String,
    pub bundle_id: String,
    pub bundle_type: BundleType,
    pub display_name: String,
    pub source_app: String,
    pub destination_path: String,
    pub conflict_strategy: String,
    pub imported_manifest_paths: Vec<String>,
    pub skipped_manifest_paths: Vec<String>,
    pub imported_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleImportRollbackPlan {
    pub bundle_id: String,
    pub destination_path: PathBuf,
    pub can_rollback_now: bool,
    pub blocking_reason: Option<String>,
    pub files: Vec<BundleImportRollbackPlanFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleImportRollbackPlanFile {
    pub manifest_path: String,
    pub destination_path: PathBuf,
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RolledBackBundleImport {
    pub bundle_id: String,
    pub destination_path: PathBuf,
    pub removed_file_count: usize,
    pub removed_manifest_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleImportPlan {
    pub bundle_id: String,
    pub bundle_type: BundleType,
    pub display_name: String,
    pub source_app: String,
    pub destination_path: PathBuf,
    pub file_count: usize,
    pub total_bytes: u64,
    pub import_allowed: bool,
    pub can_import_now: bool,
    pub destination_exists: bool,
    pub blocking_reason: Option<String>,
    pub files: Vec<BundleImportPlanFile>,
    pub conflict_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleImportPlanFile {
    pub manifest_path: String,
    pub size: u64,
    pub sha256: String,
    pub destination_path: PathBuf,
    pub destination_exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualBundleCreateRequest {
    pub source_path: PathBuf,
    pub output_root: PathBuf,
    pub bundle_id: String,
    pub bundle_type: BundleType,
    pub display_name: String,
    pub source_app: String,
    pub sender: BundleSender,
    pub created_at: String,
    pub permissions: Option<BundlePermissions>,
}
