use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::device_identity::app_config_dir;

const TRANSFER_HISTORY_SCHEMA_VERSION: u16 = 1;
const TRANSFER_HISTORY_LIMIT: usize = 80;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferHistoryRecord {
    pub schema_version: u16,
    pub id: String,
    pub direction: String,
    pub status: String,
    pub root_name: String,
    pub peer_device_id: Option<String>,
    pub peer_name: Option<String>,
    pub target_host: Option<String>,
    #[serde(default)]
    pub source_paths: Vec<String>,
    #[serde(default)]
    pub received_paths: Vec<String>,
    pub file_count: usize,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub receive_dir: Option<String>,
    pub error_message: Option<String>,
    pub created_at_ms: u128,
    pub updated_at_ms: u128,
}

pub fn load_transfer_history() -> Result<Vec<TransferHistoryRecord>, String> {
    let path = transfer_history_file_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("无法读取传输历史文件 {}: {error}", path.display()))?;
    let records = serde_json::from_str::<Vec<TransferHistoryRecord>>(&content)
        .map_err(|error| format!("传输历史文件格式无效 {}: {error}", path.display()))?;
    Ok(records
        .into_iter()
        .filter(|record| validate_transfer_history_record(record).is_ok())
        .take(TRANSFER_HISTORY_LIMIT)
        .collect())
}

pub fn push_transfer_history_record(
    records: &Arc<Mutex<Vec<TransferHistoryRecord>>>,
    record: TransferHistoryRecord,
) -> Result<(), String> {
    validate_transfer_history_record(&record)?;

    let mut records = records.lock().map_err(|error| error.to_string())?;
    records.retain(|item| item.id != record.id);
    records.insert(0, record);
    records.truncate(TRANSFER_HISTORY_LIMIT);
    save_transfer_history(&records)
}

fn save_transfer_history(records: &[TransferHistoryRecord]) -> Result<(), String> {
    let path = transfer_history_file_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("无法创建传输历史目录 {}: {error}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(records)
        .map_err(|error| format!("无法序列化传输历史: {error}"))?;
    fs::write(&path, json)
        .map_err(|error| format!("无法写入传输历史文件 {}: {error}", path.display()))
}

fn validate_transfer_history_record(record: &TransferHistoryRecord) -> Result<(), String> {
    if record.schema_version != TRANSFER_HISTORY_SCHEMA_VERSION {
        return Err(format!("不支持的传输历史版本: {}", record.schema_version));
    }
    if record.id.trim().is_empty() {
        return Err("传输历史缺少 id".to_string());
    }
    if record.direction.trim().is_empty() {
        return Err("传输历史缺少 direction".to_string());
    }
    if record.status.trim().is_empty() {
        return Err("传输历史缺少 status".to_string());
    }
    if record.root_name.trim().is_empty() {
        return Err("传输历史缺少 root_name".to_string());
    }
    Ok(())
}

fn transfer_history_file_path() -> Result<PathBuf, String> {
    Ok(app_config_dir()?.join("transfer_history.json"))
}

pub fn new_transfer_history_record(
    id: String,
    direction: impl Into<String>,
    status: impl Into<String>,
    root_name: impl Into<String>,
    file_count: usize,
    total_bytes: u64,
    transferred_bytes: u64,
    created_at_ms: u128,
) -> TransferHistoryRecord {
    TransferHistoryRecord {
        schema_version: TRANSFER_HISTORY_SCHEMA_VERSION,
        id,
        direction: direction.into(),
        status: status.into(),
        root_name: root_name.into(),
        peer_device_id: None,
        peer_name: None,
        target_host: None,
        source_paths: Vec::new(),
        received_paths: Vec::new(),
        file_count,
        total_bytes,
        transferred_bytes,
        receive_dir: None,
        error_message: None,
        created_at_ms,
        updated_at_ms: created_at_ms,
    }
}
