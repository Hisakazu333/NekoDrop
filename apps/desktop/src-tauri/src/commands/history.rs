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
