use std::fs;
use std::path::PathBuf;

use nekodrop_core::{AppConfig, ReceivePolicy};
use serde::{Deserialize, Serialize};

use crate::device_identity::app_config_dir;

const APP_CONFIG_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedAppConfig {
    schema_version: u16,
    receive_dir: String,
    receive_port: Option<u16>,
    launch_at_login: bool,
    tray_enabled: bool,
    discovery_enabled: bool,
    receive_policy: String,
}

pub fn load_app_config(device_name: &str) -> Result<AppConfig, String> {
    let path = app_config_file_path()?;
    if !path.exists() {
        let mut config = AppConfig::default();
        config.device_name = device_name.to_string();
        return Ok(config);
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("无法读取应用配置文件 {}: {error}", path.display()))?;
    app_config_from_json(device_name, &content)
        .map_err(|error| format!("应用配置文件格式无效 {}: {error}", path.display()))
}

fn app_config_from_json(device_name: &str, content: &str) -> Result<AppConfig, String> {
    let persisted =
        serde_json::from_str::<PersistedAppConfig>(content).map_err(|error| error.to_string())?;
    if persisted.schema_version != APP_CONFIG_SCHEMA_VERSION {
        return Err(format!(
            "不支持的应用配置版本: {}",
            persisted.schema_version
        ));
    }

    let mut config = AppConfig::default();
    config.device_name = device_name.to_string();
    if !persisted.receive_dir.trim().is_empty() {
        config.receive_dir = persisted.receive_dir;
    }
    config.receive_port = persisted
        .receive_port
        .filter(|port| *port > 0)
        .unwrap_or(config.receive_port);
    config.launch_at_login = persisted.launch_at_login;
    config.tray_enabled = persisted.tray_enabled;
    config.discovery_enabled = persisted.discovery_enabled;
    config.receive_policy = parse_receive_policy(&persisted.receive_policy);

    Ok(config)
}

pub fn save_app_config(config: &AppConfig) -> Result<(), String> {
    let path = app_config_file_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("无法创建应用配置目录 {}: {error}", parent.display()))?;
    }

    let json = app_config_to_json(config)?;
    fs::write(&path, json)
        .map_err(|error| format!("无法写入应用配置文件 {}: {error}", path.display()))
}

fn app_config_to_json(config: &AppConfig) -> Result<String, String> {
    let persisted = PersistedAppConfig {
        schema_version: APP_CONFIG_SCHEMA_VERSION,
        receive_dir: config.receive_dir.clone(),
        receive_port: Some(config.receive_port),
        launch_at_login: config.launch_at_login,
        tray_enabled: config.tray_enabled,
        discovery_enabled: config.discovery_enabled,
        receive_policy: receive_policy_label(config.receive_policy).to_string(),
    };
    serde_json::to_string_pretty(&persisted).map_err(|error| format!("无法序列化应用配置: {error}"))
}

fn app_config_file_path() -> Result<PathBuf, String> {
    Ok(app_config_dir()?.join("app_config.json"))
}

pub fn receive_policy_label(policy: ReceivePolicy) -> &'static str {
    match policy {
        ReceivePolicy::AlwaysAsk => "always_ask",
        ReceivePolicy::AutoAcceptTrusted => "auto_accept_trusted",
        ReceivePolicy::BlockAll => "block_all",
    }
}

fn parse_receive_policy(value: &str) -> ReceivePolicy {
    match value {
        "auto_accept_trusted" => ReceivePolicy::AutoAcceptTrusted,
        "block_all" => ReceivePolicy::BlockAll,
        _ => ReceivePolicy::AlwaysAsk,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receive_policy_labels_are_stable() {
        assert_eq!(receive_policy_label(ReceivePolicy::AlwaysAsk), "always_ask");
        assert_eq!(
            receive_policy_label(ReceivePolicy::AutoAcceptTrusted),
            "auto_accept_trusted"
        );
        assert_eq!(receive_policy_label(ReceivePolicy::BlockAll), "block_all");
    }

    #[test]
    fn parses_unknown_receive_policy_conservatively() {
        assert_eq!(parse_receive_policy("always_ask"), ReceivePolicy::AlwaysAsk);
        assert_eq!(
            parse_receive_policy("auto_accept_trusted"),
            ReceivePolicy::AutoAcceptTrusted
        );
        assert_eq!(parse_receive_policy("block_all"), ReceivePolicy::BlockAll);
        assert_eq!(parse_receive_policy("unknown"), ReceivePolicy::AlwaysAsk);
    }

    #[test]
    fn loads_default_receive_port_for_older_config_files() {
        let json = r#"{
  "schema_version": 1,
  "receive_dir": "/tmp/nekodrop",
  "launch_at_login": false,
  "tray_enabled": false,
  "discovery_enabled": true,
  "receive_policy": "always_ask"
}"#;

        let config = app_config_from_json("MacBook", json).unwrap();

        assert_eq!(config.receive_port, 45821);
    }

    #[test]
    fn loads_and_saves_receive_port() {
        let json = r#"{
  "schema_version": 1,
  "receive_dir": "/tmp/nekodrop",
  "receive_port": 45999,
  "launch_at_login": false,
  "tray_enabled": false,
  "discovery_enabled": true,
  "receive_policy": "block_all"
}"#;

        let config = app_config_from_json("MacBook", json).unwrap();
        let saved_json = app_config_to_json(&config).unwrap();
        let saved = serde_json::from_str::<serde_json::Value>(&saved_json).unwrap();

        assert_eq!(config.receive_port, 45999);
        assert_eq!(saved["receive_port"], 45999);
    }

    #[test]
    fn ignores_invalid_persisted_receive_port() {
        let json = r#"{
  "schema_version": 1,
  "receive_dir": "/tmp/nekodrop",
  "receive_port": 0,
  "launch_at_login": false,
  "tray_enabled": false,
  "discovery_enabled": true,
  "receive_policy": "always_ask"
}"#;

        let config = app_config_from_json("MacBook", json).unwrap();

        assert_eq!(config.receive_port, 45821);
    }
}
