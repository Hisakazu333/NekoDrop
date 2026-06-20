use nekolink_protocol::{
    BundlePermissionScope, BundlePermissions, BundleSecretsPolicy, BundleType, BundleWriteMode,
    BundleWritePermission,
};

pub(super) fn bundle_type_label(bundle_type: BundleType) -> &'static str {
    match bundle_type {
        BundleType::Skill => "skill",
        BundleType::Session => "session",
        BundleType::Workspace => "workspace",
        BundleType::AgentProfile => "agent_profile",
        BundleType::ConfigSnapshot => "config_snapshot",
    }
}

pub(super) fn parse_bundle_type(value: &str) -> Result<BundleType, String> {
    match value {
        "skill" => Ok(BundleType::Skill),
        "session" => Ok(BundleType::Session),
        "workspace" => Ok(BundleType::Workspace),
        "agent_profile" => Ok(BundleType::AgentProfile),
        "config_snapshot" => Ok(BundleType::ConfigSnapshot),
        _ => Err(format!("不支持的资料包类型：{value}")),
    }
}

pub(super) fn bundle_type_from_label(value: &str) -> Option<BundleType> {
    parse_bundle_type(value).ok()
}

pub(super) fn manual_bundle_id(
    display_name: &str,
    bundle_type: &BundleType,
    source_path: &std::path::Path,
) -> String {
    let mut slug = display_name
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        slug = match bundle_type {
            BundleType::Skill => "skill".to_string(),
            BundleType::Session => "session".to_string(),
            BundleType::Workspace => "workspace".to_string(),
            BundleType::AgentProfile => "agent-profile".to_string(),
            BundleType::ConfigSnapshot => "config-snapshot".to_string(),
        };
    }
    let source_hash = sha256_hex(source_path.display().to_string().as_bytes());
    format!("bundle_{slug}_{}", &source_hash[..8.min(source_hash.len())])
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub(super) fn manual_bundle_permissions(bundle_type: &BundleType) -> BundlePermissions {
    let requested_scopes = match bundle_type {
        BundleType::Skill => vec![BundlePermissionScope::SkillInstall],
        BundleType::Session => vec![BundlePermissionScope::SessionImport],
        BundleType::Workspace => vec![BundlePermissionScope::WorkspaceImport],
        BundleType::AgentProfile => vec![BundlePermissionScope::AgentProfileImport],
        BundleType::ConfigSnapshot => vec![BundlePermissionScope::ConfigImport],
    };

    let target = match bundle_type {
        BundleType::Skill => "bundle.skill",
        BundleType::Session => "bundle.session",
        BundleType::Workspace => "bundle.workspace",
        BundleType::AgentProfile => "bundle.agent_profile",
        BundleType::ConfigSnapshot => "bundle.config_snapshot",
    };

    BundlePermissions {
        requested_scopes,
        writes: vec![BundleWritePermission {
            target: target.to_string(),
            mode: BundleWriteMode::ManualImport,
        }],
        secrets: BundleSecretsPolicy {
            contains_secrets: false,
            redacted_fields: Vec::new(),
        },
    }
}
