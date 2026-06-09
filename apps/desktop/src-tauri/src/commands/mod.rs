use nekodrop_core::{Device, TransferJob};
use serde::Serialize;
use tauri::State;

use crate::app_state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct AppSnapshot {
    pub device_name: String,
    pub receive_dir: String,
    pub discovery_enabled: bool,
    pub tray_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceDto {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub host: String,
    pub port: u16,
    pub trust_state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransferDto {
    pub id: String,
    pub peer_device_id: String,
    pub direction: String,
    pub status: String,
    pub file_count: usize,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub progress: f32,
}

#[tauri::command]
pub fn get_app_snapshot(state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    let config = state.config.lock().map_err(|error| error.to_string())?;
    Ok(AppSnapshot {
        device_name: config.device_name.clone(),
        receive_dir: config.receive_dir.clone(),
        discovery_enabled: config.discovery_enabled,
        tray_enabled: config.tray_enabled,
    })
}

#[tauri::command]
pub fn list_nearby_devices(state: State<'_, AppState>) -> Result<Vec<DeviceDto>, String> {
    let devices = state.nearby_devices.lock().map_err(|error| error.to_string())?;
    Ok(devices.iter().map(device_to_dto).collect())
}

#[tauri::command]
pub fn list_transfers(state: State<'_, AppState>) -> Result<Vec<TransferDto>, String> {
    let transfers = state.transfers.lock().map_err(|error| error.to_string())?;
    Ok(transfers.iter().map(transfer_to_dto).collect())
}

fn device_to_dto(device: &Device) -> DeviceDto {
    DeviceDto {
        id: device.id.as_str().to_string(),
        name: device.name.clone(),
        platform: format!("{:?}", device.platform),
        host: device.host.clone(),
        port: device.port,
        trust_state: format!("{:?}", device.trust_state),
    }
}

fn transfer_to_dto(job: &TransferJob) -> TransferDto {
    TransferDto {
        id: job.id.as_str().to_string(),
        peer_device_id: job.peer_device_id.as_str().to_string(),
        direction: format!("{:?}", job.direction),
        status: format!("{:?}", job.status),
        file_count: job.manifest.file_count(),
        total_bytes: job.manifest.total_bytes(),
        transferred_bytes: job.transferred_bytes,
        progress: job.progress(),
    }
}
