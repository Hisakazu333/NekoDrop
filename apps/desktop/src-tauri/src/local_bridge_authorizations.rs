use std::fs;
use std::path::{Path, PathBuf};

use nekolink_protocol::LocalBridgePermissionScope;
use serde::{Deserialize, Serialize};

use crate::app_state::LocalBridgeAuthorizationRecord;
use crate::device_identity::app_config_dir;

const LOCAL_BRIDGE_AUTHORIZATIONS_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedLocalBridgeAuthorizationRecord {
    schema_version: u16,
    client_id: String,
    display_name: String,
    app_kind: Option<String>,
    scopes: Vec<LocalBridgePermissionScope>,
    granted_at_ms: u128,
    #[serde(default)]
    last_used_at_ms: Option<u128>,
    expires_at_ms: Option<u128>,
}

pub fn load_local_bridge_authorizations(
    now_ms: u128,
) -> Result<Vec<LocalBridgeAuthorizationRecord>, String> {
    let path = local_bridge_authorizations_file_path()?;
    load_local_bridge_authorizations_at(&path, now_ms)
}

pub fn save_local_bridge_authorizations(
    records: &[LocalBridgeAuthorizationRecord],
    now_ms: u128,
) -> Result<(), String> {
    let path = local_bridge_authorizations_file_path()?;
    save_local_bridge_authorizations_at(&path, records, now_ms)
}

pub(crate) fn load_local_bridge_authorizations_at(
    path: &Path,
    now_ms: u128,
) -> Result<Vec<LocalBridgeAuthorizationRecord>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(path)
        .map_err(|error| format!("无法读取本机接入授权文件 {}: {error}", path.display()))?;
    let persisted = serde_json::from_str::<Vec<PersistedLocalBridgeAuthorizationRecord>>(&content)
        .map_err(|error| format!("本机接入授权文件格式无效 {}: {error}", path.display()))?;
    Ok(normalize_authorizations(
        persisted
            .into_iter()
            .filter_map(|record| authorization_from_persisted(record).ok())
            .collect(),
        now_ms,
    ))
}

pub(crate) fn save_local_bridge_authorizations_at(
    path: &Path,
    records: &[LocalBridgeAuthorizationRecord],
    now_ms: u128,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("无法创建本机接入授权目录 {}: {error}", parent.display()))?;
    }
    let records = normalize_authorizations(records.to_vec(), now_ms);
    let persisted = records
        .into_iter()
        .map(authorization_to_persisted)
        .collect::<Vec<_>>();
    let json = serde_json::to_string_pretty(&persisted)
        .map_err(|error| format!("无法序列化本机接入授权: {error}"))?;
    fs::write(path, json)
        .map_err(|error| format!("无法写入本机接入授权文件 {}: {error}", path.display()))
}

fn normalize_authorizations(
    mut records: Vec<LocalBridgeAuthorizationRecord>,
    now_ms: u128,
) -> Vec<LocalBridgeAuthorizationRecord> {
    records.retain(|record| validate_local_bridge_authorization(record, now_ms).is_ok());
    records.sort_by(|left, right| {
        right
            .last_used_at_ms
            .cmp(&left.last_used_at_ms)
            .then_with(|| right.granted_at_ms.cmp(&left.granted_at_ms))
            .then_with(|| left.client_id.cmp(&right.client_id))
    });
    records
}

fn validate_local_bridge_authorization(
    record: &LocalBridgeAuthorizationRecord,
    now_ms: u128,
) -> Result<(), String> {
    if record.client_id.trim().is_empty() {
        return Err("本机接入授权缺少 client_id".to_string());
    }
    if record.display_name.trim().is_empty() {
        return Err("本机接入授权缺少 display_name".to_string());
    }
    if record
        .app_kind
        .as_deref()
        .is_some_and(|app_kind| app_kind.trim().is_empty())
    {
        return Err("本机接入授权 app_kind 不能为空".to_string());
    }
    if record.scopes.is_empty() {
        return Err("本机接入授权缺少 scope".to_string());
    }
    if record
        .expires_at_ms
        .is_some_and(|expires_at_ms| expires_at_ms < now_ms)
    {
        return Err("本机接入授权已过期".to_string());
    }
    Ok(())
}

fn authorization_from_persisted(
    record: PersistedLocalBridgeAuthorizationRecord,
) -> Result<LocalBridgeAuthorizationRecord, String> {
    if record.schema_version != LOCAL_BRIDGE_AUTHORIZATIONS_SCHEMA_VERSION {
        return Err(format!(
            "不支持的本机接入授权版本: {}",
            record.schema_version
        ));
    }
    Ok(LocalBridgeAuthorizationRecord {
        client_id: record.client_id,
        display_name: record.display_name,
        app_kind: record.app_kind,
        scopes: record.scopes,
        granted_at_ms: record.granted_at_ms,
        last_used_at_ms: record.last_used_at_ms.unwrap_or(record.granted_at_ms),
        expires_at_ms: record.expires_at_ms,
    })
}

fn authorization_to_persisted(
    record: LocalBridgeAuthorizationRecord,
) -> PersistedLocalBridgeAuthorizationRecord {
    PersistedLocalBridgeAuthorizationRecord {
        schema_version: LOCAL_BRIDGE_AUTHORIZATIONS_SCHEMA_VERSION,
        client_id: record.client_id,
        display_name: record.display_name,
        app_kind: record.app_kind,
        scopes: record.scopes,
        granted_at_ms: record.granted_at_ms,
        last_used_at_ms: Some(record.last_used_at_ms),
        expires_at_ms: record.expires_at_ms,
    }
}

pub(crate) fn local_bridge_authorizations_file_path() -> Result<PathBuf, String> {
    Ok(app_config_dir()?.join("local_bridge_authorizations.json"))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use nekolink_protocol::LocalBridgePermissionScope;

    use super::*;
    use crate::app_state::LocalBridgeAuthorizationRecord;

    #[test]
    fn saves_and_loads_active_local_bridge_authorizations() {
        let dir = unique_temp_dir("local-bridge-authorizations-active");
        let path = dir.join("local_bridge_authorizations.json");
        let records = vec![local_bridge_authorization(
            "local-app",
            &[LocalBridgePermissionScope::BundleSend],
            1_000,
            Some(10_000),
        )];

        save_local_bridge_authorizations_at(&path, &records, 2_000).unwrap();

        let loaded = load_local_bridge_authorizations_at(&path, 2_500).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        let value = serde_json::from_str::<serde_json::Value>(&raw).unwrap();

        assert_eq!(loaded, records);
        assert_eq!(value[0]["schema_version"], 1);
        assert_eq!(value[0]["last_used_at_ms"], 1_000);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn loading_local_bridge_authorizations_defaults_missing_last_used_to_granted_at() {
        let dir = unique_temp_dir("local-bridge-authorizations-legacy-last-used");
        let path = dir.join("local_bridge_authorizations.json");
        fs::write(
            &path,
            r#"[
  {
    "schema_version": 1,
    "client_id": "legacy-app",
    "display_name": "Legacy App",
    "app_kind": "generic",
    "scopes": ["bundle.send"],
    "granted_at_ms": 1000,
    "expires_at_ms": 10000
  }
]"#,
        )
        .unwrap();

        let loaded = load_local_bridge_authorizations_at(&path, 2_000).unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].client_id, "legacy-app");
        assert_eq!(loaded[0].granted_at_ms, 1_000);
        assert_eq!(loaded[0].last_used_at_ms, 1_000);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn loading_local_bridge_authorizations_drops_expired_records() {
        let dir = unique_temp_dir("local-bridge-authorizations-expired");
        let path = dir.join("local_bridge_authorizations.json");
        let active = local_bridge_authorization(
            "active-app",
            &[LocalBridgePermissionScope::BundleImportRequest],
            1_000,
            Some(20_000),
        );
        let expired = local_bridge_authorization(
            "expired-app",
            &[LocalBridgePermissionScope::BundleSend],
            1_000,
            Some(2_000),
        );

        save_local_bridge_authorizations_at(&path, &[active.clone(), expired], 1_500).unwrap();

        let loaded = load_local_bridge_authorizations_at(&path, 5_000).unwrap();

        assert_eq!(loaded, vec![active]);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn loading_local_bridge_authorizations_preserves_separate_scopes_for_same_client() {
        let dir = unique_temp_dir("local-bridge-authorizations-same-client-scopes");
        let path = dir.join("local_bridge_authorizations.json");
        let send = local_bridge_authorization(
            "local-app",
            &[LocalBridgePermissionScope::BundleSend],
            1_000,
            Some(20_000),
        );
        let import = local_bridge_authorization(
            "local-app",
            &[LocalBridgePermissionScope::BundleImportRequest],
            2_000,
            Some(30_000),
        );

        save_local_bridge_authorizations_at(&path, &[send.clone(), import.clone()], 2_500).unwrap();

        let loaded = load_local_bridge_authorizations_at(&path, 3_000).unwrap();

        assert_eq!(loaded, vec![import, send]);

        fs::remove_dir_all(dir).unwrap();
    }

    fn local_bridge_authorization(
        client_id: &str,
        scopes: &[LocalBridgePermissionScope],
        granted_at_ms: u128,
        expires_at_ms: Option<u128>,
    ) -> LocalBridgeAuthorizationRecord {
        LocalBridgeAuthorizationRecord {
            client_id: client_id.to_string(),
            display_name: format!("{client_id} Client"),
            app_kind: Some("generic".to_string()),
            scopes: scopes.to_vec(),
            granted_at_ms,
            last_used_at_ms: granted_at_ms,
            expires_at_ms,
        }
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "nekodrop-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
