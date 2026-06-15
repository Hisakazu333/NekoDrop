use std::collections::HashSet;
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
    #[serde(default)]
    pub security_mode: Option<String>,
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
    let mut records = records
        .into_iter()
        .filter(|record| validate_transfer_history_record(record).is_ok())
        .collect::<Vec<_>>();
    normalize_transfer_history_records(&mut records);
    records.truncate(TRANSFER_HISTORY_LIMIT);
    Ok(records)
}

pub fn push_transfer_history_record(
    records: &Arc<Mutex<Vec<TransferHistoryRecord>>>,
    record: TransferHistoryRecord,
) -> Result<(), String> {
    validate_transfer_history_record(&record)?;

    let mut records = records.lock().map_err(|error| error.to_string())?;
    records.retain(|item| item.id != record.id);
    records.insert(0, record);
    normalize_transfer_history_records(&mut records);
    records.truncate(TRANSFER_HISTORY_LIMIT);
    save_transfer_history(&records)
}

fn normalize_transfer_history_records(records: &mut Vec<TransferHistoryRecord>) {
    sort_transfer_history_records(records);
    let mut seen_ids = HashSet::new();
    records.retain(|record| seen_ids.insert(record.id.clone()));
}

fn sort_transfer_history_records(records: &mut [TransferHistoryRecord]) {
    records.sort_by(|left, right| {
        right
            .updated_at_ms
            .cmp(&left.updated_at_ms)
            .then_with(|| right.created_at_ms.cmp(&left.created_at_ms))
            .then_with(|| left.id.cmp(&right.id))
    });
}

pub fn delete_transfer_history_record(
    records: &Arc<Mutex<Vec<TransferHistoryRecord>>>,
    transfer_id: &str,
) -> Result<(), String> {
    let mut records = records.lock().map_err(|error| error.to_string())?;
    let before_len = records.len();
    records.retain(|item| item.id != transfer_id);
    if records.len() == before_len {
        return Err("找不到这条传输历史".to_string());
    }
    save_transfer_history(&records)
}

pub fn clear_transfer_history_records(
    records: &Arc<Mutex<Vec<TransferHistoryRecord>>>,
) -> Result<(), String> {
    let mut records = records.lock().map_err(|error| error.to_string())?;
    records.clear();
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
        security_mode: None,
        file_count,
        total_bytes,
        transferred_bytes,
        receive_dir: None,
        error_message: None,
        created_at_ms,
        updated_at_ms: created_at_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_transfer_history_by_latest_update() {
        let mut records = vec![
            new_transfer_history_record("old".to_string(), "send", "failed", "old", 1, 10, 3, 10),
            new_transfer_history_record(
                "new".to_string(),
                "receive",
                "completed",
                "new",
                1,
                10,
                10,
                20,
            ),
            new_transfer_history_record(
                "middle".to_string(),
                "send",
                "completed",
                "middle",
                1,
                10,
                10,
                15,
            ),
        ];
        records[0].updated_at_ms = 30;
        records[1].updated_at_ms = 50;
        records[2].updated_at_ms = 40;

        sort_transfer_history_records(&mut records);

        assert_eq!(records[0].id, "new");
        assert_eq!(records[1].id, "middle");
        assert_eq!(records[2].id, "old");
    }

    #[test]
    fn normalizes_duplicate_transfer_ids_to_latest_record() {
        let mut records = vec![
            new_transfer_history_record(
                "transfer-a".to_string(),
                "send",
                "failed",
                "old",
                1,
                10,
                3,
                10,
            ),
            new_transfer_history_record(
                "transfer-a".to_string(),
                "send",
                "completed",
                "new",
                1,
                10,
                10,
                20,
            ),
            new_transfer_history_record(
                "transfer-b".to_string(),
                "receive",
                "completed",
                "other",
                1,
                10,
                10,
                15,
            ),
        ];
        records[0].updated_at_ms = 30;
        records[1].updated_at_ms = 50;
        records[2].updated_at_ms = 40;

        normalize_transfer_history_records(&mut records);

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].id, "transfer-a");
        assert_eq!(records[0].status, "completed");
        assert_eq!(records[1].id, "transfer-b");
    }

    #[test]
    fn transfer_history_record_can_store_optional_security_mode() {
        let mut record = new_transfer_history_record(
            "receive-a".to_string(),
            "receive",
            "completed",
            "drop",
            1,
            10,
            10,
            20,
        );
        assert_eq!(record.security_mode, None);

        record.security_mode = Some("authenticated_encrypted_session".to_string());

        assert_eq!(
            record.security_mode.as_deref(),
            Some("authenticated_encrypted_session")
        );
    }
}
