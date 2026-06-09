use std::path::PathBuf;

use nekodrop_core::{Device, FileManifest, ManifestItem, ManifestItemKind, TransferJob};
use nekodrop_service::{
    create_transfer_plan as create_service_transfer_plan, TransferSourceFile, TransferSourcePlan,
};
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

#[derive(Debug, Clone, Serialize)]
pub struct ManifestItemDto {
    pub path: String,
    pub kind: String,
    pub size: u64,
    pub modified_at: Option<String>,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransferSourceFileDto {
    pub manifest_path: String,
    pub source_path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransferPlanDto {
    pub root_name: String,
    pub file_count: usize,
    pub total_bytes: u64,
    pub items: Vec<ManifestItemDto>,
    pub files: Vec<TransferSourceFileDto>,
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
    let devices = state
        .nearby_devices
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(devices.iter().map(device_to_dto).collect())
}

#[tauri::command]
pub fn list_transfers(state: State<'_, AppState>) -> Result<Vec<TransferDto>, String> {
    let transfers = state.transfers.lock().map_err(|error| error.to_string())?;
    Ok(transfers.iter().map(transfer_to_dto).collect())
}

#[tauri::command]
pub fn create_transfer_plan(paths: Vec<String>) -> Result<TransferPlanDto, String> {
    let paths = paths.into_iter().map(PathBuf::from).collect::<Vec<_>>();
    let plan = create_service_transfer_plan(&paths).map_err(|error| error.to_string())?;
    Ok(source_plan_to_dto(&plan))
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

fn source_plan_to_dto(plan: &TransferSourcePlan) -> TransferPlanDto {
    TransferPlanDto {
        root_name: plan.manifest.root_name.clone(),
        file_count: plan.file_count(),
        total_bytes: plan.total_bytes(),
        items: manifest_items_to_dto(&plan.manifest),
        files: plan.files.iter().map(source_file_to_dto).collect(),
    }
}

fn manifest_items_to_dto(manifest: &FileManifest) -> Vec<ManifestItemDto> {
    manifest.items.iter().map(manifest_item_to_dto).collect()
}

fn manifest_item_to_dto(item: &ManifestItem) -> ManifestItemDto {
    ManifestItemDto {
        path: item.path.clone(),
        kind: match item.kind {
            ManifestItemKind::File => "file",
            ManifestItemKind::Directory => "directory",
        }
        .to_string(),
        size: item.size,
        modified_at: item.modified_at.clone(),
        sha256: item.sha256.clone(),
    }
}

fn source_file_to_dto(file: &TransferSourceFile) -> TransferSourceFileDto {
    TransferSourceFileDto {
        manifest_path: file.manifest_path.clone(),
        source_path: file.source_path.display().to_string(),
        size: file.size,
        sha256: file.sha256.clone(),
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
