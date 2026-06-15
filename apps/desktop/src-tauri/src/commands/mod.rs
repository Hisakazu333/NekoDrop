use std::fs;
use std::io::ErrorKind;
use std::net::{IpAddr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nekodrop_core::{
    Device, DeviceTrustState, FileManifest, ManifestItem, ManifestItemKind, NekoDropError,
    ReceivePolicy,
};
use nekodrop_network::{
    ConnectionTicket, Endpoint, PairingDecisionPayload, PairingRequestPayload, TransferOffer,
    TransferProgress,
};
use nekodrop_service::{
    accept_incoming_stream_with_authenticated_control_bundle_staging_peer_verifier_and_cancel,
    create_transfer_plan as create_service_transfer_plan, create_transfer_plan_with_scan_progress,
    send_pairing_request, send_plan_with_authenticated_session_peer_verifier_and_cancel,
    IncomingSessionReport, ReceivedBundleReport, TransferPlanScanProgress, TransferProgressEvent,
    TransferReceiveReport, TransferSecurityMode, TransferSendReport, TransferSourceFile,
    TransferSourcePlan,
};
use nekodrop_storage::{
    build_resume_plan_for_files, create_manual_bundle_directory,
    delete_staged_bundle as delete_staged_bundle_storage,
    import_staged_bundle as import_staged_bundle_storage,
    list_staged_bundles as list_staged_bundles_storage, prune_staged_bundles_older_than,
    ManualBundleCreateRequest, ResumeExpectedFile, ResumePlan, StagedBundle,
};
use nekolink_protocol::{
    BundlePermissionScope, BundlePermissions, BundleSecretsPolicy, BundleSender, BundleType,
    BundleWriteMode, BundleWritePermission, DeviceIdentity, LocalBridgeAuthorizationRequest,
    LocalBridgeClientIdentity, LocalBridgeEvent, LocalBridgePermissionScope, LocalBridgeRequest,
    SignedSessionIdentityBinding,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::app_config::{receive_policy_label, save_app_config};
use crate::app_state::{
    ActiveReceiveSession, AppState, LocalBridgeAuthorizationRecord, LocalBridgePendingAction,
    LocalBridgePendingImportBundleAction, LocalBridgePendingSendBundleAction,
    LocalBridgeRuntimeState, PendingLocalBridgeAuthorization, PendingPairingRequest,
    PendingReceiveFile, PendingReceiveOffer, PendingReceiveResumeSummary, ReceiveDecision,
    TransferStatusState,
};
use crate::device_identity::app_config_dir;
use crate::local_bridge_authorizations::{
    local_bridge_authorizations_file_path, save_local_bridge_authorizations,
    save_local_bridge_authorizations_at,
};
use crate::local_bridge_runtime;
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
const STAGED_BUNDLE_RETENTION_SECS: u64 = 14 * 24 * 60 * 60;
const LOCAL_BRIDGE_EVENT_QUEUE_LIMIT: usize = 256;
const LOCAL_BRIDGE_PENDING_ACTION_QUEUE_LIMIT: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReceiveTrustContext {
    Untrusted,
    AuthenticatedTrusted,
}

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
    pub public_key: Option<String>,
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
    pub public_key: String,
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
    pub security_mode: Option<String>,
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
    pub staging_status: String,
    pub can_import_now: bool,
    pub import_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManualBundleCreateDto {
    pub bundle_id: String,
    pub bundle_type: String,
    pub display_name: String,
    pub source_app: String,
    pub staging_path: String,
    pub file_count: usize,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManualBundleCreateRequestDto {
    pub source_path: String,
    pub bundle_type: String,
    pub display_name: String,
    pub source_app: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReceiveReportDto {
    pub transfer_id: String,
    pub root_name: String,
    pub security_mode: String,
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
    pub public_key: String,
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
pub struct DesktopRealtimeSnapshotDto {
    pub receive_status: Option<String>,
    pub receive_session: Option<ReceiveSessionDto>,
    pub receive_report: Option<ReceiveReportDto>,
    pub pending_receive_offer: Option<PendingReceiveOfferDto>,
    pub pending_pairing_request: Option<PendingPairingRequestDto>,
    pub transfer_status: Option<TransferStatusDto>,
    pub discovery_status: DiscoveryStatusDto,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalBridgeResponseDto {
    pub request_id: String,
    pub status: String,
    pub message: String,
    pub security_state: String,
    pub requires_user_confirmation: bool,
    pub client_state: String,
    pub client_id: Option<String>,
    pub client_display_name: Option<String>,
    pub authorization_scopes: Vec<String>,
    pub authorization_reason: Option<String>,
    pub authorization_ttl_seconds: Option<u64>,
    pub authorization_code: Option<String>,
    pub authorization_expires_at_ms: Option<u128>,
    pub devices: Vec<TrustedDeviceDto>,
    pub staged_bundles: Vec<ReceivedBundleDto>,
    pub transfer_status: Option<TransferStatusDto>,
    pub events: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalBridgeAuthorizationDto {
    pub client_id: String,
    pub display_name: String,
    pub app_kind: Option<String>,
    pub scopes: Vec<String>,
    pub granted_at_ms: u128,
    pub expires_at_ms: Option<u128>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalBridgeRuntimeStatusDto {
    pub active: bool,
    pub bind_host: String,
    pub port: u16,
    pub request_path: String,
    pub max_request_bytes: usize,
    pub pending_authorization_client: Option<String>,
    pub authorization_count: usize,
    pub pending_action_count: usize,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalBridgeAuthorizationListDto {
    pub authorizations: Vec<LocalBridgeAuthorizationDto>,
    pub pruned_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalBridgeAuthorizationRevokeDto {
    pub revoked: bool,
    pub authorizations: Vec<LocalBridgeAuthorizationDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalBridgePendingActionDto {
    pub request_id: String,
    pub action_kind: String,
    pub client_id: String,
    pub client_display_name: String,
    pub bundle_type: Option<String>,
    pub target_device_id: Option<String>,
    pub staged_bundle_id: Option<String>,
    pub expected_bundle_type: Option<String>,
    pub require_trusted_device: Option<bool>,
    pub requested_at_ms: u128,
    pub bundle_root: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalBridgePendingActionListDto {
    pub actions: Vec<LocalBridgePendingActionDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalBridgePendingActionRemoveDto {
    pub removed: bool,
    pub actions: Vec<LocalBridgePendingActionDto>,
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
    let device_count = state
        .nearby_devices
        .lock()
        .map_err(|error| error.to_string())?
        .len();

    discovery_status_snapshot(&state, device_count)
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
    let local_public_key = state.device_identity.public_key()?;
    let pairing_code = pairing_code_for_device(&local_identity, &device)
        .ok_or_else(|| "这个设备缺少公开指纹，当前不能发起配对。".to_string())?;
    let request = PairingRequestPayload {
        request_id: format!("pairing-{}", now_ms()),
        device_id: local_identity.device_id.clone(),
        device_name: local_identity.device_name.clone(),
        platform: local_identity.platform.as_str().to_string(),
        public_key: local_public_key,
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
pub fn get_desktop_realtime_snapshot(
    state: State<'_, AppState>,
) -> Result<DesktopRealtimeSnapshotDto, String> {
    desktop_realtime_snapshot(&state)
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
pub fn list_staged_bundles() -> Result<Vec<ReceivedBundleDto>, String> {
    let staging_root = bundle_staging_root()?;
    list_staged_bundle_dtos_at(&staging_root)
}

#[tauri::command]
pub fn prune_staged_bundles() -> Result<Vec<String>, String> {
    let staging_root = bundle_staging_root()?;
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(STAGED_BUNDLE_RETENTION_SECS))
        .unwrap_or(UNIX_EPOCH);
    prune_staged_bundle_dtos_at(&staging_root, cutoff)
}

#[tauri::command]
pub fn delete_staged_bundle(bundle_id: String) -> Result<bool, String> {
    let staging_root = bundle_staging_root()?;
    delete_staged_bundle_at(&staging_root, &bundle_id)
}

#[tauri::command]
pub fn import_staged_bundle(bundle_id: String) -> Result<ReceivedBundleDto, String> {
    let staging_root = bundle_staging_root()?;
    let import_root = bundle_import_root()?;
    import_staged_bundle_at(&staging_root, &import_root, &bundle_id)
}

#[tauri::command]
pub fn create_manual_bundle(
    state: State<'_, AppState>,
    request: ManualBundleCreateRequestDto,
) -> Result<ManualBundleCreateDto, String> {
    let bundle_type = parse_bundle_type(&request.bundle_type)?;
    let display_name = request.display_name.trim().to_string();
    if display_name.is_empty() {
        return Err("资料包名称不能为空".to_string());
    }
    let source_app = request.source_app.trim().to_string();
    if source_app.is_empty() {
        return Err("来源应用不能为空".to_string());
    }
    let source_path = expand_home_dir(&request.source_path);
    let output_root = manual_bundle_output_root()?;
    fs::create_dir_all(&output_root)
        .map_err(|error| format!("无法创建资料包输出目录 {}: {error}", output_root.display()))?;

    let identity = state.device_identity.public_identity();
    let sender = BundleSender {
        device_id: identity.device_id,
        device_name: identity.device_name,
        fingerprint: identity.public_key_fingerprint,
    };
    let created = create_manual_bundle_directory(ManualBundleCreateRequest {
        source_path: source_path.clone(),
        output_root,
        bundle_id: manual_bundle_id(&display_name, &bundle_type, &source_path),
        bundle_type,
        display_name,
        source_app,
        sender,
        created_at: current_utc_timestamp(),
        permissions: Some(manual_bundle_permissions(&bundle_type)),
    })
    .map_err(|error| error.to_string())?;

    let manifest = &created.detected.manifest;
    Ok(ManualBundleCreateDto {
        bundle_id: manifest.bundle_id.clone(),
        bundle_type: bundle_type_label(manifest.bundle_type).to_string(),
        display_name: manifest.display_name.clone(),
        source_app: manifest.source_app.clone(),
        staging_path: created.staging_path.display().to_string(),
        file_count: manifest.summary.file_count,
        total_bytes: manifest.summary.total_bytes,
    })
}

#[tauri::command]
pub fn handle_local_bridge_request(
    state: State<'_, AppState>,
    request_json: String,
) -> Result<LocalBridgeResponseDto, String> {
    handle_local_bridge_request_for_runtime(
        &state.trusted_devices,
        &state.transfer_status,
        &state.local_bridge_runtime,
        &request_json,
    )
}

#[tauri::command]
pub fn confirm_local_bridge_authorization(
    state: State<'_, AppState>,
    authorization_code: String,
) -> Result<LocalBridgeAuthorizationDto, String> {
    let now_ms = now_ms();
    let authorization = confirm_local_bridge_runtime_authorization_and_persist(
        &state.local_bridge_runtime,
        &authorization_code,
        now_ms,
    )?;
    Ok(local_bridge_authorization_to_dto(authorization))
}

#[tauri::command]
pub fn get_local_bridge_runtime_status(
    state: State<'_, AppState>,
) -> Result<LocalBridgeRuntimeStatusDto, String> {
    Ok(local_bridge_runtime_status_to_dto(
        local_bridge_runtime::local_bridge_runtime_status(&state.local_bridge_runtime),
    ))
}

#[tauri::command]
pub fn list_local_bridge_authorizations(
    state: State<'_, AppState>,
) -> Result<LocalBridgeAuthorizationListDto, String> {
    let now_ms = now_ms();
    let pruned_count =
        prune_local_bridge_authorizations_and_persist(&state.local_bridge_runtime, now_ms)?;
    Ok(LocalBridgeAuthorizationListDto {
        authorizations: local_bridge_authorizations_to_dtos(list_local_bridge_authorizations_at(
            &state.local_bridge_runtime,
            now_ms,
        )),
        pruned_count,
    })
}

#[tauri::command]
pub fn revoke_local_bridge_authorization(
    state: State<'_, AppState>,
    client_id: String,
    scope: String,
) -> Result<LocalBridgeAuthorizationRevokeDto, String> {
    let now_ms = now_ms();
    let scope = parse_local_bridge_permission_scope(&scope)?;
    let revoked = revoke_local_bridge_authorization_and_persist(
        &state.local_bridge_runtime,
        &client_id,
        scope,
        now_ms,
    )?;
    Ok(LocalBridgeAuthorizationRevokeDto {
        revoked,
        authorizations: local_bridge_authorizations_to_dtos(list_local_bridge_authorizations_at(
            &state.local_bridge_runtime,
            now_ms,
        )),
    })
}

#[tauri::command]
pub fn list_local_bridge_pending_actions(
    state: State<'_, AppState>,
) -> Result<LocalBridgePendingActionListDto, String> {
    Ok(LocalBridgePendingActionListDto {
        actions: list_local_bridge_pending_actions_at(&state.local_bridge_runtime)?,
    })
}

#[tauri::command]
pub fn remove_local_bridge_pending_action(
    state: State<'_, AppState>,
    request_id: String,
) -> Result<LocalBridgePendingActionRemoveDto, String> {
    let removed = remove_local_bridge_pending_action_at(&state.local_bridge_runtime, &request_id)?;
    Ok(LocalBridgePendingActionRemoveDto {
        removed,
        actions: list_local_bridge_pending_actions_at(&state.local_bridge_runtime)?,
    })
}

#[tauri::command]
pub fn prune_local_bridge_authorizations(
    state: State<'_, AppState>,
) -> Result<LocalBridgeAuthorizationListDto, String> {
    let now_ms = now_ms();
    let pruned_count =
        prune_local_bridge_authorizations_and_persist(&state.local_bridge_runtime, now_ms)?;
    Ok(LocalBridgeAuthorizationListDto {
        authorizations: local_bridge_authorizations_to_dtos(list_local_bridge_authorizations_at(
            &state.local_bridge_runtime,
            now_ms,
        )),
        pruned_count,
    })
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
    trusted_public_key: Option<String>,
    trusted_public_key_fingerprint: Option<String>,
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
    let trusted_record = trusted_devices
        .iter()
        .find(|record| trusted_record_matches(device, record));
    let Some(trusted_record) = trusted_record else {
        return Err("这台设备还没有可信配对，请先完成配对再发送文件。".to_string());
    };

    let endpoint = Endpoint::tcp(device.host.clone(), device.port);
    let peer = TransferPeer {
        device_id: Some(device.id.as_str().to_string()),
        name: Some(device.name.clone()),
        fingerprint: device.public_key_fingerprint.clone(),
        trusted_public_key: Some(trusted_record.public_key.clone()),
        trusted_public_key_fingerprint: Some(trusted_record.public_key_fingerprint.clone()),
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

fn verify_signed_session_against_trusted_pin(
    identity: &DeviceIdentity,
    signed_binding: &SignedSessionIdentityBinding,
    expected_device_id: Option<&str>,
    expected_public_key: Option<&str>,
    expected_public_key_fingerprint: Option<&str>,
) -> Result<(), String> {
    signed_binding
        .binding
        .verify_identity(identity)
        .map_err(|error| format!("可信设备身份校验失败：binding 不匹配: {}", error.message))?;
    if let Some(expected_device_id) = expected_device_id {
        if identity.device_id != expected_device_id {
            return Err("可信设备身份校验失败：device_id 不匹配".to_string());
        }
    }
    if let Some(expected_fingerprint) = expected_public_key_fingerprint {
        if identity.public_key_fingerprint != expected_fingerprint {
            return Err("可信设备身份校验失败：session 指纹不匹配".to_string());
        }
        if signed_binding.public_key_fingerprint != expected_fingerprint {
            return Err("可信设备身份校验失败：签名指纹不匹配".to_string());
        }
    }
    let Some(expected_public_key) = expected_public_key else {
        return Ok(());
    };
    if expected_public_key_fingerprint.is_none() {
        return Err("可信设备身份校验失败：缺少可信指纹".to_string());
    }
    if signed_binding.public_key != expected_public_key {
        return Err("可信设备身份校验失败：长期公钥不匹配".to_string());
    }
    Ok(())
}

fn verify_peer_matches_transfer_peer(
    peer: &TransferPeer,
    identity: &DeviceIdentity,
    signed_binding: &SignedSessionIdentityBinding,
) -> Result<(), String> {
    verify_signed_session_against_trusted_pin(
        identity,
        signed_binding,
        peer.device_id.as_deref(),
        peer.trusted_public_key.as_deref(),
        peer.trusted_public_key_fingerprint
            .as_deref()
            .or(peer.fingerprint.as_deref()),
    )
}

fn verify_incoming_peer_against_trusted_devices(
    trusted_devices: &[TrustedDeviceRecord],
    identity: &DeviceIdentity,
    signed_binding: &SignedSessionIdentityBinding,
) -> Result<ReceiveTrustContext, String> {
    let Some(record) = trusted_devices
        .iter()
        .find(|record| record.device_id == identity.device_id)
    else {
        return Ok(ReceiveTrustContext::Untrusted);
    };

    verify_signed_session_against_trusted_pin(
        identity,
        signed_binding,
        Some(record.device_id.as_str()),
        Some(record.public_key.as_str()),
        Some(record.public_key_fingerprint.as_str()),
    )?;
    Ok(ReceiveTrustContext::AuthenticatedTrusted)
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
                trusted_public_key: Some(device.public_key.clone()),
                trusted_public_key_fingerprint: Some(device.public_key_fingerprint.clone()),
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
        trusted_public_key: None,
        trusted_public_key_fingerprint: None,
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
                trusted_public_key: None,
                trusted_public_key_fingerprint: None,
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
                    trusted_public_key: None,
                    trusted_public_key_fingerprint: None,
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
    let local_device_identity = state.device_identity.clone();
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
    let local_bridge_runtime = state.local_bridge_runtime.clone();
    let cancel_for_send = cancel.clone();
    let report = send_with_auto_retry(
        || {
            let transfer_status = transfer_status.clone();
            let runtime_for_progress = local_bridge_runtime.clone();
            let transfer_id_for_progress = transfer_id.clone();
            let cancel_for_attempt = cancel_for_send.clone();
            let local_device_identity = local_device_identity.clone();
            let peer_for_verifier = peer.clone();
            send_plan_with_authenticated_session_peer_verifier_and_cancel(
                &endpoint,
                plan.clone(),
                &sender_identity,
                move |binding| {
                    local_device_identity
                        .sign_session_identity_binding(binding)
                        .map_err(NekoDropError::Network)
                },
                move |identity, signed_binding| {
                    verify_peer_matches_transfer_peer(&peer_for_verifier, identity, signed_binding)
                        .map_err(NekoDropError::Network)
                },
                move |event| {
                    if let Some(status) = status_from_progress_event("send", None, event) {
                        set_transfer_status_and_push_bridge_event(
                            &transfer_status,
                            &runtime_for_progress,
                            &transfer_id_for_progress,
                            status,
                        );
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
        set_transfer_status_and_push_bridge_event(
            &state.transfer_status,
            &state.local_bridge_runtime,
            &transfer_id,
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
    set_transfer_status_and_push_bridge_event(
        &state.transfer_status,
        &state.local_bridge_runtime,
        &transfer_id,
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
pub fn select_manual_bundle_source_dir() -> Result<Option<String>, String> {
    Ok(choose_paths(PathDialogKind::BundleSourceFolder)?
        .into_iter()
        .next())
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
    let local_bridge_runtime = state.local_bridge_runtime.clone();
    let local_device_identity = state.device_identity.clone();
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
                let trusted_for_session = trusted_devices.clone();
                let receive_trust_context = Arc::new(Mutex::new(ReceiveTrustContext::Untrusted));
                let receive_trust_for_session = receive_trust_context.clone();
                let receive_trust_for_decision = receive_trust_context.clone();
                let pending_for_pairing = pending_pairing_request.clone();
                let status_for_decision = transfer_status.clone();
                let status_for_progress = transfer_status.clone();
                let runtime_for_progress = local_bridge_runtime.clone();
                let receive_dir_for_decision = receive_dir_for_thread.clone();
                let trusted_for_pairing = trusted_devices.clone();
                let local_for_pairing = local_identity.clone();
                let local_for_signing = local_device_identity.clone();
                let peer_host_for_pairing = peer_host.clone();
                let current_receive_cancel = Arc::new(AtomicBool::new(false));
                if let Ok(mut active_cancel) = active_receive_cancel.lock() {
                    *active_cancel = Some(current_receive_cancel.clone());
                }
                let result =
                    accept_incoming_stream_with_authenticated_control_bundle_staging_peer_verifier_and_cancel(
                        &mut stream,
                        &receive_dir_for_thread,
                        &bundle_staging_root_for_thread,
                        &local_identity,
                        move |binding| {
                            local_for_signing
                                .sign_session_identity_binding(binding)
                                .map_err(NekoDropError::Network)
                        },
                        move |identity, signed_binding| {
                            let trusted_devices = trusted_for_session
                                .lock()
                                .map_err(|error| NekoDropError::Network(error.to_string()))?;
                            let trust_context = verify_incoming_peer_against_trusted_devices(
                                &trusted_devices,
                                identity,
                                signed_binding,
                            )
                            .map_err(NekoDropError::Network)?;
                            if let Ok(mut slot) = receive_trust_for_session.lock() {
                                *slot = trust_context;
                            }
                            Ok(())
                        },
                        move |offer| {
                            let resume_summary =
                                pending_resume_summary_from_offer(&receive_dir_for_decision, offer);
                            let trust_context = receive_trust_for_decision
                                .lock()
                                .map(|context| *context)
                                .unwrap_or(ReceiveTrustContext::Untrusted);
                            wait_for_receive_decision(
                                offer,
                                &pending_for_decision,
                                &status_for_decision,
                                receive_policy,
                                &trusted_for_decision,
                                trust_context,
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
                                let transfer_id_for_event = status
                                    .root_name
                                    .as_deref()
                                    .unwrap_or("receive")
                                    .to_string();
                                set_transfer_status_and_push_bridge_event(
                                    &status_for_progress,
                                    &runtime_for_progress,
                                    &transfer_id_for_event,
                                    status,
                                );
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
                            set_transfer_status_and_push_bridge_event(
                                &transfer_status,
                                &local_bridge_runtime,
                                &report.transfer_id,
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
                            record.security_mode = Some(
                                transfer_security_mode_label(report.security_mode).to_string(),
                            );
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
                            if let Some(bundle) = report.bundle.as_ref() {
                                let _ = push_local_bridge_bundle_received_event(
                                    &local_bridge_runtime,
                                    &report.transfer_id,
                                    bundle,
                                );
                            }
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
    if session.is_none() {
        return Ok(receive_port_diagnostics_from_session(None, Vec::new()));
    }
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

fn desktop_realtime_snapshot(state: &AppState) -> Result<DesktopRealtimeSnapshotDto, String> {
    let receive_status = state
        .receive_status
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let receive_session = state
        .receive_session
        .lock()
        .map_err(|error| error.to_string())?
        .as_ref()
        .map(receive_session_to_dto);
    let receive_report = state
        .last_receive_report
        .lock()
        .map_err(|error| error.to_string())?
        .as_ref()
        .map(receive_report_to_dto);
    let pending_receive_offer = state
        .pending_receive_offer
        .lock()
        .map_err(|error| error.to_string())?
        .as_ref()
        .map(pending_offer_to_dto);
    let pending_pairing_request = state
        .pending_pairing_request
        .lock()
        .map_err(|error| error.to_string())?
        .as_ref()
        .map(pending_pairing_request_to_dto);
    let transfer_status = state
        .transfer_status
        .lock()
        .map_err(|error| error.to_string())?
        .as_ref()
        .map(transfer_status_to_dto);
    let device_count = state
        .nearby_devices
        .lock()
        .map_err(|error| error.to_string())?
        .len();
    let discovery_status = discovery_status_snapshot(state, device_count)?;

    Ok(DesktopRealtimeSnapshotDto {
        receive_status,
        receive_session,
        receive_report,
        pending_receive_offer,
        pending_pairing_request,
        transfer_status,
        discovery_status,
    })
}

fn discovery_status_snapshot(
    state: &AppState,
    device_count: usize,
) -> Result<DiscoveryStatusDto, String> {
    let status = state
        .discovery_status
        .lock()
        .map_err(|error| error.to_string())?;

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
        public_key: device.public_key.clone(),
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
        public_key: device.public_key.clone(),
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
        security_mode: transfer_security_mode_label(report.security_mode).to_string(),
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
        staging_status: "saved".to_string(),
        can_import_now: false,
        import_path: None,
    }
}

fn transfer_security_mode_label(mode: TransferSecurityMode) -> &'static str {
    match mode {
        TransferSecurityMode::LegacyPlain => "legacy_plain",
        TransferSecurityMode::EncryptedSession => "encrypted_session",
        TransferSecurityMode::AuthenticatedEncryptedSession => "authenticated_encrypted_session",
    }
}

fn staged_bundle_to_dto(staged: &StagedBundle) -> ReceivedBundleDto {
    let manifest = &staged.detected.manifest;
    ReceivedBundleDto {
        bundle_id: manifest.bundle_id.clone(),
        bundle_type: bundle_type_label(manifest.bundle_type).to_string(),
        display_name: manifest.display_name.clone(),
        source_app: manifest.source_app.clone(),
        file_count: manifest.summary.file_count,
        total_bytes: manifest.summary.total_bytes,
        staging_path: staged.staging_path.display().to_string(),
        import_allowed: staged.detected.import_policy
            == nekodrop_storage::BundleImportPolicy::ImportAllowed,
        staging_status: "saved".to_string(),
        can_import_now: staged.detected.import_policy
            == nekodrop_storage::BundleImportPolicy::ImportAllowed,
        import_path: None,
    }
}

fn list_staged_bundle_dtos_at(
    staging_root: &std::path::Path,
) -> Result<Vec<ReceivedBundleDto>, String> {
    list_staged_bundles_storage(staging_root)
        .map_err(|error| error.to_string())
        .map(|bundles| bundles.iter().map(staged_bundle_to_dto).collect())
}

fn find_staged_bundle_dto_at(
    staging_root: &std::path::Path,
    bundle_id: &str,
) -> Result<Option<ReceivedBundleDto>, String> {
    Ok(list_staged_bundle_dtos_at(staging_root)?
        .into_iter()
        .find(|bundle| bundle.bundle_id == bundle_id))
}

fn prune_staged_bundle_dtos_at(
    staging_root: &std::path::Path,
    cutoff: SystemTime,
) -> Result<Vec<String>, String> {
    prune_staged_bundles_older_than(staging_root, cutoff).map_err(|error| error.to_string())
}

fn delete_staged_bundle_at(
    staging_root: &std::path::Path,
    bundle_id: &str,
) -> Result<bool, String> {
    validate_safe_bundle_id(bundle_id)?;
    delete_staged_bundle_storage(staging_root, bundle_id).map_err(|error| error.to_string())
}

fn import_staged_bundle_at(
    staging_root: &std::path::Path,
    import_root: &std::path::Path,
    bundle_id: &str,
) -> Result<ReceivedBundleDto, String> {
    validate_safe_bundle_id(bundle_id)?;
    let staged_path = staging_root.join(bundle_id);
    let imported = import_staged_bundle_storage(&staged_path, import_root)
        .map_err(|error| error.to_string())?;
    Ok(ReceivedBundleDto {
        bundle_id: imported.bundle_id,
        bundle_type: bundle_type_label(imported.bundle_type).to_string(),
        display_name: imported.display_name,
        source_app: imported.source_app,
        file_count: imported.file_count,
        total_bytes: imported.total_bytes,
        staging_path: staged_path.display().to_string(),
        import_allowed: true,
        staging_status: "imported".to_string(),
        can_import_now: false,
        import_path: Some(imported.destination_path.display().to_string()),
    })
}

fn validate_safe_bundle_id(bundle_id: &str) -> Result<(), String> {
    let trimmed = bundle_id.trim();
    if trimmed.is_empty()
        || trimmed != bundle_id
        || bundle_id.contains('/')
        || bundle_id.contains('\\')
        || bundle_id.contains("..")
        || bundle_id.contains(':')
        || bundle_id.contains('\0')
    {
        return Err(format!("bundle_id 不安全: {bundle_id}"));
    }
    Ok(())
}

fn handle_local_bridge_request_at(
    request_json: &str,
    trusted_devices: &[TrustedDeviceRecord],
    transfer_status: Option<&TransferStatusState>,
    staging_root: &std::path::Path,
) -> Result<LocalBridgeResponseDto, String> {
    handle_local_bridge_request_with_auth_at(
        request_json,
        trusted_devices,
        transfer_status,
        staging_root,
        &[],
        now_ms(),
    )
}

pub(crate) fn handle_local_bridge_request_for_runtime(
    trusted_devices: &Arc<Mutex<Vec<TrustedDeviceRecord>>>,
    transfer_status: &Arc<Mutex<Option<TransferStatusState>>>,
    runtime: &LocalBridgeRuntimeState,
    request_json: &str,
) -> Result<LocalBridgeResponseDto, String> {
    let trusted_devices = trusted_devices
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let transfer_status = transfer_status
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let staging_root = bundle_staging_root()?;
    handle_local_bridge_request_with_runtime_at(
        request_json,
        &trusted_devices,
        transfer_status.as_ref(),
        &staging_root,
        runtime,
        now_ms(),
    )
}

fn handle_local_bridge_request_with_runtime_at(
    request_json: &str,
    trusted_devices: &[TrustedDeviceRecord],
    transfer_status: Option<&TransferStatusState>,
    staging_root: &std::path::Path,
    runtime: &LocalBridgeRuntimeState,
    now_ms: u128,
) -> Result<LocalBridgeResponseDto, String> {
    let request: LocalBridgeRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid bridge request JSON: {error}"))?;
    request.validate().map_err(|error| error.message)?;

    if let LocalBridgeRequest::AuthorizationRequest(request) = &request {
        let pending = pending_local_bridge_authorization_from_request(request, now_ms)?;
        let response = local_bridge_pending_authorization_response_from_pending(&pending);
        *runtime
            .pending_authorization
            .lock()
            .map_err(|error| error.to_string())? = Some(pending);
        return Ok(response);
    }

    let authorizations = runtime
        .authorizations
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let events = runtime
        .events
        .lock()
        .map_err(|error| error.to_string())?
        .clone();

    if let LocalBridgeRequest::SendBundle(request) = &request {
        if local_bridge_client_has_scope(
            request.client.as_ref(),
            &authorizations,
            LocalBridgePermissionScope::BundleSend,
            now_ms,
        ) {
            push_local_bridge_pending_action(
                runtime,
                LocalBridgePendingAction::SendBundle(
                    local_bridge_pending_send_action_from_request(request, now_ms)?,
                ),
            )?;
            return Ok(local_bridge_authorized_runtime_pending_response(
                request.request_id.clone(),
                request.client.clone(),
                "local bridge bundle send is authorized and waiting for the desktop runtime",
            ));
        }
    }

    if let LocalBridgeRequest::ImportBundle(request) = &request {
        if local_bridge_client_has_scope(
            request.client.as_ref(),
            &authorizations,
            LocalBridgePermissionScope::BundleImportRequest,
            now_ms,
        ) {
            push_local_bridge_pending_action(
                runtime,
                LocalBridgePendingAction::ImportBundle(
                    local_bridge_pending_import_action_from_request(request, now_ms)?,
                ),
            )?;
            return Ok(local_bridge_authorized_runtime_pending_response(
                request.request_id.clone(),
                request.client.clone(),
                "local bridge bundle import is authorized and waiting for the desktop runtime",
            ));
        }
    }

    handle_validated_local_bridge_request_with_auth_at(
        request,
        trusted_devices,
        transfer_status,
        staging_root,
        &authorizations,
        &events,
        now_ms,
    )
}

fn push_local_bridge_pending_action(
    runtime: &LocalBridgeRuntimeState,
    action: LocalBridgePendingAction,
) -> Result<(), String> {
    let mut actions = runtime
        .pending_actions
        .lock()
        .map_err(|error| error.to_string())?;
    actions.push(action);
    if actions.len() > LOCAL_BRIDGE_PENDING_ACTION_QUEUE_LIMIT {
        let excess = actions.len() - LOCAL_BRIDGE_PENDING_ACTION_QUEUE_LIMIT;
        actions.drain(0..excess);
    }
    Ok(())
}

fn list_local_bridge_pending_actions_at(
    runtime: &LocalBridgeRuntimeState,
) -> Result<Vec<LocalBridgePendingActionDto>, String> {
    let actions = runtime
        .pending_actions
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(actions
        .iter()
        .map(local_bridge_pending_action_to_dto)
        .collect())
}

fn remove_local_bridge_pending_action_at(
    runtime: &LocalBridgeRuntimeState,
    request_id: &str,
) -> Result<bool, String> {
    if request_id.trim().is_empty() {
        return Err("request_id 不能为空".to_string());
    }
    let mut actions = runtime
        .pending_actions
        .lock()
        .map_err(|error| error.to_string())?;
    let before_len = actions.len();
    actions.retain(|action| local_bridge_pending_action_request_id(action) != request_id);
    Ok(actions.len() != before_len)
}

fn local_bridge_pending_action_to_dto(
    action: &LocalBridgePendingAction,
) -> LocalBridgePendingActionDto {
    match action {
        LocalBridgePendingAction::SendBundle(action) => LocalBridgePendingActionDto {
            request_id: action.request_id.clone(),
            action_kind: "bundle.send".to_string(),
            client_id: action.client.client_id.clone(),
            client_display_name: action.client.display_name.clone(),
            bundle_type: Some(bundle_type_label(action.bundle_type).to_string()),
            target_device_id: action.target_device_id.clone(),
            staged_bundle_id: None,
            expected_bundle_type: None,
            require_trusted_device: Some(action.require_trusted_device),
            requested_at_ms: action.requested_at_ms,
            bundle_root: None,
        },
        LocalBridgePendingAction::ImportBundle(action) => LocalBridgePendingActionDto {
            request_id: action.request_id.clone(),
            action_kind: "bundle.import".to_string(),
            client_id: action.client.client_id.clone(),
            client_display_name: action.client.display_name.clone(),
            bundle_type: None,
            target_device_id: None,
            staged_bundle_id: Some(action.staged_bundle_id.clone()),
            expected_bundle_type: action
                .expected_bundle_type
                .map(bundle_type_label)
                .map(str::to_string),
            require_trusted_device: None,
            requested_at_ms: action.requested_at_ms,
            bundle_root: None,
        },
    }
}

fn local_bridge_pending_action_request_id(action: &LocalBridgePendingAction) -> &str {
    match action {
        LocalBridgePendingAction::SendBundle(action) => &action.request_id,
        LocalBridgePendingAction::ImportBundle(action) => &action.request_id,
    }
}

fn local_bridge_pending_send_action_from_request(
    request: &nekolink_protocol::LocalBridgeSendBundleRequest,
    now_ms: u128,
) -> Result<LocalBridgePendingSendBundleAction, String> {
    let client = request
        .client
        .clone()
        .ok_or_else(|| "authorized local bridge send requires a client identity".to_string())?;
    Ok(LocalBridgePendingSendBundleAction {
        request_id: request.request_id.clone(),
        client,
        target_device_id: request.target_device_id.clone(),
        bundle_root: request.bundle_root.clone(),
        bundle_type: request.bundle_type,
        require_trusted_device: request.require_trusted_device,
        requested_at_ms: now_ms,
    })
}

fn local_bridge_pending_import_action_from_request(
    request: &nekolink_protocol::LocalBridgeImportBundleRequest,
    now_ms: u128,
) -> Result<LocalBridgePendingImportBundleAction, String> {
    let client = request
        .client
        .clone()
        .ok_or_else(|| "authorized local bridge import requires a client identity".to_string())?;
    Ok(LocalBridgePendingImportBundleAction {
        request_id: request.request_id.clone(),
        client,
        staged_bundle_id: request.staged_bundle_id.clone(),
        expected_bundle_type: request.expected_bundle_type,
        requested_at_ms: now_ms,
    })
}

fn handle_local_bridge_request_with_auth_at(
    request_json: &str,
    trusted_devices: &[TrustedDeviceRecord],
    transfer_status: Option<&TransferStatusState>,
    staging_root: &std::path::Path,
    authorizations: &[LocalBridgeAuthorizationRecord],
    now_ms: u128,
) -> Result<LocalBridgeResponseDto, String> {
    let request: LocalBridgeRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid bridge request JSON: {error}"))?;
    request.validate().map_err(|error| error.message)?;
    handle_validated_local_bridge_request_with_auth_at(
        request,
        trusted_devices,
        transfer_status,
        staging_root,
        authorizations,
        &[],
        now_ms,
    )
}

fn handle_validated_local_bridge_request_with_auth_at(
    request: LocalBridgeRequest,
    trusted_devices: &[TrustedDeviceRecord],
    transfer_status: Option<&TransferStatusState>,
    staging_root: &std::path::Path,
    authorizations: &[LocalBridgeAuthorizationRecord],
    events: &[LocalBridgeEvent],
    now_ms: u128,
) -> Result<LocalBridgeResponseDto, String> {
    match request {
        LocalBridgeRequest::ListDevices(request) => {
            let client = request.client.clone();
            Ok(local_bridge_read_only_response(
                request.request_id,
                client,
                "local bridge read-only snapshot",
                trusted_devices.iter().map(trusted_device_to_dto).collect(),
                list_staged_bundle_dtos_at(staging_root)?,
                None,
            ))
        }
        LocalBridgeRequest::TransferStatus(request) => {
            let client = request.client.clone();
            Ok(local_bridge_read_only_response(
                request.request_id,
                client,
                "local bridge transfer status snapshot",
                Vec::new(),
                Vec::new(),
                transfer_status.map(transfer_status_to_dto),
            ))
        }
        LocalBridgeRequest::BundleDetail(request) => {
            let client = request.client.clone();
            let bundle = find_staged_bundle_dto_at(staging_root, &request.staged_bundle_id)?;
            match bundle {
                Some(bundle) => Ok(local_bridge_read_only_response(
                    request.request_id,
                    client,
                    "local bridge staged bundle detail",
                    Vec::new(),
                    vec![bundle],
                    None,
                )),
                None => Ok(local_bridge_read_only_unsupported_response(
                    request.request_id,
                    client,
                    "staged bundle not found",
                )),
            }
        }
        LocalBridgeRequest::PollEvents(request) => {
            let can_read_bundles = local_bridge_client_has_scope(
                request.client.as_ref(),
                authorizations,
                LocalBridgePermissionScope::BundleRead,
                now_ms,
            );
            let can_read_transfers = local_bridge_client_has_scope(
                request.client.as_ref(),
                authorizations,
                LocalBridgePermissionScope::TransferStatusRead,
                now_ms,
            );
            if !can_read_bundles && !can_read_transfers {
                return Ok(local_bridge_pending_confirmation_response(
                    request.request_id,
                    request.client,
                ));
            }
            let bridge_events = local_bridge_events_after(
                events,
                request.after_event_id.as_deref(),
                request.limit.unwrap_or(50),
                can_read_bundles,
                can_read_transfers,
            )?;
            Ok(local_bridge_events_response(
                request.request_id,
                request.client,
                bridge_events,
            ))
        }
        LocalBridgeRequest::SendBundle(request) => {
            if local_bridge_client_has_scope(
                request.client.as_ref(),
                authorizations,
                LocalBridgePermissionScope::BundleSend,
                now_ms,
            ) {
                Ok(local_bridge_authorized_runtime_pending_response(
                    request.request_id,
                    request.client,
                    "local bridge bundle send is authorized, but the send runtime is not connected yet",
                ))
            } else {
                Ok(local_bridge_pending_confirmation_response(
                    request.request_id,
                    request.client,
                ))
            }
        }
        LocalBridgeRequest::ImportBundle(request) => {
            if local_bridge_client_has_scope(
                request.client.as_ref(),
                authorizations,
                LocalBridgePermissionScope::BundleImportRequest,
                now_ms,
            ) {
                Ok(local_bridge_authorized_runtime_pending_response(
                    request.request_id,
                    request.client,
                    "local bridge bundle import is authorized, but the import runtime is not connected yet",
                ))
            } else {
                Ok(local_bridge_pending_confirmation_response(
                    request.request_id,
                    request.client,
                ))
            }
        }
        LocalBridgeRequest::AuthorizationRequest(request) => {
            Ok(local_bridge_pending_authorization_response(
                request.request_id,
                request.client,
                request.requested_scopes,
                request.reason,
                request.ttl_seconds,
                now_ms,
            ))
        }
    }
}

fn push_local_bridge_runtime_event(
    runtime: &LocalBridgeRuntimeState,
    event: LocalBridgeEvent,
) -> Result<(), String> {
    event.validate().map_err(|error| error.message)?;
    let mut events = runtime.events.lock().map_err(|error| error.to_string())?;
    events.push(event);
    if events.len() > LOCAL_BRIDGE_EVENT_QUEUE_LIMIT {
        let excess = events.len() - LOCAL_BRIDGE_EVENT_QUEUE_LIMIT;
        events.drain(0..excess);
    }
    Ok(())
}

fn push_local_bridge_transfer_status_event(
    runtime: &LocalBridgeRuntimeState,
    transfer_id: &str,
    status: &TransferStatusState,
) -> Result<(), String> {
    let Some(phase) = local_bridge_transfer_phase_from_status(&status.phase) else {
        return Ok(());
    };
    push_local_bridge_runtime_event(
        runtime,
        LocalBridgeEvent::TransferUpdated(nekolink_protocol::LocalBridgeTransferUpdatedEvent {
            event_id: format!(
                "transfer:{transfer_id}:{}:{}",
                status.phase, status.updated_at_ms
            ),
            transfer_id: transfer_id.to_string(),
            phase,
            bytes_transferred: status.bytes_transferred.min(status.total_bytes),
            total_bytes: status.total_bytes,
        }),
    )
}

fn push_local_bridge_bundle_received_event(
    runtime: &LocalBridgeRuntimeState,
    transfer_id: &str,
    bundle: &ReceivedBundleReport,
) -> Result<(), String> {
    push_local_bridge_runtime_event(
        runtime,
        LocalBridgeEvent::BundleReceived(nekolink_protocol::LocalBridgeBundleReceivedEvent {
            event_id: format!("bundle:{transfer_id}:{}", bundle.bundle_id),
            transfer_id: transfer_id.to_string(),
            bundle_id: bundle.bundle_id.clone(),
            bundle_type: bundle.bundle_type,
            display_name: bundle.display_name.clone(),
            source_app: bundle.source_app.clone(),
            file_count: bundle.file_count,
            total_bytes: bundle.total_bytes,
            import_allowed: bundle.import_allowed,
        }),
    )
}

fn set_transfer_status_and_push_bridge_event(
    transfer_status: &Arc<Mutex<Option<TransferStatusState>>>,
    runtime: &LocalBridgeRuntimeState,
    transfer_id: &str,
    status: TransferStatusState,
) {
    set_transfer_status(transfer_status, status.clone());
    let _ = push_local_bridge_transfer_status_event(runtime, transfer_id, &status);
}

fn local_bridge_transfer_phase_from_status(
    phase: &str,
) -> Option<nekolink_protocol::LocalBridgeTransferPhase> {
    match phase {
        "queued" | "connecting" | "awaiting_approval" | "accepted" | "auto_accepted" => {
            Some(nekolink_protocol::LocalBridgeTransferPhase::Queued)
        }
        "sending" | "transferring" | "retrying" => {
            Some(nekolink_protocol::LocalBridgeTransferPhase::Sending)
        }
        "receiving" => Some(nekolink_protocol::LocalBridgeTransferPhase::Receiving),
        "verifying" => Some(nekolink_protocol::LocalBridgeTransferPhase::Receiving),
        "completed" => Some(nekolink_protocol::LocalBridgeTransferPhase::Completed),
        "failed" | "blocked" | "expired" | "declined" => {
            Some(nekolink_protocol::LocalBridgeTransferPhase::Failed)
        }
        "cancelled" | "closed" => Some(nekolink_protocol::LocalBridgeTransferPhase::Cancelled),
        _ => None,
    }
}

fn local_bridge_events_after(
    events: &[LocalBridgeEvent],
    after_event_id: Option<&str>,
    limit: usize,
    can_read_bundles: bool,
    can_read_transfers: bool,
) -> Result<Vec<serde_json::Value>, String> {
    let mut after_cursor = after_event_id.is_none();
    let mut output = Vec::new();
    for event in events {
        if !after_cursor {
            after_cursor = local_bridge_event_id(event) == after_event_id.unwrap_or_default();
            continue;
        }
        if !local_bridge_event_is_allowed(event, can_read_bundles, can_read_transfers) {
            continue;
        }
        output.push(serde_json::to_value(event).map_err(|error| error.to_string())?);
        if output.len() >= limit {
            break;
        }
    }
    Ok(output)
}

fn local_bridge_event_id(event: &LocalBridgeEvent) -> &str {
    match event {
        LocalBridgeEvent::BundleReceived(event) => &event.event_id,
        LocalBridgeEvent::TransferUpdated(event) => &event.event_id,
    }
}

fn local_bridge_event_is_allowed(
    event: &LocalBridgeEvent,
    can_read_bundles: bool,
    can_read_transfers: bool,
) -> bool {
    match event {
        LocalBridgeEvent::BundleReceived(_) => can_read_bundles,
        LocalBridgeEvent::TransferUpdated(_) => can_read_transfers,
    }
}

fn confirm_local_bridge_runtime_authorization_at(
    runtime: &LocalBridgeRuntimeState,
    authorization_code: &str,
    now_ms: u128,
) -> Result<LocalBridgeAuthorizationRecord, String> {
    let mut pending_guard = runtime
        .pending_authorization
        .lock()
        .map_err(|error| error.to_string())?;
    let pending = pending_guard
        .as_ref()
        .ok_or_else(|| "local bridge authorization request not found".to_string())?;
    let authorization =
        confirm_pending_local_bridge_authorization(pending, authorization_code, now_ms)?;
    runtime
        .authorizations
        .lock()
        .map_err(|error| error.to_string())?
        .push(authorization.clone());
    *pending_guard = None;
    Ok(authorization)
}

fn confirm_local_bridge_runtime_authorization_and_persist(
    runtime: &LocalBridgeRuntimeState,
    authorization_code: &str,
    now_ms: u128,
) -> Result<LocalBridgeAuthorizationRecord, String> {
    let authorization =
        confirm_local_bridge_runtime_authorization_at(runtime, authorization_code, now_ms)?;
    let authorizations = runtime
        .authorizations
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    save_local_bridge_authorizations(&authorizations, now_ms)?;
    Ok(authorization)
}

fn confirm_local_bridge_runtime_authorization_and_save_at(
    runtime: &LocalBridgeRuntimeState,
    authorization_code: &str,
    now_ms: u128,
    authorizations_path: &Path,
) -> Result<LocalBridgeAuthorizationRecord, String> {
    let authorization =
        confirm_local_bridge_runtime_authorization_at(runtime, authorization_code, now_ms)?;
    let authorizations = runtime
        .authorizations
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    save_local_bridge_authorizations_at(authorizations_path, &authorizations, now_ms)?;
    Ok(authorization)
}

fn list_local_bridge_authorizations_at(
    runtime: &LocalBridgeRuntimeState,
    now_ms: u128,
) -> Vec<LocalBridgeAuthorizationRecord> {
    let mut authorizations = runtime
        .authorizations
        .lock()
        .map(|authorizations| {
            authorizations
                .iter()
                .filter(|record| local_bridge_authorization_is_active(record, now_ms))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    sort_local_bridge_authorizations(&mut authorizations);
    authorizations
}

fn revoke_local_bridge_authorization_and_persist(
    runtime: &LocalBridgeRuntimeState,
    client_id: &str,
    scope: LocalBridgePermissionScope,
    now_ms: u128,
) -> Result<bool, String> {
    let path = local_bridge_authorizations_file_path()?;
    revoke_local_bridge_authorization_at(runtime, client_id, scope, now_ms, &path)
}

fn revoke_local_bridge_authorization_at(
    runtime: &LocalBridgeRuntimeState,
    client_id: &str,
    scope: LocalBridgePermissionScope,
    now_ms: u128,
    authorizations_path: &Path,
) -> Result<bool, String> {
    let mut authorizations = runtime
        .authorizations
        .lock()
        .map_err(|error| error.to_string())?;
    let mut revoked = false;
    for record in authorizations
        .iter_mut()
        .filter(|record| record.client_id == client_id)
    {
        let before_scope_count = record.scopes.len();
        record.scopes.retain(|candidate| *candidate != scope);
        revoked |= record.scopes.len() != before_scope_count;
    }
    authorizations.retain(|record| !record.scopes.is_empty());
    save_local_bridge_authorizations_at(authorizations_path, &authorizations, now_ms)?;
    Ok(revoked)
}

fn prune_local_bridge_authorizations_and_persist(
    runtime: &LocalBridgeRuntimeState,
    now_ms: u128,
) -> Result<usize, String> {
    let path = local_bridge_authorizations_file_path()?;
    prune_local_bridge_authorizations_at(runtime, now_ms, &path)
}

fn prune_local_bridge_authorizations_at(
    runtime: &LocalBridgeRuntimeState,
    now_ms: u128,
    authorizations_path: &Path,
) -> Result<usize, String> {
    let mut authorizations = runtime
        .authorizations
        .lock()
        .map_err(|error| error.to_string())?;
    let before_len = authorizations.len();
    authorizations.retain(|record| local_bridge_authorization_is_active(record, now_ms));
    let pruned_count = before_len.saturating_sub(authorizations.len());
    save_local_bridge_authorizations_at(authorizations_path, &authorizations, now_ms)?;
    Ok(pruned_count)
}

fn local_bridge_authorization_is_active(
    record: &LocalBridgeAuthorizationRecord,
    now_ms: u128,
) -> bool {
    record
        .expires_at_ms
        .is_none_or(|expires_at_ms| expires_at_ms >= now_ms)
}

fn sort_local_bridge_authorizations(records: &mut [LocalBridgeAuthorizationRecord]) {
    records.sort_by(|left, right| {
        right
            .granted_at_ms
            .cmp(&left.granted_at_ms)
            .then_with(|| left.client_id.cmp(&right.client_id))
            .then_with(|| {
                local_bridge_permission_scopes_label(&left.scopes)
                    .cmp(&local_bridge_permission_scopes_label(&right.scopes))
            })
    });
}

fn local_bridge_client_has_scope(
    client: Option<&LocalBridgeClientIdentity>,
    authorizations: &[LocalBridgeAuthorizationRecord],
    scope: LocalBridgePermissionScope,
    now_ms: u128,
) -> bool {
    let Some(client) = client else {
        return false;
    };
    authorizations.iter().any(|record| {
        record.client_id == client.client_id
            && record
                .expires_at_ms
                .is_none_or(|expires_at_ms| expires_at_ms >= now_ms)
            && record.scopes.contains(&scope)
    })
}

fn pending_local_bridge_authorization_from_request(
    request: &LocalBridgeAuthorizationRequest,
    now_ms: u128,
) -> Result<PendingLocalBridgeAuthorization, String> {
    request.validate().map_err(|error| error.message)?;
    let ttl_ms = u128::from(request.ttl_seconds.unwrap_or(900)).saturating_mul(1_000);
    let expires_at_ms = now_ms.saturating_add(ttl_ms);
    Ok(PendingLocalBridgeAuthorization {
        request_id: request.request_id.clone(),
        client: request.client.clone(),
        requested_scopes: request.requested_scopes.clone(),
        reason: request.reason.clone(),
        authorization_code: local_bridge_authorization_code(request, now_ms),
        requested_at_ms: now_ms,
        expires_at_ms,
    })
}

fn confirm_pending_local_bridge_authorization(
    pending: &PendingLocalBridgeAuthorization,
    authorization_code: &str,
    now_ms: u128,
) -> Result<LocalBridgeAuthorizationRecord, String> {
    if now_ms > pending.expires_at_ms {
        return Err("local bridge authorization request expired".to_string());
    }
    if authorization_code.trim() != pending.authorization_code {
        return Err("local bridge authorization code mismatch".to_string());
    }
    Ok(LocalBridgeAuthorizationRecord {
        client_id: pending.client.client_id.clone(),
        display_name: pending.client.display_name.clone(),
        app_kind: pending.client.app_kind.clone(),
        scopes: pending.requested_scopes.clone(),
        granted_at_ms: now_ms,
        expires_at_ms: Some(pending.expires_at_ms),
    })
}

fn local_bridge_authorization_code(
    request: &LocalBridgeAuthorizationRequest,
    requested_at_ms: u128,
) -> String {
    let mut material = String::new();
    material.push_str(&request.request_id);
    material.push('\n');
    material.push_str(&request.client.client_id);
    material.push('\n');
    material.push_str(&request.client.display_name);
    material.push('\n');
    if let Some(app_kind) = &request.client.app_kind {
        material.push_str(app_kind);
    }
    material.push('\n');
    for scope in &request.requested_scopes {
        material.push_str(local_bridge_permission_scope_label(*scope));
        material.push('\n');
    }
    material.push_str(&request.reason);
    material.push('\n');
    material.push_str(&requested_at_ms.to_string());
    let digest = sha256_hex(material.as_bytes()).to_ascii_uppercase();
    format!("{}-{}", &digest[..3], &digest[3..6])
}

fn local_bridge_read_only_response(
    request_id: String,
    client: Option<LocalBridgeClientIdentity>,
    message: &str,
    devices: Vec<TrustedDeviceDto>,
    staged_bundles: Vec<ReceivedBundleDto>,
    transfer_status: Option<TransferStatusDto>,
) -> LocalBridgeResponseDto {
    let client_metadata = local_bridge_client_metadata(client);
    LocalBridgeResponseDto {
        request_id,
        status: "ok".to_string(),
        message: message.to_string(),
        security_state: "read_only".to_string(),
        requires_user_confirmation: false,
        client_state: client_metadata.0,
        client_id: client_metadata.1,
        client_display_name: client_metadata.2,
        authorization_scopes: Vec::new(),
        authorization_reason: None,
        authorization_ttl_seconds: None,
        authorization_code: None,
        authorization_expires_at_ms: None,
        devices,
        staged_bundles,
        transfer_status,
        events: Vec::new(),
    }
}

fn local_bridge_read_only_unsupported_response(
    request_id: String,
    client: Option<LocalBridgeClientIdentity>,
    message: &str,
) -> LocalBridgeResponseDto {
    let client_metadata = local_bridge_client_metadata(client);
    LocalBridgeResponseDto {
        request_id,
        status: "unsupported".to_string(),
        message: message.to_string(),
        security_state: "read_only".to_string(),
        requires_user_confirmation: false,
        client_state: client_metadata.0,
        client_id: client_metadata.1,
        client_display_name: client_metadata.2,
        authorization_scopes: Vec::new(),
        authorization_reason: None,
        authorization_ttl_seconds: None,
        authorization_code: None,
        authorization_expires_at_ms: None,
        devices: Vec::new(),
        staged_bundles: Vec::new(),
        transfer_status: None,
        events: Vec::new(),
    }
}

fn local_bridge_pending_confirmation_response(
    request_id: String,
    client: Option<LocalBridgeClientIdentity>,
) -> LocalBridgeResponseDto {
    let client_metadata = local_bridge_client_metadata(client);
    LocalBridgeResponseDto {
        request_id,
        status: "pending_auth".to_string(),
        message: "local bridge auth runtime is not connected; user confirmation is required before this request can run".to_string(),
        security_state: "requires_user_confirmation".to_string(),
        requires_user_confirmation: true,
        client_state: client_metadata.0,
        client_id: client_metadata.1,
        client_display_name: client_metadata.2,
        authorization_scopes: Vec::new(),
        authorization_reason: None,
        authorization_ttl_seconds: None,
        authorization_code: None,
        authorization_expires_at_ms: None,
        devices: Vec::new(),
        staged_bundles: Vec::new(),
        transfer_status: None,
        events: Vec::new(),
    }
}

fn local_bridge_authorized_runtime_pending_response(
    request_id: String,
    client: Option<LocalBridgeClientIdentity>,
    message: &str,
) -> LocalBridgeResponseDto {
    let client_metadata = local_bridge_client_metadata(client);
    LocalBridgeResponseDto {
        request_id,
        status: "pending_runtime".to_string(),
        message: message.to_string(),
        security_state: "authorized".to_string(),
        requires_user_confirmation: false,
        client_state: client_metadata.0,
        client_id: client_metadata.1,
        client_display_name: client_metadata.2,
        authorization_scopes: Vec::new(),
        authorization_reason: None,
        authorization_ttl_seconds: None,
        authorization_code: None,
        authorization_expires_at_ms: None,
        devices: Vec::new(),
        staged_bundles: Vec::new(),
        transfer_status: None,
        events: Vec::new(),
    }
}

fn local_bridge_events_response(
    request_id: String,
    client: Option<LocalBridgeClientIdentity>,
    events: Vec<serde_json::Value>,
) -> LocalBridgeResponseDto {
    let client_metadata = local_bridge_client_metadata(client);
    LocalBridgeResponseDto {
        request_id,
        status: "ok".to_string(),
        message: "local bridge event snapshot".to_string(),
        security_state: "authorized".to_string(),
        requires_user_confirmation: false,
        client_state: client_metadata.0,
        client_id: client_metadata.1,
        client_display_name: client_metadata.2,
        authorization_scopes: Vec::new(),
        authorization_reason: None,
        authorization_ttl_seconds: None,
        authorization_code: None,
        authorization_expires_at_ms: None,
        devices: Vec::new(),
        staged_bundles: Vec::new(),
        transfer_status: None,
        events,
    }
}

fn local_bridge_pending_authorization_response(
    request_id: String,
    client: LocalBridgeClientIdentity,
    requested_scopes: Vec<LocalBridgePermissionScope>,
    reason: String,
    ttl_seconds: Option<u64>,
    now_ms: u128,
) -> LocalBridgeResponseDto {
    let request = LocalBridgeAuthorizationRequest {
        request_id: request_id.clone(),
        client: client.clone(),
        requested_scopes: requested_scopes.clone(),
        reason: reason.clone(),
        ttl_seconds,
    };
    let pending = pending_local_bridge_authorization_from_request(&request, now_ms).ok();
    let client_metadata = local_bridge_client_metadata(Some(client));
    LocalBridgeResponseDto {
        request_id,
        status: "pending_auth".to_string(),
        message: "local bridge authorization request is waiting for user confirmation".to_string(),
        security_state: "requires_user_confirmation".to_string(),
        requires_user_confirmation: true,
        client_state: client_metadata.0,
        client_id: client_metadata.1,
        client_display_name: client_metadata.2,
        authorization_scopes: requested_scopes
            .into_iter()
            .map(local_bridge_permission_scope_label)
            .map(str::to_string)
            .collect(),
        authorization_reason: Some(reason),
        authorization_ttl_seconds: ttl_seconds,
        authorization_code: pending
            .as_ref()
            .map(|authorization| authorization.authorization_code.clone()),
        authorization_expires_at_ms: pending.map(|authorization| authorization.expires_at_ms),
        devices: Vec::new(),
        staged_bundles: Vec::new(),
        transfer_status: None,
        events: Vec::new(),
    }
}

fn local_bridge_pending_authorization_response_from_pending(
    pending: &PendingLocalBridgeAuthorization,
) -> LocalBridgeResponseDto {
    let client_metadata = local_bridge_client_metadata(Some(pending.client.clone()));
    LocalBridgeResponseDto {
        request_id: pending.request_id.clone(),
        status: "pending_auth".to_string(),
        message: "local bridge authorization request is waiting for user confirmation".to_string(),
        security_state: "requires_user_confirmation".to_string(),
        requires_user_confirmation: true,
        client_state: client_metadata.0,
        client_id: client_metadata.1,
        client_display_name: client_metadata.2,
        authorization_scopes: pending
            .requested_scopes
            .iter()
            .copied()
            .map(local_bridge_permission_scope_label)
            .map(str::to_string)
            .collect(),
        authorization_reason: Some(pending.reason.clone()),
        authorization_ttl_seconds: Some(
            ((pending
                .expires_at_ms
                .saturating_sub(pending.requested_at_ms))
                / 1_000) as u64,
        ),
        authorization_code: Some(pending.authorization_code.clone()),
        authorization_expires_at_ms: Some(pending.expires_at_ms),
        devices: Vec::new(),
        staged_bundles: Vec::new(),
        transfer_status: None,
        events: Vec::new(),
    }
}

fn local_bridge_authorization_to_dto(
    authorization: LocalBridgeAuthorizationRecord,
) -> LocalBridgeAuthorizationDto {
    LocalBridgeAuthorizationDto {
        client_id: authorization.client_id,
        display_name: authorization.display_name,
        app_kind: authorization.app_kind,
        scopes: authorization
            .scopes
            .into_iter()
            .map(local_bridge_permission_scope_label)
            .map(str::to_string)
            .collect(),
        granted_at_ms: authorization.granted_at_ms,
        expires_at_ms: authorization.expires_at_ms,
    }
}

fn local_bridge_authorizations_to_dtos(
    authorizations: Vec<LocalBridgeAuthorizationRecord>,
) -> Vec<LocalBridgeAuthorizationDto> {
    authorizations
        .into_iter()
        .map(local_bridge_authorization_to_dto)
        .collect()
}

fn local_bridge_runtime_status_to_dto(
    status: local_bridge_runtime::LocalBridgeRuntimeStatusSnapshot,
) -> LocalBridgeRuntimeStatusDto {
    LocalBridgeRuntimeStatusDto {
        active: status.active,
        bind_host: status.bind_host,
        port: status.port,
        request_path: status.request_path,
        max_request_bytes: status.max_request_bytes,
        pending_authorization_client: status.pending_authorization_client,
        authorization_count: status.authorization_count,
        pending_action_count: status.pending_action_count,
        last_error: status.last_error,
    }
}

fn local_bridge_client_metadata(
    client: Option<LocalBridgeClientIdentity>,
) -> (String, Option<String>, Option<String>) {
    match client {
        Some(client) => (
            "identified".to_string(),
            Some(client.client_id),
            Some(client.display_name),
        ),
        None => ("anonymous".to_string(), None, None),
    }
}

fn local_bridge_permission_scope_label(scope: LocalBridgePermissionScope) -> &'static str {
    match scope {
        LocalBridgePermissionScope::DeviceRead => "device.read",
        LocalBridgePermissionScope::TransferStatusRead => "transfer.status.read",
        LocalBridgePermissionScope::BundleRead => "bundle.read",
        LocalBridgePermissionScope::BundleSend => "bundle.send",
        LocalBridgePermissionScope::BundleImportRequest => "bundle.import.request",
    }
}

fn local_bridge_permission_scopes_label(scopes: &[LocalBridgePermissionScope]) -> String {
    scopes
        .iter()
        .copied()
        .map(local_bridge_permission_scope_label)
        .collect::<Vec<_>>()
        .join(",")
}

fn parse_local_bridge_permission_scope(value: &str) -> Result<LocalBridgePermissionScope, String> {
    match value {
        "device.read" => Ok(LocalBridgePermissionScope::DeviceRead),
        "transfer.status.read" => Ok(LocalBridgePermissionScope::TransferStatusRead),
        "bundle.read" => Ok(LocalBridgePermissionScope::BundleRead),
        "bundle.send" => Ok(LocalBridgePermissionScope::BundleSend),
        "bundle.import.request" => Ok(LocalBridgePermissionScope::BundleImportRequest),
        _ => Err(format!("未知本机接入权限: {value}")),
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

fn parse_bundle_type(value: &str) -> Result<BundleType, String> {
    match value {
        "skill" => Ok(BundleType::Skill),
        "session" => Ok(BundleType::Session),
        "workspace" => Ok(BundleType::Workspace),
        "agent_profile" => Ok(BundleType::AgentProfile),
        "config_snapshot" => Ok(BundleType::ConfigSnapshot),
        _ => Err(format!("不支持的资料包类型：{value}")),
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

fn bundle_import_root() -> Result<PathBuf, String> {
    Ok(app_config_dir()?.join("bundle_imports"))
}

fn manual_bundle_output_root() -> Result<PathBuf, String> {
    Ok(app_config_dir()?.join("manual_bundles"))
}

fn current_utc_timestamp() -> String {
    use time::{format_description::well_known::Rfc3339, OffsetDateTime};

    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn manual_bundle_id(
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

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn manual_bundle_permissions(bundle_type: &BundleType) -> BundlePermissions {
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
        public_key: request.public_key.clone(),
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
        security_mode: record.security_mode.clone(),
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
    trust_context: ReceiveTrustContext,
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

    if trust_context == ReceiveTrustContext::Untrusted
        && legacy_plain_offer_matches_trusted_device(offer, trusted_devices)
    {
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
                message: "可信设备必须使用已认证加密传输，已拒绝兼容明文请求".to_string(),
                updated_at_ms: now_ms(),
            },
        );
        return false;
    }

    if should_auto_accept_receive_offer(offer, receive_policy, trusted_devices, trust_context) {
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

fn legacy_plain_offer_matches_trusted_device(
    offer: &TransferOffer,
    trusted_devices: &Arc<Mutex<Vec<TrustedDeviceRecord>>>,
) -> bool {
    let Some(sender_device_id) = offer.sender_device_id.as_deref() else {
        return false;
    };
    let Ok(trusted_devices) = trusted_devices.lock() else {
        return false;
    };
    trusted_devices
        .iter()
        .any(|record| record.device_id == sender_device_id)
}

fn should_auto_accept_receive_offer(
    offer: &TransferOffer,
    receive_policy: ReceivePolicy,
    trusted_devices: &Arc<Mutex<Vec<TrustedDeviceRecord>>>,
    trust_context: ReceiveTrustContext,
) -> bool {
    if receive_policy != ReceivePolicy::AutoAcceptTrusted
        || trust_context != ReceiveTrustContext::AuthenticatedTrusted
    {
        return false;
    }

    let Some(sender_device_id) = offer.sender_device_id.as_deref() else {
        return false;
    };
    let Some(sender_fingerprint) = offer.sender_public_key_fingerprint.as_deref() else {
        return false;
    };
    let Ok(trusted_devices) = trusted_devices.lock() else {
        return false;
    };
    trusted_devices.iter().any(|record| {
        record.device_id == sender_device_id && record.public_key_fingerprint == sender_fingerprint
    })
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
        public_key: request.public_key.clone(),
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
        request.public_key.clone(),
        request.public_key_fingerprint.clone(),
    );
    let record = match record {
        Ok(record) => record,
        Err(error) => return PairingDecisionPayload::reject(error),
    };
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
    if report.security_mode != TransferSecurityMode::AuthenticatedEncryptedSession {
        return;
    }

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
    let Some(sender_public_key) = next_trusted_devices
        .iter()
        .find(|record| {
            record.device_id == sender_device_id
                && record.public_key_fingerprint == sender_fingerprint
        })
        .map(|record| record.public_key.clone())
    else {
        return;
    };
    let changed = refresh_trusted_device_contact(
        &mut next_trusted_devices,
        sender_device_id,
        &sender_public_key,
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
    let Some(public_key) = next_trusted_devices
        .iter()
        .find(|record| {
            record.device_id == device_id && record.public_key_fingerprint == fingerprint
        })
        .map(|record| record.public_key.clone())
    else {
        return;
    };
    let changed = refresh_trusted_device_contact(
        &mut next_trusted_devices,
        device_id,
        &public_key,
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
    use nekolink_protocol::{
        BundleChecksums, BundleCompatibility, BundleFile, BundleManifest, BundlePermissionScope,
        BundlePermissions, BundleSecretsPolicy, BundleSender, BundleSummary, BundleType,
        BundleWriteMode, BundleWritePermission, Capability, BUNDLE_CHECKSUM_SHA256,
        BUNDLE_SCHEMA_V1, PROTOCOL_VERSION,
    };
    use std::collections::BTreeMap;

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
    fn nearby_device_send_pins_saved_trusted_public_key() {
        let public_key = test_public_key("device-a");
        let mut device = nearby_device("device-a", public_key.fingerprint.as_str());
        device.public_key = Some(public_key.public_key.clone());
        let trusted = vec![trusted_record_with_public_key(
            "device-a",
            "MacBook",
            public_key.public_key.as_str(),
            public_key.fingerprint.as_str(),
        )];

        let (_endpoint, peer) = trusted_peer_from_nearby_device(&device, &trusted).unwrap();

        assert_eq!(
            peer.trusted_public_key.as_deref(),
            Some(public_key.public_key.as_str())
        );
        assert_eq!(
            peer.trusted_public_key_fingerprint.as_deref(),
            Some(public_key.fingerprint.as_str())
        );
    }

    #[test]
    fn trusted_session_pin_accepts_matching_signed_public_key() {
        let key = test_identity_signing_key("device-a");
        let identity = test_identity_with_signing_key("device-a", &key);
        let binding = nekolink_protocol::SessionIdentityBinding::new(
            nekolink_protocol::SessionParticipantRole::Initiator,
            "session-trusted-pin",
            &identity,
            "x25519:session-key",
            "sha256:1111111111111111111111111111111111111111111111111111111111111111",
        )
        .unwrap();
        let signed = SignedSessionIdentityBinding::sign(binding, &key).unwrap();
        let trusted = vec![trusted_record_with_public_key(
            "device-a",
            "MacBook",
            signed.public_key.as_str(),
            signed.public_key_fingerprint.as_str(),
        )];

        let result = verify_incoming_peer_against_trusted_devices(&trusted, &identity, &signed);

        assert!(result.is_ok());
    }

    #[test]
    fn trusted_session_pin_rejects_public_key_rotation_for_same_device() {
        let key = test_identity_signing_key("device-a");
        let rotated_key = test_identity_signing_key("device-a-rotated");
        let identity = test_identity_with_signing_key("device-a", &key);
        let binding = nekolink_protocol::SessionIdentityBinding::new(
            nekolink_protocol::SessionParticipantRole::Initiator,
            "session-trusted-pin-rotated",
            &identity,
            "x25519:session-key",
            "sha256:2222222222222222222222222222222222222222222222222222222222222222",
        )
        .unwrap();
        let signed = SignedSessionIdentityBinding::sign(binding, &key).unwrap();
        let rotated_public_key = rotated_key.public_key();
        let trusted = vec![trusted_record_with_public_key(
            "device-a",
            "MacBook",
            rotated_public_key.public_key.as_str(),
            rotated_public_key.fingerprint.as_str(),
        )];

        let error =
            verify_incoming_peer_against_trusted_devices(&trusted, &identity, &signed).unwrap_err();

        assert!(error.contains("可信设备身份校验失败"));
    }

    #[test]
    fn trusted_session_pin_rejects_binding_identity_mismatch() {
        let key = test_identity_signing_key("device-a");
        let identity = test_identity_with_signing_key("device-a", &key);
        let mismatched_identity = test_identity_with_signing_key("device-b", &key);
        let binding = nekolink_protocol::SessionIdentityBinding::new(
            nekolink_protocol::SessionParticipantRole::Initiator,
            "session-trusted-pin-mismatch",
            &mismatched_identity,
            "x25519:session-key",
            "sha256:6666666666666666666666666666666666666666666666666666666666666666",
        )
        .unwrap();
        let signed = SignedSessionIdentityBinding::sign(binding, &key).unwrap();
        let trusted = vec![trusted_record_with_public_key(
            "device-a",
            "MacBook",
            signed.public_key.as_str(),
            signed.public_key_fingerprint.as_str(),
        )];

        let error =
            verify_incoming_peer_against_trusted_devices(&trusted, &identity, &signed).unwrap_err();

        assert!(error.contains("binding"));
    }

    #[test]
    fn untrusted_authenticated_session_is_not_pinned_to_trusted_devices() {
        let key = test_identity_signing_key("device-b");
        let identity = test_identity_with_signing_key("device-b", &key);
        let binding = nekolink_protocol::SessionIdentityBinding::new(
            nekolink_protocol::SessionParticipantRole::Initiator,
            "session-untrusted",
            &identity,
            "x25519:session-key",
            "sha256:3333333333333333333333333333333333333333333333333333333333333333",
        )
        .unwrap();
        let signed = SignedSessionIdentityBinding::sign(binding, &key).unwrap();
        let trusted = vec![trusted_record("device-a", "MacBook", "sha256:device-a")];

        let result = verify_incoming_peer_against_trusted_devices(&trusted, &identity, &signed);

        assert!(result.is_ok());
    }

    #[test]
    fn connection_ticket_peer_rejects_session_fingerprint_mismatch() {
        let key = test_identity_signing_key("device-a");
        let identity = test_identity_with_signing_key("device-a", &key);
        let binding = nekolink_protocol::SessionIdentityBinding::new(
            nekolink_protocol::SessionParticipantRole::Responder,
            "session-ticket-peer",
            &identity,
            "x25519:session-key",
            "sha256:4444444444444444444444444444444444444444444444444444444444444444",
        )
        .unwrap();
        let signed = SignedSessionIdentityBinding::sign(binding, &key).unwrap();
        let peer = TransferPeer {
            device_id: Some("device-a".to_string()),
            name: Some("MacBook".to_string()),
            fingerprint: Some("sha256:different".to_string()),
            trusted_public_key: None,
            trusted_public_key_fingerprint: None,
            target_host: Some("192.168.1.20:45821".to_string()),
        };

        let error = verify_peer_matches_transfer_peer(&peer, &identity, &signed).unwrap_err();

        assert!(error.contains("指纹不匹配"));
    }

    #[test]
    fn manual_endpoint_peer_without_identity_allows_authenticated_session() {
        let key = test_identity_signing_key("device-a");
        let identity = test_identity_with_signing_key("device-a", &key);
        let binding = nekolink_protocol::SessionIdentityBinding::new(
            nekolink_protocol::SessionParticipantRole::Responder,
            "session-manual-peer",
            &identity,
            "x25519:session-key",
            "sha256:5555555555555555555555555555555555555555555555555555555555555555",
        )
        .unwrap();
        let signed = SignedSessionIdentityBinding::sign(binding, &key).unwrap();
        let peer = TransferPeer {
            device_id: None,
            name: None,
            fingerprint: None,
            trusted_public_key: None,
            trusted_public_key_fingerprint: None,
            target_host: Some("192.168.1.20:45821".to_string()),
        };

        let result = verify_peer_matches_transfer_peer(&peer, &identity, &signed);

        assert!(result.is_ok());
    }

    #[test]
    fn self_peer_is_rejected_by_device_id() {
        let identity = test_identity("device-a");
        let peer = TransferPeer {
            device_id: Some("device-a".to_string()),
            name: Some("This Mac".to_string()),
            fingerprint: Some("sha256:self".to_string()),
            trusted_public_key: None,
            trusted_public_key_fingerprint: None,
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
            trusted_public_key: None,
            trusted_public_key_fingerprint: None,
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
    fn windows_dialog_script_uses_bundle_source_prompt() {
        let script = windows_dialog_script(PathDialogKind::BundleSourceFolder);

        assert!(script.contains("选择资料包来源目录"));
        assert!(!script.contains("选择接收目录"));
    }

    #[test]
    fn current_utc_timestamp_uses_utc_iso_8601_shape() {
        let timestamp = current_utc_timestamp();

        assert!(timestamp.contains('T'));
        assert!(timestamp.ends_with('Z'));
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
            security_mode: TransferSecurityMode::LegacyPlain,
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

        assert_eq!(dto.security_mode, "legacy_plain");
        assert_eq!(dto.file_count, 100);
        assert_eq!(dto.files.len(), RECEIVE_FILE_PREVIEW_LIMIT);
        assert_eq!(dto.files[0].manifest_path, "drop/file-000.txt");
    }

    #[test]
    fn receive_report_dto_includes_bundle_preview() {
        let report = TransferReceiveReport {
            transfer_id: "transfer-a".to_string(),
            root_name: "bundle".to_string(),
            security_mode: TransferSecurityMode::AuthenticatedEncryptedSession,
            sender_device_id: None,
            sender_device_name: None,
            sender_public_key_fingerprint: None,
            bundle: Some(ReceivedBundleReport {
                bundle_id: "bundle_1234567890".to_string(),
                bundle_type: nekolink_protocol::BundleType::Skill,
                display_name: "voice_transcribe".to_string(),
                source_app: "Generic Agent App".to_string(),
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
        assert_eq!(bundle.source_app, "Generic Agent App");
        assert_eq!(bundle.file_count, 2);
        assert_eq!(bundle.total_bytes, 28);
        assert_eq!(bundle.staging_path, "/tmp/bundle_1234567890");
        assert!(bundle.import_allowed);
    }

    #[test]
    fn transfer_history_dto_exposes_optional_security_mode() {
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
        record.security_mode = Some("authenticated_encrypted_session".to_string());

        let dto = transfer_to_dto(&record);

        assert_eq!(
            dto.security_mode.as_deref(),
            Some("authenticated_encrypted_session")
        );
    }

    #[test]
    fn legacy_plain_receive_report_does_not_refresh_trusted_device_contact() {
        let public_key = test_public_key("device-a");
        let trusted = Arc::new(Mutex::new(vec![TrustedDeviceRecord {
            schema_version: 1,
            device_id: "device-a".to_string(),
            device_name: "Known Mac".to_string(),
            platform: "macos".to_string(),
            host: "192.168.1.20".to_string(),
            port: 45821,
            public_key: public_key.public_key,
            public_key_fingerprint: public_key.fingerprint.clone(),
            pairing_code: "AAA-BBB".to_string(),
            paired_at_ms: 1,
            last_seen_at_ms: 1,
        }]));
        let report = TransferReceiveReport {
            transfer_id: "transfer-a".to_string(),
            root_name: "drop".to_string(),
            security_mode: TransferSecurityMode::LegacyPlain,
            sender_device_id: Some("device-a".to_string()),
            sender_device_name: Some("Spoofed Name".to_string()),
            sender_public_key_fingerprint: Some(public_key.fingerprint),
            bundle: None,
            files: Vec::new(),
        };

        refresh_trusted_device_contact_from_receive_report(&trusted, &report);

        let trusted = trusted.lock().unwrap();
        assert_eq!(trusted[0].device_name, "Known Mac");
        assert_eq!(trusted[0].last_seen_at_ms, 1);
    }

    #[test]
    fn staged_bundle_dto_marks_saved_status() {
        let dir = unique_bundle_temp_dir("desktop-bundle-list");
        let staging_root = dir.join("bundle_staging");
        let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
        nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();

        let bundles = list_staged_bundle_dtos_at(&staging_root).unwrap();

        assert_eq!(bundles.len(), 1);
        assert_eq!(bundles[0].bundle_id, "bundle_1234567890");
        assert_eq!(bundles[0].staging_status, "saved");
        assert!(bundles[0].can_import_now);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn import_staged_bundle_at_marks_bundle_imported() {
        let dir = unique_bundle_temp_dir("desktop-bundle-import");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
        let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
        nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();

        let imported =
            import_staged_bundle_at(&staging_root, &import_root, "bundle_1234567890").unwrap();

        assert_eq!(imported.bundle_id, "bundle_1234567890");
        assert_eq!(imported.staging_status, "imported");
        assert!(!imported.can_import_now);
        assert_eq!(
            imported.import_path.as_deref(),
            Some(
                import_root
                    .join("bundle_1234567890")
                    .to_string_lossy()
                    .as_ref()
            )
        );
        assert!(import_root
            .join("bundle_1234567890")
            .join("content.bin")
            .is_file());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn import_staged_bundle_at_rejects_unsafe_bundle_id() {
        let dir = unique_bundle_temp_dir("desktop-bundle-import-unsafe");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");

        let error = import_staged_bundle_at(&staging_root, &import_root, "../bundle").unwrap_err();

        assert!(error.contains("bundle_id"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn delete_staged_bundle_at_removes_saved_bundle() {
        let dir = unique_bundle_temp_dir("desktop-bundle-delete");
        let staging_root = dir.join("bundle_staging");
        let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
        nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();

        let removed = delete_staged_bundle_at(&staging_root, "bundle_1234567890").unwrap();

        assert!(removed);
        assert!(!staging_root.join("bundle_1234567890").exists());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn prune_staged_bundle_dtos_at_removes_expired_bundles() {
        let dir = unique_bundle_temp_dir("desktop-bundle-prune");
        let staging_root = dir.join("bundle_staging");
        let expired_root = create_desktop_test_bundle(&dir, "expired", "bundle_expired");
        let fresh_root = create_desktop_test_bundle(&dir, "fresh", "bundle_fresh");
        nekodrop_storage::stage_bundle_directory(&expired_root, &staging_root).unwrap();
        let cutoff = std::time::SystemTime::now();
        nekodrop_storage::stage_bundle_directory(&fresh_root, &staging_root).unwrap();

        let pruned = prune_staged_bundle_dtos_at(&staging_root, cutoff).unwrap();

        assert_eq!(pruned, vec!["bundle_expired"]);
        let remaining = list_staged_bundle_dtos_at(&staging_root).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].bundle_id, "bundle_fresh");

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_devices_list_returns_trusted_devices_and_staged_bundles() {
        let dir = unique_bundle_temp_dir("local-bridge-devices");
        let staging_root = dir.join("bundle_staging");
        let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
        nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();
        let trusted = vec![trusted_record("device-a", "MacBook", "sha256:device-a")];
        let request = serde_json::json!({
            "kind": "devices.list",
            "payload": {
                "request_id": "bridge-request-1",
                "trusted_only": true
            }
        })
        .to_string();

        let response =
            handle_local_bridge_request_at(&request, &trusted, None, &staging_root).unwrap();

        assert_eq!(response.request_id, "bridge-request-1");
        assert_eq!(response.status, "ok");
        assert_eq!(response.devices.len(), 1);
        assert_eq!(response.devices[0].device_id, "device-a");
        assert_eq!(response.staged_bundles.len(), 1);
        assert_eq!(response.staged_bundles[0].bundle_id, "bundle_1234567890");
        assert!(response.transfer_status.is_none());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_read_only_requests_are_marked_read_only() {
        let dir = unique_bundle_temp_dir("local-bridge-read-only-security");
        let staging_root = dir.join("bundle_staging");
        let request = serde_json::json!({
            "kind": "transfer.status",
            "payload": {
                "request_id": "bridge-request-status",
                "transfer_id": null
            }
        })
        .to_string();

        let response = handle_local_bridge_request_at(&request, &[], None, &staging_root).unwrap();

        assert_eq!(response.status, "ok");
        assert_eq!(response.security_state, "read_only");
        assert!(!response.requires_user_confirmation);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_response_marks_anonymous_client() {
        let dir = unique_bundle_temp_dir("local-bridge-client-anonymous");
        let staging_root = dir.join("bundle_staging");
        let request = serde_json::json!({
            "kind": "transfer.status",
            "payload": {
                "request_id": "bridge-request-status",
                "transfer_id": null
            }
        })
        .to_string();

        let response = handle_local_bridge_request_at(&request, &[], None, &staging_root).unwrap();

        assert_eq!(response.client_state, "anonymous");
        assert!(response.client_id.is_none());
        assert!(response.client_display_name.is_none());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_response_echoes_identified_client() {
        let dir = unique_bundle_temp_dir("local-bridge-client-identified");
        let staging_root = dir.join("bundle_staging");
        let request = serde_json::json!({
            "kind": "transfer.status",
            "payload": {
                "request_id": "bridge-request-status",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "transfer_id": null
            }
        })
        .to_string();

        let response = handle_local_bridge_request_at(&request, &[], None, &staging_root).unwrap();

        assert_eq!(response.client_state, "identified");
        assert_eq!(response.client_id.as_deref(), Some("local-agent-app"));
        assert_eq!(
            response.client_display_name.as_deref(),
            Some("Local Agent App")
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_bundle_detail_returns_matching_staged_bundle() {
        let dir = unique_bundle_temp_dir("local-bridge-bundle-detail");
        let staging_root = dir.join("bundle_staging");
        let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
        nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();
        let request = serde_json::json!({
            "kind": "bundle.detail",
            "payload": {
                "request_id": "bridge-request-detail",
                "staged_bundle_id": "bundle_1234567890"
            }
        })
        .to_string();

        let response = handle_local_bridge_request_at(&request, &[], None, &staging_root).unwrap();

        assert_eq!(response.request_id, "bridge-request-detail");
        assert_eq!(response.status, "ok");
        assert_eq!(response.security_state, "read_only");
        assert_eq!(response.staged_bundles.len(), 1);
        assert_eq!(response.staged_bundles[0].bundle_id, "bundle_1234567890");
        assert!(!response.requires_user_confirmation);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_bundle_detail_returns_unsupported_for_missing_bundle() {
        let dir = unique_bundle_temp_dir("local-bridge-bundle-detail-missing");
        let staging_root = dir.join("bundle_staging");
        let request = serde_json::json!({
            "kind": "bundle.detail",
            "payload": {
                "request_id": "bridge-request-detail",
                "staged_bundle_id": "bundle_1234567890"
            }
        })
        .to_string();

        let response = handle_local_bridge_request_at(&request, &[], None, &staging_root).unwrap();

        assert_eq!(response.request_id, "bridge-request-detail");
        assert_eq!(response.status, "unsupported");
        assert_eq!(response.security_state, "read_only");
        assert!(response.staged_bundles.is_empty());
        assert!(response.message.contains("not found"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_mutating_requests_require_user_confirmation() {
        let dir = unique_bundle_temp_dir("local-bridge-mutating-security");
        let staging_root = dir.join("bundle_staging");
        let request = serde_json::json!({
            "kind": "bundle.import",
            "payload": {
                "request_id": "bridge-request-import",
                "staged_bundle_id": "bundle_1234567890",
                "expected_bundle_type": "skill"
            }
        })
        .to_string();

        let response = handle_local_bridge_request_at(&request, &[], None, &staging_root).unwrap();

        assert_eq!(response.status, "pending_auth");
        assert_eq!(response.security_state, "requires_user_confirmation");
        assert!(response.requires_user_confirmation);
        assert!(response.message.contains("user confirmation"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_bundle_mutations_are_pending_auth() {
        let dir = unique_bundle_temp_dir("local-bridge-pending");
        let staging_root = dir.join("bundle_staging");
        let send_request = serde_json::json!({
            "kind": "bundle.send",
            "payload": {
                "request_id": "bridge-request-send",
                "target_device_id": "device-a",
                "bundle_root": "bundle",
                "bundle_type": "skill",
                "require_trusted_device": true
            }
        })
        .to_string();
        let import_request = serde_json::json!({
            "kind": "bundle.import",
            "payload": {
                "request_id": "bridge-request-import",
                "staged_bundle_id": "bundle_1234567890",
                "expected_bundle_type": "skill"
            }
        })
        .to_string();

        let send_response =
            handle_local_bridge_request_at(&send_request, &[], None, &staging_root).unwrap();
        let import_response =
            handle_local_bridge_request_at(&import_request, &[], None, &staging_root).unwrap();

        assert_eq!(send_response.request_id, "bridge-request-send");
        assert_eq!(send_response.status, "pending_auth");
        assert!(send_response.message.contains("auth"));
        assert_eq!(import_response.request_id, "bridge-request-import");
        assert_eq!(import_response.status, "pending_auth");
        assert!(import_response.message.contains("runtime"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_authorized_client_can_pass_import_gate() {
        let dir = unique_bundle_temp_dir("local-bridge-authorized-import");
        let staging_root = dir.join("bundle_staging");
        let authorizations = vec![local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleImportRequest],
            1_000,
            2_000,
        )];
        let import_request = serde_json::json!({
            "kind": "bundle.import",
            "payload": {
                "request_id": "bridge-request-import",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "staged_bundle_id": "bundle_1234567890",
                "expected_bundle_type": "skill"
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_auth_at(
            &import_request,
            &[],
            None,
            &staging_root,
            &authorizations,
            1_500,
        )
        .unwrap();

        assert_eq!(response.status, "pending_runtime");
        assert_eq!(response.security_state, "authorized");
        assert!(!response.requires_user_confirmation);
        assert!(response.message.contains("authorized"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_expired_authorization_requires_confirmation_again() {
        let dir = unique_bundle_temp_dir("local-bridge-expired-auth");
        let staging_root = dir.join("bundle_staging");
        let authorizations = vec![local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleImportRequest],
            1_000,
            1_100,
        )];
        let import_request = serde_json::json!({
            "kind": "bundle.import",
            "payload": {
                "request_id": "bridge-request-import",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "staged_bundle_id": "bundle_1234567890",
                "expected_bundle_type": "skill"
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_auth_at(
            &import_request,
            &[],
            None,
            &staging_root,
            &authorizations,
            1_500,
        )
        .unwrap();

        assert_eq!(response.status, "pending_auth");
        assert_eq!(response.security_state, "requires_user_confirmation");

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_authorization_request_waits_for_user_confirmation() {
        let dir = unique_bundle_temp_dir("local-bridge-authorization");
        let staging_root = dir.join("bundle_staging");
        let request = serde_json::json!({
            "kind": "authorization.request",
            "payload": {
                "request_id": "bridge-auth-1",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "requested_scopes": [
                    "device.read",
                    "bundle.send"
                ],
                "reason": "Send a skill bundle to a trusted desktop device",
                "ttl_seconds": 900
            }
        })
        .to_string();

        let response = handle_local_bridge_request_at(&request, &[], None, &staging_root).unwrap();

        assert_eq!(response.request_id, "bridge-auth-1");
        assert_eq!(response.status, "pending_auth");
        assert_eq!(response.security_state, "requires_user_confirmation");
        assert!(response.requires_user_confirmation);
        assert_eq!(response.client_state, "identified");
        assert_eq!(response.client_id.as_deref(), Some("local-agent-app"));
        assert_eq!(
            response.authorization_scopes,
            vec!["device.read".to_string(), "bundle.send".to_string()]
        );
        assert_eq!(
            response.authorization_reason.as_deref(),
            Some("Send a skill bundle to a trusted desktop device")
        );
        assert_eq!(response.authorization_ttl_seconds, Some(900));
        assert!(response.authorization_code.as_deref().is_some_and(|code| {
            code.len() == 7
                && code.as_bytes()[3] == b'-'
                && code
                    .chars()
                    .filter(|character| *character != '-')
                    .all(|character| {
                        character.is_ascii_hexdigit() && !character.is_ascii_lowercase()
                    })
        }));
        assert!(response.authorization_expires_at_ms.is_some());
        assert!(response.message.contains("authorization"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_authorization_request_creates_short_code() {
        let request = LocalBridgeAuthorizationRequest {
            request_id: "bridge-auth-1".to_string(),
            client: LocalBridgeClientIdentity {
                client_id: "local-agent-app".to_string(),
                display_name: "Local Agent App".to_string(),
                app_kind: Some("agent".to_string()),
            },
            requested_scopes: vec![
                LocalBridgePermissionScope::DeviceRead,
                LocalBridgePermissionScope::BundleSend,
            ],
            reason: "Send a skill bundle".to_string(),
            ttl_seconds: Some(900),
        };

        let pending = pending_local_bridge_authorization_from_request(&request, 1_000).unwrap();

        assert_eq!(pending.request_id, "bridge-auth-1");
        assert_eq!(pending.client.client_id, "local-agent-app");
        assert_eq!(pending.requested_scopes, request.requested_scopes);
        assert_eq!(pending.expires_at_ms, 901_000);
        assert_eq!(pending.authorization_code.len(), 7);
        assert_eq!(pending.authorization_code.as_bytes()[3], b'-');
        assert!(pending
            .authorization_code
            .chars()
            .filter(|character| *character != '-')
            .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_lowercase()));
    }

    #[test]
    fn local_bridge_runtime_stores_pending_authorization_request() {
        let dir = unique_bundle_temp_dir("local-bridge-runtime-pending-auth");
        let staging_root = dir.join("bundle_staging");
        let runtime = LocalBridgeRuntimeState::default();
        let request = serde_json::json!({
            "kind": "authorization.request",
            "payload": {
                "request_id": "bridge-auth-runtime",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "requested_scopes": [
                    "bundle.send"
                ],
                "reason": "Send a local bundle",
                "ttl_seconds": 900
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_runtime_at(
            &request,
            &[],
            None,
            &staging_root,
            &runtime,
            1_000,
        )
        .unwrap();

        assert_eq!(response.status, "pending_auth");
        let pending = runtime
            .pending_authorization
            .lock()
            .unwrap()
            .clone()
            .unwrap();
        assert_eq!(pending.request_id, "bridge-auth-runtime");
        assert_eq!(pending.client.client_id, "local-agent-app");
        assert_eq!(
            response.authorization_code.as_deref(),
            Some(pending.authorization_code.as_str())
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn confirmed_runtime_authorization_allows_future_mutating_request() {
        let dir = unique_bundle_temp_dir("local-bridge-runtime-confirmed-auth");
        let staging_root = dir.join("bundle_staging");
        let runtime = LocalBridgeRuntimeState::default();
        let auth_request = serde_json::json!({
            "kind": "authorization.request",
            "payload": {
                "request_id": "bridge-auth-runtime",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "requested_scopes": [
                    "bundle.import.request"
                ],
                "reason": "Import a staged bundle",
                "ttl_seconds": 900
            }
        })
        .to_string();
        let import_request = serde_json::json!({
            "kind": "bundle.import",
            "payload": {
                "request_id": "bridge-request-import",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "staged_bundle_id": "bundle_1234567890",
                "expected_bundle_type": "skill"
            }
        })
        .to_string();

        let auth_response = handle_local_bridge_request_with_runtime_at(
            &auth_request,
            &[],
            None,
            &staging_root,
            &runtime,
            1_000,
        )
        .unwrap();
        let authorization = confirm_local_bridge_runtime_authorization_at(
            &runtime,
            auth_response.authorization_code.as_deref().unwrap(),
            1_500,
        )
        .unwrap();
        let response = handle_local_bridge_request_with_runtime_at(
            &import_request,
            &[],
            None,
            &staging_root,
            &runtime,
            1_600,
        )
        .unwrap();

        assert_eq!(authorization.client_id, "local-agent-app");
        assert_eq!(response.status, "pending_runtime");
        assert_eq!(response.security_state, "authorized");
        assert!(!response.requires_user_confirmation);
        assert!(runtime.pending_authorization.lock().unwrap().is_none());
        assert_eq!(runtime.authorizations.lock().unwrap().len(), 1);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn authorized_local_bridge_bundle_send_is_queued_as_pending_action() {
        let dir = unique_bundle_temp_dir("local-bridge-runtime-pending-send-action");
        let staging_root = dir.join("bundle_staging");
        let runtime = LocalBridgeRuntimeState::default();
        runtime
            .authorizations
            .lock()
            .unwrap()
            .push(local_bridge_authorization(
                "local-agent-app",
                &[LocalBridgePermissionScope::BundleSend],
                1_000,
                5_000,
            ));
        let send_request = serde_json::json!({
            "kind": "bundle.send",
            "payload": {
                "request_id": "bridge-request-send",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "target_device_id": "device-a",
                "bundle_root": "bundle",
                "bundle_type": "skill",
                "require_trusted_device": true
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_runtime_at(
            &send_request,
            &[],
            None,
            &staging_root,
            &runtime,
            1_500,
        )
        .unwrap();

        assert_eq!(response.status, "pending_runtime");
        let actions = runtime.pending_actions.lock().unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            crate::app_state::LocalBridgePendingAction::SendBundle(action) => {
                assert_eq!(action.request_id, "bridge-request-send");
                assert_eq!(action.client.client_id, "local-agent-app");
                assert_eq!(action.target_device_id.as_deref(), Some("device-a"));
                assert_eq!(action.bundle_root, "bundle");
                assert_eq!(action.bundle_type, BundleType::Skill);
                assert!(action.require_trusted_device);
                assert_eq!(action.requested_at_ms, 1_500);
            }
            other => panic!("expected send bundle action, got {other:?}"),
        }

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn authorized_local_bridge_bundle_import_is_queued_as_pending_action() {
        let dir = unique_bundle_temp_dir("local-bridge-runtime-pending-import-action");
        let staging_root = dir.join("bundle_staging");
        let runtime = LocalBridgeRuntimeState::default();
        runtime
            .authorizations
            .lock()
            .unwrap()
            .push(local_bridge_authorization(
                "local-agent-app",
                &[LocalBridgePermissionScope::BundleImportRequest],
                1_000,
                5_000,
            ));
        let import_request = serde_json::json!({
            "kind": "bundle.import",
            "payload": {
                "request_id": "bridge-request-import",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "staged_bundle_id": "bundle_1234567890",
                "expected_bundle_type": "skill"
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_runtime_at(
            &import_request,
            &[],
            None,
            &staging_root,
            &runtime,
            1_500,
        )
        .unwrap();

        assert_eq!(response.status, "pending_runtime");
        let actions = runtime.pending_actions.lock().unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            crate::app_state::LocalBridgePendingAction::ImportBundle(action) => {
                assert_eq!(action.request_id, "bridge-request-import");
                assert_eq!(action.client.client_id, "local-agent-app");
                assert_eq!(action.staged_bundle_id, "bundle_1234567890");
                assert_eq!(action.expected_bundle_type, Some(BundleType::Skill));
                assert_eq!(action.requested_at_ms, 1_500);
            }
            other => panic!("expected import bundle action, got {other:?}"),
        }

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unauthorized_local_bridge_bundle_mutation_is_not_queued() {
        let dir = unique_bundle_temp_dir("local-bridge-runtime-no-pending-action");
        let staging_root = dir.join("bundle_staging");
        let runtime = LocalBridgeRuntimeState::default();
        let import_request = serde_json::json!({
            "kind": "bundle.import",
            "payload": {
                "request_id": "bridge-request-import",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "staged_bundle_id": "bundle_1234567890",
                "expected_bundle_type": "skill"
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_runtime_at(
            &import_request,
            &[],
            None,
            &staging_root,
            &runtime,
            1_500,
        )
        .unwrap();

        assert_eq!(response.status, "pending_auth");
        assert!(runtime.pending_actions.lock().unwrap().is_empty());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_pending_actions_are_listed_as_safe_summaries() {
        let runtime = LocalBridgeRuntimeState::default();
        runtime.pending_actions.lock().unwrap().extend([
            LocalBridgePendingAction::SendBundle(LocalBridgePendingSendBundleAction {
                request_id: "bridge-send-1".to_string(),
                client: LocalBridgeClientIdentity {
                    client_id: "local-agent-app".to_string(),
                    display_name: "Local Agent App".to_string(),
                    app_kind: Some("agent".to_string()),
                },
                target_device_id: Some("device-a".to_string()),
                bundle_root: "/tmp/exported/bundle".to_string(),
                bundle_type: BundleType::Workspace,
                require_trusted_device: true,
                requested_at_ms: 1_500,
            }),
            LocalBridgePendingAction::ImportBundle(LocalBridgePendingImportBundleAction {
                request_id: "bridge-import-1".to_string(),
                client: LocalBridgeClientIdentity {
                    client_id: "local-agent-app".to_string(),
                    display_name: "Local Agent App".to_string(),
                    app_kind: Some("agent".to_string()),
                },
                staged_bundle_id: "bundle_1234567890".to_string(),
                expected_bundle_type: Some(BundleType::Skill),
                requested_at_ms: 1_600,
            }),
        ]);

        let actions = list_local_bridge_pending_actions_at(&runtime).unwrap();

        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].request_id, "bridge-send-1");
        assert_eq!(actions[0].action_kind, "bundle.send");
        assert_eq!(actions[0].client_display_name, "Local Agent App");
        assert_eq!(actions[0].bundle_type.as_deref(), Some("workspace"));
        assert_eq!(actions[0].target_device_id.as_deref(), Some("device-a"));
        assert!(actions[0].bundle_root.is_none());
        assert_eq!(actions[1].request_id, "bridge-import-1");
        assert_eq!(actions[1].action_kind, "bundle.import");
        assert_eq!(
            actions[1].staged_bundle_id.as_deref(),
            Some("bundle_1234567890")
        );
        assert_eq!(actions[1].expected_bundle_type.as_deref(), Some("skill"));
    }

    #[test]
    fn local_bridge_pending_action_can_be_removed_by_request_id() {
        let runtime = LocalBridgeRuntimeState::default();
        runtime.pending_actions.lock().unwrap().extend([
            LocalBridgePendingAction::SendBundle(LocalBridgePendingSendBundleAction {
                request_id: "bridge-send-1".to_string(),
                client: LocalBridgeClientIdentity {
                    client_id: "local-agent-app".to_string(),
                    display_name: "Local Agent App".to_string(),
                    app_kind: Some("agent".to_string()),
                },
                target_device_id: Some("device-a".to_string()),
                bundle_root: "bundle-a".to_string(),
                bundle_type: BundleType::Workspace,
                require_trusted_device: true,
                requested_at_ms: 1_500,
            }),
            LocalBridgePendingAction::ImportBundle(LocalBridgePendingImportBundleAction {
                request_id: "bridge-import-1".to_string(),
                client: LocalBridgeClientIdentity {
                    client_id: "local-agent-app".to_string(),
                    display_name: "Local Agent App".to_string(),
                    app_kind: Some("agent".to_string()),
                },
                staged_bundle_id: "bundle_1234567890".to_string(),
                expected_bundle_type: Some(BundleType::Skill),
                requested_at_ms: 1_600,
            }),
        ]);

        let removed = remove_local_bridge_pending_action_at(&runtime, "bridge-send-1").unwrap();
        let missing = remove_local_bridge_pending_action_at(&runtime, "bridge-missing").unwrap();
        let actions = list_local_bridge_pending_actions_at(&runtime).unwrap();

        assert!(removed);
        assert!(!missing);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].request_id, "bridge-import-1");
    }

    #[test]
    fn authorized_local_bridge_client_can_poll_runtime_events() {
        let dir = unique_bundle_temp_dir("local-bridge-events-poll");
        let staging_root = dir.join("bundle_staging");
        let runtime = LocalBridgeRuntimeState::default();
        runtime
            .authorizations
            .lock()
            .unwrap()
            .push(local_bridge_authorization(
                "local-agent-app",
                &[LocalBridgePermissionScope::BundleRead],
                1_000,
                5_000,
            ));
        push_local_bridge_runtime_event(
            &runtime,
            nekolink_protocol::LocalBridgeEvent::BundleReceived(
                nekolink_protocol::LocalBridgeBundleReceivedEvent {
                    event_id: "bridge-event-1".to_string(),
                    transfer_id: "transfer-1".to_string(),
                    bundle_id: "bundle_1234567890".to_string(),
                    bundle_type: BundleType::Skill,
                    display_name: "voice_transcribe".to_string(),
                    source_app: "Generic Agent App".to_string(),
                    file_count: 2,
                    total_bytes: 28,
                    import_allowed: true,
                },
            ),
        )
        .unwrap();
        let poll_request = serde_json::json!({
            "kind": "events.poll",
            "payload": {
                "request_id": "bridge-events-1",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "after_event_id": null,
                "limit": 10
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_runtime_at(
            &poll_request,
            &[],
            None,
            &staging_root,
            &runtime,
            1_500,
        )
        .unwrap();

        assert_eq!(response.status, "ok");
        assert_eq!(response.security_state, "authorized");
        assert_eq!(response.events.len(), 1);
        assert_eq!(
            response.events[0]["payload"]["event_id"].as_str(),
            Some("bridge-event-1")
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_event_poll_returns_only_events_after_cursor() {
        let dir = unique_bundle_temp_dir("local-bridge-events-after");
        let staging_root = dir.join("bundle_staging");
        let runtime = LocalBridgeRuntimeState::default();
        runtime
            .authorizations
            .lock()
            .unwrap()
            .push(local_bridge_authorization(
                "local-agent-app",
                &[LocalBridgePermissionScope::TransferStatusRead],
                1_000,
                5_000,
            ));
        push_local_bridge_runtime_event(
            &runtime,
            nekolink_protocol::LocalBridgeEvent::TransferUpdated(
                nekolink_protocol::LocalBridgeTransferUpdatedEvent {
                    event_id: "bridge-event-1".to_string(),
                    transfer_id: "transfer-1".to_string(),
                    phase: nekolink_protocol::LocalBridgeTransferPhase::Sending,
                    bytes_transferred: 10,
                    total_bytes: 100,
                },
            ),
        )
        .unwrap();
        push_local_bridge_runtime_event(
            &runtime,
            nekolink_protocol::LocalBridgeEvent::TransferUpdated(
                nekolink_protocol::LocalBridgeTransferUpdatedEvent {
                    event_id: "bridge-event-2".to_string(),
                    transfer_id: "transfer-1".to_string(),
                    phase: nekolink_protocol::LocalBridgeTransferPhase::Completed,
                    bytes_transferred: 100,
                    total_bytes: 100,
                },
            ),
        )
        .unwrap();
        let poll_request = serde_json::json!({
            "kind": "events.poll",
            "payload": {
                "request_id": "bridge-events-2",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "after_event_id": "bridge-event-1",
                "limit": 10
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_runtime_at(
            &poll_request,
            &[],
            None,
            &staging_root,
            &runtime,
            1_500,
        )
        .unwrap();

        assert_eq!(response.events.len(), 1);
        assert_eq!(
            response.events[0]["payload"]["event_id"].as_str(),
            Some("bridge-event-2")
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_event_poll_requires_authorized_client_scope() {
        let dir = unique_bundle_temp_dir("local-bridge-events-auth");
        let staging_root = dir.join("bundle_staging");
        let runtime = LocalBridgeRuntimeState::default();
        push_local_bridge_runtime_event(
            &runtime,
            nekolink_protocol::LocalBridgeEvent::BundleReceived(
                nekolink_protocol::LocalBridgeBundleReceivedEvent {
                    event_id: "bridge-event-1".to_string(),
                    transfer_id: "transfer-1".to_string(),
                    bundle_id: "bundle_1234567890".to_string(),
                    bundle_type: BundleType::Skill,
                    display_name: "voice_transcribe".to_string(),
                    source_app: "Generic Agent App".to_string(),
                    file_count: 2,
                    total_bytes: 28,
                    import_allowed: true,
                },
            ),
        )
        .unwrap();
        let poll_request = serde_json::json!({
            "kind": "events.poll",
            "payload": {
                "request_id": "bridge-events-auth",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "after_event_id": null,
                "limit": 10
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_runtime_at(
            &poll_request,
            &[],
            None,
            &staging_root,
            &runtime,
            1_500,
        )
        .unwrap();

        assert_eq!(response.status, "pending_auth");
        assert_eq!(response.security_state, "requires_user_confirmation");
        assert!(response.events.is_empty());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_transfer_status_producer_pushes_transfer_updated_event() {
        let runtime = LocalBridgeRuntimeState::default();
        let status = TransferStatusState {
            direction: "send".to_string(),
            phase: "completed".to_string(),
            root_name: Some("drop".to_string()),
            file_count: 2,
            file_index: 2,
            current_file: None,
            bytes_transferred: 200,
            total_bytes: 200,
            message: "发送完成".to_string(),
            updated_at_ms: 42,
        };

        push_local_bridge_transfer_status_event(&runtime, "transfer-1", &status).unwrap();

        let events = runtime.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            nekolink_protocol::LocalBridgeEvent::TransferUpdated(event) => {
                assert_eq!(event.event_id, "transfer:transfer-1:completed:42");
                assert_eq!(event.transfer_id, "transfer-1");
                assert_eq!(
                    event.phase,
                    nekolink_protocol::LocalBridgeTransferPhase::Completed
                );
                assert_eq!(event.bytes_transferred, 200);
                assert_eq!(event.total_bytes, 200);
            }
            other => panic!("expected transfer.updated, got {other:?}"),
        }
    }

    #[test]
    fn local_bridge_bundle_report_producer_pushes_bundle_received_event() {
        let runtime = LocalBridgeRuntimeState::default();
        let bundle = ReceivedBundleReport {
            bundle_id: "bundle_1234567890".to_string(),
            bundle_type: BundleType::Skill,
            display_name: "voice_transcribe".to_string(),
            source_app: "Generic Agent App".to_string(),
            file_count: 2,
            total_bytes: 28,
            staging_path: std::path::PathBuf::from("/tmp/staged/bundle_1234567890"),
            import_allowed: true,
        };

        push_local_bridge_bundle_received_event(&runtime, "transfer-1", &bundle).unwrap();

        let events = runtime.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            nekolink_protocol::LocalBridgeEvent::BundleReceived(event) => {
                assert_eq!(event.event_id, "bundle:transfer-1:bundle_1234567890");
                assert_eq!(event.transfer_id, "transfer-1");
                assert_eq!(event.bundle_id, "bundle_1234567890");
                assert_eq!(event.bundle_type, BundleType::Skill);
                assert!(event.import_allowed);
            }
            other => panic!("expected bundle.received, got {other:?}"),
        }
    }

    #[test]
    fn local_bridge_event_poll_reads_events_from_producers() {
        let dir = unique_bundle_temp_dir("local-bridge-produced-events-poll");
        let staging_root = dir.join("bundle_staging");
        let runtime = LocalBridgeRuntimeState::default();
        runtime
            .authorizations
            .lock()
            .unwrap()
            .push(local_bridge_authorization(
                "local-agent-app",
                &[
                    LocalBridgePermissionScope::BundleRead,
                    LocalBridgePermissionScope::TransferStatusRead,
                ],
                1_000,
                5_000,
            ));
        let status = TransferStatusState {
            direction: "receive".to_string(),
            phase: "completed".to_string(),
            root_name: Some("drop".to_string()),
            file_count: 2,
            file_index: 2,
            current_file: None,
            bytes_transferred: 28,
            total_bytes: 28,
            message: "接收完成".to_string(),
            updated_at_ms: 42,
        };
        let bundle = ReceivedBundleReport {
            bundle_id: "bundle_1234567890".to_string(),
            bundle_type: BundleType::Skill,
            display_name: "voice_transcribe".to_string(),
            source_app: "Generic Agent App".to_string(),
            file_count: 2,
            total_bytes: 28,
            staging_path: std::path::PathBuf::from("/tmp/staged/bundle_1234567890"),
            import_allowed: true,
        };
        push_local_bridge_transfer_status_event(&runtime, "transfer-1", &status).unwrap();
        push_local_bridge_bundle_received_event(&runtime, "transfer-1", &bundle).unwrap();
        let poll_request = serde_json::json!({
            "kind": "events.poll",
            "payload": {
                "request_id": "bridge-events-produced",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "after_event_id": null,
                "limit": 10
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_runtime_at(
            &poll_request,
            &[],
            None,
            &staging_root,
            &runtime,
            1_500,
        )
        .unwrap();

        assert_eq!(response.status, "ok");
        assert_eq!(response.events.len(), 2);
        assert_eq!(response.events[0]["kind"], "transfer.updated");
        assert_eq!(response.events[1]["kind"], "bundle.received");

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn confirmed_runtime_authorization_is_saved_for_restart() {
        let dir = unique_bundle_temp_dir("local-bridge-runtime-persisted-auth");
        let staging_root = dir.join("bundle_staging");
        let authorizations_path = dir.join("local_bridge_authorizations.json");
        let runtime = LocalBridgeRuntimeState::default();
        let auth_request = serde_json::json!({
            "kind": "authorization.request",
            "payload": {
                "request_id": "bridge-auth-persist",
                "client": {
                    "client_id": "generic-local-app",
                    "display_name": "Generic Local App",
                    "app_kind": "generic"
                },
                "requested_scopes": [
                    "bundle.send"
                ],
                "reason": "Send a local bundle",
                "ttl_seconds": 900
            }
        })
        .to_string();

        let auth_response = handle_local_bridge_request_with_runtime_at(
            &auth_request,
            &[],
            None,
            &staging_root,
            &runtime,
            1_000,
        )
        .unwrap();
        let authorization = confirm_local_bridge_runtime_authorization_and_save_at(
            &runtime,
            auth_response.authorization_code.as_deref().unwrap(),
            1_500,
            &authorizations_path,
        )
        .unwrap();
        let saved = crate::local_bridge_authorizations::load_local_bridge_authorizations_at(
            &authorizations_path,
            1_600,
        )
        .unwrap();

        assert_eq!(authorization.client_id, "generic-local-app");
        assert_eq!(saved, vec![authorization]);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_authorization_management_lists_revokes_and_prunes() {
        let dir = unique_bundle_temp_dir("local-bridge-authorization-management");
        let authorizations_path = dir.join("local_bridge_authorizations.json");
        let runtime = LocalBridgeRuntimeState::default();
        runtime.authorizations.lock().unwrap().extend([
            local_bridge_authorization(
                "multi-scope-app",
                &[
                    LocalBridgePermissionScope::BundleSend,
                    LocalBridgePermissionScope::BundleImportRequest,
                ],
                3_000,
                40_000,
            ),
            local_bridge_authorization(
                "local-app",
                &[LocalBridgePermissionScope::BundleSend],
                1_000,
                20_000,
            ),
            local_bridge_authorization(
                "local-app",
                &[LocalBridgePermissionScope::BundleImportRequest],
                2_000,
                30_000,
            ),
            local_bridge_authorization(
                "expired-app",
                &[LocalBridgePermissionScope::BundleSend],
                1_000,
                2_000,
            ),
        ]);

        let listed = list_local_bridge_authorizations_at(&runtime, 3_000);
        let revoked = revoke_local_bridge_authorization_at(
            &runtime,
            "local-app",
            LocalBridgePermissionScope::BundleSend,
            3_500,
            &authorizations_path,
        )
        .unwrap();
        let pruned =
            prune_local_bridge_authorizations_at(&runtime, 5_000, &authorizations_path).unwrap();
        let multi_scope_revoked = revoke_local_bridge_authorization_at(
            &runtime,
            "multi-scope-app",
            LocalBridgePermissionScope::BundleSend,
            5_100,
            &authorizations_path,
        )
        .unwrap();
        let saved = crate::local_bridge_authorizations::load_local_bridge_authorizations_at(
            &authorizations_path,
            5_500,
        )
        .unwrap();

        assert_eq!(listed.len(), 3);
        assert!(revoked);
        assert!(multi_scope_revoked);
        assert_eq!(pruned, 1);
        assert_eq!(saved.len(), 2);
        assert_eq!(
            saved
                .iter()
                .find(|record| record.client_id == "multi-scope-app")
                .unwrap()
                .scopes,
            vec![LocalBridgePermissionScope::BundleImportRequest]
        );
        assert!(saved.iter().any(|record| {
            record.client_id == "local-app"
                && record.scopes == vec![LocalBridgePermissionScope::BundleImportRequest]
        }));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn confirmed_local_bridge_authorization_grants_requested_scopes() {
        let pending = PendingLocalBridgeAuthorization {
            request_id: "bridge-auth-1".to_string(),
            client: LocalBridgeClientIdentity {
                client_id: "local-agent-app".to_string(),
                display_name: "Local Agent App".to_string(),
                app_kind: Some("agent".to_string()),
            },
            requested_scopes: vec![LocalBridgePermissionScope::BundleImportRequest],
            reason: "Import staged bundle".to_string(),
            authorization_code: "ABC-123".to_string(),
            requested_at_ms: 1_000,
            expires_at_ms: 11_000,
        };

        let authorization =
            confirm_pending_local_bridge_authorization(&pending, "ABC-123", 2_000).unwrap();

        assert_eq!(authorization.client_id, "local-agent-app");
        assert_eq!(
            authorization.scopes,
            vec![LocalBridgePermissionScope::BundleImportRequest]
        );
        assert_eq!(authorization.granted_at_ms, 2_000);
        assert_eq!(authorization.expires_at_ms, Some(11_000));
        assert!(local_bridge_client_has_scope(
            Some(&pending.client),
            &[authorization],
            LocalBridgePermissionScope::BundleImportRequest,
            3_000,
        ));
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
            ReceiveTrustContext::Untrusted,
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
        let public_key = test_public_key("device-a");
        let trusted = Arc::new(Mutex::new(vec![TrustedDeviceRecord {
            schema_version: 1,
            device_id: "device-a".to_string(),
            device_name: "MacBook".to_string(),
            platform: "macos".to_string(),
            host: "192.168.1.20".to_string(),
            port: 45821,
            public_key: public_key.public_key.clone(),
            public_key_fingerprint: public_key.fingerprint.clone(),
            pairing_code: "AAA-BBB".to_string(),
            paired_at_ms: 1,
            last_seen_at_ms: 1,
        }]));
        let mut offer = TransferOffer::new("transfer-a", "example.txt", Vec::new());
        offer.sender_device_id = Some("device-a".to_string());
        offer.sender_public_key_fingerprint = Some(public_key.fingerprint);

        let accepted = should_auto_accept_receive_offer(
            &offer,
            ReceivePolicy::AutoAcceptTrusted,
            &trusted,
            ReceiveTrustContext::Untrusted,
        );

        assert!(!accepted);
    }

    #[test]
    fn receive_policy_auto_accept_trusted_accepts_authenticated_trusted_session() {
        let public_key = test_public_key("device-a");
        let trusted = Arc::new(Mutex::new(vec![trusted_record_with_public_key(
            "device-a",
            "MacBook",
            public_key.public_key.as_str(),
            public_key.fingerprint.as_str(),
        )]));
        let mut offer = TransferOffer::new("transfer-a", "example.txt", Vec::new());
        offer.sender_device_id = Some("device-a".to_string());
        offer.sender_public_key_fingerprint = Some(public_key.fingerprint);

        let accepted = should_auto_accept_receive_offer(
            &offer,
            ReceivePolicy::AutoAcceptTrusted,
            &trusted,
            ReceiveTrustContext::AuthenticatedTrusted,
        );

        assert!(accepted);
    }

    #[test]
    fn authenticated_trusted_receive_policy_auto_accepts_without_pending_prompt() {
        let public_key = test_public_key("device-a");
        let pending = Arc::new(Mutex::new(None));
        let status = Arc::new(Mutex::new(None));
        let trusted = Arc::new(Mutex::new(vec![trusted_record_with_public_key(
            "device-a",
            "MacBook",
            public_key.public_key.as_str(),
            public_key.fingerprint.as_str(),
        )]));
        let mut offer = TransferOffer::new("transfer-a", "example.txt", Vec::new());
        offer.sender_device_id = Some("device-a".to_string());
        offer.sender_public_key_fingerprint = Some(public_key.fingerprint);

        let accepted = wait_for_receive_decision(
            &offer,
            &pending,
            &status,
            ReceivePolicy::AutoAcceptTrusted,
            &trusted,
            ReceiveTrustContext::AuthenticatedTrusted,
            None,
        );

        assert!(accepted);
        assert!(pending.lock().unwrap().is_none());
        let status = status.lock().unwrap().clone().unwrap();
        assert_eq!(status.phase, "auto_accepted");
    }

    #[test]
    fn legacy_plain_offer_from_trusted_device_identity_is_rejected_before_prompt() {
        let public_key = test_public_key("device-a");
        let pending = Arc::new(Mutex::new(None));
        let status = Arc::new(Mutex::new(None));
        let trusted = Arc::new(Mutex::new(vec![trusted_record_with_public_key(
            "device-a",
            "MacBook",
            public_key.public_key.as_str(),
            public_key.fingerprint.as_str(),
        )]));
        let mut offer = TransferOffer::new("transfer-a", "example.txt", Vec::new());
        offer.sender_device_id = Some("device-a".to_string());
        offer.sender_public_key_fingerprint = Some(public_key.fingerprint);

        let accepted = wait_for_receive_decision(
            &offer,
            &pending,
            &status,
            ReceivePolicy::AlwaysAsk,
            &trusted,
            ReceiveTrustContext::Untrusted,
            None,
        );

        assert!(!accepted);
        assert!(pending.lock().unwrap().is_none());
        let status = status.lock().unwrap().clone().unwrap();
        assert_eq!(status.phase, "blocked");
        assert!(status.message.contains("明文"));
    }

    #[test]
    fn legacy_plain_offer_rejects_known_device_id_even_with_mismatched_fingerprint() {
        let public_key = test_public_key("device-a");
        let pending = Arc::new(Mutex::new(None));
        let status = Arc::new(Mutex::new(None));
        let trusted = Arc::new(Mutex::new(vec![trusted_record_with_public_key(
            "device-a",
            "MacBook",
            public_key.public_key.as_str(),
            public_key.fingerprint.as_str(),
        )]));
        let mut offer = TransferOffer::new("transfer-a", "example.txt", Vec::new());
        offer.sender_device_id = Some("device-a".to_string());
        offer.sender_public_key_fingerprint = Some("sha256:rotated".to_string());

        let accepted = wait_for_receive_decision(
            &offer,
            &pending,
            &status,
            ReceivePolicy::AlwaysAsk,
            &trusted,
            ReceiveTrustContext::Untrusted,
            None,
        );

        assert!(!accepted);
        assert!(pending.lock().unwrap().is_none());
        let status = status.lock().unwrap().clone().unwrap();
        assert_eq!(status.phase, "blocked");
        assert!(status.message.contains("已认证加密"));
    }

    #[test]
    fn legacy_plain_offer_from_unknown_identity_remains_manual_compatibility() {
        let trusted_public_key = test_public_key("device-a");
        let unknown_public_key = test_public_key("device-b");
        let trusted = Arc::new(Mutex::new(vec![trusted_record_with_public_key(
            "device-a",
            "MacBook",
            trusted_public_key.public_key.as_str(),
            trusted_public_key.fingerprint.as_str(),
        )]));
        let mut offer = TransferOffer::new("transfer-b", "example.txt", Vec::new());
        offer.sender_device_id = Some("device-b".to_string());
        offer.sender_public_key_fingerprint = Some(unknown_public_key.fingerprint);

        assert!(!legacy_plain_offer_matches_trusted_device(&offer, &trusted));
    }

    #[test]
    fn legacy_plain_offer_matches_trusted_device_by_known_device_id() {
        let trusted_public_key = test_public_key("device-a");
        let trusted = Arc::new(Mutex::new(vec![trusted_record_with_public_key(
            "device-a",
            "MacBook",
            trusted_public_key.public_key.as_str(),
            trusted_public_key.fingerprint.as_str(),
        )]));
        let mut offer = TransferOffer::new("transfer-a", "example.txt", Vec::new());
        offer.sender_device_id = Some("device-a".to_string());
        offer.sender_public_key_fingerprint = Some("sha256:rotated".to_string());

        assert!(legacy_plain_offer_matches_trusted_device(&offer, &trusted));
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
        let public_key = test_public_key(device_id);
        let mut device = Device::new(
            nekodrop_core::DeviceId::new(device_id).unwrap(),
            "MacBook",
            nekodrop_core::DevicePlatform::MacOS,
            "192.168.1.20",
            45821,
        )
        .unwrap();
        device.public_key = Some(public_key.public_key);
        device.public_key_fingerprint = Some(fingerprint.to_string());
        device
    }

    fn trusted_record(
        device_id: &str,
        device_name: &str,
        public_key_fingerprint: &str,
    ) -> TrustedDeviceRecord {
        let public_key = test_public_key(device_id);
        trusted_record_with_public_key(
            device_id,
            device_name,
            public_key.public_key.as_str(),
            public_key_fingerprint,
        )
    }

    fn trusted_record_with_public_key(
        device_id: &str,
        device_name: &str,
        public_key: &str,
        public_key_fingerprint: &str,
    ) -> TrustedDeviceRecord {
        TrustedDeviceRecord {
            schema_version: 1,
            device_id: device_id.to_string(),
            device_name: device_name.to_string(),
            platform: "macos".to_string(),
            host: "192.168.1.20".to_string(),
            port: 45821,
            public_key: public_key.to_string(),
            public_key_fingerprint: public_key_fingerprint.to_string(),
            pairing_code: "AAA-BBB".to_string(),
            paired_at_ms: 1,
            last_seen_at_ms: 1,
        }
    }

    fn local_bridge_authorization(
        client_id: &str,
        scopes: &[LocalBridgePermissionScope],
        granted_at_ms: u128,
        expires_at_ms: u128,
    ) -> LocalBridgeAuthorizationRecord {
        LocalBridgeAuthorizationRecord {
            client_id: client_id.to_string(),
            display_name: "Local Agent App".to_string(),
            app_kind: Some("agent".to_string()),
            scopes: scopes.to_vec(),
            granted_at_ms,
            expires_at_ms: Some(expires_at_ms),
        }
    }

    fn test_public_key(label: &str) -> nekolink_protocol::DeviceIdentityPublicKey {
        let mut seed = [0_u8; nekolink_protocol::DEVICE_IDENTITY_SIGNING_KEY_LEN];
        for (index, byte) in label.as_bytes().iter().enumerate() {
            seed[index % seed.len()] ^= *byte;
        }
        nekolink_protocol::DeviceIdentitySigningKey::from_seed(seed).public_key()
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

    fn test_identity_signing_key(device_id: &str) -> nekolink_protocol::DeviceIdentitySigningKey {
        let mut seed = [0_u8; nekolink_protocol::DEVICE_IDENTITY_SIGNING_KEY_LEN];
        for (index, byte) in device_id.as_bytes().iter().enumerate() {
            seed[index % seed.len()] ^= byte.rotate_left((index % 8) as u32);
        }
        nekolink_protocol::DeviceIdentitySigningKey::from_seed(seed)
    }

    fn test_identity_with_signing_key(
        device_id: &str,
        signing_key: &nekolink_protocol::DeviceIdentitySigningKey,
    ) -> DeviceIdentity {
        let public_key = signing_key.public_key();
        DeviceIdentity::new(
            device_id,
            "This Mac",
            nekolink_protocol::DeviceKind::Desktop,
            nekolink_protocol::PlatformKind::Macos,
            public_key.fingerprint,
            [nekolink_protocol::Capability::EncryptedSession],
        )
    }

    fn create_desktop_test_bundle(
        dir: &std::path::Path,
        directory_name: &str,
        bundle_id: &str,
    ) -> PathBuf {
        let root = dir.join(directory_name);
        fs::create_dir_all(root.join("files")).unwrap();
        fs::write(
            root.join("files").join("manifest.json"),
            b"{\"kind\":\"skill\"}",
        )
        .unwrap();
        fs::write(root.join("files").join("content.bin"), b"hello bundle").unwrap();
        let mut manifest = desktop_test_bundle_manifest();
        manifest.bundle_id = bundle_id.to_string();
        write_json(root.join("bundle.json"), &manifest);
        write_json(
            root.join("checksums.json"),
            &desktop_test_bundle_checksums(),
        );
        write_json(
            root.join("permissions.json"),
            &desktop_test_bundle_permissions(),
        );
        root
    }

    fn desktop_test_bundle_manifest() -> BundleManifest {
        BundleManifest {
            schema: BUNDLE_SCHEMA_V1.to_string(),
            bundle_id: "bundle_1234567890".to_string(),
            bundle_type: BundleType::Skill,
            display_name: "voice_transcribe".to_string(),
            source_app: "Generic Agent App".to_string(),
            created_at: "2026-06-14T10:30:00Z".to_string(),
            sender: BundleSender {
                device_id: "neko-device-1234567890".to_string(),
                device_name: "MacBook".to_string(),
                fingerprint: "sha256:0123456789abcdef".to_string(),
            },
            compatibility: BundleCompatibility {
                min_nekolink_version: PROTOCOL_VERSION,
                required_capabilities: vec![Capability::BundleTransfer],
            },
            summary: BundleSummary {
                file_count: 2,
                total_bytes: 28,
            },
            files: vec![
                BundleFile {
                    path: "files/manifest.json".to_string(),
                    size: 16,
                    sha256: "0bc3f835203da0c2bbb44658e66c6bc0449e7f00bd9bd8fecd5d12283baaf5c9"
                        .to_string(),
                    role: "manifest".to_string(),
                },
                BundleFile {
                    path: "files/content.bin".to_string(),
                    size: 12,
                    sha256: "04cfecf64270c52b81da10bf6890b24fa73ee79715c44d1bc443dd9dd1de04d0"
                        .to_string(),
                    role: "payload".to_string(),
                },
            ],
        }
    }

    fn desktop_test_bundle_checksums() -> BundleChecksums {
        let mut files = BTreeMap::new();
        files.insert(
            "files/manifest.json".to_string(),
            "0bc3f835203da0c2bbb44658e66c6bc0449e7f00bd9bd8fecd5d12283baaf5c9".to_string(),
        );
        files.insert(
            "files/content.bin".to_string(),
            "04cfecf64270c52b81da10bf6890b24fa73ee79715c44d1bc443dd9dd1de04d0".to_string(),
        );
        BundleChecksums {
            algorithm: BUNDLE_CHECKSUM_SHA256.to_string(),
            files,
        }
    }

    fn desktop_test_bundle_permissions() -> BundlePermissions {
        BundlePermissions {
            requested_scopes: vec![BundlePermissionScope::SkillInstall],
            writes: vec![BundleWritePermission {
                target: "agent.skills".to_string(),
                mode: BundleWriteMode::CreateOnly,
            }],
            secrets: BundleSecretsPolicy {
                contains_secrets: false,
                redacted_fields: Vec::new(),
            },
        }
    }

    fn write_json(path: impl AsRef<std::path::Path>, value: &impl serde::Serialize) {
        fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    }

    fn unique_bundle_temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "nekodrop-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
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
    BundleSourceFolder,
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
        PathDialogKind::BundleSourceFolder => {
            r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.FolderBrowserDialog
$dialog.Description = '选择资料包来源目录'
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
        PathDialogKind::BundleSourceFolder => {
            r#"
set pickedItem to choose folder with prompt "选择资料包来源目录"
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
        PathDialogKind::BundleSourceFolder => {
            args.push("--directory".to_string());
            args.push("--title=选择资料包来源目录".to_string());
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
