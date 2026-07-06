use nekolink_protocol::{BundlePermissionScope, BundleType};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const ADAPTER_DESCRIPTOR_SCHEMA: &str = "nekolink.adapter.v1";
pub const ADAPTER_APP_MANIFEST_SCHEMA: &str = "nekolink.adapter.app_manifest.v1";
pub const ADAPTER_IMPORT_PLAN_SCHEMA: &str = "nekolink.adapter.import_plan.v1";
pub const ADAPTER_IMPORT_RECEIPT_SCHEMA: &str = "nekolink.adapter.import_receipt.v1";
pub const ADAPTER_IMPORT_TRANSACTION_SCHEMA: &str = "nekolink.adapter.import_transaction.v1";
pub const ADAPTER_TRANSACTION_CONTRACT_SCHEMA: &str = "nekolink.adapter.transaction_contract.v1";
pub const ADAPTER_IMPORT_RECEIPT_VERSION: u16 = 1;
pub const ADAPTER_IMPORT_TRANSACTION_VERSION: u16 = 1;

pub const ADAPTER_IMPORT_PLAN_STATES: &[&str] = &[
    "would_import",
    "would_conflict",
    "would_skip",
    "cannot_import",
];

pub const ADAPTER_IMPORT_TRANSACTION_STATES: &[&str] =
    &["prepared", "copied", "committed", "failed", "recovered"];

pub const ADAPTER_ROLLBACK_BLOCKING_REASONS: &[&str] = &[
    "target_missing",
    "receipt_already_rolled_back",
    "imported_path_unsafe",
    "imported_file_missing",
    "imported_file_changed",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterConflictStrategy {
    Reject,
    Rename,
    SkipConflicts,
}

impl AdapterConflictStrategy {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "reject" => Ok(Self::Reject),
            "rename" => Ok(Self::Rename),
            "skip_conflicts" => Ok(Self::SkipConflicts),
            other => Err(format!(
                "adapter conflict_strategy must be reject, rename, or skip_conflicts: {other}"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reject => "reject",
            Self::Rename => "rename",
            Self::SkipConflicts => "skip_conflicts",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterMigrationPolicy {
    ManualOnly,
    AdapterManaged,
}

impl AdapterMigrationPolicy {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "manual_only" => Ok(Self::ManualOnly),
            "adapter_managed" => Ok(Self::AdapterManaged),
            other => Err(format!(
                "adapter migration_policy must be manual_only or adapter_managed: {other}"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ManualOnly => "manual_only",
            Self::AdapterManaged => "adapter_managed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterResourceKind {
    Workspace,
    Session,
    Skill,
    AgentProfile,
}

impl AdapterResourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::Session => "session",
            Self::Skill => "skill",
            Self::AgentProfile => "agent_profile",
        }
    }

    pub fn bundle_type(self) -> BundleType {
        match self {
            Self::Workspace => BundleType::Workspace,
            Self::Session => BundleType::Session,
            Self::Skill => BundleType::Skill,
            Self::AgentProfile => BundleType::AgentProfile,
        }
    }

    pub fn permission_scope(self) -> BundlePermissionScope {
        match self {
            Self::Workspace => BundlePermissionScope::WorkspaceImport,
            Self::Session => BundlePermissionScope::SessionImport,
            Self::Skill => BundlePermissionScope::SkillInstall,
            Self::AgentProfile => BundlePermissionScope::AgentProfileImport,
        }
    }

    pub fn write_target(self) -> &'static str {
        match self {
            Self::Workspace => "nekobuddy.workspace",
            Self::Session => "nekobuddy.session",
            Self::Skill => "nekobuddy.skill",
            Self::AgentProfile => "nekobuddy.agent_profile",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterSchemaDescriptor {
    pub schema_id: String,
    pub current_version: u16,
    pub supported_versions: Vec<u16>,
    pub default_migration_policy: AdapterMigrationPolicy,
}

pub fn transaction_contract(
    adapter_id: &str,
    resource_kind: AdapterResourceKind,
    schema: AdapterSchemaDescriptor,
) -> Value {
    json!({
        "schema": ADAPTER_TRANSACTION_CONTRACT_SCHEMA,
        "adapter_id": adapter_id,
        "resource_kind": resource_kind.as_str(),
        "bundle_type": resource_kind.bundle_type(),
        "resource_schema": {
            "schema_id": schema.schema_id,
            "current_version": schema.current_version,
            "supported_versions": schema.supported_versions,
            "default_migration_policy": schema.default_migration_policy.as_str(),
            "supported_migration_policies": ["manual_only", "adapter_managed"],
            "unsupported_version_state": "cannot_import"
        },
        "import_plan": {
            "schema": ADAPTER_IMPORT_PLAN_SCHEMA,
            "stable_states": ADAPTER_IMPORT_PLAN_STATES,
            "migration_fields_required": true
        },
        "import_transaction": {
            "schema": ADAPTER_IMPORT_TRANSACTION_SCHEMA,
            "version": ADAPTER_IMPORT_TRANSACTION_VERSION,
            "states": ADAPTER_IMPORT_TRANSACTION_STATES,
            "failure_recovery_action": "recover-import"
        },
        "receipt": {
            "schema": ADAPTER_IMPORT_RECEIPT_SCHEMA,
            "version": ADAPTER_IMPORT_RECEIPT_VERSION,
            "records_resource_schema_version": true,
            "records_migration_status": true,
            "records_transaction_id": true
        },
        "rollback": {
            "blocking_reasons": ADAPTER_ROLLBACK_BLOCKING_REASONS,
            "changed_files_block_rollback": true,
            "missing_files_block_rollback": true
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_stable_shared_contract_sets() {
        assert_eq!(
            ADAPTER_IMPORT_PLAN_STATES,
            &[
                "would_import",
                "would_conflict",
                "would_skip",
                "cannot_import"
            ]
        );
        assert_eq!(
            AdapterConflictStrategy::parse("skip_conflicts").unwrap(),
            AdapterConflictStrategy::SkipConflicts
        );
        assert!(AdapterConflictStrategy::parse("overwrite").is_err());
        assert_eq!(
            AdapterResourceKind::AgentProfile.permission_scope(),
            BundlePermissionScope::AgentProfileImport
        );
    }

    #[test]
    fn transaction_contract_uses_generic_adapter_schemas() {
        let contract = transaction_contract(
            "adapter.test",
            AdapterResourceKind::Session,
            AdapterSchemaDescriptor {
                schema_id: "nekobuddy.session".to_string(),
                current_version: 1,
                supported_versions: vec![1],
                default_migration_policy: AdapterMigrationPolicy::ManualOnly,
            },
        );

        assert_eq!(contract["schema"], ADAPTER_TRANSACTION_CONTRACT_SCHEMA);
        assert_eq!(contract["resource_kind"], "session");
        assert_eq!(
            contract["import_transaction"]["states"],
            json!(["prepared", "copied", "committed", "failed", "recovered"])
        );
        assert_eq!(
            contract["rollback"]["blocking_reasons"],
            json!([
                "target_missing",
                "receipt_already_rolled_back",
                "imported_path_unsafe",
                "imported_file_missing",
                "imported_file_changed"
            ])
        );
    }
}
