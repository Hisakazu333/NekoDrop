use super::*;

#[tauri::command(async)]
pub fn list_transfers(state: State<'_, AppState>) -> Result<Vec<TransferDto>, String> {
    let transfers = state
        .transfer_history
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(transfers.iter().map(transfer_to_dto).collect())
}

#[tauri::command(async)]
pub fn delete_transfer(state: State<'_, AppState>, transfer_id: String) -> Result<(), String> {
    delete_transfer_history_record(&state.transfer_history, &transfer_id)
}

#[tauri::command(async)]
pub fn clear_transfer_history(state: State<'_, AppState>) -> Result<(), String> {
    clear_transfer_history_records(&state.transfer_history)
}

/// 接收的文本片段直达剪贴板：读取该接收记录里唯一的 .txt（≤2MB）内容。
/// Read the single received .txt of a receive record for one-tap clipboard copy.
#[tauri::command(async)]
pub fn read_received_text(
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<String, String> {
    let record = transfer_history_record_by_id(&state, &transfer_id)?;
    read_received_text_from_record(&record).map_err(|error| error.to_string())
}

pub(crate) fn read_received_text_from_record(
    record: &TransferHistoryRecord,
) -> Result<String, NekoDropError> {
    if record.direction != "receive" {
        return Err(NekoDropError::Storage("只有接收记录能读取文本".to_string()));
    }
    if !matches!(record.status.as_str(), "succeeded" | "done") {
        return Err(NekoDropError::Storage(
            "传输未成功完成，无法读取文本".to_string(),
        ));
    }
    let text_paths: Vec<&str> = record
        .received_paths
        .iter()
        .map(|path| path.as_str())
        .filter(|path| Path::new(path).extension().and_then(|ext| ext.to_str()) == Some("txt"))
        .collect();
    if text_paths.len() != 1 {
        return Err(NekoDropError::Storage(
            "这条记录不是单个文本片段".to_string(),
        ));
    }
    let path = Path::new(text_paths[0]);
    let metadata = std::fs::metadata(path).map_err(|error| NekoDropError::Io {
        kind: error.kind().to_string(),
        message: error.to_string(),
    })?;
    const MAX_RECEIVED_TEXT_BYTES: u64 = 2 * 1024 * 1024;
    if metadata.len() > MAX_RECEIVED_TEXT_BYTES {
        return Err(NekoDropError::Storage("文本超过 2 MB 上限".to_string()));
    }
    std::fs::read_to_string(path).map_err(|error| NekoDropError::Io {
        kind: error.kind().to_string(),
        message: error.to_string(),
    })
}

pub(crate) fn transfer_history_record_by_id(
    state: &AppState,
    transfer_id: &str,
) -> Result<TransferHistoryRecord, String> {
    let transfers = state
        .transfer_history
        .lock()
        .map_err(|error| error.to_string())?;
    transfers
        .iter()
        .find(|record| record.id == transfer_id)
        .cloned()
        .ok_or_else(|| "找不到这条传输历史".to_string())
}
