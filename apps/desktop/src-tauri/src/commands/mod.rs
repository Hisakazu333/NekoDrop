use std::fs;
use std::io::ErrorKind;
use std::net::{IpAddr, SocketAddr, TcpListener};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nekodrop_core::{
    Device, DeviceTrustState, FileManifest, ManifestItem, ManifestItemKind, ReceivePolicy,
};
use nekodrop_network::{
    ConnectionTicket, Endpoint, PairingDecisionPayload, PairingRequestPayload, TransferOffer,
    TransferProgress,
};
use nekodrop_service::{
    accept_incoming_stream_with_encrypted_control_bundle_staging_and_cancel,
    create_transfer_plan as create_service_transfer_plan, create_transfer_plan_with_scan_progress,
    send_pairing_request, send_plan_with_encrypted_control_and_cancel, IncomingSessionReport,
    ReceivedBundleReport, TransferPlanScanProgress, TransferProgressEvent, TransferReceiveReport,
    TransferSendReport, TransferSourceFile, TransferSourcePlan,
};
use nekodrop_storage::{build_resume_plan_for_files, ResumeExpectedFile, ResumePlan};
use nekolink_protocol::DeviceIdentity;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::app_config::{receive_policy_label, save_app_config};
use crate::app_state::{
    ActiveReceiveSession, AppState, PendingPairingRequest, PendingReceiveFile, PendingReceiveOffer,
    PendingReceiveResumeSummary, ReceiveDecision, TransferStatusState,
};
use crate::device_identity::app_config_dir;
use crate::network::{local_lan_ips, primary_lan_ip};
use crate::transfer_history::{
    clear_transfer_history_records, delete_transfer_history_record, new_transfer_history_record,
    push_transfer_history_record, TransferHistoryRecord,
};
use crate::trusted_devices::{
    pairing_code_for_device, pairing_code_for_values, refresh_trusted_device_contact,
    save_trusted_devices, trust_device_record, trusted_device_record_from_remote,
    trusted_record_matches, upsert_trusted_device, TrustedDeviceRecord,
};

const TRANSFER_SCAN_PROGRESS_EVENT: &str = "transfer_scan_progress";
const RECEIVE_FILE_PREVIEW_LIMIT: usize = 20;

#[derive(Debug, Clone, Serialize)]
pub struct AppSnapshot {
    pub device_name: String,
    pub receive_dir: String,
    pub receive_port: u16,
    pub receive_policy: String,
    pub discovery_enabled: bool,
    pub tray_enabled: bool,
    pub device_identity: DeviceIdentityDto,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceIdentityDto {
    pub device_id: String,
    pub device_name: String,
    pub device_kind: String,
    pub platform: String,
    pub public_key_fingerprint: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceDto {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub host: String,
    pub port: u16,
    pub trust_state: String,
    pub public_key_fingerprint: Option<String>,
    pub pairing_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrustedDeviceDto {
    pub device_id: String,
    pub device_name: String,
    pub platform: String,
    pub host: String,
    pub port: u16,
    pub public_key_fingerprint: String,
    pub pairing_code: String,
    pub paired_at_ms: u128,
    pub last_seen_at_ms: u128,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransferDto {
    pub id: String,
    pub root_name: String,
    pub peer_device_id: Option<String>,
    pub peer_name: Option<String>,
    pub target_host: Option<String>,
    pub source_paths: Vec<String>,
    pub received_paths: Vec<String>,
    pub direction: String,
    pub status: String,
    pub file_count: usize,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub progress: f32,
    pub receive_dir: Option<String>,
    pub error_message: Option<String>,
    pub created_at_ms: u128,
    pub updated_at_ms: u128,
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

#[derive(Debug, Clone, Serialize)]
pub struct TransferScanProgressDto {
    pub phase: String,
    pub current_path: Option<String>,
    pub files_found: usize,
    pub directories_found: usize,
    pub bytes_found: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReceiveSessionDto {
    pub bind_addr: String,
    pub receive_dir: String,
    pub connection_code: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReceivePortDiagnosticsDto {
    pub phase: String,
    pub listening: bool,
    pub bind_addr: Option<String>,
    pub advertised_host: Option<String>,
    pub port: Option<u16>,
    pub lan_ips: Vec<String>,
    pub message: String,
    pub checks: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SentFileDto {
    pub manifest_path: String,
    pub bytes_sent: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SendReportDto {
    pub root_name: String,
    pub file_count: usize,
    pub total_bytes: u64,
    pub sent_files: Vec<SentFileDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReceivedFileDto {
    pub path: String,
    pub manifest_path: String,
    pub bytes_written: u64,
    pub sha256: String,
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReceivedBundleDto {
    pub bundle_id: String,
    pub bundle_type: String,
    pub display_name: String,
    pub source_app: String,
    pub file_count: usize,
    pub total_bytes: u64,
    pub staging_path: String,
    pub import_allowed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReceiveReportDto {
    pub transfer_id: String,
    pub root_name: String,
    pub sender_device_id: Option<String>,
    pub sender_device_name: Option<String>,
    pub sender_public_key_fingerprint: Option<String>,
    pub file_count: usize,
    pub bundle: Option<ReceivedBundleDto>,
    pub files: Vec<ReceivedFileDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingReceiveFileDto {
    pub manifest_path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReceiveResumeSummaryDto {
    pub resumable_file_count: usize,
    pub completed_file_count: usize,
    pub partial_file_count: usize,
    pub received_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingReceiveOfferDto {
    pub transfer_id: String,
    pub root_name: String,
    pub file_count: usize,
    pub total_bytes: u64,
    pub sender_device_id: Option<String>,
    pub sender_device_name: Option<String>,
    pub sender_public_key_fingerprint: Option<String>,
    pub preview_file_count: usize,
    pub files: Vec<PendingReceiveFileDto>,
    pub resume_summary: Option<ReceiveResumeSummaryDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingPairingRequestDto {
    pub request_id: String,
    pub device_id: String,
    pub device_name: String,
    pub platform: String,
    pub host: String,
    pub port: u16,
    pub public_key_fingerprint: String,
    pub pairing_code: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransferStatusDto {
    pub direction: String,
    pub phase: String,
    pub root_name: Option<String>,
    pub file_count: usize,
    pub file_index: usize,
    pub current_file: Option<String>,
    pub bytes_transferred: u64,
    pub total_bytes: u64,
    pub progress: f32,
    pub message: String,
    pub updated_at_ms: u128,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveryStatusDto {
    pub phase: String,
    pub message: String,
    pub service_type: String,
    pub advertised: bool,
    pub lan_ip: Option<String>,
    pub port: Option<u16>,
    pub device_count: usize,
    pub last_seen_seconds_ago: Option<u64>,
    pub last_error: Option<String>,
}

#[tauri::command]
pub fn get_app_snapshot(state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    let config = state.config.lock().map_err(|error| error.to_string())?;
    let identity = state.device_identity.public_identity();
    Ok(AppSnapshot {
        device_name: config.device_name.clone(),
        receive_dir: config.receive_dir.clone(),
        receive_port: config.receive_port,
        receive_policy: receive_policy_label(config.receive_policy).to_string(),
        discovery_enabled: config.discovery_enabled,
        tray_enabled: config.tray_enabled,
        device_identity: device_identity_to_dto(&identity),
    })
}

#[tauri::command]
pub fn list_nearby_devices(state: State<'_, AppState>) -> Result<Vec<DeviceDto>, String> {
    let devices = state
        .nearby_devices
        .lock()
        .map_err(|error| error.to_string())?;
    let trusted_devices = state
        .trusted_devices
        .lock()
        .map_err(|error| error.to_string())?;
    let local_identity = state.device_identity.public_identity();
    Ok(devices
        .iter()
        .map(|device| device_to_dto(device, &local_identity, &trusted_devices))
        .collect())
}

#[tauri::command]
pub fn get_discovery_status(state: State<'_, AppState>) -> Result<DiscoveryStatusDto, String> {
    let status = state
        .discovery_status
        .lock()
        .map_err(|error| error.to_string())?;
    let device_count = state
        .nearby_devices
        .lock()
        .map_err(|error| error.to_string())?
        .len();

    Ok(DiscoveryStatusDto {
        phase: status.phase.clone(),
        message: status.message.clone(),
        service_type: status.service_type.clone(),
        advertised: status.advertised,
        lan_ip: status.lan_ip.clone(),
        port: status.port,
        device_count,
        last_seen_seconds_ago: status
            .last_seen_at
            .map(|seen_at| seen_at.elapsed().as_secs()),
        last_error: status.last_error.clone(),
    })
}

#[tauri::command]
pub fn list_trusted_devices(state: State<'_, AppState>) -> Result<Vec<TrustedDeviceDto>, String> {
    let trusted_devices = state
        .trusted_devices
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(trusted_devices.iter().map(trusted_device_to_dto).collect())
}

#[tauri::command]
pub fn trust_nearby_device(
    state: State<'_, AppState>,
    device_id: String,
) -> Result<TrustedDeviceDto, String> {
    let device = {
        let devices = state
            .nearby_devices
            .lock()
            .map_err(|error| error.to_string())?;
        devices
            .iter()
            .find(|device| device.id.as_str() == device_id)
            .cloned()
            .ok_or_else(|| "设备不在线或尚未被自动扫描到".to_string())?
    };

    let local_identity = state.device_identity.public_identity();
    let record = trust_device_record(&local_identity, &device)?;
    {
        let mut trusted_devices = state
            .trusted_devices
            .lock()
            .map_err(|error| error.to_string())?;
        let mut next_trusted_devices = trusted_devices.clone();
        upsert_trusted_device(&mut next_trusted_devices, record.clone());
        save_trusted_devices(&next_trusted_devices)?;
        *trusted_devices = next_trusted_devices;
    }
    if let Ok(mut devices) = state.nearby_devices.lock() {
        if let Some(device) = devices
            .iter_mut()
            .find(|device| device.id.as_str() == device_id)
        {
            device.trust_state = DeviceTrustState::Trusted;
        }
    }

    Ok(trusted_device_to_dto(&record))
}

#[tauri::command]
pub fn request_device_pairing(
    state: State<'_, AppState>,
    device_id: String,
) -> Result<TrustedDeviceDto, String> {
    let device = {
        let devices = state
            .nearby_devices
            .lock()
            .map_err(|error| error.to_string())?;
        devices
            .iter()
            .find(|device| device.id.as_str() == device_id)
            .cloned()
            .ok_or_else(|| "设备不在线或尚未被自动扫描到".to_string())?
    };
    let listen_port = current_receive_session_port(&state)?
        .ok_or_else(|| "请先打开后台收件，再发起配对。".to_string())?;
    let local_identity = state.device_identity.public_identity();
    let pairing_code = pairing_code_for_device(&local_identity, &device)
        .ok_or_else(|| "这个设备缺少公开指纹，当前不能发起配对。".to_string())?;
    let request = PairingRequestPayload {
        request_id: format!("pairing-{}", now_ms()),
        device_id: local_identity.device_id.clone(),
        device_name: local_identity.device_name.clone(),
        platform: local_identity.platform.as_str().to_string(),
        public_key_fingerprint: local_identity.public_key_fingerprint.clone(),
        pairing_code,
        listen_port,
    };
    let endpoint = Endpoint::tcp(device.host.clone(), device.port);
    validate_endpoint_for_desktop_send(&endpoint)?;
    let decision = send_pairing_request(&endpoint, request)
        .map_err(|error| friendly_transfer_error(&error.to_string()))?;
    if !decision.accepted {
        return Err(format!(
            "对方拒绝配对：{}",
            decision.reason.unwrap_or_else(|| "未提供原因".to_string())
        ));
    }

    let record = trust_device_record(&local_identity, &device)?;
    persist_trusted_device(&state, record.clone())?;
    if let Ok(mut devices) = state.nearby_devices.lock() {
        if let Some(device) = devices
            .iter_mut()
            .find(|device| device.id.as_str() == device_id)
        {
            device.trust_state = DeviceTrustState::Trusted;
        }
    }
    Ok(trusted_device_to_dto(&record))
}

#[tauri::command]
pub fn forget_trusted_device(state: State<'_, AppState>, device_id: String) -> Result<(), String> {
    {
        let mut trusted_devices = state
            .trusted_devices
            .lock()
            .map_err(|error| error.to_string())?;
        let mut next_trusted_devices = trusted_devices.clone();
        next_trusted_devices.retain(|device| device.device_id != device_id);
        save_trusted_devices(&next_trusted_devices)?;
        *trusted_devices = next_trusted_devices;
    }
    if let Ok(mut devices) = state.nearby_devices.lock() {
        if let Some(device) = devices
            .iter_mut()
            .find(|device| device.id.as_str() == device_id)
        {
            device.trust_state = DeviceTrustState::Untrusted;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_pending_pairing_request(
    state: State<'_, AppState>,
) -> Result<Option<PendingPairingRequestDto>, String> {
    let request = state
        .pending_pairing_request
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(request.as_ref().map(pending_pairing_request_to_dto))
}

#[tauri::command]
pub fn respond_pairing_request(state: State<'_, AppState>, accept: bool) -> Result<(), String> {
    let request = state
        .pending_pairing_request
        .lock()
        .map_err(|error| error.to_string())?
        .clone()
        .ok_or_else(|| "当前没有等待确认的配对请求".to_string())?;
    let (decision_lock, decision_cvar) = &*request.decision;
    let mut decision = decision_lock.lock().map_err(|error| error.to_string())?;
    *decision = Some(if accept {
        ReceiveDecision::Accept
    } else {
        ReceiveDecision::Decline
    });
    decision_cvar.notify_all();
    if let Ok(mut pending) = state.pending_pairing_request.lock() {
        *pending = None;
    }
    Ok(())
}

#[tauri::command]
pub fn list_transfers(state: State<'_, AppState>) -> Result<Vec<TransferDto>, String> {
    let transfers = state
        .transfer_history
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(transfers.iter().map(transfer_to_dto).collect())
}

#[tauri::command]
pub fn delete_transfer(state: State<'_, AppState>, transfer_id: String) -> Result<(), String> {
    delete_transfer_history_record(&state.transfer_history, &transfer_id)
}

#[tauri::command]
pub fn clear_transfer_history(state: State<'_, AppState>) -> Result<(), String> {
    clear_transfer_history_records(&state.transfer_history)
}

#[tauri::command]
pub fn create_transfer_plan(app: AppHandle, paths: Vec<String>) -> Result<TransferPlanDto, String> {
    let paths = string_paths_to_path_bufs(paths)?;
    let plan = create_transfer_plan_with_scan_progress(&paths, |progress| {
        emit_transfer_scan_progress(&app, progress);
    })
    .map_err(|error| error.to_string())?;
    Ok(source_plan_to_dto(&plan))
}

#[tauri::command]
pub fn create_transfer_plan_from_text(
    app: AppHandle,
    paths_text: String,
) -> Result<TransferPlanDto, String> {
    let paths = parse_paths_text(&paths_text)?;
    let plan = create_transfer_plan_with_scan_progress(&paths, |progress| {
        emit_transfer_scan_progress(&app, progress);
    })
    .map_err(|error| error.to_string())?;
    Ok(source_plan_to_dto(&plan))
}

#[tauri::command]
pub fn send_paths_to_code(
    state: State<'_, AppState>,
    connection_code: String,
    paths_text: String,
) -> Result<SendReportDto, String> {
    let (endpoint, peer) = endpoint_and_peer_from_connection_input(&connection_code)?;
    send_paths_to_endpoint(&state, endpoint, paths_text, peer)
}

#[tauri::command]
pub fn send_paths_to_device(
    state: State<'_, AppState>,
    device_id: String,
    paths_text: String,
) -> Result<SendReportDto, String> {
    let (endpoint, peer) = endpoint_and_peer_for_device_id(&state, &device_id)?;
    send_paths_to_endpoint(&state, endpoint, paths_text, peer)
}

#[tauri::command]
pub fn resend_transfer(
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<SendReportDto, String> {
    let record = transfer_history_record_by_id(&state, &transfer_id)?;
    if record.direction != "send" {
        return Err("接收记录不能重发".to_string());
    }
    if record.source_paths.is_empty() {
        return Err("这条历史没有可重发的源路径".to_string());
    }

    let (endpoint, peer) = endpoint_and_peer_for_history_record(&state, &record)?;
    send_paths_to_endpoint_with_history_id(
        &state,
        endpoint,
        record.source_paths.join("\n"),
        peer,
        Some(record.id.clone()),
    )
}

#[tauri::command]
pub fn open_transfer_location(
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<(), String> {
    let record = transfer_history_record_by_id(&state, &transfer_id)?;
    let path = record
        .received_paths
        .first()
        .or_else(|| record.source_paths.first())
        .or(record.receive_dir.as_ref())
        .map(|value| expand_home_dir(value))
        .ok_or_else(|| "这条历史没有可打开的位置".to_string())?;
    let target = if path.exists() {
        path
    } else {
        path.parent()
            .filter(|parent| parent.exists())
            .map(PathBuf::from)
            .ok_or_else(|| format!("路径不存在：{}", path.display()))?
    };

    open_path_with_system(target)
}

#[derive(Debug, Clone)]
struct TransferPeer {
    device_id: Option<String>,
    name: Option<String>,
    fingerprint: Option<String>,
    target_host: Option<String>,
}

fn endpoint_and_peer_for_device_id(
    state: &AppState,
    device_id: &str,
) -> Result<(Endpoint, TransferPeer), String> {
    if let Some((endpoint, peer)) = endpoint_and_peer_from_nearby_device(state, device_id)? {
        return Ok((endpoint, peer));
    }
    if let Some((endpoint, peer)) = endpoint_and_peer_from_trusted_device(state, device_id)? {
        return Ok((endpoint, peer));
    }
    Err("设备不在线或尚未被自动扫描到，请确认对方收件开启后重试。".to_string())
}

fn endpoint_and_peer_from_nearby_device(
    state: &AppState,
    device_id: &str,
) -> Result<Option<(Endpoint, TransferPeer)>, String> {
    let device = {
        let devices = state
            .nearby_devices
            .lock()
            .map_err(|error| error.to_string())?;
        devices
            .iter()
            .find(|item| item.id.as_str() == device_id)
            .cloned()
    };
    let Some(device) = device else {
        return Ok(None);
    };

    let trusted_devices = state
        .trusted_devices
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(Some(trusted_peer_from_nearby_device(
        &device,
        &trusted_devices,
    )?))
}

fn trusted_peer_from_nearby_device(
    device: &Device,
    trusted_devices: &[TrustedDeviceRecord],
) -> Result<(Endpoint, TransferPeer), String> {
    let is_trusted = trusted_devices
        .iter()
        .any(|record| trusted_record_matches(device, record));
    if !is_trusted {
        return Err("这台设备还没有可信配对，请先完成配对再发送文件。".to_string());
    }

    let endpoint = Endpoint::tcp(device.host.clone(), device.port);
    let peer = TransferPeer {
        device_id: Some(device.id.as_str().to_string()),
        name: Some(device.name.clone()),
        fingerprint: device.public_key_fingerprint.clone(),
        target_host: Some(endpoint_label(&endpoint)),
    };
    Ok((endpoint, peer))
}

fn reject_self_peer(local_identity: &DeviceIdentity, peer: &TransferPeer) -> Result<(), String> {
    if peer
        .device_id
        .as_deref()
        .is_some_and(|device_id| device_id == local_identity.device_id)
    {
        return Err("不能把文件发送给本机，请选择另一台设备。".to_string());
    }
    Ok(())
}

fn endpoint_and_peer_from_trusted_device(
    state: &AppState,
    device_id: &str,
) -> Result<Option<(Endpoint, TransferPeer)>, String> {
    let trusted_devices = state
        .trusted_devices
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(trusted_devices
        .iter()
        .find(|item| item.device_id == device_id)
        .map(|device| {
            let endpoint = Endpoint::tcp(device.host.clone(), device.port);
            let peer = TransferPeer {
                device_id: Some(device.device_id.clone()),
                name: Some(device.device_name.clone()),
                fingerprint: Some(device.public_key_fingerprint.clone()),
                target_host: Some(endpoint_label(&endpoint)),
            };
            (endpoint, peer)
        }))
}

fn endpoint_and_peer_for_history_record(
    state: &AppState,
    record: &TransferHistoryRecord,
) -> Result<(Endpoint, TransferPeer), String> {
    if let Some(device_id) = record.peer_device_id.as_deref() {
        return endpoint_and_peer_for_device_id(state, device_id)
            .map_err(|error| format!("这条历史记录绑定的设备当前不能重发：{error}"));
    }

    let target_host = record
        .target_host
        .as_deref()
        .ok_or_else(|| "这条历史没有可重连的目标地址".to_string())?;
    let endpoint = endpoint_from_label(target_host)?;
    let peer = TransferPeer {
        device_id: record.peer_device_id.clone(),
        name: record.peer_name.clone(),
        fingerprint: None,
        target_host: Some(endpoint_label(&endpoint)),
    };
    Ok((endpoint, peer))
}

fn endpoint_and_peer_from_connection_input(
    value: &str,
) -> Result<(Endpoint, TransferPeer), String> {
    match ConnectionTicket::parse(value) {
        Ok(ticket) => {
            let endpoint = ticket.endpoint.clone();
            let peer = TransferPeer {
                device_id: ticket.device_id.clone(),
                name: ticket.device_name.clone(),
                fingerprint: ticket.fingerprint.clone(),
                target_host: Some(endpoint_label(&endpoint)),
            };
            Ok((endpoint, peer))
        }
        Err(error) => {
            if looks_like_endpoint_label(value) {
                let endpoint = endpoint_from_label(value)?;
                let peer = TransferPeer {
                    device_id: None,
                    name: None,
                    fingerprint: None,
                    target_host: Some(endpoint_label(&endpoint)),
                };
                return Ok((endpoint, peer));
            }
            Err(friendly_transfer_error(&error.to_string()))
        }
    }
}

fn looks_like_endpoint_label(value: &str) -> bool {
    let value = value.trim();
    !value.starts_with("nekodrop-v1") && value.rsplit_once(':').is_some()
}

fn clear_active_send_cancel(
    active_send_cancel: &Arc<Mutex<Option<Arc<AtomicBool>>>>,
    cancel: &Arc<AtomicBool>,
) {
    if let Ok(mut active) = active_send_cancel.lock() {
        if active
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, cancel))
        {
            *active = None;
        }
    }
}

fn clear_active_receive_cancel(
    active_receive_cancel: &Arc<Mutex<Option<Arc<AtomicBool>>>>,
    cancel: &Arc<AtomicBool>,
) {
    if let Ok(mut active) = active_receive_cancel.lock() {
        if active
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, cancel))
        {
            *active = None;
        }
    }
}

fn transfer_history_record_by_id(
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

fn send_paths_to_endpoint(
    state: &AppState,
    endpoint: Endpoint,
    paths_text: String,
    peer: TransferPeer,
) -> Result<SendReportDto, String> {
    send_paths_to_endpoint_with_history_id(state, endpoint, paths_text, peer, None)
}

fn send_paths_to_endpoint_with_history_id(
    state: &AppState,
    endpoint: Endpoint,
    paths_text: String,
    peer: TransferPeer,
    history_id_override: Option<String>,
) -> Result<SendReportDto, String> {
    validate_endpoint_for_desktop_send(&endpoint)?;
    let paths = parse_paths_text(&paths_text)?;
    let source_paths = path_bufs_to_strings(&paths);
    let plan = create_service_transfer_plan(&paths).map_err(|error| error.to_string())?;
    let sender_identity = state.device_identity.public_identity();
    reject_self_peer(&sender_identity, &peer)?;
    let started_at_ms = now_ms();
    let transfer_id = history_transfer_id(started_at_ms, history_id_override.as_deref());
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut active_cancel = state
            .active_send_cancel
            .lock()
            .map_err(|error| error.to_string())?;
        if active_cancel.is_some() {
            return Err("已有发送任务进行中".to_string());
        }
        *active_cancel = Some(cancel.clone());
    }
    set_transfer_status(
        &state.transfer_status,
        TransferStatusState {
            direction: "send".to_string(),
            phase: "connecting".to_string(),
            root_name: Some(plan.manifest.root_name.clone()),
            file_count: plan.file_count(),
            file_index: 0,
            current_file: None,
            bytes_transferred: 0,
            total_bytes: plan.total_bytes(),
            message: "正在连接对方电脑".to_string(),
            updated_at_ms: now_ms(),
        },
    );
    let transfer_status = state.transfer_status.clone();
    let cancel_for_send = cancel.clone();
    let report = send_with_auto_retry(
        || {
            let transfer_status = transfer_status.clone();
            let cancel_for_attempt = cancel_for_send.clone();
            send_plan_with_encrypted_control_and_cancel(
                &endpoint,
                plan.clone(),
                &sender_identity,
                move |event| {
                    if let Some(status) = status_from_progress_event("send", None, event) {
                        set_transfer_status(&transfer_status, status);
                    }
                },
                || cancel_for_attempt.load(Ordering::SeqCst),
            )
            .map_err(|error| error.to_string())
        },
        |retry_number, retry_limit, error| {
            let (file_index, current_file, bytes_transferred, _) =
                current_transfer_progress(&state.transfer_status);
            set_transfer_status(
                &state.transfer_status,
                TransferStatusState {
                    direction: "send".to_string(),
                    phase: "retrying".to_string(),
                    root_name: Some(plan.manifest.root_name.clone()),
                    file_count: plan.file_count(),
                    file_index,
                    current_file,
                    bytes_transferred,
                    total_bytes: plan.total_bytes(),
                    message: format!(
                        "连接中断，正在自动重试 {retry_number}/{retry_limit}：{}",
                        friendly_transfer_error(error)
                    ),
                    updated_at_ms: now_ms(),
                },
            );
        },
    )
    .map_err(|error| {
        let cancelled = cancel.load(Ordering::SeqCst) || error.contains("transfer cancelled");
        let message = if cancelled {
            "传输已取消".to_string()
        } else {
            friendly_transfer_error(&error)
        };
        let status_phase = if cancelled { "cancelled" } else { "failed" };
        let (file_index, current_file, bytes_transferred, _) =
            current_transfer_progress(&state.transfer_status);
        clear_active_send_cancel(&state.active_send_cancel, &cancel);
        set_transfer_status(
            &state.transfer_status,
            TransferStatusState {
                direction: "send".to_string(),
                phase: status_phase.to_string(),
                root_name: Some(plan.manifest.root_name.clone()),
                file_count: plan.file_count(),
                file_index,
                current_file,
                bytes_transferred,
                total_bytes: plan.total_bytes(),
                message: message.clone(),
                updated_at_ms: now_ms(),
            },
        );
        let mut record = new_transfer_history_record(
            transfer_id.clone(),
            "send",
            status_phase,
            plan.manifest.root_name.clone(),
            plan.file_count(),
            plan.total_bytes(),
            bytes_transferred,
            started_at_ms,
        );
        record.peer_device_id = peer.device_id.clone();
        record.peer_name = peer.name.clone();
        record.target_host = peer.target_host.clone();
        record.source_paths = source_paths.clone();
        if !cancelled {
            record.error_message = Some(message.clone());
        }
        record.updated_at_ms = now_ms();
        let _ = push_transfer_history_record(&state.transfer_history, record);
        message
    })?;
    clear_active_send_cancel(&state.active_send_cancel, &cancel);
    let transferred_bytes = report.plan.total_bytes();
    set_transfer_status(
        &state.transfer_status,
        TransferStatusState {
            direction: "send".to_string(),
            phase: "completed".to_string(),
            root_name: Some(report.plan.manifest.root_name.clone()),
            file_count: report.plan.file_count(),
            file_index: report.plan.file_count(),
            current_file: None,
            bytes_transferred: report.plan.total_bytes(),
            total_bytes: report.plan.total_bytes(),
            message: "发送完成，等待对方校验结果".to_string(),
            updated_at_ms: now_ms(),
        },
    );
    let mut record = new_transfer_history_record(
        transfer_id,
        "send",
        "completed",
        report.plan.manifest.root_name.clone(),
        report.plan.file_count(),
        report.plan.total_bytes(),
        transferred_bytes,
        started_at_ms,
    );
    record.peer_device_id = peer.device_id.clone();
    record.peer_name = peer.name.clone();
    record.target_host = peer.target_host.clone();
    record.source_paths = source_paths;
    record.updated_at_ms = now_ms();
    refresh_trusted_device_contact_from_peer(&state.trusted_devices, &peer);
    let _ = push_transfer_history_record(&state.transfer_history, record);
    Ok(send_report_to_dto(&report))
}

fn history_transfer_id(started_at_ms: u128, existing_transfer_id: Option<&str>) -> String {
    existing_transfer_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("send-{started_at_ms}"))
}

const SEND_AUTO_RETRY_LIMIT: usize = 1;

fn send_with_auto_retry<T, S, R>(mut send: S, mut on_retry: R) -> Result<T, String>
where
    S: FnMut() -> Result<T, String>,
    R: FnMut(usize, usize, &str),
{
    for attempt_index in 0..=SEND_AUTO_RETRY_LIMIT {
        match send() {
            Ok(result) => return Ok(result),
            Err(error)
                if attempt_index < SEND_AUTO_RETRY_LIMIT && is_retryable_send_error(&error) =>
            {
                on_retry(attempt_index + 1, SEND_AUTO_RETRY_LIMIT, &error);
            }
            Err(error) => return Err(error),
        }
    }

    unreachable!("send retry loop always returns from success or final error")
}

fn is_retryable_send_error(error: &str) -> bool {
    let lower = error.to_lowercase();

    if lower.contains("transfer cancelled")
        || lower.contains("receiver declined")
        || lower.contains("transfer declined by receiver")
        || lower.contains("checksum")
        || lower.contains("sha-256")
        || lower.contains("sha256")
        || lower.contains("does not match accepted offer")
        || lower.contains("no such file")
        || lower.contains("not found")
        || lower.contains("路径不存在")
        || lower.contains("permission denied")
        || lower.contains("access is denied")
        || lower.contains("operation not permitted")
        || lower.contains("unsupported connection code")
        || lower.contains("invalid connection code")
        || lower.contains("invalid endpoint")
        || lower.contains("transport is not available")
        || lower.contains("unsupported transport")
        || lower.contains("requested iroh")
        || lower.contains("requested relay")
        || lower.contains("requested quic")
    {
        return false;
    }

    lower.contains("failed to connect")
        || lower.contains("connection refused")
        || lower.contains("actively refused")
        || lower.contains("connection reset")
        || lower.contains("connection aborted")
        || lower.contains("broken pipe")
        || lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("network is unreachable")
        || lower.contains("no route to host")
        || lower.contains("host unreachable")
        || lower.contains("连接尝试失败")
        || lower.contains("由于目标计算机积极拒绝")
}

#[tauri::command]
pub fn select_send_files() -> Result<Vec<String>, String> {
    choose_paths(PathDialogKind::Files)
}

#[tauri::command]
pub fn select_send_folders() -> Result<Vec<String>, String> {
    choose_paths(PathDialogKind::Folders)
}

#[tauri::command]
pub fn select_receive_dir() -> Result<Option<String>, String> {
    Ok(choose_paths(PathDialogKind::SingleFolder)?
        .into_iter()
        .next())
}

#[tauri::command]
pub fn set_receive_dir(state: State<'_, AppState>, receive_dir: String) -> Result<(), String> {
    persist_receive_dir(&state, &receive_dir)
}

#[tauri::command]
pub fn set_receive_port(state: State<'_, AppState>, receive_port: u16) -> Result<(), String> {
    persist_receive_port(&state, receive_port)
}

#[tauri::command]
pub fn set_receive_policy(
    state: State<'_, AppState>,
    receive_policy: String,
) -> Result<(), String> {
    let receive_policy = receive_policy_from_input(&receive_policy)?;
    persist_receive_policy(&state, receive_policy)
}

#[tauri::command]
pub fn set_device_name(state: State<'_, AppState>, device_name: String) -> Result<String, String> {
    persist_device_name(&state, &device_name)
}

#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    let target = expand_home_dir(path.trim());
    if !target.exists() {
        return Err(format!("路径不存在：{}", target.display()));
    }

    open_path_with_system(target)
}

#[tauri::command]
pub fn start_receive_once(
    state: State<'_, AppState>,
    bind_host: Option<String>,
    port: Option<u16>,
    receive_dir: Option<String>,
) -> Result<ReceiveSessionDto, String> {
    let bind_host = bind_host
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "0.0.0.0".to_string());
    let port = match port {
        Some(port) => port,
        None => state
            .config
            .lock()
            .map(|config| config.receive_port)
            .unwrap_or(45821),
    };
    if port == 0 {
        return Err("端口不能为 0".into());
    }

    if let Some(session) = state
        .receive_session
        .lock()
        .map_err(|error| error.to_string())?
        .clone()
    {
        return Ok(receive_session_to_dto(&session));
    }

    let receive_dir = receive_dir
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            state
                .config
                .lock()
                .map(|config| expand_home_dir(&config.receive_dir).display().to_string())
                .unwrap_or_else(|_| default_receive_dir().display().to_string())
        });
    let receive_dir_path = expand_home_dir(&receive_dir);
    fs::create_dir_all(&receive_dir_path)
        .map_err(|error| format!("无法创建接收目录 {}: {error}", receive_dir_path.display()))?;
    persist_receive_dir_path(&state, &receive_dir_path)?;

    let listener = bind_available_listener(&bind_host, port)?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("无法设置收件监听状态: {error}"))?;
    let bundle_staging_root = bundle_staging_root()?;
    fs::create_dir_all(&bundle_staging_root).map_err(|error| {
        format!(
            "无法创建 bundle 暂存目录 {}: {error}",
            bundle_staging_root.display()
        )
    })?;
    let local_addr = listener
        .local_addr()
        .map_err(|error| format!("无法读取监听地址: {error}"))?;
    let code_host = if local_addr.ip().is_unspecified() {
        primary_lan_ip()
            .map(|ip| ip.to_string())
            .ok_or_else(|| {
                "无法找到可用于其他设备连接的局域网地址，请确认已连接到同一局域网，或关闭代理/虚拟网卡后重试。".to_string()
            })?
    } else {
        local_addr.ip().to_string()
    };
    let identity = state.device_identity.public_identity();
    let connection_code = ConnectionTicket::new(Endpoint::tcp(code_host, local_addr.port()))
        .map(|ticket| ticket.with_device_identity(&identity))
        .and_then(|ticket| ticket.to_code())
        .map_err(|error| error.to_string())?;
    let bind_addr = local_addr.to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    let session = ActiveReceiveSession {
        bind_addr: bind_addr.clone(),
        receive_dir: receive_dir_path.display().to_string(),
        connection_code,
        cancel: cancel.clone(),
    };

    {
        let mut receive_status = state
            .receive_status
            .lock()
            .map_err(|error| error.to_string())?;
        *receive_status = Some("等待接收中".to_string());
    }
    {
        let mut last_receive_report = state
            .last_receive_report
            .lock()
            .map_err(|error| error.to_string())?;
        *last_receive_report = None;
    }
    {
        let mut receive_session = state
            .receive_session
            .lock()
            .map_err(|error| error.to_string())?;
        *receive_session = Some(session.clone());
    }
    set_transfer_status(
        &state.transfer_status,
        TransferStatusState {
            direction: "receive".to_string(),
            phase: "listening".to_string(),
            root_name: None,
            file_count: 0,
            file_index: 0,
            current_file: None,
            bytes_transferred: 0,
            total_bytes: 0,
            message: "收件已打开，等待连接".to_string(),
            updated_at_ms: now_ms(),
        },
    );

    let receive_status = state.receive_status.clone();
    let receive_session = state.receive_session.clone();
    let pending_receive_offer = state.pending_receive_offer.clone();
    let pending_pairing_request = state.pending_pairing_request.clone();
    let config = state.config.clone();
    let transfer_status = state.transfer_status.clone();
    let last_receive_report = state.last_receive_report.clone();
    let trusted_devices = state.trusted_devices.clone();
    let transfer_history = state.transfer_history.clone();
    let active_receive_cancel = state.active_receive_cancel.clone();
    let local_identity = state.device_identity.public_identity();
    let receive_dir_for_thread = receive_dir_path.clone();
    let bundle_staging_root_for_thread = bundle_staging_root.clone();
    thread::spawn(move || loop {
        if cancel.load(Ordering::SeqCst) {
            if let Ok(mut status) = receive_status.lock() {
                *status = Some("收件已关闭".to_string());
            }
            if let Ok(mut active_session) = receive_session.lock() {
                *active_session = None;
            }
            set_transfer_status(
                &transfer_status,
                TransferStatusState {
                    direction: "receive".to_string(),
                    phase: "closed".to_string(),
                    root_name: None,
                    file_count: 0,
                    file_index: 0,
                    current_file: None,
                    bytes_transferred: 0,
                    total_bytes: 0,
                    message: "收件已关闭".to_string(),
                    updated_at_ms: now_ms(),
                },
            );
            return;
        }

        match listener.accept() {
            Ok((mut stream, peer_addr)) => {
                if let Err(error) = stream.set_nonblocking(false) {
                    set_transfer_status(
                        &transfer_status,
                        TransferStatusState {
                            direction: "receive".to_string(),
                            phase: "failed".to_string(),
                            root_name: None,
                            file_count: 0,
                            file_index: 0,
                            current_file: None,
                            bytes_transferred: 0,
                            total_bytes: 0,
                            message: format!("接收连接准备失败：{error}"),
                            updated_at_ms: now_ms(),
                        },
                    );
                    continue;
                }
                let peer_host = peer_addr.ip().to_string();
                let receive_policy = config
                    .lock()
                    .map(|config| config.receive_policy)
                    .unwrap_or(ReceivePolicy::AlwaysAsk);
                let pending_for_decision = pending_receive_offer.clone();
                let trusted_for_decision = trusted_devices.clone();
                let pending_for_pairing = pending_pairing_request.clone();
                let status_for_decision = transfer_status.clone();
                let status_for_progress = transfer_status.clone();
                let receive_dir_for_decision = receive_dir_for_thread.clone();
                let trusted_for_pairing = trusted_devices.clone();
                let local_for_pairing = local_identity.clone();
                let peer_host_for_pairing = peer_host.clone();
                let current_receive_cancel = Arc::new(AtomicBool::new(false));
                if let Ok(mut active_cancel) = active_receive_cancel.lock() {
                    *active_cancel = Some(current_receive_cancel.clone());
                }
                let result =
                    accept_incoming_stream_with_encrypted_control_bundle_staging_and_cancel(
                        &mut stream,
                        &receive_dir_for_thread,
                        &bundle_staging_root_for_thread,
                        &local_identity,
                        move |offer| {
                            let resume_summary =
                                pending_resume_summary_from_offer(&receive_dir_for_decision, offer);
                            wait_for_receive_decision(
                                offer,
                                &pending_for_decision,
                                &status_for_decision,
                                receive_policy,
                                &trusted_for_decision,
                                resume_summary,
                            )
                        },
                        move |request| {
                            wait_for_pairing_decision(
                                request,
                                &peer_host_for_pairing,
                                &pending_for_pairing,
                                &trusted_for_pairing,
                                &local_for_pairing,
                            )
                        },
                        move |event| {
                            if let Some(status) = status_from_progress_event("receive", None, event)
                            {
                                set_transfer_status(&status_for_progress, status);
                            }
                        },
                        || {
                            cancel.load(Ordering::SeqCst)
                                || current_receive_cancel.load(Ordering::SeqCst)
                        },
                    );
                clear_active_receive_cancel(&active_receive_cancel, &current_receive_cancel);
                if let Ok(mut status) = receive_status.lock() {
                    *status = Some(match &result {
                        Ok(IncomingSessionReport::Transfer(report)) => {
                            format!("接收完成：{} 个文件", report.files.len())
                        }
                        Ok(IncomingSessionReport::Pairing(decision)) if decision.accepted => {
                            "配对完成".to_string()
                        }
                        Ok(IncomingSessionReport::Pairing(_)) => "已拒绝配对".to_string(),
                        Err(_)
                            if is_receive_terminal_offer_status(&transfer_status, "declined") =>
                        {
                            "已拒绝这次传输".to_string()
                        }
                        Err(_) if is_receive_terminal_offer_status(&transfer_status, "expired") => {
                            "等待确认超时，已自动拒绝".to_string()
                        }
                        Err(_) if is_receive_terminal_offer_status(&transfer_status, "closed") => {
                            "收件已关闭".to_string()
                        }
                        Err(_) if is_receive_terminal_offer_status(&transfer_status, "blocked") => {
                            "已阻止这次传输".to_string()
                        }
                        Err(_)
                            if is_receive_terminal_offer_status(&transfer_status, "cancelled") =>
                        {
                            "接收已取消".to_string()
                        }
                        Err(error) => {
                            format!("接收失败：{}", friendly_transfer_error(&error.to_string()))
                        }
                    });
                }
                if let Ok(mut pending) = pending_receive_offer.lock() {
                    *pending = None;
                }
                if let Ok(mut pending) = pending_pairing_request.lock() {
                    *pending = None;
                }
                if let Ok(report) = result {
                    match report {
                        IncomingSessionReport::Transfer(report) => {
                            let total_bytes =
                                report.files.iter().map(|file| file.bytes_written).sum();
                            set_transfer_status(
                                &transfer_status,
                                TransferStatusState {
                                    direction: "receive".to_string(),
                                    phase: "completed".to_string(),
                                    root_name: None,
                                    file_count: report.files.len(),
                                    file_index: report.files.len(),
                                    current_file: None,
                                    bytes_transferred: total_bytes,
                                    total_bytes,
                                    message: "接收完成，继续等待下一次连接".to_string(),
                                    updated_at_ms: now_ms(),
                                },
                            );
                            let mut record = new_transfer_history_record(
                                format!("receive-{}", now_ms()),
                                "receive",
                                "completed",
                                received_root_name(&report),
                                report.files.len(),
                                total_bytes,
                                total_bytes,
                                now_ms(),
                            );
                            record.peer_device_id = report.sender_device_id.clone();
                            record.peer_name = report.sender_device_name.clone();
                            record.target_host = Some(peer_host.clone());
                            record.receive_dir = Some(receive_dir_for_thread.display().to_string());
                            record.received_paths = report
                                .files
                                .iter()
                                .map(|file| file.path.display().to_string())
                                .collect();
                            refresh_trusted_device_contact_from_receive_report(
                                &trusted_devices,
                                &report,
                            );
                            let _ = push_transfer_history_record(&transfer_history, record);
                            if let Ok(mut last_report) = last_receive_report.lock() {
                                *last_report = Some(report);
                            }
                        }
                        IncomingSessionReport::Pairing(decision) => {
                            set_transfer_status(
                                &transfer_status,
                                TransferStatusState {
                                    direction: "receive".to_string(),
                                    phase: if decision.accepted {
                                        "completed"
                                    } else {
                                        "declined"
                                    }
                                    .to_string(),
                                    root_name: None,
                                    file_count: 0,
                                    file_index: 0,
                                    current_file: None,
                                    bytes_transferred: 0,
                                    total_bytes: 0,
                                    message: if decision.accepted {
                                        "配对完成，继续等待下一次连接"
                                    } else {
                                        "已拒绝配对"
                                    }
                                    .to_string(),
                                    updated_at_ms: now_ms(),
                                },
                            );
                        }
                    }
                } else if !is_receive_terminal_offer_status(&transfer_status, "declined")
                    && !is_receive_terminal_offer_status(&transfer_status, "expired")
                    && !is_receive_terminal_offer_status(&transfer_status, "closed")
                    && !is_receive_terminal_offer_status(&transfer_status, "blocked")
                    && !is_receive_terminal_offer_status(&transfer_status, "cancelled")
                {
                    if let Ok(status) = receive_status.lock() {
                        let failure_message = status
                            .clone()
                            .unwrap_or_else(|| "接收失败，继续等待下一次连接".to_string());
                        set_transfer_status(
                            &transfer_status,
                            TransferStatusState {
                                direction: "receive".to_string(),
                                phase: "failed".to_string(),
                                root_name: None,
                                file_count: 0,
                                file_index: 0,
                                current_file: None,
                                bytes_transferred: 0,
                                total_bytes: 0,
                                message: failure_message.clone(),
                                updated_at_ms: now_ms(),
                            },
                        );
                        push_receive_failure_history(
                            &transfer_history,
                            &transfer_status,
                            &peer_host,
                            &receive_dir_for_thread,
                            failure_message,
                        );
                    }
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(120));
            }
            Err(error) => {
                if let Ok(mut status) = receive_status.lock() {
                    *status = Some(format!("接收监听异常：{error}"));
                }
                set_transfer_status(
                    &transfer_status,
                    TransferStatusState {
                        direction: "receive".to_string(),
                        phase: "failed".to_string(),
                        root_name: None,
                        file_count: 0,
                        file_index: 0,
                        current_file: None,
                        bytes_transferred: 0,
                        total_bytes: 0,
                        message: format!("接收监听异常：{error}"),
                        updated_at_ms: now_ms(),
                    },
                );
                thread::sleep(Duration::from_millis(500));
            }
        }
    });

    Ok(receive_session_to_dto(&session))
}

#[tauri::command]
pub fn stop_receive_once(state: State<'_, AppState>) -> Result<(), String> {
    let receive_was_active = is_receive_transfer_active(&state.transfer_status);
    if let Some(cancel) = state
        .active_receive_cancel
        .lock()
        .map_err(|error| error.to_string())?
        .clone()
    {
        cancel.store(true, Ordering::SeqCst);
    }

    let session = state
        .receive_session
        .lock()
        .map_err(|error| error.to_string())?
        .take();
    if let Some(session) = session {
        session.cancel.store(true, Ordering::SeqCst);
    }

    let pending = state
        .pending_receive_offer
        .lock()
        .map_err(|error| error.to_string())?
        .take();
    if let Some(offer) = pending {
        let (decision_lock, decision_cvar) = &*offer.decision;
        if let Ok(mut decision) = decision_lock.lock() {
            *decision = Some(ReceiveDecision::Decline);
            decision_cvar.notify_all();
        }
    }
    let pending_pairing = state
        .pending_pairing_request
        .lock()
        .map_err(|error| error.to_string())?
        .take();
    if let Some(request) = pending_pairing {
        let (decision_lock, decision_cvar) = &*request.decision;
        if let Ok(mut decision) = decision_lock.lock() {
            *decision = Some(ReceiveDecision::Decline);
            decision_cvar.notify_all();
        }
    }

    {
        let mut receive_status = state
            .receive_status
            .lock()
            .map_err(|error| error.to_string())?;
        *receive_status = Some(if receive_was_active {
            "正在取消接收".to_string()
        } else {
            "收件已关闭".to_string()
        });
    }
    let (file_index, current_file, bytes_transferred, total_bytes) =
        current_transfer_progress(&state.transfer_status);
    set_transfer_status(
        &state.transfer_status,
        TransferStatusState {
            direction: "receive".to_string(),
            phase: if receive_was_active {
                "cancelled"
            } else {
                "closed"
            }
            .to_string(),
            root_name: None,
            file_count: 0,
            file_index,
            current_file,
            bytes_transferred,
            total_bytes,
            message: if receive_was_active {
                "正在取消接收"
            } else {
                "收件已关闭"
            }
            .to_string(),
            updated_at_ms: now_ms(),
        },
    );
    Ok(())
}

#[tauri::command]
pub fn cancel_current_transfer(state: State<'_, AppState>) -> Result<(), String> {
    let cancel = state
        .active_send_cancel
        .lock()
        .map_err(|error| error.to_string())?
        .clone()
        .ok_or_else(|| "当前没有可取消的发送任务".to_string())?;
    cancel.store(true, Ordering::SeqCst);

    let mut transfer_status = state
        .transfer_status
        .lock()
        .map_err(|error| error.to_string())?;
    if let Some(status) = transfer_status.as_mut() {
        if status.direction == "send"
            && !matches!(status.phase.as_str(), "completed" | "failed" | "cancelled")
        {
            status.phase = "cancelled".to_string();
            status.message = "正在取消发送".to_string();
            status.updated_at_ms = now_ms();
        }
    }

    Ok(())
}

#[tauri::command]
pub fn get_receive_status(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let status = state
        .receive_status
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(status.clone())
}

#[tauri::command]
pub fn get_receive_session(
    state: State<'_, AppState>,
) -> Result<Option<ReceiveSessionDto>, String> {
    let session = state
        .receive_session
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(session.as_ref().map(receive_session_to_dto))
}

#[tauri::command]
pub fn get_receive_port_diagnostics(
    state: State<'_, AppState>,
) -> Result<ReceivePortDiagnosticsDto, String> {
    let session = state
        .receive_session
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    Ok(receive_port_diagnostics_from_session(
        session.as_ref(),
        local_lan_ips(),
    ))
}

#[tauri::command]
pub fn get_last_receive_report(
    state: State<'_, AppState>,
) -> Result<Option<ReceiveReportDto>, String> {
    let report = state
        .last_receive_report
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(report.as_ref().map(receive_report_to_dto))
}

#[tauri::command]
pub fn get_pending_receive_offer(
    state: State<'_, AppState>,
) -> Result<Option<PendingReceiveOfferDto>, String> {
    let offer = state
        .pending_receive_offer
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(offer.as_ref().map(pending_offer_to_dto))
}

#[tauri::command]
pub fn respond_receive_offer(state: State<'_, AppState>, accept: bool) -> Result<(), String> {
    let offer = state
        .pending_receive_offer
        .lock()
        .map_err(|error| error.to_string())?
        .clone()
        .ok_or_else(|| "当前没有等待确认的接收请求".to_string())?;
    let (decision_lock, decision_cvar) = &*offer.decision;
    let mut decision = decision_lock.lock().map_err(|error| error.to_string())?;
    *decision = Some(if accept {
        ReceiveDecision::Accept
    } else {
        ReceiveDecision::Decline
    });
    decision_cvar.notify_all();
    if let Ok(mut pending) = state.pending_receive_offer.lock() {
        *pending = None;
    }
    if accept {
        set_transfer_status(
            &state.transfer_status,
            TransferStatusState {
                direction: "receive".to_string(),
                phase: "accepted".to_string(),
                root_name: Some(offer.root_name),
                file_count: offer.file_count,
                file_index: 0,
                current_file: None,
                bytes_transferred: 0,
                total_bytes: offer.total_bytes,
                message: "已接受，等待对方开始发送".to_string(),
                updated_at_ms: now_ms(),
            },
        );
    } else {
        set_transfer_status(
            &state.transfer_status,
            TransferStatusState {
                direction: "receive".to_string(),
                phase: "declined".to_string(),
                root_name: Some(offer.root_name),
                file_count: offer.file_count,
                file_index: 0,
                current_file: None,
                bytes_transferred: 0,
                total_bytes: offer.total_bytes,
                message: "已拒绝这次传输".to_string(),
                updated_at_ms: now_ms(),
            },
        );
    }
    Ok(())
}

#[tauri::command]
pub fn get_transfer_status(
    state: State<'_, AppState>,
) -> Result<Option<TransferStatusDto>, String> {
    let status = state
        .transfer_status
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(status.as_ref().map(transfer_status_to_dto))
}

fn receive_session_to_dto(session: &ActiveReceiveSession) -> ReceiveSessionDto {
    ReceiveSessionDto {
        bind_addr: session.bind_addr.clone(),
        receive_dir: session.receive_dir.clone(),
        connection_code: session.connection_code.clone(),
    }
}

fn receive_port_diagnostics_from_session(
    session: Option<&ActiveReceiveSession>,
    lan_ips: Vec<IpAddr>,
) -> ReceivePortDiagnosticsDto {
    let lan_ip_labels = lan_ips.iter().map(ToString::to_string).collect::<Vec<_>>();
    let Some(session) = session else {
        return ReceivePortDiagnosticsDto {
            phase: "closed".to_string(),
            listening: false,
            bind_addr: None,
            advertised_host: None,
            port: None,
            lan_ips: lan_ip_labels,
            message: "收件未开启，当前没有监听端口".to_string(),
            checks: vec!["打开收件后才会生成连接码和监听端口".to_string()],
        };
    };

    let Some((bind_ip, port)) = parse_receive_bind_addr(&session.bind_addr) else {
        return ReceivePortDiagnosticsDto {
            phase: "invalid_bind_addr".to_string(),
            listening: true,
            bind_addr: Some(session.bind_addr.clone()),
            advertised_host: None,
            port: None,
            lan_ips: lan_ip_labels,
            message: "收件监听地址异常，请关闭收件后重新开启".to_string(),
            checks: receive_port_diagnostic_checks(),
        };
    };

    let advertised_host = if bind_ip.is_unspecified() {
        lan_ips.first().map(ToString::to_string)
    } else {
        Some(bind_ip.to_string())
    };
    let phase = if advertised_host.is_some() {
        "listening"
    } else {
        "no_lan_ip"
    };
    let message = if let Some(host) = advertised_host.as_deref() {
        format!("收件监听中，其他设备应连接 {host}:{port}")
    } else {
        "收件监听已开启，但没有可用于其他设备连接的局域网地址".to_string()
    };

    ReceivePortDiagnosticsDto {
        phase: phase.to_string(),
        listening: true,
        bind_addr: Some(session.bind_addr.clone()),
        advertised_host,
        port: Some(port),
        lan_ips: lan_ip_labels,
        message,
        checks: receive_port_diagnostic_checks(),
    }
}

fn parse_receive_bind_addr(bind_addr: &str) -> Option<(IpAddr, u16)> {
    bind_addr
        .parse::<SocketAddr>()
        .ok()
        .map(|addr| (addr.ip(), addr.port()))
}

fn receive_port_diagnostic_checks() -> Vec<String> {
    vec![
        "确认两台设备在同一局域网，且没有被路由器 AP 隔离".to_string(),
        "Windows 防火墙需要允许 NekoDrop 访问专用网络".to_string(),
        "VPN、代理或虚拟网卡可能让连接码拿到错误地址".to_string(),
    ]
}

fn device_to_dto(
    device: &Device,
    local_identity: &DeviceIdentity,
    trusted_devices: &[TrustedDeviceRecord],
) -> DeviceDto {
    let is_trusted = trusted_devices
        .iter()
        .any(|record| trusted_record_matches(device, record));
    DeviceDto {
        id: device.id.as_str().to_string(),
        name: device.name.clone(),
        platform: format!("{:?}", device.platform),
        host: device.host.clone(),
        port: device.port,
        trust_state: if is_trusted {
            "Trusted".to_string()
        } else {
            format!("{:?}", device.trust_state)
        },
        public_key_fingerprint: device.public_key_fingerprint.clone(),
        pairing_code: pairing_code_for_device(local_identity, device),
    }
}

fn trusted_device_to_dto(device: &TrustedDeviceRecord) -> TrustedDeviceDto {
    TrustedDeviceDto {
        device_id: device.device_id.clone(),
        device_name: device.device_name.clone(),
        platform: device.platform.clone(),
        host: device.host.clone(),
        port: device.port,
        public_key_fingerprint: device.public_key_fingerprint.clone(),
        pairing_code: device.pairing_code.clone(),
        paired_at_ms: device.paired_at_ms,
        last_seen_at_ms: device.last_seen_at_ms,
    }
}

fn device_identity_to_dto(identity: &DeviceIdentity) -> DeviceIdentityDto {
    DeviceIdentityDto {
        device_id: identity.device_id.clone(),
        device_name: identity.device_name.clone(),
        device_kind: identity.device_kind.as_str().to_string(),
        platform: identity.platform.as_str().to_string(),
        public_key_fingerprint: identity.public_key_fingerprint.clone(),
        capabilities: identity
            .capabilities
            .iter()
            .map(|capability| capability.as_str().to_string())
            .collect(),
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

fn emit_transfer_scan_progress(app: &AppHandle, progress: TransferPlanScanProgress) {
    let _ = app.emit(
        TRANSFER_SCAN_PROGRESS_EVENT,
        transfer_scan_progress_to_dto(progress),
    );
}

fn transfer_scan_progress_to_dto(progress: TransferPlanScanProgress) -> TransferScanProgressDto {
    TransferScanProgressDto {
        phase: progress.phase.as_str().to_string(),
        current_path: progress.current_path,
        files_found: progress.files_found,
        directories_found: progress.directories_found,
        bytes_found: progress.bytes_found,
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

fn send_report_to_dto(report: &TransferSendReport) -> SendReportDto {
    SendReportDto {
        root_name: report.plan.manifest.root_name.clone(),
        file_count: report.plan.file_count(),
        total_bytes: report.plan.total_bytes(),
        sent_files: report
            .sent_files
            .iter()
            .map(|file| SentFileDto {
                manifest_path: file.manifest_path.clone(),
                bytes_sent: file.bytes_sent,
            })
            .collect(),
    }
}

fn receive_report_to_dto(report: &TransferReceiveReport) -> ReceiveReportDto {
    ReceiveReportDto {
        transfer_id: report.transfer_id.clone(),
        root_name: report.root_name.clone(),
        sender_device_id: report.sender_device_id.clone(),
        sender_device_name: report.sender_device_name.clone(),
        sender_public_key_fingerprint: report.sender_public_key_fingerprint.clone(),
        file_count: report.files.len(),
        bundle: report.bundle.as_ref().map(received_bundle_to_dto),
        files: report
            .files
            .iter()
            .take(RECEIVE_FILE_PREVIEW_LIMIT)
            .map(|file| ReceivedFileDto {
                path: file.path.display().to_string(),
                manifest_path: file.manifest_path.clone(),
                bytes_written: file.bytes_written,
                sha256: file.sha256.clone(),
                verified: file.verified,
            })
            .collect(),
    }
}

fn received_bundle_to_dto(bundle: &ReceivedBundleReport) -> ReceivedBundleDto {
    ReceivedBundleDto {
        bundle_id: bundle.bundle_id.clone(),
        bundle_type: bundle_type_label(bundle.bundle_type).to_string(),
        display_name: bundle.display_name.clone(),
        source_app: bundle.source_app.clone(),
        file_count: bundle.file_count,
        total_bytes: bundle.total_bytes,
        staging_path: bundle.staging_path.display().to_string(),
        import_allowed: bundle.import_allowed,
    }
}

fn bundle_type_label(bundle_type: nekolink_protocol::BundleType) -> &'static str {
    match bundle_type {
        nekolink_protocol::BundleType::Skill => "skill",
        nekolink_protocol::BundleType::Session => "session",
        nekolink_protocol::BundleType::Workspace => "workspace",
        nekolink_protocol::BundleType::AgentProfile => "agent_profile",
        nekolink_protocol::BundleType::ConfigSnapshot => "config_snapshot",
    }
}

fn received_root_name(report: &TransferReceiveReport) -> String {
    if !report.root_name.trim().is_empty() {
        return report.root_name.clone();
    }

    let Some(first_file) = report.files.first() else {
        return "接收文件".to_string();
    };
    let first_path = first_file.manifest_path.trim_matches('/');
    let Some((root, _)) = first_path.split_once('/') else {
        return first_path.to_string();
    };
    if root.trim().is_empty() {
        "接收文件".to_string()
    } else {
        root.to_string()
    }
}

fn bundle_staging_root() -> Result<PathBuf, String> {
    Ok(app_config_dir()?.join("bundle_staging"))
}

fn push_receive_failure_history(
    transfer_history: &Arc<Mutex<Vec<TransferHistoryRecord>>>,
    transfer_status: &Arc<Mutex<Option<TransferStatusState>>>,
    peer_host: &str,
    receive_dir: &PathBuf,
    error_message: String,
) {
    let status = transfer_status
        .lock()
        .ok()
        .and_then(|status| status.clone());
    let now = now_ms();
    let mut record = new_transfer_history_record(
        format!("receive-{now}"),
        "receive",
        "failed",
        status
            .as_ref()
            .and_then(|status| status.root_name.clone())
            .unwrap_or_else(|| "接收失败".to_string()),
        status.as_ref().map(|status| status.file_count).unwrap_or(0),
        status
            .as_ref()
            .map(|status| status.total_bytes)
            .unwrap_or(0),
        status
            .as_ref()
            .map(|status| status.bytes_transferred)
            .unwrap_or(0),
        now,
    );
    record.target_host = Some(peer_host.to_string());
    record.receive_dir = Some(receive_dir.display().to_string());
    record.error_message = Some(error_message);
    let _ = push_transfer_history_record(transfer_history, record);
}

fn pending_offer_to_dto(offer: &PendingReceiveOffer) -> PendingReceiveOfferDto {
    PendingReceiveOfferDto {
        transfer_id: offer.transfer_id.clone(),
        root_name: offer.root_name.clone(),
        file_count: offer.file_count,
        total_bytes: offer.total_bytes,
        sender_device_id: offer.sender_device_id.clone(),
        sender_device_name: offer.sender_device_name.clone(),
        sender_public_key_fingerprint: offer.sender_public_key_fingerprint.clone(),
        preview_file_count: offer.files.len().min(RECEIVE_FILE_PREVIEW_LIMIT),
        files: offer
            .files
            .iter()
            .take(RECEIVE_FILE_PREVIEW_LIMIT)
            .map(|file| PendingReceiveFileDto {
                manifest_path: file.manifest_path.clone(),
                size: file.size,
                sha256: file.sha256.clone(),
            })
            .collect(),
        resume_summary: offer.resume_summary.map(|summary| ReceiveResumeSummaryDto {
            resumable_file_count: summary.resumable_file_count,
            completed_file_count: summary.completed_file_count,
            partial_file_count: summary.partial_file_count,
            received_bytes: summary.received_bytes,
        }),
    }
}

fn pending_resume_summary_from_offer(
    receive_dir: &std::path::Path,
    offer: &TransferOffer,
) -> Option<PendingReceiveResumeSummary> {
    let mut expected_files = Vec::with_capacity(offer.files.len());
    for file in &offer.files {
        expected_files.push(
            ResumeExpectedFile::new(
                file.manifest_path.clone(),
                file.size,
                Some(file.sha256.clone()),
            )
            .ok()?,
        );
    }

    let plan =
        build_resume_plan_for_files(receive_dir, &offer.transfer_id, &expected_files).ok()?;
    pending_resume_summary_from_plan(&plan)
}

fn pending_resume_summary_from_plan(plan: &ResumePlan) -> Option<PendingReceiveResumeSummary> {
    if plan.is_empty() {
        return None;
    }

    Some(PendingReceiveResumeSummary {
        resumable_file_count: plan.files.len(),
        completed_file_count: plan.completed_file_count(),
        partial_file_count: plan.partial_file_count(),
        received_bytes: plan.total_received_bytes(),
    })
}

fn pending_pairing_request_to_dto(request: &PendingPairingRequest) -> PendingPairingRequestDto {
    PendingPairingRequestDto {
        request_id: request.request_id.clone(),
        device_id: request.device_id.clone(),
        device_name: request.device_name.clone(),
        platform: request.platform.clone(),
        host: request.host.clone(),
        port: request.port,
        public_key_fingerprint: request.public_key_fingerprint.clone(),
        pairing_code: request.pairing_code.clone(),
    }
}

fn transfer_status_to_dto(status: &TransferStatusState) -> TransferStatusDto {
    let progress = if status.total_bytes == 0 {
        0.0
    } else {
        (status.bytes_transferred as f32 / status.total_bytes as f32).clamp(0.0, 1.0)
    };
    TransferStatusDto {
        direction: status.direction.clone(),
        phase: status.phase.clone(),
        root_name: status.root_name.clone(),
        file_count: status.file_count,
        file_index: status.file_index,
        current_file: status.current_file.clone(),
        bytes_transferred: status.bytes_transferred,
        total_bytes: status.total_bytes,
        progress,
        message: status.message.clone(),
        updated_at_ms: status.updated_at_ms,
    }
}

fn transfer_to_dto(record: &TransferHistoryRecord) -> TransferDto {
    let progress = if record.total_bytes == 0 {
        0.0
    } else {
        (record.transferred_bytes as f32 / record.total_bytes as f32).clamp(0.0, 1.0)
    };
    TransferDto {
        id: record.id.clone(),
        root_name: record.root_name.clone(),
        peer_device_id: record.peer_device_id.clone(),
        peer_name: record.peer_name.clone(),
        target_host: record.target_host.clone(),
        source_paths: record.source_paths.clone(),
        received_paths: record.received_paths.clone(),
        direction: record.direction.clone(),
        status: record.status.clone(),
        file_count: record.file_count,
        total_bytes: record.total_bytes,
        transferred_bytes: record.transferred_bytes,
        progress,
        receive_dir: record.receive_dir.clone(),
        error_message: record.error_message.clone(),
        created_at_ms: record.created_at_ms,
        updated_at_ms: record.updated_at_ms,
    }
}

fn wait_for_receive_decision(
    offer: &TransferOffer,
    pending_receive_offer: &Arc<Mutex<Option<PendingReceiveOffer>>>,
    transfer_status: &Arc<Mutex<Option<TransferStatusState>>>,
    receive_policy: ReceivePolicy,
    trusted_devices: &Arc<Mutex<Vec<TrustedDeviceRecord>>>,
    resume_summary: Option<PendingReceiveResumeSummary>,
) -> bool {
    if receive_policy == ReceivePolicy::BlockAll {
        set_transfer_status(
            transfer_status,
            TransferStatusState {
                direction: "receive".to_string(),
                phase: "blocked".to_string(),
                root_name: Some(offer.root_name.clone()),
                file_count: offer.file_count,
                file_index: 0,
                current_file: None,
                bytes_transferred: 0,
                total_bytes: offer.total_bytes,
                message: "当前接收策略已阻止传输请求".to_string(),
                updated_at_ms: now_ms(),
            },
        );
        return false;
    }

    if should_auto_accept_receive_offer(offer, receive_policy, trusted_devices) {
        set_transfer_status(
            transfer_status,
            TransferStatusState {
                direction: "receive".to_string(),
                phase: "auto_accepted".to_string(),
                root_name: Some(offer.root_name.clone()),
                file_count: offer.file_count,
                file_index: 0,
                current_file: None,
                bytes_transferred: 0,
                total_bytes: offer.total_bytes,
                message: "可信设备已自动接受".to_string(),
                updated_at_ms: now_ms(),
            },
        );
        return true;
    }

    let decision = Arc::new((Mutex::new(None), Condvar::new()));
    let pending = PendingReceiveOffer {
        transfer_id: offer.transfer_id.clone(),
        root_name: offer.root_name.clone(),
        file_count: offer.file_count,
        total_bytes: offer.total_bytes,
        sender_device_id: offer.sender_device_id.clone(),
        sender_device_name: offer.sender_device_name.clone(),
        sender_public_key_fingerprint: offer.sender_public_key_fingerprint.clone(),
        files: offer
            .files
            .iter()
            .map(|file| PendingReceiveFile {
                manifest_path: file.manifest_path.clone(),
                size: file.size,
                sha256: file.sha256.clone(),
            })
            .collect(),
        resume_summary,
        decision: decision.clone(),
    };

    if let Ok(mut offer_slot) = pending_receive_offer.lock() {
        *offer_slot = Some(pending);
    }
    set_transfer_status(
        transfer_status,
        TransferStatusState {
            direction: "receive".to_string(),
            phase: "awaiting_approval".to_string(),
            root_name: Some(offer.root_name.clone()),
            file_count: offer.file_count,
            file_index: 0,
            current_file: None,
            bytes_transferred: 0,
            total_bytes: offer.total_bytes,
            message: "收到传输请求，等待确认".to_string(),
            updated_at_ms: now_ms(),
        },
    );

    let (decision_lock, decision_cvar) = &*decision;
    let mut guard = match decision_lock.lock() {
        Ok(guard) => guard,
        Err(_) => return false,
    };
    while guard.is_none() {
        let next = decision_cvar.wait_timeout(guard, Duration::from_secs(300));
        let Ok((next_guard, timeout)) = next else {
            return false;
        };
        guard = next_guard;
        if timeout.timed_out() {
            set_transfer_status(
                transfer_status,
                TransferStatusState {
                    direction: "receive".to_string(),
                    phase: "expired".to_string(),
                    root_name: Some(offer.root_name.clone()),
                    file_count: offer.file_count,
                    file_index: 0,
                    current_file: None,
                    bytes_transferred: 0,
                    total_bytes: offer.total_bytes,
                    message: "等待确认超时，已自动拒绝".to_string(),
                    updated_at_ms: now_ms(),
                },
            );
            return false;
        }
    }

    matches!(*guard, Some(ReceiveDecision::Accept))
}

fn should_auto_accept_receive_offer(
    offer: &TransferOffer,
    receive_policy: ReceivePolicy,
    trusted_devices: &Arc<Mutex<Vec<TrustedDeviceRecord>>>,
) -> bool {
    let _ = (offer, receive_policy, trusted_devices);
    // Auto-accept needs authenticated encrypted sessions. Current trusted records
    // identify devices but do not prove possession on each incoming connection.
    false
}

fn wait_for_pairing_decision(
    request: &PairingRequestPayload,
    peer_host: &str,
    pending_pairing_request: &Arc<Mutex<Option<PendingPairingRequest>>>,
    trusted_devices: &Arc<Mutex<Vec<TrustedDeviceRecord>>>,
    local_identity: &DeviceIdentity,
) -> PairingDecisionPayload {
    let expected_code = pairing_code_for_values(
        &local_identity.device_id,
        &local_identity.public_key_fingerprint,
        &request.device_id,
        &request.public_key_fingerprint,
    );
    if expected_code != request.pairing_code {
        return PairingDecisionPayload::reject("配对码不匹配");
    }

    let decision = Arc::new((Mutex::new(None), Condvar::new()));
    let pending = PendingPairingRequest {
        request_id: request.request_id.clone(),
        device_id: request.device_id.clone(),
        device_name: request.device_name.clone(),
        platform: request.platform.clone(),
        host: peer_host.to_string(),
        port: request.listen_port,
        public_key_fingerprint: request.public_key_fingerprint.clone(),
        pairing_code: request.pairing_code.clone(),
        decision: decision.clone(),
    };

    if let Ok(mut request_slot) = pending_pairing_request.lock() {
        *request_slot = Some(pending);
    }

    let (decision_lock, decision_cvar) = &*decision;
    let mut guard = match decision_lock.lock() {
        Ok(guard) => guard,
        Err(_) => return PairingDecisionPayload::reject("配对确认状态异常"),
    };
    while guard.is_none() {
        let next = decision_cvar.wait_timeout(guard, Duration::from_secs(300));
        let Ok((next_guard, timeout)) = next else {
            return PairingDecisionPayload::reject("配对确认状态异常");
        };
        guard = next_guard;
        if timeout.timed_out() {
            if let Ok(mut request_slot) = pending_pairing_request.lock() {
                *request_slot = None;
            }
            return PairingDecisionPayload::reject("等待确认超时");
        }
    }

    if !matches!(*guard, Some(ReceiveDecision::Accept)) {
        return PairingDecisionPayload::reject("用户拒绝配对");
    }

    let record = trusted_device_record_from_remote(
        local_identity,
        request.device_id.clone(),
        request.device_name.clone(),
        request.platform.clone(),
        peer_host.to_string(),
        request.listen_port,
        request.public_key_fingerprint.clone(),
    );
    match persist_trusted_device_records(trusted_devices, record) {
        Ok(()) => PairingDecisionPayload::accept(),
        Err(error) => PairingDecisionPayload::reject(error),
    }
}

fn persist_trusted_device(state: &AppState, record: TrustedDeviceRecord) -> Result<(), String> {
    persist_trusted_device_records(&state.trusted_devices, record)
}

fn persist_receive_dir(state: &AppState, receive_dir: &str) -> Result<(), String> {
    if receive_dir.trim().is_empty() {
        return Err("接收目录不能为空".to_string());
    }
    let receive_dir_path = expand_home_dir(receive_dir);
    fs::create_dir_all(&receive_dir_path)
        .map_err(|error| format!("无法创建接收目录 {}: {error}", receive_dir_path.display()))?;
    persist_receive_dir_path(state, &receive_dir_path)
}

fn persist_receive_dir_path(state: &AppState, receive_dir_path: &PathBuf) -> Result<(), String> {
    let receive_dir = receive_dir_path.display().to_string();
    let mut config = state.config.lock().map_err(|error| error.to_string())?;
    if config.receive_dir == receive_dir {
        return Ok(());
    }
    let mut next_config = config.clone();
    next_config.receive_dir = receive_dir;
    save_app_config(&next_config)?;
    *config = next_config;
    Ok(())
}

fn persist_receive_policy(state: &AppState, receive_policy: ReceivePolicy) -> Result<(), String> {
    let mut config = state.config.lock().map_err(|error| error.to_string())?;
    if config.receive_policy == receive_policy {
        return Ok(());
    }

    let mut next_config = config.clone();
    next_config.receive_policy = receive_policy;
    save_app_config(&next_config)?;
    *config = next_config;
    Ok(())
}

fn persist_receive_port(state: &AppState, receive_port: u16) -> Result<(), String> {
    if receive_port == 0 {
        return Err("端口必须是 1-65535".to_string());
    }

    let mut config = state.config.lock().map_err(|error| error.to_string())?;
    if config.receive_port == receive_port {
        return Ok(());
    }

    let mut next_config = config.clone();
    next_config.receive_port = receive_port;
    save_app_config(&next_config)?;
    *config = next_config;
    Ok(())
}

fn persist_device_name(state: &AppState, device_name: &str) -> Result<String, String> {
    let device_name = state.device_identity.save_device_name(device_name)?;
    let mut config = state.config.lock().map_err(|error| error.to_string())?;
    if config.device_name == device_name {
        return Ok(device_name);
    }

    config.device_name = device_name.clone();
    Ok(device_name)
}

fn receive_policy_from_input(value: &str) -> Result<ReceivePolicy, String> {
    match value {
        "always_ask" => Ok(ReceivePolicy::AlwaysAsk),
        "auto_accept_trusted" => Ok(ReceivePolicy::AutoAcceptTrusted),
        "block_all" => Ok(ReceivePolicy::BlockAll),
        _ => Err("未知接收策略".to_string()),
    }
}

fn persist_trusted_device_records(
    trusted_devices: &Arc<Mutex<Vec<TrustedDeviceRecord>>>,
    record: TrustedDeviceRecord,
) -> Result<(), String> {
    let mut trusted_devices = trusted_devices.lock().map_err(|error| error.to_string())?;
    let mut next_trusted_devices = trusted_devices.clone();
    upsert_trusted_device(&mut next_trusted_devices, record);
    save_trusted_devices(&next_trusted_devices)?;
    *trusted_devices = next_trusted_devices;
    Ok(())
}

fn refresh_trusted_device_contact_from_receive_report(
    trusted_devices: &Arc<Mutex<Vec<TrustedDeviceRecord>>>,
    report: &TransferReceiveReport,
) {
    let Some(sender_device_id) = report.sender_device_id.as_deref() else {
        return;
    };
    let Some(sender_fingerprint) = report.sender_public_key_fingerprint.as_deref() else {
        return;
    };

    let Ok(mut trusted_devices) = trusted_devices.lock() else {
        return;
    };
    let mut next_trusted_devices = trusted_devices.clone();
    let changed = refresh_trusted_device_contact(
        &mut next_trusted_devices,
        sender_device_id,
        sender_fingerprint,
        report.sender_device_name.as_deref(),
        now_ms(),
    );
    if !changed {
        return;
    }
    if save_trusted_devices(&next_trusted_devices).is_ok() {
        *trusted_devices = next_trusted_devices;
    }
}

fn refresh_trusted_device_contact_from_peer(
    trusted_devices: &Arc<Mutex<Vec<TrustedDeviceRecord>>>,
    peer: &TransferPeer,
) {
    let Some(device_id) = peer.device_id.as_deref() else {
        return;
    };
    let Some(fingerprint) = peer.fingerprint.as_deref() else {
        return;
    };

    let Ok(mut trusted_devices) = trusted_devices.lock() else {
        return;
    };
    let mut next_trusted_devices = trusted_devices.clone();
    let changed = refresh_trusted_device_contact(
        &mut next_trusted_devices,
        device_id,
        fingerprint,
        peer.name.as_deref(),
        now_ms(),
    );
    if !changed {
        return;
    }
    if save_trusted_devices(&next_trusted_devices).is_ok() {
        *trusted_devices = next_trusted_devices;
    }
}

fn current_receive_session_port(state: &AppState) -> Result<Option<u16>, String> {
    let session = state
        .receive_session
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(session
        .as_ref()
        .and_then(|session| session.bind_addr.rsplit_once(':'))
        .and_then(|(_, port)| port.parse::<u16>().ok()))
}

fn is_receive_terminal_offer_status(
    transfer_status: &Arc<Mutex<Option<TransferStatusState>>>,
    phase: &str,
) -> bool {
    transfer_status
        .lock()
        .ok()
        .and_then(|status| status.as_ref().map(|status| status.phase.clone()))
        .is_some_and(|current_phase| current_phase == phase)
}

fn is_receive_transfer_active(transfer_status: &Arc<Mutex<Option<TransferStatusState>>>) -> bool {
    transfer_status
        .lock()
        .ok()
        .and_then(|status| {
            status
                .as_ref()
                .map(|status| (status.direction.clone(), status.phase.clone()))
        })
        .is_some_and(|(direction, phase)| {
            direction == "receive"
                && matches!(phase.as_str(), "accepted" | "transferring" | "verifying")
        })
}

fn status_from_progress_event(
    direction: &str,
    root_name: Option<String>,
    event: TransferProgressEvent,
) -> Option<TransferStatusState> {
    match event {
        TransferProgressEvent::AwaitingApproval {
            root_name: event_root_name,
            file_count,
            total_bytes,
        } => Some(TransferStatusState {
            direction: direction.to_string(),
            phase: "awaiting_approval".to_string(),
            root_name: root_name.or(Some(event_root_name)),
            file_count,
            file_index: 0,
            current_file: None,
            bytes_transferred: 0,
            total_bytes,
            message: "已发送传输请求，等待对方确认".to_string(),
            updated_at_ms: now_ms(),
        }),
        TransferProgressEvent::Sending(progress) => Some(status_from_transfer_progress(
            direction,
            "transferring",
            "正在发送文件",
            root_name,
            progress,
        )),
        TransferProgressEvent::Receiving(progress) => Some(status_from_transfer_progress(
            direction,
            "transferring",
            "正在接收文件",
            root_name,
            progress,
        )),
        TransferProgressEvent::Verifying {
            manifest_path,
            bytes_transferred,
            total_bytes,
        } => Some(TransferStatusState {
            direction: direction.to_string(),
            phase: "verifying".to_string(),
            root_name,
            file_count: 0,
            file_index: 0,
            current_file: Some(manifest_path),
            bytes_transferred,
            total_bytes,
            message: "正在校验文件".to_string(),
            updated_at_ms: now_ms(),
        }),
    }
}

fn status_from_transfer_progress(
    direction: &str,
    phase: &str,
    message: &str,
    root_name: Option<String>,
    progress: TransferProgress,
) -> TransferStatusState {
    TransferStatusState {
        direction: direction.to_string(),
        phase: phase.to_string(),
        root_name,
        file_count: progress.file_count,
        file_index: progress.file_index,
        current_file: Some(progress.manifest_path),
        bytes_transferred: progress.bytes_transferred,
        total_bytes: progress.total_bytes,
        message: message.to_string(),
        updated_at_ms: now_ms(),
    }
}

fn set_transfer_status(
    transfer_status: &Arc<Mutex<Option<TransferStatusState>>>,
    status: TransferStatusState,
) {
    if let Ok(mut slot) = transfer_status.lock() {
        *slot = Some(status);
    }
}

fn current_transfer_progress(
    transfer_status: &Arc<Mutex<Option<TransferStatusState>>>,
) -> (usize, Option<String>, u64, u64) {
    transfer_status
        .lock()
        .ok()
        .and_then(|status| status.clone())
        .map(|status| {
            (
                status.file_index,
                status.current_file,
                status.bytes_transferred.min(status.total_bytes),
                status.total_bytes,
            )
        })
        .unwrap_or((0, None, 0, 0))
}

fn endpoint_label(endpoint: &Endpoint) -> String {
    format!("{}:{}", endpoint.host, endpoint.port)
}

fn endpoint_from_label(value: &str) -> Result<Endpoint, String> {
    let (host, port) = value
        .rsplit_once(':')
        .ok_or_else(|| friendly_transfer_error(&format!("invalid endpoint label: {value}")))?;
    let port = port
        .parse::<u16>()
        .map_err(|error| friendly_transfer_error(&format!("invalid endpoint port: {error}")))?;
    if host.trim().is_empty() {
        return Err(friendly_transfer_error("empty endpoint host"));
    }
    Ok(Endpoint::tcp(host.to_string(), port))
}

fn validate_endpoint_for_desktop_send(endpoint: &Endpoint) -> Result<(), String> {
    if endpoint.transport.as_str() != "tcp" {
        return Err(friendly_transfer_error(&format!(
            "unsupported transport: requested {}",
            endpoint.transport.as_str()
        )));
    }
    if endpoint.port == 0 {
        return Err("目标端口无效，请重新从附近设备发送，或重新复制连接码。".to_string());
    }

    let host = endpoint.host.trim();
    if host.is_empty() {
        return Err("目标地址缺少主机，请重新从附近设备发送，或重新复制连接码。".to_string());
    }

    let lower = host.to_lowercase();
    if lower == "localhost" {
        return Err(friendly_transfer_error("failed to connect to localhost"));
    }

    if let Ok(ip) = host.parse::<IpAddr>() {
        if ip.is_loopback() {
            return Err(friendly_transfer_error(&format!(
                "failed to connect to {host}:{}",
                endpoint.port
            )));
        }
        if is_current_lan_ip(ip, &local_lan_ips()) {
            return Err(
                "目标地址是本机局域网地址，不能把文件发送给自己。请选择另一台设备或复制对方连接码。"
                    .to_string(),
            );
        }
        if ip.is_unspecified() {
            return Err(
                "目标地址是 0.0.0.0 或 ::，这只是监听地址，不能被另一台设备连接。请重新复制接收端连接码。"
                    .to_string(),
            );
        }
        if let IpAddr::V4(ipv4) = ip {
            let octets = ipv4.octets();
            if octets[0] == 198 && (octets[1] == 18 || octets[1] == 19) {
                return Err(friendly_transfer_error(&format!(
                    "failed to connect to {host}:{}",
                    endpoint.port
                )));
            }
            if octets[0] == 169 && octets[1] == 254 {
                return Err(
                    "目标地址是 169.254.x.x，这通常表示没有拿到可用局域网地址。请确认两台设备在同一网络，或重新打开接收端生成连接码。"
                        .to_string(),
                );
            }
        }
    }

    Ok(())
}

fn is_current_lan_ip(target: IpAddr, current_lan_ips: &[IpAddr]) -> bool {
    current_lan_ips.contains(&target)
}

fn friendly_transfer_error(error: &str) -> String {
    let lower = error.to_lowercase();

    if lower.contains("receiver declined") || lower.contains("transfer declined by receiver") {
        return "对方拒绝了这次传输".to_string();
    }
    if lower.contains("transfer cancelled") {
        return "传输已取消".to_string();
    }
    if lower.contains("insufficient receive space") || lower.contains("disk full") {
        return "接收目录所在磁盘空间不足。请清理空间，或在设置里选择另一个接收目录后重试。"
            .to_string();
    }

    if lower.contains("unsupported connection code")
        || lower.contains("connection code missing")
        || lower.contains("invalid connection code")
        || lower.contains("invalid percent encoding")
        || lower.contains("connection field is not utf-8")
        || lower.contains("connection ticket only supports")
    {
        return "连接码无效，请重新复制对方生成的连接码。".to_string();
    }

    if lower.contains("invalid endpoint label")
        || lower.contains("invalid endpoint port")
        || lower.contains("empty endpoint host")
    {
        return "历史记录里的目标地址无效，请重新从附近设备发送，或重新复制连接码。".to_string();
    }

    if lower.contains("transport is not available")
        || lower.contains("unsupported transport")
        || lower.contains("requested iroh")
        || lower.contains("requested relay")
        || lower.contains("requested quic")
    {
        return "当前版本还没有接入这个传输通道。请先使用局域网自动发现或连接码兜底。".to_string();
    }

    if lower.contains("198.18.") || lower.contains("198.19.") {
        return "连接地址落在 198.18/198.19 测试网段，通常是代理、VPN 或虚拟网卡。请关闭相关网络工具，或改用真实局域网地址/连接码。".to_string();
    }

    if lower.contains("127.0.0.1") || lower.contains("localhost") {
        return "连接地址指向了本机，另一台电脑无法访问。请重新打开接收端，复制新的连接码，或使用附近设备自动发现。".to_string();
    }

    if lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("由于连接方在一段时间后没有正确答复")
        || lower.contains("连接尝试失败")
    {
        return "连接超时。常见原因是 Windows 防火墙拦截、两台设备不在同一网段、路由器隔离了有线/无线，或 VPN/代理影响了局域网连接。".to_string();
    }

    if lower.contains("connection refused")
        || lower.contains("actively refused")
        || lower.contains("connection reset")
        || lower.contains("failed to connect")
        || lower.contains("由于目标计算机积极拒绝")
    {
        return "无法连接对方电脑。请确认对方 NekoDrop 正在运行、收件已开启、防火墙允许访问，且两台设备网络互通。".to_string();
    }

    if lower.contains("network is unreachable")
        || lower.contains("no route to host")
        || lower.contains("host unreachable")
        || lower.contains("无法访问目标主机")
    {
        return "当前网络无法到达对方设备。请确认两台设备在同一局域网，或使用连接码/后续 Relay 方案。".to_string();
    }

    if lower.contains("permission denied")
        || lower.contains("access is denied")
        || lower.contains("operation not permitted")
        || lower.contains("权限")
    {
        return "系统权限阻止了这次操作。请检查接收目录权限、防火墙权限，或重新选择一个可写入的接收目录。".to_string();
    }

    if lower.contains("checksum")
        || lower.contains("sha-256")
        || lower.contains("sha256")
        || lower.contains("does not match accepted offer")
    {
        return "文件校验失败，已拒绝把不一致的内容当作完成文件。请重新发送。".to_string();
    }

    if lower.contains("no such file") || lower.contains("not found") || lower.contains("路径不存在")
    {
        return "文件或目录不存在，请确认源文件没有被移动、删除，或重新选择文件。".to_string();
    }

    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nekodrop_service::TransferPlanScanPhase;

    #[test]
    fn friendly_transfer_error_explains_connection_failures() {
        let refused = friendly_transfer_error(
            "network error: failed to connect to 192.168.1.8:45821: Connection refused",
        );
        assert!(refused.contains("无法连接对方电脑"));

        let timeout = friendly_transfer_error(
            "network error: failed to connect to 192.168.1.8:45821: timed out",
        );
        assert!(timeout.contains("连接超时"));
        assert!(timeout.contains("防火墙"));
    }

    #[test]
    fn friendly_transfer_error_explains_bad_network_addresses() {
        let benchmark =
            friendly_transfer_error("network error: failed to connect to 198.18.0.1:45821");
        assert!(benchmark.contains("198.18/198.19"));

        let loopback = friendly_transfer_error("failed to connect to 127.0.0.1:45821");
        assert!(loopback.contains("指向了本机"));
    }

    #[test]
    fn friendly_transfer_error_explains_unsupported_transport_and_integrity_failures() {
        let transport = friendly_transfer_error("iroh transport is not available in this build");
        assert!(transport.contains("还没有接入这个传输通道"));

        let checksum = friendly_transfer_error("incoming file does not match accepted offer");
        assert!(checksum.contains("文件校验失败"));
    }

    #[test]
    fn friendly_transfer_error_explains_insufficient_receive_space() {
        let message = friendly_transfer_error(
            "storage error: insufficient receive space: need 100 bytes, available 70 bytes",
        );

        assert!(message.contains("接收目录"));
        assert!(message.contains("空间不足"));
    }

    #[test]
    fn desktop_endpoint_preflight_rejects_unusable_addresses() {
        assert!(validate_endpoint_for_desktop_send(&Endpoint::tcp("192.168.1.20", 45821)).is_ok());

        let loopback =
            validate_endpoint_for_desktop_send(&Endpoint::tcp("127.0.0.1", 45821)).unwrap_err();
        assert!(loopback.contains("指向了本机"));

        let unspecified =
            validate_endpoint_for_desktop_send(&Endpoint::tcp("0.0.0.0", 45821)).unwrap_err();
        assert!(unspecified.contains("监听地址"));

        let benchmark =
            validate_endpoint_for_desktop_send(&Endpoint::tcp("198.18.0.1", 45821)).unwrap_err();
        assert!(benchmark.contains("198.18/198.19"));

        let link_local =
            validate_endpoint_for_desktop_send(&Endpoint::tcp("169.254.0.2", 45821)).unwrap_err();
        assert!(link_local.contains("169.254"));
    }

    #[test]
    fn receive_port_diagnostics_reports_closed_receiver() {
        let diagnostics = receive_port_diagnostics_from_session(None, vec![]);

        assert_eq!(diagnostics.phase, "closed");
        assert!(!diagnostics.listening);
        assert_eq!(diagnostics.bind_addr, None);
        assert_eq!(diagnostics.port, None);
        assert!(diagnostics.message.contains("收件未开启"));
    }

    #[test]
    fn receive_port_diagnostics_uses_lan_ip_for_unspecified_bind() {
        let session = ActiveReceiveSession {
            bind_addr: "0.0.0.0:45821".to_string(),
            receive_dir: "/tmp/nekodrop".to_string(),
            connection_code: "ticket".to_string(),
            cancel: Arc::new(AtomicBool::new(false)),
        };

        let diagnostics = receive_port_diagnostics_from_session(
            Some(&session),
            vec![IpAddr::from([192, 168, 1, 20]), IpAddr::from([10, 0, 0, 8])],
        );

        assert_eq!(diagnostics.phase, "listening");
        assert!(diagnostics.listening);
        assert_eq!(diagnostics.bind_addr.as_deref(), Some("0.0.0.0:45821"));
        assert_eq!(diagnostics.advertised_host.as_deref(), Some("192.168.1.20"));
        assert_eq!(diagnostics.port, Some(45821));
        assert!(diagnostics
            .checks
            .iter()
            .any(|check| check.contains("防火墙")));
    }

    #[test]
    fn receive_port_diagnostics_warns_when_no_lan_ip_is_available() {
        let session = ActiveReceiveSession {
            bind_addr: "0.0.0.0:45821".to_string(),
            receive_dir: "/tmp/nekodrop".to_string(),
            connection_code: "ticket".to_string(),
            cancel: Arc::new(AtomicBool::new(false)),
        };

        let diagnostics = receive_port_diagnostics_from_session(Some(&session), vec![]);

        assert_eq!(diagnostics.phase, "no_lan_ip");
        assert!(diagnostics.listening);
        assert_eq!(diagnostics.advertised_host, None);
        assert_eq!(diagnostics.port, Some(45821));
        assert!(diagnostics.message.contains("局域网地址"));
    }

    #[test]
    fn current_lan_ip_is_treated_as_self_target() {
        let current = vec![IpAddr::from([10, 0, 0, 8]), IpAddr::from([192, 168, 1, 20])];

        assert!(is_current_lan_ip(IpAddr::from([192, 168, 1, 20]), &current));
        assert!(!is_current_lan_ip(
            IpAddr::from([192, 168, 1, 30]),
            &current
        ));
    }

    #[test]
    fn connection_input_accepts_endpoint_label_as_manual_fallback() {
        let (endpoint, peer) =
            endpoint_and_peer_from_connection_input("192.168.1.20:45821").unwrap();

        assert_eq!(endpoint, Endpoint::tcp("192.168.1.20", 45821));
        assert_eq!(peer.target_host.as_deref(), Some("192.168.1.20:45821"));
        assert!(peer.device_id.is_none());
    }

    #[test]
    fn connection_input_keeps_connection_ticket_identity() {
        let code = ConnectionTicket::new(Endpoint::tcp("192.168.1.20", 45821))
            .unwrap()
            .with_device_id("device-a")
            .with_device_name("MacBook")
            .with_fingerprint("sha256:abc")
            .to_code()
            .unwrap();
        let (endpoint, peer) = endpoint_and_peer_from_connection_input(&code).unwrap();

        assert_eq!(endpoint, Endpoint::tcp("192.168.1.20", 45821));
        assert_eq!(peer.device_id.as_deref(), Some("device-a"));
        assert_eq!(peer.name.as_deref(), Some("MacBook"));
        assert_eq!(peer.fingerprint.as_deref(), Some("sha256:abc"));
    }

    #[test]
    fn nearby_device_requires_trusted_identity_before_send() {
        let device = nearby_device("device-a", "sha256:device-a");

        let result = trusted_peer_from_nearby_device(&device, &[]);

        assert!(result.unwrap_err().contains("可信配对"));
    }

    #[test]
    fn nearby_device_uses_current_endpoint_after_trust_match() {
        let device = nearby_device("device-a", "sha256:device-a");
        let trusted = vec![trusted_record("device-a", "MacBook", "sha256:device-a")];

        let (endpoint, peer) = trusted_peer_from_nearby_device(&device, &trusted).unwrap();

        assert_eq!(endpoint, Endpoint::tcp("192.168.1.20", 45821));
        assert_eq!(peer.device_id.as_deref(), Some("device-a"));
        assert_eq!(peer.fingerprint.as_deref(), Some("sha256:device-a"));
    }

    #[test]
    fn self_peer_is_rejected_by_device_id() {
        let identity = test_identity("device-a");
        let peer = TransferPeer {
            device_id: Some("device-a".to_string()),
            name: Some("This Mac".to_string()),
            fingerprint: Some("sha256:self".to_string()),
            target_host: Some("192.168.1.20:45821".to_string()),
        };

        let error = reject_self_peer(&identity, &peer).unwrap_err();

        assert!(error.contains("本机"));
    }

    #[test]
    fn manual_endpoint_without_identity_is_not_self_rejected() {
        let identity = test_identity("device-a");
        let peer = TransferPeer {
            device_id: None,
            name: None,
            fingerprint: None,
            target_host: Some("192.168.1.30:45821".to_string()),
        };

        assert!(reject_self_peer(&identity, &peer).is_ok());
    }

    #[test]
    fn history_retry_reuses_existing_transfer_id() {
        assert_eq!(
            history_transfer_id(42, Some("send-original")),
            "send-original"
        );
        assert_eq!(history_transfer_id(42, None), "send-42");
    }

    #[test]
    fn send_auto_retry_retries_once_for_transient_network_error() {
        let mut attempts = 0;
        let mut retry_events = Vec::new();

        let result = send_with_auto_retry(
            || {
                attempts += 1;
                if attempts == 1 {
                    Err("failed to connect to 192.168.1.20:45821: Connection refused".to_string())
                } else {
                    Ok("sent")
                }
            },
            |retry_number, retry_limit, error| {
                retry_events.push((retry_number, retry_limit, error.to_string()));
            },
        );

        assert_eq!(result.unwrap(), "sent");
        assert_eq!(attempts, 2);
        assert_eq!(retry_events.len(), 1);
        assert_eq!(retry_events[0].0, 1);
        assert_eq!(retry_events[0].1, 1);
        assert!(retry_events[0].2.contains("Connection refused"));
    }

    #[test]
    fn send_auto_retry_does_not_retry_terminal_failures() {
        for error in [
            "transfer cancelled",
            "receiver declined transfer: no reason provided",
            "incoming file does not match accepted offer",
        ] {
            let mut attempts = 0;

            let result = send_with_auto_retry(
                || {
                    attempts += 1;
                    Err::<(), _>(error.to_string())
                },
                |_, _, _| panic!("terminal send failures must not be retried"),
            );

            assert_eq!(result.unwrap_err(), error);
            assert_eq!(attempts, 1);
        }
    }

    #[test]
    fn send_auto_retry_stops_after_retry_limit() {
        let mut attempts = 0;

        let result = send_with_auto_retry(
            || {
                attempts += 1;
                Err::<(), _>("connection reset by peer".to_string())
            },
            |_, _, _| {},
        );

        assert_eq!(result.unwrap_err(), "connection reset by peer");
        assert_eq!(attempts, 2);
    }

    #[test]
    fn parse_dialog_output_strips_utf8_bom_from_windows_stdout() {
        let output = b"\xEF\xBB\xBFI:\\\xe6\x96\x87\xe4\xbb\xb6\\asmr\\z\\16\xe5\x88\x86\xe9\x92\x9f.m4a\r\n";

        let paths = parse_dialog_output(output);

        assert_eq!(paths, vec!["I:\\文件\\asmr\\z\\16分钟.m4a"]);
    }

    #[test]
    fn windows_dialog_script_forces_utf8_stdout_for_chinese_paths() {
        let script = windows_dialog_script(PathDialogKind::Files);

        assert!(script.contains("[Console]::OutputEncoding"));
        assert!(script.contains("UTF8Encoding"));
        assert!(script.contains("$OutputEncoding"));
    }

    #[test]
    fn manual_path_rejects_replacement_character_before_exists_check() {
        let error = normalize_user_path(r"I:\�ļ�\asmr\z\����\16����.m4a").unwrap_err();

        assert!(error.contains("路径编码已经损坏"));
    }

    #[test]
    fn manual_path_rejects_windows_unsafe_components_before_exists_check() {
        for path in [
            r"C:\drop\CON.txt",
            r"C:\drop\audio.m4a:Zone.Identifier",
            r"C:\drop\trailing.",
            r"C:\drop\trailing ",
        ] {
            let error = normalize_user_path(path).unwrap_err();

            assert!(
                error.contains("Windows 不安全路径"),
                "unexpected error for {path}: {error}"
            );
        }
    }

    #[test]
    fn transfer_scan_progress_dto_uses_stable_wire_labels() {
        let dto = transfer_scan_progress_to_dto(TransferPlanScanProgress {
            phase: TransferPlanScanPhase::Hashing,
            current_path: Some("drop/audio.m4a".to_string()),
            files_found: 2,
            directories_found: 1,
            bytes_found: 4096,
        });

        assert_eq!(dto.phase, "hashing");
        assert_eq!(dto.current_path.as_deref(), Some("drop/audio.m4a"));
        assert_eq!(dto.files_found, 2);
        assert_eq!(dto.directories_found, 1);
        assert_eq!(dto.bytes_found, 4096);
    }

    #[test]
    fn pending_receive_offer_dto_includes_resume_summary() {
        let offer = PendingReceiveOffer {
            transfer_id: "transfer-a".to_string(),
            root_name: "drop".to_string(),
            file_count: 2,
            total_bytes: 4096,
            sender_device_id: None,
            sender_device_name: None,
            sender_public_key_fingerprint: None,
            files: Vec::new(),
            resume_summary: Some(PendingReceiveResumeSummary {
                resumable_file_count: 2,
                completed_file_count: 1,
                partial_file_count: 1,
                received_bytes: 1536,
            }),
            decision: Arc::new((Mutex::new(None), Condvar::new())),
        };

        let dto = pending_offer_to_dto(&offer);

        let summary = dto
            .resume_summary
            .expect("resume summary should be present");
        assert_eq!(summary.resumable_file_count, 2);
        assert_eq!(summary.completed_file_count, 1);
        assert_eq!(summary.partial_file_count, 1);
        assert_eq!(summary.received_bytes, 1536);
    }

    #[test]
    fn pending_receive_offer_dto_limits_file_preview_for_large_folders() {
        let offer = PendingReceiveOffer {
            transfer_id: "transfer-a".to_string(),
            root_name: "drop".to_string(),
            file_count: 100,
            total_bytes: 4096,
            sender_device_id: None,
            sender_device_name: None,
            sender_public_key_fingerprint: None,
            files: (0..100)
                .map(|index| PendingReceiveFile {
                    manifest_path: format!("drop/file-{index:03}.txt"),
                    size: 1,
                    sha256: "a".repeat(64),
                })
                .collect(),
            resume_summary: None,
            decision: Arc::new((Mutex::new(None), Condvar::new())),
        };

        let dto = pending_offer_to_dto(&offer);

        assert_eq!(dto.file_count, 100);
        assert_eq!(dto.preview_file_count, RECEIVE_FILE_PREVIEW_LIMIT);
        assert_eq!(dto.files.len(), RECEIVE_FILE_PREVIEW_LIMIT);
        assert_eq!(dto.files[0].manifest_path, "drop/file-000.txt");
    }

    #[test]
    fn receive_report_dto_limits_file_preview_for_large_folders() {
        let report = TransferReceiveReport {
            transfer_id: "transfer-a".to_string(),
            root_name: "drop".to_string(),
            sender_device_id: None,
            sender_device_name: None,
            sender_public_key_fingerprint: None,
            bundle: None,
            files: (0..100)
                .map(|index| nekodrop_storage::ReceivedFile {
                    path: PathBuf::from(format!("/tmp/drop/file-{index:03}.txt")),
                    manifest_path: format!("drop/file-{index:03}.txt"),
                    bytes_written: 1,
                    sha256: "a".repeat(64),
                    verified: true,
                })
                .collect(),
        };

        let dto = receive_report_to_dto(&report);

        assert_eq!(dto.file_count, 100);
        assert_eq!(dto.files.len(), RECEIVE_FILE_PREVIEW_LIMIT);
        assert_eq!(dto.files[0].manifest_path, "drop/file-000.txt");
    }

    #[test]
    fn receive_report_dto_includes_bundle_preview() {
        let report = TransferReceiveReport {
            transfer_id: "transfer-a".to_string(),
            root_name: "bundle".to_string(),
            sender_device_id: None,
            sender_device_name: None,
            sender_public_key_fingerprint: None,
            bundle: Some(ReceivedBundleReport {
                bundle_id: "bundle_1234567890".to_string(),
                bundle_type: nekolink_protocol::BundleType::Skill,
                display_name: "voice_transcribe".to_string(),
                source_app: "OpenNeko".to_string(),
                file_count: 2,
                total_bytes: 28,
                staging_path: PathBuf::from("/tmp/bundle_1234567890"),
                import_allowed: true,
            }),
            files: Vec::new(),
        };

        let dto = receive_report_to_dto(&report);
        let bundle = dto.bundle.expect("bundle preview should be exposed");

        assert_eq!(bundle.bundle_id, "bundle_1234567890");
        assert_eq!(bundle.bundle_type, "skill");
        assert_eq!(bundle.display_name, "voice_transcribe");
        assert_eq!(bundle.source_app, "OpenNeko");
        assert_eq!(bundle.file_count, 2);
        assert_eq!(bundle.total_bytes, 28);
        assert_eq!(bundle.staging_path, "/tmp/bundle_1234567890");
        assert!(bundle.import_allowed);
    }

    #[test]
    fn receive_policy_block_all_rejects_offer_without_pending_prompt() {
        let pending = Arc::new(Mutex::new(None));
        let status = Arc::new(Mutex::new(None));
        let trusted = Arc::new(Mutex::new(Vec::new()));
        let offer = TransferOffer::new("transfer-a", "example.txt", Vec::new());

        let accepted = wait_for_receive_decision(
            &offer,
            &pending,
            &status,
            ReceivePolicy::BlockAll,
            &trusted,
            None,
        );

        assert!(!accepted);
        assert!(pending.lock().unwrap().is_none());
        let status = status.lock().unwrap().clone().unwrap();
        assert_eq!(status.phase, "blocked");
        assert!(status.message.contains("阻止"));
    }

    #[test]
    fn receive_policy_auto_accept_trusted_requires_authenticated_session() {
        let trusted = Arc::new(Mutex::new(vec![TrustedDeviceRecord {
            schema_version: 1,
            device_id: "device-a".to_string(),
            device_name: "MacBook".to_string(),
            platform: "macos".to_string(),
            host: "192.168.1.20".to_string(),
            port: 45821,
            public_key_fingerprint: "sha256:abc".to_string(),
            pairing_code: "AAA-BBB".to_string(),
            paired_at_ms: 1,
            last_seen_at_ms: 1,
        }]));
        let mut offer = TransferOffer::new("transfer-a", "example.txt", Vec::new());
        offer.sender_device_id = Some("device-a".to_string());
        offer.sender_public_key_fingerprint = Some("sha256:abc".to_string());

        let accepted =
            should_auto_accept_receive_offer(&offer, ReceivePolicy::AutoAcceptTrusted, &trusted);

        assert!(!accepted);
    }

    #[test]
    fn receive_policy_input_rejects_unknown_values() {
        assert_eq!(
            receive_policy_from_input("always_ask").unwrap(),
            ReceivePolicy::AlwaysAsk
        );
        assert_eq!(
            receive_policy_from_input("auto_accept_trusted").unwrap(),
            ReceivePolicy::AutoAcceptTrusted
        );
        assert_eq!(
            receive_policy_from_input("block_all").unwrap(),
            ReceivePolicy::BlockAll
        );
        assert!(receive_policy_from_input("unknown").is_err());
    }

    #[test]
    fn current_transfer_progress_uses_last_status_bytes() {
        let status = Arc::new(Mutex::new(Some(TransferStatusState {
            direction: "send".to_string(),
            phase: "sending".to_string(),
            root_name: Some("drop".to_string()),
            file_count: 2,
            file_index: 1,
            current_file: Some("drop/a.txt".to_string()),
            bytes_transferred: 42,
            total_bytes: 100,
            message: "发送中".to_string(),
            updated_at_ms: 1,
        })));

        let (file_index, current_file, bytes_transferred, total_bytes) =
            current_transfer_progress(&status);

        assert_eq!(file_index, 1);
        assert_eq!(current_file.as_deref(), Some("drop/a.txt"));
        assert_eq!(bytes_transferred, 42);
        assert_eq!(total_bytes, 100);
    }

    fn nearby_device(device_id: &str, fingerprint: &str) -> Device {
        let mut device = Device::new(
            nekodrop_core::DeviceId::new(device_id).unwrap(),
            "MacBook",
            nekodrop_core::DevicePlatform::MacOS,
            "192.168.1.20",
            45821,
        )
        .unwrap();
        device.public_key_fingerprint = Some(fingerprint.to_string());
        device
    }

    fn trusted_record(
        device_id: &str,
        device_name: &str,
        public_key_fingerprint: &str,
    ) -> TrustedDeviceRecord {
        TrustedDeviceRecord {
            schema_version: 1,
            device_id: device_id.to_string(),
            device_name: device_name.to_string(),
            platform: "macos".to_string(),
            host: "192.168.1.20".to_string(),
            port: 45821,
            public_key_fingerprint: public_key_fingerprint.to_string(),
            pairing_code: "AAA-BBB".to_string(),
            paired_at_ms: 1,
            last_seen_at_ms: 1,
        }
    }

    fn test_identity(device_id: &str) -> DeviceIdentity {
        DeviceIdentity::new(
            device_id,
            "This Mac",
            nekolink_protocol::DeviceKind::Desktop,
            nekolink_protocol::PlatformKind::Macos,
            "sha256:self",
            [],
        )
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn string_paths_to_path_bufs(paths: Vec<String>) -> Result<Vec<PathBuf>, String> {
    if paths.is_empty() {
        return Err("请至少输入一个文件或文件夹路径".into());
    }

    paths
        .into_iter()
        .map(|path| normalize_user_path(&path))
        .collect()
}

fn path_bufs_to_strings(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect()
}

fn parse_paths_text(paths_text: &str) -> Result<Vec<PathBuf>, String> {
    let paths = paths_text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.trim_matches('"').trim_matches('\'').to_string())
        .collect::<Vec<_>>();

    string_paths_to_path_bufs(paths)
}

fn normalize_user_path(path: &str) -> Result<PathBuf, String> {
    let path = strip_outer_path_quotes(path);
    validate_user_path_text(path)?;
    let expanded = expand_home_dir(path);
    if !expanded.exists() {
        return Err(format!("路径不存在：{}", expanded.display()));
    }
    Ok(expanded)
}

fn strip_outer_path_quotes(path: &str) -> &str {
    let trimmed_start = path.trim_start();
    let maybe_quoted = trimmed_start.trim_end();
    if maybe_quoted.len() >= 2 {
        let bytes = maybe_quoted.as_bytes();
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &maybe_quoted[1..maybe_quoted.len() - 1];
        }
    }
    trimmed_start
}

fn validate_user_path_text(path: &str) -> Result<(), String> {
    if path.contains('\u{fffd}') {
        return Err(
            "路径编码已经损坏，里面出现了 �。请重新用系统文件选择器选择文件，或从原始位置重新复制路径。"
                .to_string(),
        );
    }

    if let Some(reason) = windows_unsafe_user_path_reason(path) {
        return Err(format!(
            "Windows 不安全路径：{reason}。请重命名文件/文件夹后再发送，或重新选择正确路径。"
        ));
    }

    Ok(())
}

fn windows_unsafe_user_path_reason(path: &str) -> Option<String> {
    for (index, component) in path
        .split(['/', '\\'])
        .filter(|component| !component.is_empty())
        .enumerate()
    {
        if index == 0 && is_windows_drive_prefix(component) {
            continue;
        }
        if component.ends_with(' ') || component.ends_with('.') {
            return Some(format!("路径片段不能以空格或点结尾：{component}"));
        }
        if component
            .chars()
            .any(|ch| matches!(ch, '<' | '>' | '"' | '|' | '?' | '*'))
        {
            return Some(format!("路径片段包含 Windows 非法字符：{component}"));
        }
        if component.contains(':') {
            return Some(format!("路径片段包含 ADS 或非法冒号：{component}"));
        }
        if is_windows_reserved_user_path_component(component) {
            return Some(format!("路径片段使用了 Windows 保留名称：{component}"));
        }
    }
    None
}

fn is_windows_drive_prefix(component: &str) -> bool {
    let bytes = component.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn is_windows_reserved_user_path_component(component: &str) -> bool {
    let stem = component.split('.').next().unwrap_or(component);
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

fn expand_home_dir(path: &str) -> PathBuf {
    if path == "~" {
        return home_dir().unwrap_or_else(|| PathBuf::from(path));
    }

    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            return home.join(rest);
        }
    }

    PathBuf::from(path)
}

fn default_receive_dir() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Downloads")
        .join("NekoDrop")
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn bind_available_listener(bind_host: &str, requested_port: u16) -> Result<TcpListener, String> {
    let mut last_error = None;

    for offset in 0..20 {
        let Some(port) = requested_port.checked_add(offset) else {
            break;
        };
        match TcpListener::bind((bind_host, port)) {
            Ok(listener) => return Ok(listener),
            Err(error) => last_error = Some(format!("{bind_host}:{port}: {error}")),
        }
    }

    Err(format!(
        "无法监听端口，从 {requested_port} 起连续尝试失败：{}",
        last_error.unwrap_or_else(|| "没有可用端口".to_string())
    ))
}

#[derive(Debug, Clone, Copy)]
enum PathDialogKind {
    Files,
    Folders,
    SingleFolder,
}

fn parse_dialog_output(output: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(output)
        .lines()
        .map(|line| line.trim_start_matches('\u{feff}').trim())
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(any(target_os = "windows", test))]
fn windows_dialog_script(kind: PathDialogKind) -> String {
    let picker_script = match kind {
        PathDialogKind::Files => {
            r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.OpenFileDialog
$dialog.Multiselect = $true
$dialog.Title = '选择要发送的文件'
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  $dialog.FileNames -join "`n"
}
"#
        }
        PathDialogKind::Folders => {
            r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.FolderBrowserDialog
$dialog.Description = '选择要发送的文件夹'
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  $dialog.SelectedPath
}
"#
        }
        PathDialogKind::SingleFolder => {
            r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.FolderBrowserDialog
$dialog.Description = '选择接收目录'
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  $dialog.SelectedPath
}
"#
        }
    };

    format!(
        r#"
$utf8NoBom = New-Object System.Text.UTF8Encoding -ArgumentList $false
[Console]::OutputEncoding = $utf8NoBom
$OutputEncoding = $utf8NoBom
{picker_script}
"#
    )
}

#[cfg(target_os = "macos")]
fn choose_paths(kind: PathDialogKind) -> Result<Vec<String>, String> {
    let script = match kind {
        PathDialogKind::Files => {
            r#"
set pickedItems to choose file with prompt "选择要发送的文件" with multiple selections allowed
set outputText to ""
repeat with pickedItem in pickedItems
  set outputText to outputText & POSIX path of pickedItem & linefeed
end repeat
return outputText
"#
        }
        PathDialogKind::Folders => {
            r#"
set pickedItems to choose folder with prompt "选择要发送的文件夹" with multiple selections allowed
set outputText to ""
repeat with pickedItem in pickedItems
  set outputText to outputText & POSIX path of pickedItem & linefeed
end repeat
return outputText
"#
        }
        PathDialogKind::SingleFolder => {
            r#"
set pickedItem to choose folder with prompt "选择接收目录"
return POSIX path of pickedItem
"#
        }
    };

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|error| format!("无法打开系统选择窗口：{error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("User canceled") || stderr.contains("-128") {
            return Ok(Vec::new());
        }
        return Err(format!("系统选择窗口失败：{}", stderr.trim()));
    }

    Ok(parse_dialog_output(&output.stdout))
}

#[cfg(target_os = "windows")]
fn choose_paths(kind: PathDialogKind) -> Result<Vec<String>, String> {
    let script = windows_dialog_script(kind);

    let output = Command::new("powershell")
        .args(["-NoProfile", "-STA", "-Command", &script])
        .output()
        .map_err(|error| format!("无法打开系统选择窗口：{error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("系统选择窗口失败：{}", stderr.trim()));
    }

    Ok(parse_dialog_output(&output.stdout))
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn choose_paths(kind: PathDialogKind) -> Result<Vec<String>, String> {
    let mut args = vec!["--file-selection".to_string()];
    match kind {
        PathDialogKind::Files => {
            args.push("--multiple".to_string());
            args.push("--separator=\n".to_string());
            args.push("--title=选择要发送的文件".to_string());
        }
        PathDialogKind::Folders => {
            args.push("--directory".to_string());
            args.push("--multiple".to_string());
            args.push("--separator=\n".to_string());
            args.push("--title=选择要发送的文件夹".to_string());
        }
        PathDialogKind::SingleFolder => {
            args.push("--directory".to_string());
            args.push("--title=选择接收目录".to_string());
        }
    }

    let output = Command::new("zenity")
        .args(args)
        .output()
        .map_err(|error| format!("无法打开系统选择窗口：{error}"))?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    Ok(parse_dialog_output(&output.stdout))
}

fn open_path_with_system(path: PathBuf) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(&path);
        command
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer");
        command.arg(&path);
        command
    };

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(&path);
        command
    };

    command
        .spawn()
        .map_err(|error| format!("无法打开 {}：{error}", path.display()))?;
    Ok(())
}
