use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use nekodrop_core::{NekoDropError, NekoDropResult};
use nekolink_adapter_contract::{
    transaction_contract, AdapterConflictStrategy, AdapterMigrationPolicy, AdapterResourceKind,
    AdapterSchemaDescriptor, ADAPTER_APP_MANIFEST_SCHEMA, ADAPTER_DESCRIPTOR_SCHEMA,
    ADAPTER_IMPORT_PLAN_SCHEMA, ADAPTER_IMPORT_RECEIPT_SCHEMA, ADAPTER_IMPORT_RECEIPT_VERSION,
    ADAPTER_IMPORT_TRANSACTION_SCHEMA, ADAPTER_IMPORT_TRANSACTION_VERSION,
    ADAPTER_ROLLBACK_BLOCKING_REASONS,
};
use nekolink_protocol::{
    BundleChecksums, BundleCompatibility, BundleFile, BundleManifest, BundlePermissionScope,
    BundlePermissions, BundleSecretsPolicy, BundleSender, BundleSummary, BundleType,
    BundleWriteMode, BundleWritePermission, Capability, LocalBridgeActionResultsRequest,
    LocalBridgeAuthorizationRequest, LocalBridgeBundleDetailRequest, LocalBridgeClientIdentity,
    LocalBridgeImportBundleRequest, LocalBridgePermissionScope, LocalBridgePollEventsRequest,
    LocalBridgeRequest, LocalBridgeRollbackBundleImportRequest, LocalBridgeSendBundleRequest,
    BUNDLE_CHECKSUM_SHA256, BUNDLE_SCHEMA_V1,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

const SOURCE_APP: &str = "NekoBuddy";
const APP_KIND: &str = "nekobuddy";
const IMPORT_TRANSACTIONS_DIR: &str = ".nekobuddy-resource-import-transactions";
const IMPORT_TEMP_DIR: &str = ".nekobuddy-resource-import-temp";
const LATEST_RECEIPT_FILE: &str = ".nekobuddy-resource-latest-import-receipt.json";
const ROLLBACK_RECEIPT_FILE: &str = ".nekobuddy-resource-rollback-receipt.json";
const RESOURCE_SCHEMA_VERSION: u16 = 1;

const SECRET_KEY_PARTS: &[&str] = &[
    "token",
    "cookie",
    "secret",
    "private_key",
    "privatekey",
    "api_key",
    "apikey",
    "password",
    "credential",
    "ssh_key",
    "access_key",
    "refresh_token",
    "client_secret",
];

const LOCAL_PATH_KEY_PARTS: &[&str] = &[
    "path",
    "dir",
    "directory",
    "root",
    "home",
    "cache",
    "keychain",
    "socket",
    "pid_file",
];

const ACCOUNT_KEY_PARTS: &[&str] = &[
    "account_id",
    "account",
    "email",
    "user_id",
    "tenant_id",
    "organization_id",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NekobuddyResourceKind {
    Session,
    Skill,
    AgentProfile,
}

impl NekobuddyResourceKind {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "session" | "session-adapter" => Ok(Self::Session),
            "skill" | "skill-adapter" => Ok(Self::Skill),
            "agent_profile" | "agent-profile" | "agent-profile-adapter" => Ok(Self::AgentProfile),
            other => Err(format!(
                "resource kind must be session, skill, or agent-profile: {other}"
            )),
        }
    }

    fn spec(self) -> ResourceSpec {
        match self {
            Self::Session => ResourceSpec {
                kind: self,
                contract_kind: AdapterResourceKind::Session,
                adapter_id: "nekobuddy.session.adapter",
                display_name: "NekoBuddy Session Adapter",
                resource_name: "session",
                resource_label: "Session",
                schema_id: "nekobuddy.session",
                write_target: "nekobuddy.session",
                target_dir: "sessions",
                primary_file: "session.json",
                primary_role: "session",
                permission_scope: BundlePermissionScope::SessionImport,
                local_bridge_reason:
                    "NekoBuddy session adapter needs to send and import portable session bundles",
                sensitive: true,
            },
            Self::Skill => ResourceSpec {
                kind: self,
                contract_kind: AdapterResourceKind::Skill,
                adapter_id: "nekobuddy.skill.adapter",
                display_name: "NekoBuddy Skill Adapter",
                resource_name: "skill",
                resource_label: "Skill",
                schema_id: "nekobuddy.skill",
                write_target: "nekobuddy.skill",
                target_dir: "skills",
                primary_file: "skill.json",
                primary_role: "skill_manifest",
                permission_scope: BundlePermissionScope::SkillInstall,
                local_bridge_reason:
                    "NekoBuddy skill adapter needs to send and install user-approved skill bundles",
                sensitive: true,
            },
            Self::AgentProfile => ResourceSpec {
                kind: self,
                contract_kind: AdapterResourceKind::AgentProfile,
                adapter_id: "nekobuddy.agent_profile.adapter",
                display_name: "NekoBuddy Agent Profile Adapter",
                resource_name: "agent_profile",
                resource_label: "Agent Profile",
                schema_id: "nekobuddy.agent_profile",
                write_target: "nekobuddy.agent_profile",
                target_dir: "agent_profiles",
                primary_file: "profile.json",
                primary_role: "agent_profile",
                permission_scope: BundlePermissionScope::AgentProfileImport,
                local_bridge_reason:
                    "NekoBuddy agent profile adapter needs to send and import portable profile bundles",
                sensitive: true,
            },
        }
    }

    fn adapter_command(self) -> &'static str {
        match self {
            Self::Session => "session-adapter",
            Self::Skill => "skill-adapter",
            Self::AgentProfile => "agent-profile-adapter",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ResourceSpec {
    kind: NekobuddyResourceKind,
    contract_kind: AdapterResourceKind,
    adapter_id: &'static str,
    display_name: &'static str,
    resource_name: &'static str,
    resource_label: &'static str,
    schema_id: &'static str,
    write_target: &'static str,
    target_dir: &'static str,
    primary_file: &'static str,
    primary_role: &'static str,
    permission_scope: BundlePermissionScope,
    local_bridge_reason: &'static str,
    sensitive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportResourceBundleRequest {
    pub kind: NekobuddyResourceKind,
    pub source_path: PathBuf,
    pub output_root: PathBuf,
    pub bundle_id: String,
    pub display_name: String,
    pub contains_secrets: bool,
    pub migration_policy: AdapterMigrationPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportedResourceBundle {
    pub bundle_root: PathBuf,
    pub bundle_id: String,
    pub bundle_type: BundleType,
    pub resource_kind: String,
    pub resource_id: String,
    pub schema_id: String,
    pub schema_version: u16,
    pub file_count: usize,
    pub total_bytes: u64,
    pub redacted_fields: Vec<String>,
    pub declared_permissions: Vec<String>,
    pub executable_files: Vec<String>,
    pub requires_trusted_device: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportResourceRequest {
    pub kind: NekobuddyResourceKind,
    pub bundle_root: PathBuf,
    pub target_root: PathBuf,
    pub conflict_strategy: AdapterConflictStrategy,
    pub simulate_fail_after_copy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceBridgeRequestOptions {
    pub request_id: Option<String>,
    pub target_device_id: Option<String>,
    pub bundle_root: Option<PathBuf>,
    pub staged_bundle_id: Option<String>,
    pub bundle_id: Option<String>,
    pub action_request_id: Option<String>,
    pub ttl_seconds: Option<u64>,
    pub conflict_strategy: Option<AdapterConflictStrategy>,
}

impl Default for ResourceBridgeRequestOptions {
    fn default() -> Self {
        Self {
            request_id: None,
            target_device_id: None,
            bundle_root: None,
            staged_bundle_id: None,
            bundle_id: None,
            action_request_id: None,
            ttl_seconds: Some(3600),
            conflict_strategy: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResourceImportDryRun {
    pub bundle_id: String,
    pub bundle_type: BundleType,
    pub resource_kind: String,
    pub resource_id: String,
    pub display_name: String,
    pub schema_id: String,
    pub schema_version: u16,
    pub target_schema_version: u16,
    pub migration: ResourceMigrationPlan,
    pub target_root: PathBuf,
    pub target_path: PathBuf,
    pub status: String,
    pub dry_run: bool,
    pub conflict_strategy: String,
    pub would_import_file_count: usize,
    pub would_skip_file_count: usize,
    pub conflict_count: usize,
    pub conflicts: Vec<String>,
    pub declared_permissions: Vec<String>,
    pub executable_files: Vec<String>,
    pub receipt_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub plan: ResourceImportPlan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResourceImportConfirm {
    pub bundle_id: String,
    pub bundle_type: BundleType,
    pub resource_kind: String,
    pub resource_id: String,
    pub display_name: String,
    pub schema_id: String,
    pub schema_version: u16,
    pub target_schema_version: u16,
    pub migration: ResourceMigrationPlan,
    pub transaction_id: Option<String>,
    pub transaction_path: Option<PathBuf>,
    pub target_root: PathBuf,
    pub target_path: PathBuf,
    pub status: String,
    pub conflict_strategy: String,
    pub imported_file_count: usize,
    pub skipped_file_count: usize,
    pub conflict_count: usize,
    pub conflicts: Vec<String>,
    pub receipt_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResourceRollback {
    pub bundle_id: String,
    pub bundle_type: BundleType,
    pub resource_kind: String,
    pub resource_id: String,
    pub schema_id: String,
    pub schema_version: u16,
    pub receipt_version: u16,
    pub transaction_id: Option<String>,
    pub target_path: PathBuf,
    pub status: String,
    pub reason: Option<String>,
    pub rollback_blocking_reason: Option<String>,
    pub removed_file_count: usize,
    pub removed_manifest_paths: Vec<String>,
    pub skipped_manifest_paths: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blocked_manifest_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResourceImportRecovery {
    pub schema: String,
    pub transaction_id: String,
    pub bundle_id: String,
    pub resource_kind: String,
    pub resource_id: String,
    pub status: String,
    pub reason: Option<String>,
    pub removed_temp: bool,
    pub removed_file_count: usize,
    pub removed_manifest_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResourceImportPlan {
    pub schema: String,
    pub state: String,
    pub next_action: String,
    pub bundle_id: String,
    pub bundle_type: BundleType,
    pub resource_kind: String,
    pub resource_id: String,
    pub display_name: String,
    pub schema_id: String,
    pub schema_version: u16,
    pub target_schema_version: u16,
    pub migration: ResourceMigrationPlan,
    pub conflict_strategy: String,
    pub target_root: PathBuf,
    pub target_path: PathBuf,
    pub file_count: usize,
    pub would_import_file_count: usize,
    pub would_skip_file_count: usize,
    pub conflict_count: usize,
    pub conflicts: Vec<String>,
    pub would_import_paths: Vec<String>,
    pub would_skip_paths: Vec<String>,
    pub declared_permissions: Vec<String>,
    pub executable_files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceMigrationPlan {
    pub schema_id: String,
    pub source_version: u16,
    pub target_version: u16,
    pub supported_versions: Vec<u16>,
    pub migration_policy: AdapterMigrationPolicy,
    pub status: String,
    pub can_import: bool,
    pub blocking_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ResourceApplicationSchema {
    schema_id: String,
    version: u16,
    supported_versions: Vec<u16>,
    migration_policy: AdapterMigrationPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ResourceDescriptor {
    kind: String,
    resource_id: String,
    declared_permissions: Vec<String>,
    executable_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ResourceBundleManifest {
    schema: String,
    bundle_id: String,
    bundle_type: BundleType,
    display_name: String,
    source_app: String,
    created_at: String,
    sender: BundleSender,
    compatibility: BundleCompatibility,
    application_schema: ResourceApplicationSchema,
    resource: ResourceDescriptor,
    summary: BundleSummary,
    files: Vec<BundleFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ResourceTransportPolicy {
    requires_trusted_device: bool,
    requires_authenticated_encrypted_session: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ResourceBundlePermissions {
    requested_scopes: Vec<BundlePermissionScope>,
    writes: Vec<BundleWritePermission>,
    transport: ResourceTransportPolicy,
    secrets: BundleSecretsPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ResourceImportTransaction {
    schema: String,
    transaction_version: u16,
    transaction_id: String,
    bundle_id: String,
    bundle_type: BundleType,
    resource_kind: String,
    resource_id: String,
    schema_id: String,
    source_schema_version: u16,
    target_schema_version: u16,
    migration_policy: AdapterMigrationPolicy,
    migration_status: String,
    conflict_strategy: String,
    state: String,
    target_path: PathBuf,
    temp_path: PathBuf,
    transaction_path: PathBuf,
    manifest_paths: Vec<String>,
    files: Vec<ResourceImportFileRecord>,
    copied_manifest_paths: Vec<String>,
    committed_manifest_paths: Vec<String>,
    skipped_manifest_paths: Vec<String>,
    receipt_path: Option<PathBuf>,
    recovery_action: String,
    rollback_blocking_reasons: Vec<String>,
    created_at_ms: u128,
    copied_at_ms: Option<u128>,
    committed_at_ms: Option<u128>,
    failed_at_ms: Option<u128>,
    failure_reason: Option<String>,
    recovered_at_ms: Option<u128>,
    removed_manifest_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ResourceImportReceipt {
    schema: String,
    receipt_version: u16,
    bundle_id: String,
    bundle_type: BundleType,
    resource_kind: String,
    resource_id: String,
    display_name: String,
    source_app: String,
    schema_id: String,
    schema_version: u16,
    source_schema_version: u16,
    migration_policy: AdapterMigrationPolicy,
    migration_status: String,
    transaction_id: String,
    transaction_schema: String,
    transaction_version: u16,
    target_path: PathBuf,
    conflict_strategy: String,
    imported_manifest_paths: Vec<String>,
    imported_files: Vec<ResourceImportFileRecord>,
    skipped_manifest_paths: Vec<String>,
    imported_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ResourceRollbackReceipt {
    #[serde(flatten)]
    receipt: ResourceImportReceipt,
    rolled_back_at_ms: u128,
    removed_manifest_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ResourceImportFileRecord {
    manifest_path: String,
    size: u64,
    sha256: String,
}

#[derive(Debug, Clone)]
struct ResourceBundle {
    root: PathBuf,
    manifest: ResourceBundleManifest,
    permissions: ResourceBundlePermissions,
}

#[derive(Debug, Clone)]
struct PlannedImportFile {
    manifest_path: String,
    size: u64,
    sha256: String,
    destination_exists: bool,
}

pub fn build_descriptor(kind: NekobuddyResourceKind) -> Value {
    let spec = kind.spec();
    json!({
        "schema": ADAPTER_DESCRIPTOR_SCHEMA,
        "adapter_id": spec.adapter_id,
        "display_name": spec.display_name,
        "app_kind": APP_KIND,
        "client": bridge_client(&spec),
        "bridge": {
            "requested_scopes": [
                "bundle.read",
                "bundle.send",
                "bundle.import.request",
                "transfer.status.read"
            ],
            "default_ttl_seconds": 3600
        },
        "runtime": {
            "invocation": "argv",
            "working_directory": "adapter_root",
            "actions": [
                {
                    "action": "export_bundle",
                    "command": "nekodrop-sidecar",
                    "args": [
                        kind.adapter_command(),
                        "export",
                        "--source",
                        format!("{{{}_source}}", spec.resource_name),
                        "--output",
                        "{bundle_output}",
                        "--bundle-id",
                        "{bundle_id}",
                        "--name",
                        "{display_name}"
                    ]
                },
                {
                    "action": "import_bundle",
                    "command": "nekodrop-sidecar",
                    "args": [
                        kind.adapter_command(),
                        "import-confirm",
                        "--bundle-root",
                        "{bundle_root}",
                        "--target-root",
                        "{target_root}",
                        "--conflict-strategy",
                        "{conflict_strategy}"
                    ]
                },
                {
                    "action": "rollback_import",
                    "command": "nekodrop-sidecar",
                    "args": [
                        kind.adapter_command(),
                        "rollback",
                        "--receipt",
                        "{adapter_receipt}"
                    ]
                }
            ]
        },
        "transactions": {
            "dry_run_required": true,
            "receipt_required": true,
            "receipt_schema": ADAPTER_IMPORT_RECEIPT_SCHEMA,
            "receipt_version": ADAPTER_IMPORT_RECEIPT_VERSION,
            "rollback_supported": true,
            "rollback_requires_receipt": true,
            "conflict_resolution_required": true,
            "migration_policy": "manual_only",
            "transaction_schema": ADAPTER_IMPORT_TRANSACTION_SCHEMA,
            "transaction_version": ADAPTER_IMPORT_TRANSACTION_VERSION,
            "failure_recovery_action": "recover-import",
            "rollback_blocking_reasons": ADAPTER_ROLLBACK_BLOCKING_REASONS
        },
        "resource_schema": resource_schema_descriptor(&spec),
        "bundle_types": [
            {
                "bundle_type": spec.contract_kind.bundle_type(),
                "resource_kind": spec.resource_name,
                "can_export": true,
                "can_import": true,
                "permission_scope": permission_scope_label(spec.permission_scope),
                "write_target": spec.write_target,
                "sensitive": spec.sensitive,
                "requires_trusted_device": true,
                "conflict_strategies": ["reject", "rename", "skip_conflicts"],
                "schema_id": spec.schema_id,
                "supported_schema_versions": [RESOURCE_SCHEMA_VERSION]
            }
        ],
        "security": security_descriptor(&spec)
    })
}

pub fn build_app_manifest(kind: NekobuddyResourceKind) -> Value {
    let spec = kind.spec();
    json!({
        "schema": ADAPTER_APP_MANIFEST_SCHEMA,
        "app_id": "nekobuddy.app",
        "display_name": "NekoBuddy",
        "app_kind": APP_KIND,
        "adapter_id": spec.adapter_id,
        "resources": [
            {
                "resource_id": format!("{}.default", spec.resource_name),
                "bundle_type": spec.contract_kind.bundle_type(),
                "display_name": spec.resource_label,
                "direction": "both",
                "logical_source": format!("nekobuddy.{}.selected", spec.resource_name),
                "logical_target": spec.write_target,
                "permission_scope": permission_scope_label(spec.permission_scope),
                "export_action": "export_bundle",
                "import_action": "import_bundle",
                "rollback_action": "rollback_import",
                "sensitive": spec.sensitive,
                "requires_trusted_device": true,
                "conflict_strategies": ["reject", "rename", "skip_conflicts"],
                "migration_policy": "manual_only",
                "schema_id": spec.schema_id,
                "schema_version": RESOURCE_SCHEMA_VERSION,
                "supported_schema_versions": [RESOURCE_SCHEMA_VERSION]
            }
        ],
        "safety": {
            "never_include": safety_never_include(&spec),
            "require_user_selected_source": true,
            "require_dry_run_before_import": true,
            "require_receipt_for_import": true,
            "require_authenticated_encrypted_session_for_sensitive_bundles": true,
            "dangerous_content_is_never_executed_by_import": true
        }
    })
}

pub fn build_transaction_contract(kind: NekobuddyResourceKind) -> Value {
    let spec = kind.spec();
    transaction_contract(
        spec.adapter_id,
        spec.contract_kind,
        AdapterSchemaDescriptor {
            schema_id: spec.schema_id.to_string(),
            current_version: RESOURCE_SCHEMA_VERSION,
            supported_versions: vec![RESOURCE_SCHEMA_VERSION],
            default_migration_policy: AdapterMigrationPolicy::ManualOnly,
        },
    )
}

pub fn export_resource_bundle(
    request: ExportResourceBundleRequest,
) -> NekoDropResult<ExportedResourceBundle> {
    let spec = request.kind.spec();
    validate_resource_id(&request.bundle_id, "bundle_id")?;
    if request.display_name.trim().is_empty() {
        return Err(storage_error(format!(
            "{} bundle display_name is required",
            spec.resource_name
        )));
    }

    let prepared = prepare_source_files(&spec, &request.source_path)?;
    if prepared.files.is_empty() {
        return Err(storage_error(format!(
            "{} export must include at least one file",
            spec.resource_name
        )));
    }
    validate_resource_id(&prepared.resource_id, "resource_id")?;

    let bundle_root = request.output_root.join(&request.bundle_id);
    if bundle_root.exists() {
        fs::remove_dir_all(&bundle_root).map_err(|error| {
            storage_error(format!(
                "failed to replace {} bundle {}: {error}",
                spec.resource_name,
                bundle_root.display()
            ))
        })?;
    }
    fs::create_dir_all(bundle_root.join("files")).map_err(|error| {
        storage_error(format!(
            "failed to create {} bundle {}: {error}",
            spec.resource_name,
            bundle_root.display()
        ))
    })?;

    let mut files = Vec::new();
    let mut checksums = BTreeMap::new();
    for prepared_file in &prepared.files {
        let destination = bundle_root.join(&prepared_file.manifest_path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                storage_error(format!(
                    "failed to create {} bundle payload directory {}: {error}",
                    spec.resource_name,
                    parent.display()
                ))
            })?;
        }
        write_prepared_file(&destination, &prepared_file.content)?;
        let metadata = fs::metadata(&destination).map_err(|error| {
            storage_error(format!(
                "failed to read {} bundle payload {}: {error}",
                spec.resource_name,
                destination.display()
            ))
        })?;
        let checksum = sha256_file(&destination)?;
        files.push(BundleFile {
            path: prepared_file.manifest_path.clone(),
            size: metadata.len(),
            sha256: checksum.clone(),
            role: prepared_file.role.clone(),
        });
        checksums.insert(prepared_file.manifest_path.clone(), checksum);
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));

    let redacted_fields = prepared.redacted_fields.iter().cloned().collect::<Vec<_>>();
    let manifest = ResourceBundleManifest {
        schema: BUNDLE_SCHEMA_V1.to_string(),
        bundle_id: request.bundle_id.clone(),
        bundle_type: spec.contract_kind.bundle_type(),
        display_name: request.display_name.clone(),
        source_app: SOURCE_APP.to_string(),
        created_at: format!("unix_ms:{}", now_ms()),
        sender: BundleSender {
            device_id: spec.adapter_id.to_string(),
            device_name: spec.display_name.to_string(),
            fingerprint: "sha256:adapter-local".to_string(),
        },
        compatibility: BundleCompatibility {
            min_nekolink_version: 1,
            required_capabilities: vec![Capability::BundleTransfer, Capability::EncryptedSession],
        },
        application_schema: ResourceApplicationSchema {
            schema_id: spec.schema_id.to_string(),
            version: RESOURCE_SCHEMA_VERSION,
            supported_versions: vec![RESOURCE_SCHEMA_VERSION],
            migration_policy: request.migration_policy,
        },
        resource: ResourceDescriptor {
            kind: spec.resource_name.to_string(),
            resource_id: prepared.resource_id.clone(),
            declared_permissions: prepared.declared_permissions.clone(),
            executable_files: prepared.executable_files.clone(),
        },
        summary: BundleSummary {
            file_count: files.len(),
            total_bytes: files.iter().map(|file| file.size).sum(),
        },
        files,
    };
    let checksums = BundleChecksums {
        algorithm: BUNDLE_CHECKSUM_SHA256.to_string(),
        files: checksums,
    };
    let permissions = ResourceBundlePermissions {
        requested_scopes: vec![spec.permission_scope],
        writes: vec![BundleWritePermission {
            target: spec.write_target.to_string(),
            mode: BundleWriteMode::ManualImport,
        }],
        transport: ResourceTransportPolicy {
            requires_trusted_device: true,
            requires_authenticated_encrypted_session: true,
        },
        secrets: BundleSecretsPolicy {
            contains_secrets: request.contains_secrets,
            redacted_fields,
        },
    };

    write_json_file(&bundle_root.join("bundle.json"), &manifest)?;
    write_json_file(&bundle_root.join("checksums.json"), &checksums)?;
    write_json_file(&bundle_root.join("permissions.json"), &permissions)?;

    Ok(ExportedResourceBundle {
        bundle_root,
        bundle_id: request.bundle_id,
        bundle_type: spec.contract_kind.bundle_type(),
        resource_kind: spec.resource_name.to_string(),
        resource_id: prepared.resource_id,
        schema_id: spec.schema_id.to_string(),
        schema_version: RESOURCE_SCHEMA_VERSION,
        file_count: manifest.summary.file_count,
        total_bytes: manifest.summary.total_bytes,
        redacted_fields: permissions.secrets.redacted_fields,
        declared_permissions: prepared.declared_permissions,
        executable_files: prepared.executable_files,
        requires_trusted_device: true,
    })
}

pub fn dry_run_resource_import(
    request: ImportResourceRequest,
) -> NekoDropResult<ResourceImportDryRun> {
    let spec = request.kind.spec();
    let bundle = read_resource_bundle(&spec, &request.bundle_root)?;
    let (migration, import_blocking_reason) = importability(&spec, &bundle);
    let target_path = resource_target_path(
        &spec,
        &request.target_root,
        &bundle.manifest.resource.resource_id,
        request.conflict_strategy,
    );
    let files = planned_import_files(&bundle, &target_path)?;
    let conflicts = conflict_manifest_paths(&files);
    let would_skip = if request.conflict_strategy == AdapterConflictStrategy::SkipConflicts {
        conflicts.clone()
    } else {
        Vec::new()
    };
    let would_import = files
        .iter()
        .map(|file| file.manifest_path.clone())
        .filter(|manifest_path| !would_skip.contains(manifest_path))
        .collect::<Vec<_>>();
    let conflict_count = conflict_count(&target_path, conflicts.len());
    let (state, reason) = import_plan_state(
        &migration,
        import_blocking_reason,
        request.conflict_strategy,
        conflict_count,
        would_import.len(),
        would_skip.len(),
    );
    let plan = ResourceImportPlan {
        schema: ADAPTER_IMPORT_PLAN_SCHEMA.to_string(),
        state: state.clone(),
        next_action: next_action_for_import_state(&state).to_string(),
        bundle_id: bundle.manifest.bundle_id.clone(),
        bundle_type: bundle.manifest.bundle_type,
        resource_kind: spec.resource_name.to_string(),
        resource_id: bundle.manifest.resource.resource_id.clone(),
        display_name: bundle.manifest.display_name.clone(),
        schema_id: migration.schema_id.clone(),
        schema_version: migration.source_version,
        target_schema_version: migration.target_version,
        migration: migration.clone(),
        conflict_strategy: request.conflict_strategy.as_str().to_string(),
        target_root: request.target_root.clone(),
        target_path: target_path.clone(),
        file_count: files.len(),
        would_import_file_count: would_import.len(),
        would_skip_file_count: would_skip.len(),
        conflict_count,
        conflicts: conflicts.clone(),
        would_import_paths: would_import.clone(),
        would_skip_paths: would_skip,
        declared_permissions: bundle.manifest.resource.declared_permissions.clone(),
        executable_files: bundle.manifest.resource.executable_files.clone(),
        reason: reason.clone(),
    };
    Ok(ResourceImportDryRun {
        bundle_id: bundle.manifest.bundle_id,
        bundle_type: bundle.manifest.bundle_type,
        resource_kind: spec.resource_name.to_string(),
        resource_id: bundle.manifest.resource.resource_id,
        display_name: bundle.manifest.display_name,
        schema_id: migration.schema_id.clone(),
        schema_version: migration.source_version,
        target_schema_version: migration.target_version,
        migration,
        target_root: request.target_root,
        target_path,
        status: state,
        dry_run: true,
        conflict_strategy: request.conflict_strategy.as_str().to_string(),
        would_import_file_count: would_import.len(),
        would_skip_file_count: plan.would_skip_file_count,
        conflict_count,
        conflicts,
        declared_permissions: plan.declared_permissions.clone(),
        executable_files: plan.executable_files.clone(),
        receipt_path: None,
        reason,
        plan,
    })
}

pub fn confirm_resource_import(
    request: ImportResourceRequest,
) -> NekoDropResult<ResourceImportConfirm> {
    let spec = request.kind.spec();
    let bundle = read_resource_bundle(&spec, &request.bundle_root)?;
    let (migration, import_blocking_reason) = importability(&spec, &bundle);
    let target_path = resource_target_path(
        &spec,
        &request.target_root,
        &bundle.manifest.resource.resource_id,
        request.conflict_strategy,
    );
    let files = planned_import_files(&bundle, &target_path)?;
    let conflicts = conflict_manifest_paths(&files);
    let conflict_count = conflict_count(&target_path, conflicts.len());
    let (state, reason) = import_plan_state(
        &migration,
        import_blocking_reason,
        request.conflict_strategy,
        conflict_count,
        files.len().saturating_sub(conflicts.len()),
        conflicts.len(),
    );
    if state == "cannot_import" || state == "would_conflict" {
        return Ok(ResourceImportConfirm {
            bundle_id: bundle.manifest.bundle_id,
            bundle_type: bundle.manifest.bundle_type,
            resource_kind: spec.resource_name.to_string(),
            resource_id: bundle.manifest.resource.resource_id,
            display_name: bundle.manifest.display_name,
            schema_id: migration.schema_id.clone(),
            schema_version: migration.source_version,
            target_schema_version: migration.target_version,
            migration,
            transaction_id: None,
            transaction_path: None,
            target_root: request.target_root,
            target_path,
            status: if state == "cannot_import" {
                "cannot_import".to_string()
            } else {
                "conflict".to_string()
            },
            conflict_strategy: request.conflict_strategy.as_str().to_string(),
            imported_file_count: 0,
            skipped_file_count: 0,
            conflict_count,
            conflicts,
            receipt_path: None,
            temp_path: None,
            recovery_action: None,
            reason,
        });
    }

    let mut transaction = begin_import_transaction(
        &spec,
        &request.target_root,
        &target_path,
        &bundle,
        &migration,
        request.conflict_strategy,
        &files,
    )?;
    let import_result = commit_import_transaction(
        &spec,
        &bundle,
        &target_path,
        &files,
        request.conflict_strategy,
        &mut transaction,
        request.simulate_fail_after_copy,
    );
    let (imported, skipped) = match import_result {
        Ok(result) => result,
        Err(error) => {
            transaction.state = "failed".to_string();
            transaction.failed_at_ms = Some(now_ms());
            transaction.failure_reason = Some(error.to_string());
            write_json_file(&transaction.transaction_path, &transaction)?;
            return Ok(ResourceImportConfirm {
                bundle_id: bundle.manifest.bundle_id,
                bundle_type: bundle.manifest.bundle_type,
                resource_kind: spec.resource_name.to_string(),
                resource_id: bundle.manifest.resource.resource_id,
                display_name: bundle.manifest.display_name,
                schema_id: migration.schema_id.clone(),
                schema_version: migration.source_version,
                target_schema_version: migration.target_version,
                migration,
                transaction_id: Some(transaction.transaction_id),
                transaction_path: Some(transaction.transaction_path),
                target_root: request.target_root,
                target_path,
                status: "failed".to_string(),
                conflict_strategy: request.conflict_strategy.as_str().to_string(),
                imported_file_count: 0,
                skipped_file_count: 0,
                conflict_count,
                conflicts,
                receipt_path: None,
                temp_path: Some(transaction.temp_path),
                recovery_action: Some("recover-import".to_string()),
                reason: Some(error.to_string()),
            });
        }
    };

    let receipt = ResourceImportReceipt {
        schema: ADAPTER_IMPORT_RECEIPT_SCHEMA.to_string(),
        receipt_version: ADAPTER_IMPORT_RECEIPT_VERSION,
        bundle_id: bundle.manifest.bundle_id.clone(),
        bundle_type: bundle.manifest.bundle_type,
        resource_kind: spec.resource_name.to_string(),
        resource_id: bundle.manifest.resource.resource_id.clone(),
        display_name: bundle.manifest.display_name.clone(),
        source_app: bundle.manifest.source_app.clone(),
        schema_id: migration.schema_id.clone(),
        schema_version: migration.target_version,
        source_schema_version: migration.source_version,
        migration_policy: migration.migration_policy,
        migration_status: migration.status.clone(),
        transaction_id: transaction.transaction_id.clone(),
        transaction_schema: ADAPTER_IMPORT_TRANSACTION_SCHEMA.to_string(),
        transaction_version: ADAPTER_IMPORT_TRANSACTION_VERSION,
        target_path: target_path.clone(),
        conflict_strategy: request.conflict_strategy.as_str().to_string(),
        imported_manifest_paths: imported
            .iter()
            .map(|file| file.manifest_path.clone())
            .collect(),
        imported_files: imported
            .iter()
            .map(|file| ResourceImportFileRecord {
                manifest_path: file.manifest_path.clone(),
                size: file.size,
                sha256: file.sha256.clone(),
            })
            .collect(),
        skipped_manifest_paths: skipped.clone(),
        imported_at_ms: now_ms(),
    };
    let receipt_path = resource_receipt_path(
        &target_path,
        &bundle.manifest.resource.resource_id,
        request.conflict_strategy,
        receipt.imported_at_ms,
    );
    write_json_file(&receipt_path, &receipt)?;
    write_json_file(&target_path.join(LATEST_RECEIPT_FILE), &receipt)?;
    transaction.state = "committed".to_string();
    transaction.receipt_path = Some(receipt_path.clone());
    transaction.committed_at_ms = Some(now_ms());
    write_json_file(&transaction.transaction_path, &transaction)?;

    Ok(ResourceImportConfirm {
        bundle_id: bundle.manifest.bundle_id,
        bundle_type: bundle.manifest.bundle_type,
        resource_kind: spec.resource_name.to_string(),
        resource_id: bundle.manifest.resource.resource_id,
        display_name: bundle.manifest.display_name,
        schema_id: migration.schema_id.clone(),
        schema_version: migration.target_version,
        target_schema_version: migration.target_version,
        migration,
        transaction_id: Some(transaction.transaction_id),
        transaction_path: Some(transaction.transaction_path),
        target_root: request.target_root,
        target_path,
        status: "imported".to_string(),
        conflict_strategy: request.conflict_strategy.as_str().to_string(),
        imported_file_count: imported.len(),
        skipped_file_count: skipped.len(),
        conflict_count,
        conflicts,
        receipt_path: Some(receipt_path),
        temp_path: None,
        recovery_action: None,
        reason: None,
    })
}

pub fn rollback_resource_import(
    kind: NekobuddyResourceKind,
    receipt_path: &Path,
) -> NekoDropResult<ResourceRollback> {
    let spec = kind.spec();
    if !receipt_path.is_file() {
        return Err(storage_error(format!(
            "{} import receipt must be a file: {}",
            spec.resource_name,
            receipt_path.display()
        )));
    }
    let receipt: ResourceImportReceipt = read_json_file(receipt_path)?;
    validate_receipt(&spec, &receipt)?;
    if !path_is_inside(receipt_path, &receipt.target_path) {
        return Err(storage_error(
            "adapter import receipt must live inside its target_path",
        ));
    }
    if !receipt.target_path.is_dir() {
        return Ok(rollback_blocked(&receipt, "target_missing", Vec::new()));
    }
    if receipt.target_path.join(ROLLBACK_RECEIPT_FILE).exists() {
        return Ok(rollback_blocked(
            &receipt,
            "receipt_already_rolled_back",
            Vec::new(),
        ));
    }

    let mut blocked = Vec::new();
    for imported_file in &receipt.imported_files {
        let destination =
            import_payload_destination(&receipt.target_path, &imported_file.manifest_path)?;
        if !path_is_inside(&destination, &receipt.target_path) {
            blocked.push((imported_file.manifest_path.clone(), "imported_path_unsafe"));
            continue;
        }
        if !destination.is_file() {
            blocked.push((imported_file.manifest_path.clone(), "imported_file_missing"));
            continue;
        }
        let metadata = fs::metadata(&destination).map_err(|error| {
            storage_error(format!(
                "failed to read {} rollback file {}: {error}",
                receipt.resource_kind,
                destination.display()
            ))
        })?;
        let checksum = sha256_file(&destination)?;
        if metadata.len() != imported_file.size || checksum != imported_file.sha256 {
            blocked.push((imported_file.manifest_path.clone(), "imported_file_changed"));
        }
    }
    if let Some((_, reason)) = blocked.first() {
        return Ok(rollback_blocked(
            &receipt,
            reason,
            blocked
                .into_iter()
                .map(|(manifest_path, _)| manifest_path)
                .collect(),
        ));
    }

    let mut removed = Vec::new();
    for imported_file in &receipt.imported_files {
        let destination =
            import_payload_destination(&receipt.target_path, &imported_file.manifest_path)?;
        fs::remove_file(&destination).map_err(|error| {
            storage_error(format!(
                "failed to remove {} import file {}: {error}",
                receipt.resource_kind,
                destination.display()
            ))
        })?;
        removed.push(imported_file.manifest_path.clone());
    }
    remove_empty_dirs(&receipt.target_path, &receipt.target_path);
    write_json_file(
        &receipt.target_path.join(ROLLBACK_RECEIPT_FILE),
        &ResourceRollbackReceipt {
            receipt: receipt.clone(),
            rolled_back_at_ms: now_ms(),
            removed_manifest_paths: removed.clone(),
        },
    )?;

    Ok(ResourceRollback {
        bundle_id: receipt.bundle_id,
        bundle_type: receipt.bundle_type,
        resource_kind: receipt.resource_kind,
        resource_id: receipt.resource_id,
        schema_id: receipt.schema_id,
        schema_version: receipt.schema_version,
        receipt_version: receipt.receipt_version,
        transaction_id: Some(receipt.transaction_id),
        target_path: receipt.target_path,
        status: "rolled_back".to_string(),
        reason: None,
        rollback_blocking_reason: None,
        removed_file_count: removed.len(),
        removed_manifest_paths: removed,
        skipped_manifest_paths: receipt.skipped_manifest_paths,
        blocked_manifest_paths: Vec::new(),
    })
}

pub fn recover_resource_import(
    kind: NekobuddyResourceKind,
    transaction_path: &Path,
) -> NekoDropResult<ResourceImportRecovery> {
    let spec = kind.spec();
    if !transaction_path.is_file() {
        return Err(storage_error(format!(
            "{} import transaction must be a file: {}",
            spec.resource_name,
            transaction_path.display()
        )));
    }
    let mut transaction: ResourceImportTransaction = read_json_file(transaction_path)?;
    validate_transaction(&spec, &transaction)?;
    if transaction.state == "committed" {
        return Ok(ResourceImportRecovery {
            schema: ADAPTER_IMPORT_TRANSACTION_SCHEMA.to_string(),
            transaction_id: transaction.transaction_id,
            bundle_id: transaction.bundle_id,
            resource_kind: transaction.resource_kind,
            resource_id: transaction.resource_id,
            status: "not_recovered".to_string(),
            reason: Some("transaction_already_committed".to_string()),
            removed_temp: false,
            removed_file_count: 0,
            removed_manifest_paths: Vec::new(),
        });
    }

    let mut removed_manifest_paths = Vec::new();
    if transaction.target_path.exists() {
        for manifest_path in transaction.committed_manifest_paths.iter().rev() {
            let Some(record) = transaction
                .files
                .iter()
                .find(|file| file.manifest_path == *manifest_path)
            else {
                continue;
            };
            let destination = import_payload_destination(&transaction.target_path, manifest_path)?;
            if destination.is_file() {
                let metadata = fs::metadata(&destination).map_err(|error| {
                    storage_error(format!(
                        "failed to read committed {} import file {}: {error}",
                        transaction.resource_kind,
                        destination.display()
                    ))
                })?;
                if metadata.len() == record.size && sha256_file(&destination)? == record.sha256 {
                    fs::remove_file(&destination).map_err(|error| {
                        storage_error(format!(
                            "failed to remove committed {} import file {}: {error}",
                            transaction.resource_kind,
                            destination.display()
                        ))
                    })?;
                    removed_manifest_paths.push(manifest_path.clone());
                }
            }
        }
    }
    let removed_temp = transaction.temp_path.exists();
    if removed_temp {
        fs::remove_dir_all(&transaction.temp_path).map_err(|error| {
            storage_error(format!(
                "failed to remove {} import temp {}: {error}",
                transaction.resource_kind,
                transaction.temp_path.display()
            ))
        })?;
    }
    transaction.state = "recovered".to_string();
    transaction.recovered_at_ms = Some(now_ms());
    transaction.removed_manifest_paths = removed_manifest_paths.clone();
    write_json_file(transaction_path, &transaction)?;

    Ok(ResourceImportRecovery {
        schema: ADAPTER_IMPORT_TRANSACTION_SCHEMA.to_string(),
        transaction_id: transaction.transaction_id,
        bundle_id: transaction.bundle_id,
        resource_kind: transaction.resource_kind,
        resource_id: transaction.resource_id,
        status: "recovered".to_string(),
        reason: None,
        removed_temp,
        removed_file_count: removed_manifest_paths.len(),
        removed_manifest_paths,
    })
}

pub fn build_bridge_request(
    kind: NekobuddyResourceKind,
    request_kind: &str,
    options: ResourceBridgeRequestOptions,
) -> NekoDropResult<Value> {
    let spec = kind.spec();
    let request_id = options
        .request_id
        .unwrap_or_else(|| format!("{}-{request_kind}-{}", spec.resource_name, unique_suffix()));
    let request = match request_kind {
        "auth" => LocalBridgeRequest::AuthorizationRequest(LocalBridgeAuthorizationRequest {
            request_id,
            client: bridge_client_identity(&spec),
            requested_scopes: vec![
                LocalBridgePermissionScope::BundleRead,
                LocalBridgePermissionScope::BundleSend,
                LocalBridgePermissionScope::BundleImportRequest,
                LocalBridgePermissionScope::TransferStatusRead,
            ],
            reason: spec.local_bridge_reason.to_string(),
            ttl_seconds: options.ttl_seconds,
        }),
        "send" => LocalBridgeRequest::SendBundle(LocalBridgeSendBundleRequest {
            request_id,
            client: Some(bridge_client_identity(&spec)),
            target_device_id: options.target_device_id,
            bundle_root: options
                .bundle_root
                .ok_or_else(|| storage_error("request send requires bundle_root"))?
                .display()
                .to_string(),
            bundle_type: spec.contract_kind.bundle_type(),
            require_trusted_device: true,
        }),
        "detail" => LocalBridgeRequest::BundleDetail(LocalBridgeBundleDetailRequest {
            request_id,
            client: Some(bridge_client_identity(&spec)),
            staged_bundle_id: options
                .staged_bundle_id
                .ok_or_else(|| storage_error("request detail requires staged_bundle_id"))?,
        }),
        "import" => LocalBridgeRequest::ImportBundle(LocalBridgeImportBundleRequest {
            request_id,
            client: Some(bridge_client_identity(&spec)),
            staged_bundle_id: options
                .staged_bundle_id
                .ok_or_else(|| storage_error("request import requires staged_bundle_id"))?,
            expected_bundle_type: Some(spec.contract_kind.bundle_type()),
            conflict_strategy: options
                .conflict_strategy
                .map(|strategy| strategy.as_str().to_string()),
        }),
        "rollback" => {
            LocalBridgeRequest::RollbackBundleImport(LocalBridgeRollbackBundleImportRequest {
                request_id,
                client: Some(bridge_client_identity(&spec)),
                bundle_id: options
                    .bundle_id
                    .ok_or_else(|| storage_error("request rollback requires bundle_id"))?,
            })
        }
        "events" => LocalBridgeRequest::PollEvents(LocalBridgePollEventsRequest {
            request_id,
            client: Some(bridge_client_identity(&spec)),
            after_event_id: None,
            action_request_id: options.action_request_id,
            limit: Some(25),
            timeout_ms: Some(0),
        }),
        "results" => LocalBridgeRequest::ActionResults(LocalBridgeActionResultsRequest {
            request_id,
            client: Some(bridge_client_identity(&spec)),
            action_request_id: options.action_request_id,
            after_claimed_at_ms: None,
            limit: Some(25),
        }),
        other => {
            return Err(storage_error(format!(
                "unknown {} bridge request kind: {other}",
                spec.resource_name
            )))
        }
    };
    request.validate().map_err(|error| {
        storage_error(format!("invalid local bridge request: {}", error.message))
    })?;
    serde_json::to_value(request).map_err(|error| {
        storage_error(format!(
            "failed to serialize {} bridge request {request_kind}: {error}",
            spec.resource_name
        ))
    })
}

#[derive(Debug)]
struct PreparedSource {
    resource_id: String,
    files: Vec<PreparedFile>,
    redacted_fields: BTreeSet<String>,
    declared_permissions: Vec<String>,
    executable_files: Vec<String>,
}

#[derive(Debug)]
struct PreparedFile {
    manifest_path: String,
    role: String,
    content: PreparedContent,
}

#[derive(Debug)]
enum PreparedContent {
    Bytes(Vec<u8>),
    Json(Value),
}

fn prepare_source_files(spec: &ResourceSpec, source_path: &Path) -> NekoDropResult<PreparedSource> {
    match spec.kind {
        NekobuddyResourceKind::Session => prepare_single_json_source(spec, source_path),
        NekobuddyResourceKind::AgentProfile => prepare_single_json_source(spec, source_path),
        NekobuddyResourceKind::Skill => prepare_skill_source(spec, source_path),
    }
}

fn prepare_single_json_source(
    spec: &ResourceSpec,
    source_path: &Path,
) -> NekoDropResult<PreparedSource> {
    let source = if source_path.is_dir() {
        source_path.join(spec.primary_file)
    } else {
        source_path.to_path_buf()
    };
    if !source.is_file() {
        return Err(storage_error(format!(
            "{} source must be a {} file or a directory containing it: {}",
            spec.resource_name,
            spec.primary_file,
            source_path.display()
        )));
    }
    let mut value = read_json_file::<Value>(&source)?;
    let resource_id =
        extract_resource_id(&value, &["session_id", "profile_id", "id"]).ok_or_else(|| {
            storage_error(format!(
                "{} source must include id/session_id/profile_id",
                spec.resource_name
            ))
        })?;
    let mut redacted_fields = BTreeSet::new();
    sanitize_json_value(&mut value, "", spec, &mut redacted_fields);
    let executable_files = Vec::new();
    Ok(PreparedSource {
        resource_id,
        files: vec![PreparedFile {
            manifest_path: format!("files/{}", spec.primary_file),
            role: spec.primary_role.to_string(),
            content: PreparedContent::Json(value),
        }],
        redacted_fields,
        declared_permissions: Vec::new(),
        executable_files,
    })
}

fn prepare_skill_source(spec: &ResourceSpec, source_path: &Path) -> NekoDropResult<PreparedSource> {
    if !source_path.is_dir() {
        return Err(storage_error(format!(
            "skill source must be a directory containing {}: {}",
            spec.primary_file,
            source_path.display()
        )));
    }
    let manifest_path = source_path.join(spec.primary_file);
    if !manifest_path.is_file() {
        return Err(storage_error(format!(
            "skill source is missing {}",
            spec.primary_file
        )));
    }
    let mut manifest_value = read_json_file::<Value>(&manifest_path)?;
    let resource_id = extract_resource_id(&manifest_value, &["skill_id", "id"])
        .ok_or_else(|| storage_error("skill manifest must include skill_id or id"))?;
    let mut redacted_fields = BTreeSet::new();
    sanitize_json_value(&mut manifest_value, "", spec, &mut redacted_fields);
    let declared_permissions = extract_string_array(&manifest_value, "permissions");
    let mut files = vec![PreparedFile {
        manifest_path: format!("files/{}", spec.primary_file),
        role: spec.primary_role.to_string(),
        content: PreparedContent::Json(manifest_value),
    }];
    let mut executable_files = Vec::new();

    for entry in WalkDir::new(source_path)
        .follow_links(false)
        .sort_by_file_name()
    {
        let entry = entry.map_err(|error| {
            storage_error(format!(
                "failed to scan skill source {}: {error}",
                source_path.display()
            ))
        })?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            storage_error(format!(
                "failed to read skill source {}: {error}",
                path.display()
            ))
        })?;
        if metadata.file_type().is_symlink() {
            return Err(storage_error(format!(
                "skill export refuses symlink: {}",
                path.display()
            )));
        }
        if metadata.is_dir() {
            continue;
        }
        if path == manifest_path {
            continue;
        }
        if !metadata.is_file() {
            return Err(storage_error(format!(
                "unsupported skill source entry: {}",
                path.display()
            )));
        }
        let relative = path.strip_prefix(source_path).map_err(|error| {
            storage_error(format!(
                "failed to normalize skill source {}: {error}",
                path.display()
            ))
        })?;
        let relative_manifest_path = path_to_manifest_path(relative)?;
        if relative_manifest_path.starts_with('.') || relative_manifest_path.contains("/.") {
            continue;
        }
        let manifest_payload_path = format!("files/{relative_manifest_path}");
        let bytes = fs::read(path).map_err(|error| {
            storage_error(format!(
                "failed to read skill source {}: {error}",
                path.display()
            ))
        })?;
        if is_executable_like_path(&relative_manifest_path) {
            executable_files.push(manifest_payload_path.clone());
        }
        files.push(PreparedFile {
            manifest_path: manifest_payload_path,
            role: skill_file_role(&relative_manifest_path).to_string(),
            content: PreparedContent::Bytes(bytes),
        });
    }
    files.sort_by(|left, right| left.manifest_path.cmp(&right.manifest_path));
    executable_files.sort();
    executable_files.dedup();
    Ok(PreparedSource {
        resource_id,
        files,
        redacted_fields,
        declared_permissions,
        executable_files,
    })
}

fn read_resource_bundle(spec: &ResourceSpec, root: &Path) -> NekoDropResult<ResourceBundle> {
    if !root.is_dir() {
        return Err(storage_error(format!(
            "{} bundle root must be a directory: {}",
            spec.resource_name,
            root.display()
        )));
    }
    reject_unknown_root_entries(spec, root)?;
    let manifest: ResourceBundleManifest = read_json_file(&root.join("bundle.json"))?;
    validate_resource_manifest(spec, &manifest)?;
    let checksums: BundleChecksums = read_json_file(&root.join("checksums.json"))?;
    checksums
        .validate_against(&protocol_manifest(&manifest))
        .map_err(|error| {
            storage_error(format!(
                "invalid {} checksums: {}",
                spec.resource_name, error.message
            ))
        })?;
    let permissions: ResourceBundlePermissions = read_json_file(&root.join("permissions.json"))?;
    validate_resource_permissions(spec, &permissions)?;
    verify_payload_files(spec, root, &manifest, &checksums)?;
    reject_undeclared_payload_files(spec, root, &manifest)?;
    Ok(ResourceBundle {
        root: root.to_path_buf(),
        manifest,
        permissions,
    })
}

fn validate_resource_manifest(
    spec: &ResourceSpec,
    manifest: &ResourceBundleManifest,
) -> NekoDropResult<()> {
    protocol_manifest(manifest).validate().map_err(|error| {
        storage_error(format!(
            "invalid {} manifest: {}",
            spec.resource_name, error.message
        ))
    })?;
    validate_resource_id(&manifest.bundle_id, "bundle_id")?;
    validate_resource_id(&manifest.resource.resource_id, "resource_id")?;
    if manifest.bundle_type != spec.contract_kind.bundle_type() {
        return Err(storage_error(format!(
            "{} adapter only accepts {} bundles",
            spec.resource_name, spec.resource_name
        )));
    }
    if manifest.application_schema.schema_id != spec.schema_id {
        return Err(storage_error(format!(
            "unsupported {} schema id: {}",
            spec.resource_name, manifest.application_schema.schema_id
        )));
    }
    if manifest.resource.kind != spec.resource_name {
        return Err(storage_error(format!(
            "{} bundle resource kind mismatch: {}",
            spec.resource_name, manifest.resource.kind
        )));
    }
    if !manifest
        .files
        .iter()
        .any(|file| file.path == format!("files/{}", spec.primary_file))
    {
        return Err(storage_error(format!(
            "{} bundle must include files/{}",
            spec.resource_name, spec.primary_file
        )));
    }
    for file in &manifest.files {
        if !file.path.starts_with("files/") {
            return Err(storage_error(format!(
                "{} payload path must be under files/: {}",
                spec.resource_name, file.path
            )));
        }
        let _ = safe_manifest_payload_relative_path(&file.path)?;
    }
    Ok(())
}

fn validate_resource_permissions(
    spec: &ResourceSpec,
    permissions: &ResourceBundlePermissions,
) -> NekoDropResult<()> {
    BundlePermissions {
        requested_scopes: permissions.requested_scopes.clone(),
        writes: permissions.writes.clone(),
        secrets: permissions.secrets.clone(),
    }
    .validate()
    .map_err(|error| {
        storage_error(format!(
            "invalid {} permissions: {}",
            spec.resource_name, error.message
        ))
    })?;
    if !permissions
        .requested_scopes
        .contains(&spec.permission_scope)
    {
        return Err(storage_error(format!(
            "{} permissions must request {}",
            spec.resource_name,
            permission_scope_label(spec.permission_scope)
        )));
    }
    if !permissions.writes.iter().any(|write| {
        write.target == spec.write_target && write.mode == BundleWriteMode::ManualImport
    }) {
        return Err(storage_error(format!(
            "{} permissions must declare manual import to {}",
            spec.resource_name, spec.write_target
        )));
    }
    if !permissions.transport.requires_trusted_device
        || !permissions
            .transport
            .requires_authenticated_encrypted_session
    {
        return Err(storage_error(format!(
            "{} bundles must require trusted authenticated encrypted transport",
            spec.resource_name
        )));
    }
    Ok(())
}

fn protocol_manifest(manifest: &ResourceBundleManifest) -> BundleManifest {
    BundleManifest {
        schema: manifest.schema.clone(),
        bundle_id: manifest.bundle_id.clone(),
        bundle_type: manifest.bundle_type,
        display_name: manifest.display_name.clone(),
        source_app: manifest.source_app.clone(),
        created_at: manifest.created_at.clone(),
        sender: manifest.sender.clone(),
        compatibility: manifest.compatibility.clone(),
        summary: manifest.summary.clone(),
        files: manifest.files.clone(),
    }
}

fn importability(
    spec: &ResourceSpec,
    bundle: &ResourceBundle,
) -> (ResourceMigrationPlan, Option<String>) {
    let source_version = bundle.manifest.application_schema.version;
    let mut migration = ResourceMigrationPlan {
        schema_id: bundle.manifest.application_schema.schema_id.clone(),
        source_version,
        target_version: RESOURCE_SCHEMA_VERSION,
        supported_versions: vec![RESOURCE_SCHEMA_VERSION],
        migration_policy: bundle.manifest.application_schema.migration_policy,
        status: "not_required".to_string(),
        can_import: true,
        blocking_reason: None,
    };
    if bundle.permissions.secrets.contains_secrets {
        migration.can_import = false;
        migration.status = "blocked".to_string();
        migration.blocking_reason = Some("bundle_contains_secrets".to_string());
        return (migration, Some("bundle_contains_secrets".to_string()));
    }
    if migration.schema_id != spec.schema_id {
        migration.can_import = false;
        migration.status = "unsupported_schema".to_string();
        migration.blocking_reason = Some("unsupported_resource_schema_id".to_string());
        return (
            migration,
            Some("unsupported_resource_schema_id".to_string()),
        );
    }
    if source_version != RESOURCE_SCHEMA_VERSION {
        migration.can_import = false;
        migration.status = "unsupported_source_version".to_string();
        migration.blocking_reason = Some("unsupported_resource_schema_version".to_string());
        return (
            migration,
            Some("unsupported_resource_schema_version".to_string()),
        );
    }
    (migration, None)
}

fn import_plan_state(
    migration: &ResourceMigrationPlan,
    blocking_reason: Option<String>,
    strategy: AdapterConflictStrategy,
    conflict_count: usize,
    would_import_count: usize,
    would_skip_count: usize,
) -> (String, Option<String>) {
    if !migration.can_import {
        return (
            "cannot_import".to_string(),
            blocking_reason.or_else(|| migration.blocking_reason.clone()),
        );
    }
    if conflict_count > 0 && strategy == AdapterConflictStrategy::Reject {
        return ("would_conflict".to_string(), None);
    }
    if strategy == AdapterConflictStrategy::SkipConflicts
        && conflict_count > 0
        && would_import_count == 0
        && would_skip_count > 0
    {
        return ("would_skip".to_string(), None);
    }
    ("would_import".to_string(), None)
}

fn next_action_for_import_state(state: &str) -> &'static str {
    match state {
        "would_import" => "confirm_import",
        "would_conflict" => "choose_conflict_strategy",
        "would_skip" => "confirm_skip_or_choose_rename",
        "cannot_import" => "cancel_import_or_run_manual_migration",
        _ => "inspect_plan",
    }
}

fn planned_import_files(
    bundle: &ResourceBundle,
    target_path: &Path,
) -> NekoDropResult<Vec<PlannedImportFile>> {
    bundle
        .manifest
        .files
        .iter()
        .map(|file| {
            let destination_path = import_payload_destination(target_path, &file.path)?;
            Ok(PlannedImportFile {
                manifest_path: file.path.clone(),
                destination_exists: destination_path.exists(),
                size: file.size,
                sha256: file.sha256.clone(),
            })
        })
        .collect()
}

fn conflict_manifest_paths(files: &[PlannedImportFile]) -> Vec<String> {
    files
        .iter()
        .filter(|file| file.destination_exists)
        .map(|file| file.manifest_path.clone())
        .collect()
}

fn conflict_count(target_path: &Path, file_conflict_count: usize) -> usize {
    if target_path.exists() && file_conflict_count == 0 {
        1
    } else {
        file_conflict_count
    }
}

fn begin_import_transaction(
    spec: &ResourceSpec,
    target_root: &Path,
    target_path: &Path,
    bundle: &ResourceBundle,
    migration: &ResourceMigrationPlan,
    strategy: AdapterConflictStrategy,
    files: &[PlannedImportFile],
) -> NekoDropResult<ResourceImportTransaction> {
    let transaction_id = format!("{}-{}", bundle.manifest.bundle_id, unique_suffix());
    let transaction_path = target_root
        .join(IMPORT_TRANSACTIONS_DIR)
        .join(format!("{transaction_id}.json"));
    let temp_path = target_root.join(IMPORT_TEMP_DIR).join(&transaction_id);
    let transaction = ResourceImportTransaction {
        schema: ADAPTER_IMPORT_TRANSACTION_SCHEMA.to_string(),
        transaction_version: ADAPTER_IMPORT_TRANSACTION_VERSION,
        transaction_id,
        bundle_id: bundle.manifest.bundle_id.clone(),
        bundle_type: bundle.manifest.bundle_type,
        resource_kind: spec.resource_name.to_string(),
        resource_id: bundle.manifest.resource.resource_id.clone(),
        schema_id: migration.schema_id.clone(),
        source_schema_version: migration.source_version,
        target_schema_version: migration.target_version,
        migration_policy: migration.migration_policy,
        migration_status: migration.status.clone(),
        conflict_strategy: strategy.as_str().to_string(),
        state: "prepared".to_string(),
        target_path: target_path.to_path_buf(),
        temp_path,
        transaction_path,
        manifest_paths: files
            .iter()
            .map(|file| file.manifest_path.clone())
            .collect(),
        files: files
            .iter()
            .map(|file| ResourceImportFileRecord {
                manifest_path: file.manifest_path.clone(),
                size: file.size,
                sha256: file.sha256.clone(),
            })
            .collect(),
        copied_manifest_paths: Vec::new(),
        committed_manifest_paths: Vec::new(),
        skipped_manifest_paths: Vec::new(),
        receipt_path: None,
        recovery_action: "recover-import".to_string(),
        rollback_blocking_reasons: ADAPTER_ROLLBACK_BLOCKING_REASONS
            .iter()
            .map(|reason| (*reason).to_string())
            .collect(),
        created_at_ms: now_ms(),
        copied_at_ms: None,
        committed_at_ms: None,
        failed_at_ms: None,
        failure_reason: None,
        recovered_at_ms: None,
        removed_manifest_paths: Vec::new(),
    };
    write_json_file(&transaction.transaction_path, &transaction)?;
    Ok(transaction)
}

fn commit_import_transaction(
    spec: &ResourceSpec,
    bundle: &ResourceBundle,
    target_path: &Path,
    files: &[PlannedImportFile],
    strategy: AdapterConflictStrategy,
    transaction: &mut ResourceImportTransaction,
    simulate_fail_after_copy: bool,
) -> NekoDropResult<(Vec<PlannedImportFile>, Vec<String>)> {
    if transaction.temp_path.exists() {
        fs::remove_dir_all(&transaction.temp_path).map_err(|error| {
            storage_error(format!(
                "failed to clean {} import temp {}: {error}",
                spec.resource_name,
                transaction.temp_path.display()
            ))
        })?;
    }
    fs::create_dir_all(&transaction.temp_path).map_err(|error| {
        storage_error(format!(
            "failed to create {} import temp {}: {error}",
            spec.resource_name,
            transaction.temp_path.display()
        ))
    })?;

    let mut imported = Vec::new();
    let mut skipped = Vec::new();
    for file in files {
        if file.destination_exists && strategy == AdapterConflictStrategy::SkipConflicts {
            skipped.push(file.manifest_path.clone());
            continue;
        }
        let relative = safe_manifest_payload_relative_path(&file.manifest_path)?;
        let staged_destination = transaction.temp_path.join(&relative);
        if let Some(parent) = staged_destination.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                storage_error(format!(
                    "failed to create {} import temp payload {}: {error}",
                    spec.resource_name,
                    parent.display()
                ))
            })?;
        }
        fs::copy(bundle.root.join(&file.manifest_path), &staged_destination).map_err(|error| {
            storage_error(format!(
                "failed to copy {} import payload {}: {error}",
                spec.resource_name, file.manifest_path
            ))
        })?;
        imported.push(file.clone());
    }
    transaction.state = "copied".to_string();
    transaction.copied_manifest_paths = imported
        .iter()
        .map(|file| file.manifest_path.clone())
        .collect();
    transaction.skipped_manifest_paths = skipped.clone();
    transaction.copied_at_ms = Some(now_ms());
    write_json_file(&transaction.transaction_path, transaction)?;

    if simulate_fail_after_copy {
        return Err(storage_error("simulated_failure_after_copy"));
    }

    if strategy == AdapterConflictStrategy::SkipConflicts && target_path.exists() {
        let mut committed = Vec::new();
        for file in &imported {
            let relative = safe_manifest_payload_relative_path(&file.manifest_path)?;
            let destination = target_path.join(&relative);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    storage_error(format!(
                        "failed to create {} import destination {}: {error}",
                        spec.resource_name,
                        parent.display()
                    ))
                })?;
            }
            fs::copy(transaction.temp_path.join(&relative), &destination).map_err(|error| {
                recover_committed_files(target_path, &committed);
                storage_error(format!(
                    "failed to commit {} import payload {}: {error}",
                    spec.resource_name, file.manifest_path
                ))
            })?;
            committed.push(file.clone());
        }
        transaction.committed_manifest_paths = committed
            .iter()
            .map(|file| file.manifest_path.clone())
            .collect();
        fs::remove_dir_all(&transaction.temp_path).map_err(|error| {
            storage_error(format!(
                "failed to remove {} import temp {}: {error}",
                spec.resource_name,
                transaction.temp_path.display()
            ))
        })?;
    } else {
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                storage_error(format!(
                    "failed to create {} import parent {}: {error}",
                    spec.resource_name,
                    parent.display()
                ))
            })?;
        }
        fs::rename(&transaction.temp_path, target_path).map_err(|error| {
            let _ = fs::remove_dir_all(&transaction.temp_path);
            storage_error(format!(
                "failed to finalize {} import {}: {error}",
                spec.resource_name,
                target_path.display()
            ))
        })?;
        transaction.committed_manifest_paths = imported
            .iter()
            .map(|file| file.manifest_path.clone())
            .collect();
    }
    write_json_file(&transaction.transaction_path, transaction)?;
    Ok((imported, skipped))
}

fn recover_committed_files(target_path: &Path, committed: &[PlannedImportFile]) {
    for file in committed.iter().rev() {
        let Ok(relative) = safe_manifest_payload_relative_path(&file.manifest_path) else {
            continue;
        };
        let destination = target_path.join(relative);
        if destination.is_file() {
            let Ok(metadata) = fs::metadata(&destination) else {
                continue;
            };
            let Ok(checksum) = sha256_file(&destination) else {
                continue;
            };
            if metadata.len() == file.size && checksum == file.sha256 {
                let _ = fs::remove_file(destination);
            }
        }
    }
}

fn rollback_blocked(
    receipt: &ResourceImportReceipt,
    reason: &str,
    blocked_manifest_paths: Vec<String>,
) -> ResourceRollback {
    ResourceRollback {
        bundle_id: receipt.bundle_id.clone(),
        bundle_type: receipt.bundle_type,
        resource_kind: receipt.resource_kind.clone(),
        resource_id: receipt.resource_id.clone(),
        schema_id: receipt.schema_id.clone(),
        schema_version: receipt.schema_version,
        receipt_version: receipt.receipt_version,
        transaction_id: Some(receipt.transaction_id.clone()),
        target_path: receipt.target_path.clone(),
        status: "blocked".to_string(),
        reason: Some(reason.to_string()),
        rollback_blocking_reason: Some(reason.to_string()),
        removed_file_count: 0,
        removed_manifest_paths: Vec::new(),
        skipped_manifest_paths: receipt.skipped_manifest_paths.clone(),
        blocked_manifest_paths,
    }
}

fn validate_receipt(spec: &ResourceSpec, receipt: &ResourceImportReceipt) -> NekoDropResult<()> {
    if receipt.schema != ADAPTER_IMPORT_RECEIPT_SCHEMA {
        return Err(storage_error(format!(
            "unsupported {} import receipt schema: {}",
            spec.resource_name, receipt.schema
        )));
    }
    if receipt.receipt_version != ADAPTER_IMPORT_RECEIPT_VERSION {
        return Err(storage_error(format!(
            "unsupported {} import receipt version: {}",
            spec.resource_name, receipt.receipt_version
        )));
    }
    validate_resource_id(&receipt.bundle_id, "bundle_id")?;
    validate_resource_id(&receipt.resource_id, "resource_id")?;
    if receipt.bundle_type != spec.contract_kind.bundle_type() {
        return Err(storage_error(format!(
            "{} receipt must be for {} bundle",
            spec.resource_name, spec.resource_name
        )));
    }
    if receipt.resource_kind != spec.resource_name {
        return Err(storage_error(format!(
            "{} receipt resource kind mismatch: {}",
            spec.resource_name, receipt.resource_kind
        )));
    }
    for file in &receipt.imported_files {
        let _ = safe_manifest_payload_relative_path(&file.manifest_path)?;
    }
    Ok(())
}

fn validate_transaction(
    spec: &ResourceSpec,
    transaction: &ResourceImportTransaction,
) -> NekoDropResult<()> {
    if transaction.schema != ADAPTER_IMPORT_TRANSACTION_SCHEMA {
        return Err(storage_error(format!(
            "unsupported {} import transaction schema: {}",
            spec.resource_name, transaction.schema
        )));
    }
    if transaction.transaction_version != ADAPTER_IMPORT_TRANSACTION_VERSION {
        return Err(storage_error(format!(
            "unsupported {} import transaction version: {}",
            spec.resource_name, transaction.transaction_version
        )));
    }
    if transaction.resource_kind != spec.resource_name {
        return Err(storage_error(format!(
            "{} transaction resource kind mismatch: {}",
            spec.resource_name, transaction.resource_kind
        )));
    }
    validate_resource_id(&transaction.bundle_id, "bundle_id")?;
    validate_resource_id(&transaction.resource_id, "resource_id")
}

fn resource_target_path(
    spec: &ResourceSpec,
    target_root: &Path,
    resource_id: &str,
    strategy: AdapterConflictStrategy,
) -> PathBuf {
    let base = target_root.join(spec.target_dir).join(resource_id);
    if strategy != AdapterConflictStrategy::Rename || !base.exists() {
        return base;
    }
    for index in 2..10_000 {
        let candidate = target_root
            .join(spec.target_dir)
            .join(format!("{resource_id}-{index}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    target_root
        .join(spec.target_dir)
        .join(format!("{resource_id}-{}", unique_suffix()))
}

fn resource_receipt_path(
    target_path: &Path,
    resource_id: &str,
    strategy: AdapterConflictStrategy,
    imported_at_ms: u128,
) -> PathBuf {
    target_path.join(format!(
        ".nekobuddy-resource-import-receipt-{resource_id}-{}-{imported_at_ms}.json",
        strategy.as_str()
    ))
}

fn reject_unknown_root_entries(spec: &ResourceSpec, root: &Path) -> NekoDropResult<()> {
    for entry in fs::read_dir(root).map_err(|error| {
        storage_error(format!(
            "failed to read {} bundle root {}: {error}",
            spec.resource_name,
            root.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            storage_error(format!(
                "failed to read {} bundle root entry: {error}",
                spec.resource_name
            ))
        })?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !matches!(
            name.as_ref(),
            "bundle.json" | "checksums.json" | "permissions.json" | "files"
        ) {
            return Err(storage_error(format!(
                "unknown {} bundle root entry: {name}",
                spec.resource_name
            )));
        }
    }
    if !root.join("files").is_dir() {
        return Err(storage_error(format!(
            "{} bundle files/ directory is required",
            spec.resource_name
        )));
    }
    Ok(())
}

fn reject_undeclared_payload_files(
    spec: &ResourceSpec,
    root: &Path,
    manifest: &ResourceBundleManifest,
) -> NekoDropResult<()> {
    let declared = manifest
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<BTreeSet<_>>();
    for entry in WalkDir::new(root.join("files"))
        .follow_links(false)
        .sort_by_file_name()
    {
        let entry = entry.map_err(|error| {
            storage_error(format!(
                "failed to scan {} bundle payloads {}: {error}",
                spec.resource_name,
                root.display()
            ))
        })?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            storage_error(format!(
                "failed to read {} bundle payload {}: {error}",
                spec.resource_name,
                path.display()
            ))
        })?;
        if metadata.is_dir() {
            continue;
        }
        let relative = path.strip_prefix(root).map_err(|error| {
            storage_error(format!(
                "failed to normalize {} bundle payload {}: {error}",
                spec.resource_name,
                path.display()
            ))
        })?;
        let manifest_path = path_to_manifest_path(relative)?;
        if !declared.contains(manifest_path.as_str()) {
            return Err(storage_error(format!(
                "undeclared {} bundle payload: {manifest_path}",
                spec.resource_name
            )));
        }
    }
    Ok(())
}

fn verify_payload_files(
    spec: &ResourceSpec,
    root: &Path,
    manifest: &ResourceBundleManifest,
    checksums: &BundleChecksums,
) -> NekoDropResult<()> {
    for file in &manifest.files {
        let payload_path = root.join(&file.path);
        let metadata = fs::symlink_metadata(&payload_path).map_err(|error| {
            storage_error(format!(
                "failed to read {} bundle payload {}: {error}",
                spec.resource_name,
                payload_path.display()
            ))
        })?;
        if metadata.file_type().is_symlink() {
            return Err(storage_error(format!(
                "{} bundle payload symlinks are not supported: {}",
                spec.resource_name, file.path
            )));
        }
        if !metadata.is_file() {
            return Err(storage_error(format!(
                "{} bundle payload is not a file: {}",
                spec.resource_name, file.path
            )));
        }
        if metadata.len() != file.size {
            return Err(storage_error(format!(
                "{} bundle file size mismatch for {}: {} != {}",
                spec.resource_name,
                file.path,
                metadata.len(),
                file.size
            )));
        }
        let checksum = sha256_file(&payload_path)?;
        if checksum != file.sha256
            || checksums
                .files
                .get(&file.path)
                .map(|expected| expected != &checksum)
                .unwrap_or(true)
        {
            return Err(storage_error(format!(
                "{} bundle checksum mismatch for {}",
                spec.resource_name, file.path
            )));
        }
    }
    Ok(())
}

fn import_payload_destination(target_path: &Path, manifest_path: &str) -> NekoDropResult<PathBuf> {
    Ok(target_path.join(safe_manifest_payload_relative_path(manifest_path)?))
}

fn safe_manifest_payload_relative_path(manifest_path: &str) -> NekoDropResult<PathBuf> {
    let path = manifest_path.strip_prefix("files/").ok_or_else(|| {
        storage_error(format!(
            "resource payload path must be under files/: {manifest_path}"
        ))
    })?;
    if path.is_empty() {
        return Err(storage_error("resource payload path cannot be empty"));
    }
    let mut result = PathBuf::new();
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." || segment.contains('\\') {
            return Err(storage_error(format!(
                "unsafe resource payload path: {manifest_path}"
            )));
        }
        result.push(segment);
    }
    Ok(result)
}

fn path_to_manifest_path(path: &Path) -> NekoDropResult<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => {
                let Some(value) = value.to_str() else {
                    return Err(storage_error(format!(
                        "resource path is not utf-8: {}",
                        path.display()
                    )));
                };
                if value.is_empty()
                    || value == "."
                    || value == ".."
                    || value.contains('/')
                    || value.contains('\\')
                {
                    return Err(storage_error(format!(
                        "unsafe resource path segment: {}",
                        path.display()
                    )));
                }
                parts.push(value.to_string());
            }
            _ => {
                return Err(storage_error(format!(
                    "unsafe resource path: {}",
                    path.display()
                )))
            }
        }
    }
    if parts.is_empty() {
        return Err(storage_error("resource path cannot be empty"));
    }
    Ok(parts.join("/"))
}

fn validate_resource_id(value: &str, field: &str) -> NekoDropResult<()> {
    if value.trim().is_empty() {
        return Err(storage_error(format!("{field} is required")));
    }
    if value.len() > 120
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return Err(storage_error(format!(
            "{field} must use ascii letters, numbers, '.', '_' or '-': {value}"
        )));
    }
    Ok(())
}

fn sanitize_json_value(
    value: &mut Value,
    path: &str,
    spec: &ResourceSpec,
    redacted_fields: &mut BTreeSet<String>,
) {
    match value {
        Value::Object(map) => {
            let keys = map.keys().cloned().collect::<Vec<_>>();
            for key in keys {
                let field_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                if should_remove_json_key(&key, spec) {
                    map.remove(&key);
                    redacted_fields.insert(field_path);
                    continue;
                }
                if let Some(child) = map.get_mut(&key) {
                    sanitize_json_value(child, &field_path, spec, redacted_fields);
                }
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter_mut().enumerate() {
                sanitize_json_value(item, &format!("{path}[{index}]"), spec, redacted_fields);
            }
        }
        Value::String(text) => {
            if is_local_path_string(text) {
                *text = "[redacted-local-path]".to_string();
                redacted_fields.insert(path.to_string());
            }
        }
        _ => {}
    }
}

fn should_remove_json_key(key: &str, spec: &ResourceSpec) -> bool {
    let key = key.to_ascii_lowercase();
    if SECRET_KEY_PARTS.iter().any(|part| key.contains(part))
        || LOCAL_PATH_KEY_PARTS.iter().any(|part| key.contains(part))
    {
        return true;
    }
    spec.kind == NekobuddyResourceKind::AgentProfile
        && ACCOUNT_KEY_PARTS.iter().any(|part| key.contains(part))
}

fn is_local_path_string(value: &str) -> bool {
    value.starts_with('/')
        || value.starts_with("~/")
        || value.starts_with("file://")
        || value.contains(":\\")
        || value.starts_with("%USERPROFILE%")
}

fn extract_resource_id(value: &Value, keys: &[&str]) -> Option<String> {
    let object = value.as_object()?;
    keys.iter()
        .find_map(|key| object.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn extract_string_array(value: &Value, key: &str) -> Vec<String> {
    value
        .as_object()
        .and_then(|object| object.get(key))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn is_executable_like_path(path: &str) -> bool {
    let filename = path.rsplit('/').next().unwrap_or(path);
    let extension = filename.rsplit('.').next().unwrap_or("");
    matches!(
        extension,
        "sh" | "bash" | "zsh" | "fish" | "py" | "js" | "mjs" | "ts" | "tsx" | "rb" | "pl"
    )
}

fn skill_file_role(relative_manifest_path: &str) -> &'static str {
    let filename = relative_manifest_path
        .rsplit('/')
        .next()
        .unwrap_or(relative_manifest_path);
    if filename == "skill.json" {
        "skill_manifest"
    } else if filename.ends_with(".md") {
        "documentation"
    } else if is_executable_like_path(relative_manifest_path) {
        "script_payload"
    } else {
        "payload"
    }
}

fn write_prepared_file(path: &Path, content: &PreparedContent) -> NekoDropResult<()> {
    match content {
        PreparedContent::Bytes(bytes) => fs::write(path, bytes)
            .map_err(|error| storage_error(format!("failed to write {}: {error}", path.display()))),
        PreparedContent::Json(value) => write_json_file(path, value),
    }
}

fn security_descriptor(spec: &ResourceSpec) -> Value {
    match spec.kind {
        NekobuddyResourceKind::Session => json!({
            "rejects_contains_secrets": true,
            "strips_local_paths": true,
            "strips_tokens_cookies_and_private_keys": true,
            "exports_portable_session_state_only": true,
            "refuses_runtime_process_state": true,
            "requires_authenticated_encrypted_session_for_sensitive_bundles": true,
            "refuses_untrusted_sensitive_send": true
        }),
        NekobuddyResourceKind::Skill => json!({
            "rejects_contains_secrets": true,
            "strips_tokens_cookies_and_private_keys": true,
            "declares_permissions": true,
            "records_executable_files": true,
            "import_never_executes_scripts": true,
            "requires_authenticated_encrypted_session_for_sensitive_bundles": true,
            "refuses_untrusted_sensitive_send": true
        }),
        NekobuddyResourceKind::AgentProfile => json!({
            "rejects_contains_secrets": true,
            "strips_local_paths": true,
            "strips_tokens_cookies_and_private_keys": true,
            "strips_account_identity_fields": true,
            "exports_profile_structure_not_provider_tokens": true,
            "requires_authenticated_encrypted_session_for_sensitive_bundles": true,
            "refuses_untrusted_sensitive_send": true
        }),
    }
}

fn safety_never_include(spec: &ResourceSpec) -> Vec<&'static str> {
    match spec.kind {
        NekobuddyResourceKind::Session => vec![
            "provider tokens",
            "cookies",
            "private keys",
            "machine local absolute paths",
            "runtime process state",
            "temporary cache files",
        ],
        NekobuddyResourceKind::Skill => vec![
            "provider tokens",
            "cookies",
            "private keys",
            "undeclared writable targets",
            "automatic script execution",
        ],
        NekobuddyResourceKind::AgentProfile => vec![
            "provider tokens",
            "cookies",
            "private keys",
            "account identifiers",
            "machine local absolute paths",
            "keychain or credential-manager references",
        ],
    }
}

fn resource_schema_descriptor(spec: &ResourceSpec) -> Value {
    json!({
        "schema_id": spec.schema_id,
        "current_version": RESOURCE_SCHEMA_VERSION,
        "supported_versions": [RESOURCE_SCHEMA_VERSION],
        "migration_policy": "manual_only"
    })
}

fn bridge_client(spec: &ResourceSpec) -> Value {
    json!({
        "client_id": spec.adapter_id,
        "display_name": spec.display_name,
        "app_kind": APP_KIND
    })
}

fn bridge_client_identity(spec: &ResourceSpec) -> LocalBridgeClientIdentity {
    LocalBridgeClientIdentity {
        client_id: spec.adapter_id.to_string(),
        display_name: spec.display_name.to_string(),
        app_kind: Some(APP_KIND.to_string()),
    }
}

fn permission_scope_label(scope: BundlePermissionScope) -> &'static str {
    match scope {
        BundlePermissionScope::SkillInstall => "skill.install",
        BundlePermissionScope::SessionImport => "session.import",
        BundlePermissionScope::WorkspaceImport => "workspace.import",
        BundlePermissionScope::AgentProfileImport => "agent_profile.import",
        BundlePermissionScope::ConfigImport => "config.import",
    }
}

fn path_is_inside(path: &Path, root: &Path) -> bool {
    if path.exists() && root.exists() {
        let Ok(path) = path.canonicalize() else {
            return false;
        };
        let Ok(root) = root.canonicalize() else {
            return false;
        };
        path.starts_with(root)
    } else {
        path.starts_with(root)
    }
}

fn remove_empty_dirs(root: &Path, current: &Path) -> bool {
    let Ok(entries) = fs::read_dir(current) else {
        return false;
    };
    let mut is_empty = true;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if !remove_empty_dirs(root, &path) {
                is_empty = false;
            }
        } else {
            is_empty = false;
        }
    }
    if current != root && is_empty {
        let _ = fs::remove_dir(current);
        true
    } else {
        is_empty
    }
}

fn read_json_file<T: for<'de> Deserialize<'de>>(path: &Path) -> NekoDropResult<T> {
    let bytes = fs::read(path)
        .map_err(|error| storage_error(format!("failed to read {}: {error}", path.display())))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| storage_error(format!("failed to parse {}: {error}", path.display())))
}

fn write_json_file<T: Serialize>(path: &Path, value: &T) -> NekoDropResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            storage_error(format!(
                "failed to create parent directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    let json = serde_json::to_vec_pretty(value).map_err(|error| {
        storage_error(format!("failed to serialize {}: {error}", path.display()))
    })?;
    fs::write(path, json)
        .map_err(|error| storage_error(format!("failed to write {}: {error}", path.display())))
}

fn sha256_file(path: &Path) -> NekoDropResult<String> {
    let bytes = fs::read(path)
        .map_err(|error| storage_error(format!("failed to read {}: {error}", path.display())))?;
    Ok(sha256_bytes(&bytes))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}-{nanos}", std::process::id())
}

fn storage_error(message: impl Into<String>) -> NekoDropError {
    NekoDropError::Storage(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_export_import_and_rollback_use_resource_contract() {
        let root = unique_temp_dir("session");
        let source = root.join("session.json");
        let output = root.join("out");
        let target_root = root.join("target");
        fs::write(
            &source,
            r#"{
              "session_id":"session_demo",
              "title":"Portable session",
              "provider_token":"must-not-leak",
              "cache_path":"/Users/someone/.cache/neko/session",
              "messages":[{"role":"user","content":"hello"}]
            }"#,
        )
        .unwrap();

        let exported = export_resource_bundle(ExportResourceBundleRequest {
            kind: NekobuddyResourceKind::Session,
            source_path: source,
            output_root: output,
            bundle_id: "bundle_session_demo".to_string(),
            display_name: "Portable session".to_string(),
            contains_secrets: false,
            migration_policy: AdapterMigrationPolicy::ManualOnly,
        })
        .unwrap();
        assert_eq!(exported.bundle_type, BundleType::Session);
        assert!(exported
            .redacted_fields
            .iter()
            .any(|field| field == "provider_token"));

        let dry_run = dry_run_resource_import(ImportResourceRequest {
            kind: NekobuddyResourceKind::Session,
            bundle_root: exported.bundle_root.clone(),
            target_root: target_root.clone(),
            conflict_strategy: AdapterConflictStrategy::Reject,
            simulate_fail_after_copy: false,
        })
        .unwrap();
        assert_eq!(dry_run.status, "would_import");
        assert_eq!(dry_run.resource_id, "session_demo");

        let imported = confirm_resource_import(ImportResourceRequest {
            kind: NekobuddyResourceKind::Session,
            bundle_root: exported.bundle_root,
            target_root,
            conflict_strategy: AdapterConflictStrategy::Reject,
            simulate_fail_after_copy: false,
        })
        .unwrap();
        assert_eq!(imported.status, "imported");
        let receipt = imported.receipt_path.unwrap();
        let rolled_back =
            rollback_resource_import(NekobuddyResourceKind::Session, &receipt).unwrap();
        assert_eq!(rolled_back.status, "rolled_back");
        assert_eq!(rolled_back.resource_kind, "session");
    }

    #[test]
    fn skill_adapter_records_permissions_scripts_and_conflicts() {
        let root = unique_temp_dir("skill");
        let source = root.join("skill");
        let output = root.join("out");
        let target_root = root.join("target");
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("skill.json"),
            r#"{"skill_id":"skill_demo","name":"Demo","permissions":["network","filesystem.read"]}"#,
        )
        .unwrap();
        fs::write(source.join("main.ts"), "export default function run() {}\n").unwrap();

        let exported = export_resource_bundle(ExportResourceBundleRequest {
            kind: NekobuddyResourceKind::Skill,
            source_path: source,
            output_root: output,
            bundle_id: "bundle_skill_demo".to_string(),
            display_name: "Demo skill".to_string(),
            contains_secrets: false,
            migration_policy: AdapterMigrationPolicy::ManualOnly,
        })
        .unwrap();
        assert_eq!(exported.bundle_type, BundleType::Skill);
        assert_eq!(
            exported.declared_permissions,
            vec!["network", "filesystem.read"]
        );
        assert_eq!(exported.executable_files, vec!["files/main.ts"]);

        let imported = confirm_resource_import(ImportResourceRequest {
            kind: NekobuddyResourceKind::Skill,
            bundle_root: exported.bundle_root.clone(),
            target_root: target_root.clone(),
            conflict_strategy: AdapterConflictStrategy::Reject,
            simulate_fail_after_copy: false,
        })
        .unwrap();
        assert_eq!(imported.status, "imported");
        let conflict = dry_run_resource_import(ImportResourceRequest {
            kind: NekobuddyResourceKind::Skill,
            bundle_root: exported.bundle_root,
            target_root,
            conflict_strategy: AdapterConflictStrategy::Reject,
            simulate_fail_after_copy: false,
        })
        .unwrap();
        assert_eq!(conflict.status, "would_conflict");
        assert!(conflict.conflict_count > 0);
    }

    #[test]
    fn agent_profile_blocks_secret_bundle_and_recovers_failed_import() {
        let root = unique_temp_dir("profile");
        let source = root.join("profile.json");
        let output = root.join("out");
        let target_root = root.join("target");
        fs::write(
            &source,
            r#"{
              "profile_id":"profile_demo",
              "name":"Portable profile",
              "provider":"openai",
              "api_key":"must-not-leak",
              "account_id":"acct_local",
              "tool_path":"/Users/someone/bin/tool"
            }"#,
        )
        .unwrap();

        let exported = export_resource_bundle(ExportResourceBundleRequest {
            kind: NekobuddyResourceKind::AgentProfile,
            source_path: source,
            output_root: output,
            bundle_id: "bundle_profile_demo".to_string(),
            display_name: "Portable profile".to_string(),
            contains_secrets: true,
            migration_policy: AdapterMigrationPolicy::ManualOnly,
        })
        .unwrap();
        assert!(exported
            .redacted_fields
            .iter()
            .any(|field| field == "api_key"));
        assert!(exported
            .redacted_fields
            .iter()
            .any(|field| field == "account_id"));
        let blocked = dry_run_resource_import(ImportResourceRequest {
            kind: NekobuddyResourceKind::AgentProfile,
            bundle_root: exported.bundle_root.clone(),
            target_root: target_root.clone(),
            conflict_strategy: AdapterConflictStrategy::Reject,
            simulate_fail_after_copy: false,
        })
        .unwrap();
        assert_eq!(blocked.status, "cannot_import");
        assert_eq!(blocked.reason.as_deref(), Some("bundle_contains_secrets"));

        let permissions_path = exported.bundle_root.join("permissions.json");
        let mut permissions: ResourceBundlePermissions = read_json_file(&permissions_path).unwrap();
        permissions.secrets.contains_secrets = false;
        write_json_file(&permissions_path, &permissions).unwrap();
        let failed = confirm_resource_import(ImportResourceRequest {
            kind: NekobuddyResourceKind::AgentProfile,
            bundle_root: exported.bundle_root,
            target_root,
            conflict_strategy: AdapterConflictStrategy::Reject,
            simulate_fail_after_copy: true,
        })
        .unwrap();
        assert_eq!(failed.status, "failed");
        let recovered = recover_resource_import(
            NekobuddyResourceKind::AgentProfile,
            &failed.transaction_path.unwrap(),
        )
        .unwrap();
        assert_eq!(recovered.status, "recovered");
        assert!(recovered.removed_temp);
    }

    #[test]
    fn descriptors_and_bridge_requests_are_resource_specific() {
        let descriptor = build_descriptor(NekobuddyResourceKind::AgentProfile);
        assert_eq!(descriptor["schema"], ADAPTER_DESCRIPTOR_SCHEMA);
        assert_eq!(descriptor["adapter_id"], "nekobuddy.agent_profile.adapter");
        assert_eq!(
            build_transaction_contract(NekobuddyResourceKind::Skill)["resource_kind"],
            "skill"
        );
        let request = build_bridge_request(
            NekobuddyResourceKind::Session,
            "import",
            ResourceBridgeRequestOptions {
                request_id: Some("session-import-1".to_string()),
                staged_bundle_id: Some("bundle_session_demo".to_string()),
                conflict_strategy: Some(AdapterConflictStrategy::Rename),
                ..ResourceBridgeRequestOptions::default()
            },
        )
        .unwrap();
        assert_eq!(request["kind"], "bundle.import");
        assert_eq!(request["payload"]["expected_bundle_type"], "session");
        assert_eq!(request["payload"]["conflict_strategy"], "rename");
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "nekobuddy-resource-adapters-{name}-{}",
            unique_suffix()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
