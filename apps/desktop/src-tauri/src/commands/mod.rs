use std::fs;
use std::io::ErrorKind;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use nekodrop_core::{Device, DeviceTrustState, NekoDropError, ReceivePolicy};
use nekodrop_network::{
    ConnectionTicket, Endpoint, PairingDecisionPayload, PairingRequestPayload, TransferOffer,
    TransferProgress,
};
use nekodrop_service::{
    accept_incoming_stream_with_authenticated_control_bundle_staging_peer_verifier_and_cancel,
    create_transfer_plan as create_service_transfer_plan, create_transfer_plan_with_scan_progress,
    send_pairing_request, send_plan_with_authenticated_session_peer_verifier_and_cancel,
    IncomingSessionReport, ReceivedBundleReport, TransferPlanScanProgress, TransferProgressEvent,
    TransferReceiveReport, TransferSecurityMode,
};
use nekodrop_storage::{
    create_manual_bundle_directory, detect_bundle_directory, ManualBundleCreateRequest,
};
use nekolink_protocol::{
    BundleSender, BundleType, DeviceIdentity, LocalBridgeActionLifecycleStatus,
    LocalBridgeActionUpdatedEvent, LocalBridgeAuthorizationRequest,
    LocalBridgeBundleSendPreflightEvent, LocalBridgeBundleSendPreflightStatus,
    LocalBridgeClientIdentity, LocalBridgeEvent, LocalBridgePermissionScope, LocalBridgeRequest,
    SignedSessionIdentityBinding,
};
use tauri::Manager;
use tauri::{AppHandle, Emitter, State};

mod bundle_helpers;
mod device_dtos;
mod dto;
mod local_bridge_action_results;
mod local_bridge_dtos;
mod local_bridge_events;
mod local_bridge_responses;
mod path_dialog;
mod receive_diagnostics;
mod staged_bundles;
mod transfer_dtos;
mod transfer_feedback;
mod transfer_targets;
mod user_paths;
pub use dto::*;

use bundle_helpers::{
    bundle_type_from_label, bundle_type_label, manual_bundle_id, manual_bundle_permissions,
    parse_bundle_type, sha256_hex,
};
use device_dtos::{
    device_identity_to_dto, device_to_dto, discovery_status_snapshot, trusted_device_to_dto,
};
use local_bridge_action_results::{
    local_bridge_action_lifecycle_result, local_bridge_bundle_import_result,
    local_bridge_bundle_rollback_result, local_bridge_bundle_send_result,
    local_bridge_bundle_send_result_from_preflight,
};
use local_bridge_dtos::{
    local_bridge_authorization_to_dto, local_bridge_authorizations_to_dtos,
    local_bridge_pending_action_result_to_dto, local_bridge_pending_action_to_dto,
    local_bridge_runtime_status_to_dto,
};
use local_bridge_events::{local_bridge_event_id, local_bridge_events_after};
use local_bridge_responses::{
    local_bridge_action_results_response, local_bridge_authorized_runtime_pending_response,
    local_bridge_client_metadata, local_bridge_events_response,
    local_bridge_pending_authorization_response_from_pending,
    local_bridge_pending_confirmation_response, local_bridge_read_only_response,
    local_bridge_read_only_unsupported_response,
};
use path_dialog::{
    bind_available_listener, choose_paths, default_receive_dir, expand_home_dir,
    open_path_with_system, PathDialogKind,
};
use receive_diagnostics::{receive_port_diagnostics_from_session, receive_session_to_dto};
use staged_bundles::{
    delete_staged_bundle_at, find_staged_bundle_dto_at, import_staged_bundle_at,
    import_staged_bundle_with_strategy_at, latest_bundle_import_receipt_dto_at,
    list_staged_bundle_dtos_at, parse_import_conflict_strategy, prune_staged_bundle_dtos_at,
    rollback_imported_bundle_at, validate_safe_bundle_id,
};
use transfer_dtos::{
    pending_offer_to_dto, pending_pairing_request_to_dto, pending_resume_summary_from_offer,
    receive_report_to_dto, send_report_to_dto, source_plan_to_dto, transfer_scan_progress_to_dto,
    transfer_security_mode_label, transfer_status_to_dto, transfer_to_dto,
};
use transfer_feedback::friendly_transfer_error;
use transfer_targets::{
    endpoint_and_peer_for_device_id, endpoint_and_peer_for_history_record,
    endpoint_and_peer_from_connection_input, reject_self_peer, validate_endpoint_for_desktop_send,
    verify_incoming_peer_against_trusted_devices, verify_peer_matches_transfer_peer, TransferPeer,
};
#[cfg(test)]
use transfer_targets::{is_current_lan_ip, trusted_peer_from_nearby_device};
use user_paths::{parse_paths_text, path_bufs_to_strings, string_paths_to_path_bufs};

use crate::app_config::{receive_policy_label, save_app_config};
use crate::app_state::{
    ActiveReceiveSession, AppState, LocalBridgeAuthorizationRecord, LocalBridgePendingAction,
    LocalBridgePendingActionResult, LocalBridgePendingImportBundleAction,
    LocalBridgePendingRollbackBundleImportAction, LocalBridgePendingSendBundleAction,
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
    upsert_trusted_device, TrustedDeviceRecord,
};

const TRANSFER_SCAN_PROGRESS_EVENT: &str = "transfer_scan_progress";
const STAGED_BUNDLE_RETENTION_SECS: u64 = 14 * 24 * 60 * 60;
const LOCAL_BRIDGE_EVENT_QUEUE_LIMIT: usize = 256;
const LOCAL_BRIDGE_PENDING_ACTION_QUEUE_LIMIT: usize = 128;
const LOCAL_BRIDGE_PENDING_ACTION_RESULT_LIMIT: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReceiveTrustContext {
    Untrusted,
    AuthenticatedTrusted,
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
    let import_root = bundle_import_root()?;
    list_staged_bundle_dtos_at(&staging_root, &import_root)
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
pub fn import_staged_bundle(
    request: ImportStagedBundleRequestDto,
) -> Result<ReceivedBundleDto, String> {
    let staging_root = bundle_staging_root()?;
    let import_root = bundle_import_root()?;
    let strategy = parse_import_conflict_strategy(request.conflict_strategy.as_deref())?;
    import_staged_bundle_with_strategy_at(&staging_root, &import_root, &request.bundle_id, strategy)
}

#[tauri::command]
pub fn rollback_imported_bundle(
    request: RollbackImportedBundleRequestDto,
) -> Result<ReceivedBundleDto, String> {
    let import_root = bundle_import_root()?;
    rollback_imported_bundle_at(&import_root, &request.bundle_id)
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
pub fn list_local_bridge_pending_action_results(
    state: State<'_, AppState>,
) -> Result<LocalBridgePendingActionResultListDto, String> {
    Ok(LocalBridgePendingActionResultListDto {
        results: list_local_bridge_pending_action_results_at(&state.local_bridge_runtime)?,
    })
}

#[tauri::command]
pub fn take_next_local_bridge_pending_action(
    state: State<'_, AppState>,
) -> Result<LocalBridgePendingActionTakeDto, String> {
    let action = take_next_local_bridge_pending_action_at(&state.local_bridge_runtime)?;
    let remaining_count = state
        .local_bridge_runtime
        .pending_actions
        .lock()
        .map_err(|error| error.to_string())?
        .len();
    Ok(LocalBridgePendingActionTakeDto {
        action,
        remaining_count,
    })
}

#[tauri::command]
pub fn preflight_next_local_bridge_bundle_send(
    state: State<'_, AppState>,
) -> Result<LocalBridgeBundleSendPreflightDto, String> {
    let trusted_devices = state
        .trusted_devices
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    preflight_next_local_bridge_bundle_send_at(
        &state.local_bridge_runtime,
        &trusted_devices,
        now_ms(),
    )
}

#[tauri::command]
pub fn execute_next_local_bridge_bundle_import(
    state: State<'_, AppState>,
) -> Result<Option<LocalBridgePendingActionResultDto>, String> {
    let staging_root = bundle_staging_root()?;
    let import_root = bundle_import_root()?;
    execute_next_local_bridge_bundle_import_at(
        &state.local_bridge_runtime,
        &staging_root,
        &import_root,
        now_ms(),
    )
}

#[tauri::command]
pub fn execute_next_local_bridge_bundle_send(
    state: State<'_, AppState>,
) -> Result<Option<LocalBridgePendingActionResultDto>, String> {
    let trusted_devices = state
        .trusted_devices
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    execute_next_local_bridge_bundle_send_at(&state, &trusted_devices, now_ms())
}

#[tauri::command]
pub fn run_local_bridge_runtime_worker_once(
    state: State<'_, AppState>,
) -> Result<Option<LocalBridgePendingActionResultDto>, String> {
    run_local_bridge_runtime_worker_once_at(&state, now_ms())
}

pub(crate) fn start_local_bridge_runtime_worker(app: AppHandle) {
    thread::spawn(move || loop {
        let state = app.state::<AppState>();
        if let Ok(mut actions) = state.local_bridge_runtime.pending_actions.lock() {
            while actions.is_empty() {
                match state
                    .local_bridge_runtime
                    .pending_actions_signal
                    .wait_timeout(actions, Duration::from_secs(30))
                {
                    Ok((next_actions, wait_result)) => {
                        actions = next_actions;
                        if !actions.is_empty() || !wait_result.timed_out() {
                            break;
                        }
                    }
                    Err(error) => {
                        eprintln!("local bridge worker wait failed: {error}");
                        break;
                    }
                }
            }
        }
        if let Err(error) = run_local_bridge_runtime_worker_once_at(&state, now_ms()) {
            eprintln!("local bridge worker failed: {error}");
            thread::sleep(Duration::from_millis(500));
        }
    });
}

pub(crate) fn run_local_bridge_runtime_worker_once_at(
    state: &AppState,
    now_ms: u128,
) -> Result<Option<LocalBridgePendingActionResultDto>, String> {
    let action = take_next_local_bridge_pending_action_raw(&state.local_bridge_runtime)?;
    match action {
        Some(LocalBridgePendingAction::SendBundle(action)) => {
            execute_local_bridge_bundle_send_action_at(state, action, now_ms).map(Some)
        }
        Some(LocalBridgePendingAction::ImportBundle(action)) => {
            let staging_root = bundle_staging_root()?;
            let import_root = bundle_import_root()?;
            let result = execute_local_bridge_bundle_import_action(
                action,
                &staging_root,
                &import_root,
                now_ms,
            )?;
            push_local_bridge_pending_action_result_record(
                &state.local_bridge_runtime,
                result.clone(),
            )?;
            Ok(Some(local_bridge_pending_action_result_to_dto(
                &result, false,
            )))
        }
        Some(LocalBridgePendingAction::RollbackBundleImport(action)) => {
            let import_root = bundle_import_root()?;
            let result = execute_local_bridge_bundle_rollback_action(action, &import_root, now_ms)?;
            push_local_bridge_action_lifecycle_result(&state.local_bridge_runtime, result.clone())?;
            Ok(Some(local_bridge_pending_action_result_to_dto(
                &result, false,
            )))
        }
        None => Ok(None),
    }
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

fn emit_transfer_scan_progress(app: &AppHandle, progress: TransferPlanScanProgress) {
    let _ = app.emit(
        TRANSFER_SCAN_PROGRESS_EVENT,
        transfer_scan_progress_to_dto(progress),
    );
}

fn handle_local_bridge_request_at(
    request_json: &str,
    trusted_devices: &[TrustedDeviceRecord],
    transfer_status: Option<&TransferStatusState>,
    staging_root: &std::path::Path,
) -> Result<LocalBridgeResponseDto, String> {
    let import_root = bundle_import_root()?;
    handle_local_bridge_request_with_auth_at(
        request_json,
        trusted_devices,
        transfer_status,
        staging_root,
        &import_root,
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
    let import_root = bundle_import_root()?;
    handle_local_bridge_request_with_runtime_at(
        request_json,
        &trusted_devices,
        transfer_status.as_ref(),
        &staging_root,
        &import_root,
        runtime,
        true,
        now_ms(),
    )
}

fn handle_local_bridge_request_with_runtime_at(
    request_json: &str,
    trusted_devices: &[TrustedDeviceRecord],
    transfer_status: Option<&TransferStatusState>,
    staging_root: &std::path::Path,
    import_root: &std::path::Path,
    runtime: &LocalBridgeRuntimeState,
    allow_wait: bool,
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
    let action_results = runtime
        .pending_action_results
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let pending_actions = runtime
        .pending_actions
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
            push_local_bridge_pending_action_queued(
                runtime,
                LocalBridgePendingAction::SendBundle(
                    local_bridge_pending_send_action_from_request(request, now_ms)?,
                ),
                now_ms,
            )?;
            mark_local_bridge_authorization_used(
                runtime,
                request.client.as_ref(),
                LocalBridgePermissionScope::BundleSend,
                now_ms,
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
            push_local_bridge_pending_action_queued(
                runtime,
                LocalBridgePendingAction::ImportBundle(
                    local_bridge_pending_import_action_from_request(request, now_ms)?,
                ),
                now_ms,
            )?;
            mark_local_bridge_authorization_used(
                runtime,
                request.client.as_ref(),
                LocalBridgePermissionScope::BundleImportRequest,
                now_ms,
            )?;
            return Ok(local_bridge_authorized_runtime_pending_response(
                request.request_id.clone(),
                request.client.clone(),
                "local bridge bundle import is authorized and waiting for the desktop runtime",
            ));
        }
    }

    if let LocalBridgeRequest::RollbackBundleImport(request) = &request {
        if local_bridge_client_has_scope(
            request.client.as_ref(),
            &authorizations,
            LocalBridgePermissionScope::BundleImportRequest,
            now_ms,
        ) {
            push_local_bridge_pending_action_queued(
                runtime,
                LocalBridgePendingAction::RollbackBundleImport(
                    local_bridge_pending_rollback_action_from_request(request, now_ms)?,
                ),
                now_ms,
            )?;
            mark_local_bridge_authorization_used(
                runtime,
                request.client.as_ref(),
                LocalBridgePermissionScope::BundleImportRequest,
                now_ms,
            )?;
            return Ok(local_bridge_authorized_runtime_pending_response(
                request.request_id.clone(),
                request.client.clone(),
                "local bridge bundle rollback is authorized and waiting for the desktop runtime",
            ));
        }
    }

    let request_for_usage = request.clone();
    let used_client = local_bridge_request_client(&request).cloned();
    let response = handle_validated_local_bridge_request_with_auth_at(
        request,
        trusted_devices,
        transfer_status,
        staging_root,
        import_root,
        &authorizations,
        &events,
        &pending_actions,
        &action_results,
        now_ms,
    )?;

    if !allow_wait || response.status != "ok" || !response.events.is_empty() {
        mark_local_bridge_authorization_used_for_response(
            runtime,
            used_client.as_ref(),
            &request_for_usage,
            &response,
            now_ms,
        )?;
        return Ok(response);
    }

    let request: LocalBridgeRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid bridge request JSON: {error}"))?;
    let LocalBridgeRequest::PollEvents(request) = request else {
        return Ok(response);
    };
    let Some(timeout_ms) = request.timeout_ms else {
        return Ok(response);
    };
    if timeout_ms == 0 {
        mark_local_bridge_authorization_used_for_response(
            runtime,
            used_client.as_ref(),
            &request_for_usage,
            &response,
            now_ms,
        )?;
        return Ok(response);
    }

    let response = wait_for_local_bridge_events(
        runtime,
        request,
        trusted_devices,
        transfer_status,
        staging_root,
        import_root,
        &authorizations,
        now_ms,
        Duration::from_millis(timeout_ms.min(30_000)),
    )?;
    mark_local_bridge_authorization_used_for_response(
        runtime,
        used_client.as_ref(),
        &request_for_usage,
        &response,
        now_ms,
    )?;
    Ok(response)
}

fn push_local_bridge_pending_action_queued(
    runtime: &LocalBridgeRuntimeState,
    action: LocalBridgePendingAction,
    now_ms: u128,
) -> Result<(), String> {
    let result = local_bridge_action_lifecycle_result(
        &action,
        LocalBridgeActionLifecycleStatus::Queued,
        None,
        "local bridge action is queued for the desktop runtime",
        local_bridge_pending_action_bundle_id(&action),
        local_bridge_pending_action_bundle_type(&action),
        local_bridge_pending_action_target_device_id(&action),
        now_ms,
    );
    let mut actions = runtime
        .pending_actions
        .lock()
        .map_err(|error| error.to_string())?;
    actions.push(action);
    if actions.len() > LOCAL_BRIDGE_PENDING_ACTION_QUEUE_LIMIT {
        let excess = actions.len() - LOCAL_BRIDGE_PENDING_ACTION_QUEUE_LIMIT;
        actions.drain(0..excess);
    }
    runtime.pending_actions_signal.notify_one();
    push_local_bridge_action_lifecycle_result(runtime, result)?;
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
        .map(|action| local_bridge_pending_action_to_dto(action, false))
        .collect())
}

fn take_next_local_bridge_pending_action_at(
    runtime: &LocalBridgeRuntimeState,
) -> Result<Option<LocalBridgePendingActionDto>, String> {
    Ok(take_next_local_bridge_pending_action_raw(runtime)?
        .map(|action| local_bridge_pending_action_to_dto(&action, true)))
}

fn take_next_local_bridge_pending_action_raw(
    runtime: &LocalBridgeRuntimeState,
) -> Result<Option<LocalBridgePendingAction>, String> {
    let mut actions = runtime
        .pending_actions
        .lock()
        .map_err(|error| error.to_string())?;
    if actions.is_empty() {
        return Ok(None);
    }
    Ok(Some(actions.remove(0)))
}

fn preflight_next_local_bridge_bundle_send_at(
    runtime: &LocalBridgeRuntimeState,
    trusted_devices: &[TrustedDeviceRecord],
    now_ms: u128,
) -> Result<LocalBridgeBundleSendPreflightDto, String> {
    let action = {
        let mut actions = runtime
            .pending_actions
            .lock()
            .map_err(|error| error.to_string())?;
        let Some(LocalBridgePendingAction::SendBundle(_)) = actions.first() else {
            return Ok(LocalBridgeBundleSendPreflightDto {
                status: "skipped".to_string(),
                request_id: None,
                reason: Some("no_bundle_send_action".to_string()),
                message: "no pending local bridge bundle send action".to_string(),
                client_id: None,
                client_display_name: None,
                client_app_kind: None,
                bundle_id: None,
                bundle_type: None,
                bundle_root: None,
                target_device_id: None,
                require_trusted_device: None,
                requested_at_ms: None,
                claimed_at_ms: Some(now_ms),
            });
        };
        match actions.remove(0) {
            LocalBridgePendingAction::SendBundle(action) => action,
            LocalBridgePendingAction::ImportBundle(_) => unreachable!("first action checked above"),
            LocalBridgePendingAction::RollbackBundleImport(_) => {
                unreachable!("first action checked above")
            }
        }
    };

    let result = preflight_local_bridge_bundle_send_action(action, trusted_devices, now_ms)?;
    push_local_bridge_pending_action_result(runtime, &result)?;
    Ok(result)
}

fn execute_next_local_bridge_bundle_import_at(
    runtime: &LocalBridgeRuntimeState,
    staging_root: &std::path::Path,
    import_root: &std::path::Path,
    now_ms: u128,
) -> Result<Option<LocalBridgePendingActionResultDto>, String> {
    let action = match take_next_local_bridge_pending_action_raw(runtime)? {
        Some(LocalBridgePendingAction::ImportBundle(action)) => action,
        Some(LocalBridgePendingAction::SendBundle(action)) => {
            push_front_local_bridge_pending_action(
                runtime,
                LocalBridgePendingAction::SendBundle(action),
            )?;
            return Ok(None);
        }
        Some(LocalBridgePendingAction::RollbackBundleImport(action)) => {
            push_front_local_bridge_pending_action(
                runtime,
                LocalBridgePendingAction::RollbackBundleImport(action),
            )?;
            return Ok(None);
        }
        None => return Ok(None),
    };

    push_local_bridge_action_lifecycle_result(
        runtime,
        local_bridge_action_lifecycle_result(
            &LocalBridgePendingAction::ImportBundle(action.clone()),
            LocalBridgeActionLifecycleStatus::Running,
            None,
            "local bridge bundle import is running",
            None,
            action.expected_bundle_type,
            None,
            now_ms,
        ),
    )?;
    let result =
        execute_local_bridge_bundle_import_action(action, staging_root, import_root, now_ms)?;
    push_local_bridge_action_lifecycle_result(runtime, result.clone())?;
    Ok(Some(local_bridge_pending_action_result_to_dto(
        &result, false,
    )))
}

fn execute_next_local_bridge_bundle_send_at(
    state: &AppState,
    trusted_devices: &[TrustedDeviceRecord],
    now_ms: u128,
) -> Result<Option<LocalBridgePendingActionResultDto>, String> {
    execute_next_local_bridge_bundle_send_with(
        &state.local_bridge_runtime,
        trusted_devices,
        now_ms,
        |action| {
            let target_device_id = action
                .target_device_id
                .as_deref()
                .ok_or_else(|| "local bridge bundle send requires target_device_id".to_string())?;
            let (endpoint, peer) = endpoint_and_peer_for_device_id(state, target_device_id)?;
            send_paths_to_endpoint(state, endpoint, action.bundle_root.clone(), peer).map(|_| ())
        },
    )
}

fn execute_local_bridge_bundle_send_action_at(
    state: &AppState,
    action: LocalBridgePendingSendBundleAction,
    now_ms: u128,
) -> Result<LocalBridgePendingActionResultDto, String> {
    let trusted_devices = state
        .trusted_devices
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    execute_local_bridge_bundle_send_action_with(
        &state.local_bridge_runtime,
        &trusted_devices,
        action,
        now_ms,
        |action| {
            let target_device_id = action
                .target_device_id
                .as_deref()
                .ok_or_else(|| "local bridge bundle send requires target_device_id".to_string())?;
            let (endpoint, peer) = endpoint_and_peer_for_device_id(state, target_device_id)?;
            send_paths_to_endpoint(state, endpoint, action.bundle_root.clone(), peer).map(|_| ())
        },
    )
}

fn execute_next_local_bridge_bundle_send_with<S>(
    runtime: &LocalBridgeRuntimeState,
    trusted_devices: &[TrustedDeviceRecord],
    now_ms: u128,
    mut send_bundle: S,
) -> Result<Option<LocalBridgePendingActionResultDto>, String>
where
    S: FnMut(&LocalBridgePendingSendBundleAction) -> Result<(), String>,
{
    let action = match take_next_local_bridge_pending_action_raw(runtime)? {
        Some(LocalBridgePendingAction::SendBundle(action)) => action,
        Some(LocalBridgePendingAction::ImportBundle(action)) => {
            push_front_local_bridge_pending_action(
                runtime,
                LocalBridgePendingAction::ImportBundle(action),
            )?;
            return Ok(None);
        }
        Some(LocalBridgePendingAction::RollbackBundleImport(action)) => {
            push_front_local_bridge_pending_action(
                runtime,
                LocalBridgePendingAction::RollbackBundleImport(action),
            )?;
            return Ok(None);
        }
        None => return Ok(None),
    };

    execute_local_bridge_bundle_send_action_with(
        runtime,
        trusted_devices,
        action,
        now_ms,
        &mut send_bundle,
    )
    .map(Some)
}

fn execute_local_bridge_bundle_send_action_with<S>(
    runtime: &LocalBridgeRuntimeState,
    trusted_devices: &[TrustedDeviceRecord],
    action: LocalBridgePendingSendBundleAction,
    now_ms: u128,
    mut send_bundle: S,
) -> Result<LocalBridgePendingActionResultDto, String>
where
    S: FnMut(&LocalBridgePendingSendBundleAction) -> Result<(), String>,
{
    push_local_bridge_action_lifecycle_result(
        runtime,
        local_bridge_action_lifecycle_result(
            &LocalBridgePendingAction::SendBundle(action.clone()),
            LocalBridgeActionLifecycleStatus::Running,
            None,
            "local bridge bundle send is running",
            None,
            Some(action.bundle_type),
            action.target_device_id.as_deref(),
            now_ms,
        ),
    )?;
    let preflight =
        preflight_local_bridge_bundle_send_action(action.clone(), trusted_devices, now_ms)?;
    let result = if preflight.status != "ready" {
        local_bridge_bundle_send_result_from_preflight("failed", &preflight, &action, now_ms)
    } else if action.target_device_id.as_deref().is_none() {
        local_bridge_bundle_send_result(
            "failed",
            &action,
            preflight.bundle_id.as_deref(),
            preflight
                .bundle_type
                .as_deref()
                .and_then(bundle_type_from_label),
            Some("target_device_required"),
            "local bridge bundle send requires target_device_id before desktop execution",
            now_ms,
        )
    } else {
        match send_bundle(&action) {
            Ok(()) => local_bridge_bundle_send_result(
                "completed",
                &action,
                preflight.bundle_id.as_deref(),
                preflight
                    .bundle_type
                    .as_deref()
                    .and_then(bundle_type_from_label),
                None,
                "local bridge bundle was sent by the desktop runtime",
                now_ms,
            ),
            Err(error) => local_bridge_bundle_send_result(
                "failed",
                &action,
                preflight.bundle_id.as_deref(),
                preflight
                    .bundle_type
                    .as_deref()
                    .and_then(bundle_type_from_label),
                Some("bundle_send_failed"),
                &format!(
                    "local bridge bundle send failed: {}",
                    friendly_transfer_error(&error)
                ),
                now_ms,
            ),
        }
    };
    push_local_bridge_action_lifecycle_result(runtime, result.clone())?;
    Ok(local_bridge_pending_action_result_to_dto(&result, false))
}

fn execute_local_bridge_bundle_import_action(
    action: LocalBridgePendingImportBundleAction,
    staging_root: &std::path::Path,
    import_root: &std::path::Path,
    now_ms: u128,
) -> Result<LocalBridgePendingActionResult, String> {
    validate_safe_bundle_id(&action.staged_bundle_id)?;
    let staged_path = staging_root.join(&action.staged_bundle_id);
    let detected = match detect_bundle_directory(&staged_path) {
        Ok(Some(detected)) => detected,
        Ok(None) => {
            return Ok(local_bridge_bundle_import_result(
                "failed",
                &action,
                Some("bundle_manifest_missing"),
                "local bridge staged bundle does not contain bundle.json",
                None,
                None,
                0,
                None,
                0,
                now_ms,
            ));
        }
        Err(error) => {
            return Ok(local_bridge_bundle_import_result(
                "failed",
                &action,
                Some("bundle_invalid"),
                &format!("local bridge staged bundle validation failed: {error}"),
                None,
                None,
                0,
                None,
                0,
                now_ms,
            ));
        }
    };

    let bundle_id = detected.manifest.bundle_id.clone();
    let bundle_type = detected.manifest.bundle_type;
    if let Some(expected_bundle_type) = action.expected_bundle_type {
        if bundle_type != expected_bundle_type {
            return Ok(local_bridge_bundle_import_result(
                "failed",
                &action,
                Some("bundle_type_mismatch"),
                "local bridge expected bundle_type does not match the staged bundle manifest",
                Some(bundle_id.as_str()),
                Some(bundle_type),
                0,
                None,
                0,
                now_ms,
            ));
        }
    }

    let conflict_strategy =
        parse_import_conflict_strategy(Some(action.conflict_strategy.as_str()))?;
    match import_staged_bundle_with_strategy_at(
        staging_root,
        import_root,
        &action.staged_bundle_id,
        conflict_strategy,
    ) {
        Ok(imported) => Ok(local_bridge_bundle_import_result(
            "completed",
            &action,
            None,
            "local bridge staged bundle was imported",
            Some(imported.bundle_id.as_str()),
            bundle_type_from_label(&imported.bundle_type).or(Some(bundle_type)),
            imported.import_skipped_file_count,
            imported.import_receipt_path.as_deref(),
            imported.rollback_file_count,
            now_ms,
        )),
        Err(error) => {
            let reason = local_bridge_bundle_import_failure_reason(&error);
            Ok(local_bridge_bundle_import_result(
                "failed",
                &action,
                Some(reason),
                &format!("local bridge staged bundle import failed: {error}"),
                Some(bundle_id.as_str()),
                Some(bundle_type),
                0,
                None,
                0,
                now_ms,
            ))
        }
    }
}

fn execute_local_bridge_bundle_rollback_action(
    action: LocalBridgePendingRollbackBundleImportAction,
    import_root: &std::path::Path,
    now_ms: u128,
) -> Result<LocalBridgePendingActionResult, String> {
    validate_safe_bundle_id(&action.bundle_id)?;
    match rollback_imported_bundle_at(import_root, &action.bundle_id) {
        Ok(rolled_back) => Ok(local_bridge_bundle_rollback_result(
            "completed",
            &action,
            None,
            None,
            "local bridge bundle import was rolled back",
            rolled_back.rolled_back_file_count,
            now_ms,
        )),
        Err(error) => {
            let reason = local_bridge_bundle_rollback_failure_reason(&error);
            let rollback_blocking_reason = local_bridge_bundle_rollback_blocking_reason(&error);
            Ok(local_bridge_bundle_rollback_result(
                "failed",
                &action,
                Some(reason),
                rollback_blocking_reason,
                &format!("local bridge bundle rollback failed: {error}"),
                0,
                now_ms,
            ))
        }
    }
}

fn local_bridge_bundle_import_failure_reason(error: &str) -> &'static str {
    if error.contains("destination already exists") {
        return "bundle_import_conflict";
    }
    "bundle_import_failed"
}

fn local_bridge_bundle_rollback_failure_reason(error: &str) -> &'static str {
    if error.contains("没有找到资料包导入记录") {
        return "bundle_import_receipt_missing";
    }
    if error.contains("destination_missing")
        || error.contains("imported_file_missing")
        || error.contains("already_rolled_back")
    {
        return "bundle_rollback_blocked";
    }
    "bundle_rollback_failed"
}

fn local_bridge_bundle_rollback_blocking_reason(error: &str) -> Option<&'static str> {
    if error.contains("destination_missing") {
        return Some("destination_missing");
    }
    if error.contains("imported_file_missing") {
        return Some("imported_file_missing");
    }
    if error.contains("already_rolled_back") {
        return Some("already_rolled_back");
    }
    None
}

fn preflight_local_bridge_bundle_send_action(
    action: LocalBridgePendingSendBundleAction,
    trusted_devices: &[TrustedDeviceRecord],
    now_ms: u128,
) -> Result<LocalBridgeBundleSendPreflightDto, String> {
    let bundle_root = Path::new(&action.bundle_root);
    if !bundle_root.exists() || !bundle_root.is_dir() {
        return Ok(local_bridge_bundle_send_preflight_result(
            "failed_preflight",
            &action,
            None,
            None,
            Some("bundle_root_missing"),
            "local bridge bundle_root is missing or is not a directory",
            now_ms,
        ));
    }

    let detected = match detect_bundle_directory(bundle_root) {
        Ok(Some(detected)) => detected,
        Ok(None) => {
            return Ok(local_bridge_bundle_send_preflight_result(
                "failed_preflight",
                &action,
                None,
                None,
                Some("bundle_manifest_missing"),
                "local bridge bundle_root does not contain bundle.json",
                now_ms,
            ));
        }
        Err(error) => {
            return Ok(local_bridge_bundle_send_preflight_result(
                "failed_preflight",
                &action,
                None,
                None,
                Some("bundle_invalid"),
                &format!("local bridge bundle validation failed: {error}"),
                now_ms,
            ));
        }
    };

    let detected_type = detected.manifest.bundle_type;
    if detected_type != action.bundle_type {
        return Ok(local_bridge_bundle_send_preflight_result(
            "failed_preflight",
            &action,
            Some(detected.manifest.bundle_id.as_str()),
            Some(detected_type),
            Some("bundle_type_mismatch"),
            "local bridge bundle_type does not match the detected bundle manifest",
            now_ms,
        ));
    }

    if detected_type.requires_authenticated_encrypted_session() && !action.require_trusted_device {
        return Ok(local_bridge_bundle_send_preflight_result(
            "failed_preflight",
            &action,
            Some(detected.manifest.bundle_id.as_str()),
            Some(detected_type),
            Some("sensitive_bundle_requires_trusted_device"),
            "local bridge sensitive bundle send requires a trusted authenticated session target",
            now_ms,
        ));
    }

    if action.require_trusted_device {
        let Some(target_device_id) = action.target_device_id.as_deref() else {
            return Ok(local_bridge_bundle_send_preflight_result(
                "failed_preflight",
                &action,
                Some(detected.manifest.bundle_id.as_str()),
                Some(detected_type),
                Some("trusted_target_required"),
                "local bridge bundle send requires a trusted target device",
                now_ms,
            ));
        };
        if !trusted_devices
            .iter()
            .any(|device| device.device_id == target_device_id)
        {
            return Ok(local_bridge_bundle_send_preflight_result(
                "failed_preflight",
                &action,
                Some(detected.manifest.bundle_id.as_str()),
                Some(detected_type),
                Some("trusted_target_missing"),
                "local bridge bundle send target is not a trusted device",
                now_ms,
            ));
        }
    }

    Ok(local_bridge_bundle_send_preflight_result(
        "ready",
        &action,
        Some(detected.manifest.bundle_id.as_str()),
        Some(detected_type),
        None,
        "local bridge bundle send is ready for the desktop send worker",
        now_ms,
    ))
}

fn local_bridge_bundle_send_preflight_result(
    status: &str,
    action: &LocalBridgePendingSendBundleAction,
    bundle_id: Option<&str>,
    bundle_type: Option<BundleType>,
    reason: Option<&str>,
    message: &str,
    now_ms: u128,
) -> LocalBridgeBundleSendPreflightDto {
    LocalBridgeBundleSendPreflightDto {
        status: status.to_string(),
        request_id: Some(action.request_id.clone()),
        reason: reason.map(str::to_string),
        message: message.to_string(),
        client_id: Some(action.client.client_id.clone()),
        client_display_name: Some(action.client.display_name.clone()),
        client_app_kind: action.client.app_kind.clone(),
        bundle_id: bundle_id.map(str::to_string),
        bundle_type: bundle_type.map(bundle_type_label).map(str::to_string),
        bundle_root: Some(action.bundle_root.clone()),
        target_device_id: action.target_device_id.clone(),
        require_trusted_device: Some(action.require_trusted_device),
        requested_at_ms: Some(action.requested_at_ms),
        claimed_at_ms: Some(now_ms),
    }
}

fn push_local_bridge_pending_action_result(
    runtime: &LocalBridgeRuntimeState,
    result: &LocalBridgeBundleSendPreflightDto,
) -> Result<(), String> {
    let Some(request_id) = result.request_id.clone() else {
        return Ok(());
    };
    let Some(client_id) = result.client_id.clone() else {
        return Ok(());
    };
    let Some(client_display_name) = result.client_display_name.clone() else {
        return Ok(());
    };
    let client_app_kind = result.client_app_kind.clone();
    let Some(requested_at_ms) = result.requested_at_ms else {
        return Ok(());
    };
    let Some(claimed_at_ms) = result.claimed_at_ms else {
        return Ok(());
    };
    let event_id = format!("bridge-action-{request_id}-{claimed_at_ms}");

    let result = LocalBridgePendingActionResult {
        request_id: request_id.clone(),
        action_kind: "bundle.send".to_string(),
        client_id: client_id.clone(),
        client_display_name,
        client_app_kind,
        status: result.status.clone(),
        lifecycle_status: None,
        reason: result.reason.clone(),
        message: result.message.clone(),
        bundle_id: result.bundle_id.clone(),
        bundle_type: result.bundle_type.clone(),
        bundle_root: result.bundle_root.clone(),
        target_device_id: result.target_device_id.clone(),
        require_trusted_device: result.require_trusted_device,
        conflict_strategy: None,
        skipped_file_count: 0,
        import_receipt_path: None,
        rollback_file_count: 0,
        rollback_blocking_reason: None,
        rolled_back_file_count: 0,
        requested_at_ms,
        claimed_at_ms,
    };
    push_local_bridge_pending_action_result_record(runtime, result.clone())?;

    let status = match result.status.as_str() {
        "ready" => LocalBridgeBundleSendPreflightStatus::Ready,
        "failed_preflight" => LocalBridgeBundleSendPreflightStatus::FailedPreflight,
        _ => return Ok(()),
    };
    push_local_bridge_runtime_event(
        runtime,
        LocalBridgeEvent::BundleSendPreflight(LocalBridgeBundleSendPreflightEvent {
            event_id,
            request_id,
            client_id,
            client_app_kind: result.client_app_kind.clone(),
            status,
            reason: result.reason.clone(),
            bundle_id: result.bundle_id.clone(),
            bundle_type: result
                .bundle_type
                .as_deref()
                .and_then(bundle_type_from_label),
            target_device_id: result.target_device_id.clone(),
        }),
    )?;
    Ok(())
}

fn push_local_bridge_pending_action_result_record(
    runtime: &LocalBridgeRuntimeState,
    result: LocalBridgePendingActionResult,
) -> Result<(), String> {
    let mut results = runtime
        .pending_action_results
        .lock()
        .map_err(|error| error.to_string())?;
    results.retain(|existing| {
        existing.request_id != result.request_id
            || existing.action_kind != result.action_kind
            || existing.client_id != result.client_id
            || existing.client_app_kind != result.client_app_kind
    });
    results.push(result);
    if results.len() > LOCAL_BRIDGE_PENDING_ACTION_RESULT_LIMIT {
        let excess = results.len() - LOCAL_BRIDGE_PENDING_ACTION_RESULT_LIMIT;
        results.drain(0..excess);
    }
    runtime.events_signal.notify_all();
    Ok(())
}

fn push_local_bridge_action_lifecycle_result(
    runtime: &LocalBridgeRuntimeState,
    result: LocalBridgePendingActionResult,
) -> Result<(), String> {
    push_local_bridge_pending_action_result_record(runtime, result.clone())?;
    push_local_bridge_runtime_event(
        runtime,
        LocalBridgeEvent::ActionUpdated(LocalBridgeActionUpdatedEvent {
            event_id: format!(
                "bridge-action-{}-{}-{}",
                result.request_id,
                result
                    .lifecycle_status
                    .as_deref()
                    .unwrap_or(result.status.as_str()),
                result.claimed_at_ms
            ),
            request_id: result.request_id,
            action_kind: result.action_kind,
            client_id: result.client_id,
            client_app_kind: result.client_app_kind,
            status: local_bridge_lifecycle_status_from_label(
                result
                    .lifecycle_status
                    .as_deref()
                    .unwrap_or(result.status.as_str()),
            ),
            reason: result.reason,
            message: result.message,
            bundle_id: result.bundle_id,
            bundle_type: result
                .bundle_type
                .as_deref()
                .and_then(bundle_type_from_label),
            target_device_id: result.target_device_id,
            updated_at_ms: result.claimed_at_ms,
        }),
    )
}

fn local_bridge_lifecycle_status_from_label(label: &str) -> LocalBridgeActionLifecycleStatus {
    match label {
        "queued" => LocalBridgeActionLifecycleStatus::Queued,
        "running" => LocalBridgeActionLifecycleStatus::Running,
        "succeeded" => LocalBridgeActionLifecycleStatus::Succeeded,
        "conflict" => LocalBridgeActionLifecycleStatus::Conflict,
        "cancelled" => LocalBridgeActionLifecycleStatus::Cancelled,
        _ => LocalBridgeActionLifecycleStatus::Failed,
    }
}

fn push_front_local_bridge_pending_action(
    runtime: &LocalBridgeRuntimeState,
    action: LocalBridgePendingAction,
) -> Result<(), String> {
    let mut actions = runtime
        .pending_actions
        .lock()
        .map_err(|error| error.to_string())?;
    actions.insert(0, action);
    runtime.pending_actions_signal.notify_one();
    Ok(())
}

fn list_local_bridge_pending_action_results_at(
    runtime: &LocalBridgeRuntimeState,
) -> Result<Vec<LocalBridgePendingActionResultDto>, String> {
    let results = runtime
        .pending_action_results
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(results
        .iter()
        .map(|result| local_bridge_pending_action_result_to_dto(result, false))
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
    let Some(position) = actions
        .iter()
        .position(|action| local_bridge_pending_action_request_id(action) == request_id)
    else {
        return Ok(false);
    };
    let action = actions.remove(position);
    drop(actions);
    push_local_bridge_action_lifecycle_result(
        runtime,
        local_bridge_action_lifecycle_result(
            &action,
            LocalBridgeActionLifecycleStatus::Cancelled,
            None,
            "local bridge action was cancelled before execution",
            None,
            match &action {
                LocalBridgePendingAction::SendBundle(action) => Some(action.bundle_type),
                LocalBridgePendingAction::ImportBundle(action) => action.expected_bundle_type,
                LocalBridgePendingAction::RollbackBundleImport(_) => None,
            },
            match &action {
                LocalBridgePendingAction::SendBundle(action) => action.target_device_id.as_deref(),
                LocalBridgePendingAction::ImportBundle(_) => None,
                LocalBridgePendingAction::RollbackBundleImport(_) => None,
            },
            now_ms(),
        ),
    )?;
    Ok(true)
}

fn local_bridge_pending_action_request_id(action: &LocalBridgePendingAction) -> &str {
    match action {
        LocalBridgePendingAction::SendBundle(action) => &action.request_id,
        LocalBridgePendingAction::ImportBundle(action) => &action.request_id,
        LocalBridgePendingAction::RollbackBundleImport(action) => &action.request_id,
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
        conflict_strategy: request
            .conflict_strategy
            .clone()
            .unwrap_or_else(|| "reject".to_string()),
        requested_at_ms: now_ms,
    })
}

fn local_bridge_pending_rollback_action_from_request(
    request: &nekolink_protocol::LocalBridgeRollbackBundleImportRequest,
    now_ms: u128,
) -> Result<LocalBridgePendingRollbackBundleImportAction, String> {
    let client = request
        .client
        .clone()
        .ok_or_else(|| "authorized local bridge rollback requires a client identity".to_string())?;
    Ok(LocalBridgePendingRollbackBundleImportAction {
        request_id: request.request_id.clone(),
        client,
        bundle_id: request.bundle_id.clone(),
        requested_at_ms: now_ms,
    })
}

fn handle_local_bridge_request_with_auth_at(
    request_json: &str,
    trusted_devices: &[TrustedDeviceRecord],
    transfer_status: Option<&TransferStatusState>,
    staging_root: &std::path::Path,
    import_root: &std::path::Path,
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
        import_root,
        authorizations,
        &[],
        &[],
        &[],
        now_ms,
    )
}

fn handle_validated_local_bridge_request_with_auth_at(
    request: LocalBridgeRequest,
    trusted_devices: &[TrustedDeviceRecord],
    transfer_status: Option<&TransferStatusState>,
    staging_root: &std::path::Path,
    import_root: &std::path::Path,
    authorizations: &[LocalBridgeAuthorizationRecord],
    events: &[LocalBridgeEvent],
    pending_actions: &[LocalBridgePendingAction],
    action_results: &[LocalBridgePendingActionResult],
    now_ms: u128,
) -> Result<LocalBridgeResponseDto, String> {
    match request {
        LocalBridgeRequest::ListDevices(request) => {
            if !local_bridge_client_has_scope(
                request.client.as_ref(),
                authorizations,
                LocalBridgePermissionScope::DeviceRead,
                now_ms,
            ) {
                return Ok(local_bridge_pending_confirmation_response(
                    request.request_id,
                    request.client,
                ));
            }
            let can_read_bundles = local_bridge_client_has_scope(
                request.client.as_ref(),
                authorizations,
                LocalBridgePermissionScope::BundleRead,
                now_ms,
            );
            let client = request.client.clone();
            Ok(local_bridge_read_only_response(
                request.request_id,
                client,
                "local bridge read-only snapshot",
                trusted_devices.iter().map(trusted_device_to_dto).collect(),
                if can_read_bundles {
                    list_staged_bundle_dtos_at(staging_root, import_root)?
                } else {
                    Vec::new()
                },
                None,
            ))
        }
        LocalBridgeRequest::TransferStatus(request) => {
            if !local_bridge_client_has_scope(
                request.client.as_ref(),
                authorizations,
                LocalBridgePermissionScope::TransferStatusRead,
                now_ms,
            ) {
                return Ok(local_bridge_pending_confirmation_response(
                    request.request_id,
                    request.client,
                ));
            }
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
            if !local_bridge_client_has_scope(
                request.client.as_ref(),
                authorizations,
                LocalBridgePermissionScope::BundleRead,
                now_ms,
            ) {
                return Ok(local_bridge_pending_confirmation_response(
                    request.request_id,
                    request.client,
                ));
            }
            let client = request.client.clone();
            let bundle =
                find_staged_bundle_dto_at(staging_root, import_root, &request.staged_bundle_id)?;
            let bundle = match bundle {
                Some(bundle) => Some(bundle),
                None => {
                    latest_bundle_import_receipt_dto_at(import_root, &request.staged_bundle_id)?
                }
            };
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
            let can_send_bundles = local_bridge_client_has_scope(
                request.client.as_ref(),
                authorizations,
                LocalBridgePermissionScope::BundleSend,
                now_ms,
            );
            let can_import_bundles = local_bridge_client_has_scope(
                request.client.as_ref(),
                authorizations,
                LocalBridgePermissionScope::BundleImportRequest,
                now_ms,
            );
            if !can_read_bundles && !can_read_transfers && !can_send_bundles && !can_import_bundles
            {
                return Ok(local_bridge_pending_confirmation_response(
                    request.request_id,
                    request.client,
                ));
            }
            let bridge_events = local_bridge_events_after(
                events,
                request.client.as_ref(),
                request.after_event_id.as_deref(),
                request.action_request_id.as_deref(),
                request.limit.unwrap_or(50),
                can_read_bundles,
                can_read_transfers,
                can_send_bundles,
                can_import_bundles,
            )?;
            Ok(local_bridge_events_response(
                request.request_id,
                request.client,
                bridge_events,
            ))
        }
        LocalBridgeRequest::ActionResults(request) => {
            let action_results = local_bridge_action_results_for_client(
                request.client.as_ref(),
                authorizations,
                pending_actions,
                action_results,
                request.action_request_id.as_deref(),
                request.after_claimed_at_ms,
                request.limit.unwrap_or(50),
                now_ms,
            )?;
            if action_results.is_none() {
                return Ok(local_bridge_pending_confirmation_response(
                    request.request_id,
                    request.client,
                ));
            }
            Ok(local_bridge_action_results_response(
                request.request_id,
                request.client,
                action_results.unwrap_or_default(),
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
        LocalBridgeRequest::RollbackBundleImport(request) => {
            if local_bridge_client_has_scope(
                request.client.as_ref(),
                authorizations,
                LocalBridgePermissionScope::BundleImportRequest,
                now_ms,
            ) {
                Ok(local_bridge_authorized_runtime_pending_response(
                    request.request_id,
                    request.client,
                    "local bridge bundle rollback is authorized, but the rollback runtime is not connected yet",
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
    runtime.events_signal.notify_all();
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

fn local_bridge_action_results_for_client(
    client: Option<&LocalBridgeClientIdentity>,
    authorizations: &[LocalBridgeAuthorizationRecord],
    pending_actions: &[LocalBridgePendingAction],
    results: &[LocalBridgePendingActionResult],
    action_request_id: Option<&str>,
    after_claimed_at_ms: Option<u128>,
    limit: usize,
    now_ms: u128,
) -> Result<Option<Vec<LocalBridgePendingActionResultDto>>, String> {
    let Some(client) = client else {
        return Ok(None);
    };
    let can_read_send_results = local_bridge_client_has_scope(
        Some(client),
        authorizations,
        LocalBridgePermissionScope::BundleSend,
        now_ms,
    );
    let can_read_import_results = local_bridge_client_has_scope(
        Some(client),
        authorizations,
        LocalBridgePermissionScope::BundleImportRequest,
        now_ms,
    );
    if !can_read_send_results && !can_read_import_results {
        return Ok(None);
    }

    let limit = limit.min(100);
    let output = results
        .iter()
        .filter(|result| local_bridge_action_result_matches_client(result, client))
        .filter(|result| action_request_id.is_none_or(|request_id| result.request_id == request_id))
        .filter(|result| after_claimed_at_ms.is_none_or(|after| result.claimed_at_ms > after))
        .filter(|result| match result.action_kind.as_str() {
            "bundle.send" => can_read_send_results,
            "bundle.import" | "bundle.rollback" => can_read_import_results,
            _ => false,
        })
        .take(limit)
        .map(|result| local_bridge_pending_action_result_to_dto(result, false))
        .collect::<Vec<_>>();
    if !output.is_empty() {
        return Ok(Some(output));
    }
    let Some(request_id) = action_request_id else {
        return Ok(Some(output));
    };
    let Some(pending_action) = pending_actions.iter().find(|action| {
        local_bridge_pending_action_request_id(action) == request_id
            && local_bridge_pending_action_matches_client(action, client)
    }) else {
        return Ok(Some(output));
    };
    if !local_bridge_client_can_read_pending_action(
        pending_action,
        can_read_send_results,
        can_read_import_results,
    ) {
        return Ok(Some(output));
    }

    let queued_result = local_bridge_action_lifecycle_result(
        pending_action,
        LocalBridgeActionLifecycleStatus::Queued,
        None,
        "local bridge action is queued for the desktop runtime",
        local_bridge_pending_action_bundle_id(pending_action),
        local_bridge_pending_action_bundle_type(pending_action),
        local_bridge_pending_action_target_device_id(pending_action),
        local_bridge_pending_action_requested_at_ms(pending_action),
    );
    if after_claimed_at_ms.is_some_and(|after| queued_result.claimed_at_ms <= after) {
        return Ok(Some(output));
    }

    let output = vec![local_bridge_pending_action_result_to_dto(
        &queued_result,
        false,
    )];
    Ok(Some(output))
}

fn local_bridge_action_result_matches_client(
    result: &LocalBridgePendingActionResult,
    client: &LocalBridgeClientIdentity,
) -> bool {
    result.client_id == client.client_id && result.client_app_kind == client.app_kind
}

fn local_bridge_pending_action_matches_client(
    action: &LocalBridgePendingAction,
    client: &LocalBridgeClientIdentity,
) -> bool {
    match action {
        LocalBridgePendingAction::SendBundle(action) => action.client == *client,
        LocalBridgePendingAction::ImportBundle(action) => action.client == *client,
        LocalBridgePendingAction::RollbackBundleImport(action) => action.client == *client,
    }
}

fn local_bridge_pending_action_requested_at_ms(action: &LocalBridgePendingAction) -> u128 {
    match action {
        LocalBridgePendingAction::SendBundle(action) => action.requested_at_ms,
        LocalBridgePendingAction::ImportBundle(action) => action.requested_at_ms,
        LocalBridgePendingAction::RollbackBundleImport(action) => action.requested_at_ms,
    }
}

fn local_bridge_pending_action_bundle_type(
    action: &LocalBridgePendingAction,
) -> Option<BundleType> {
    match action {
        LocalBridgePendingAction::SendBundle(action) => Some(action.bundle_type),
        LocalBridgePendingAction::ImportBundle(action) => action.expected_bundle_type,
        LocalBridgePendingAction::RollbackBundleImport(_) => None,
    }
}

fn local_bridge_pending_action_bundle_id(action: &LocalBridgePendingAction) -> Option<&str> {
    match action {
        LocalBridgePendingAction::SendBundle(_) => None,
        LocalBridgePendingAction::ImportBundle(action) => Some(action.staged_bundle_id.as_str()),
        LocalBridgePendingAction::RollbackBundleImport(action) => Some(action.bundle_id.as_str()),
    }
}

fn local_bridge_pending_action_target_device_id(action: &LocalBridgePendingAction) -> Option<&str> {
    match action {
        LocalBridgePendingAction::SendBundle(action) => action.target_device_id.as_deref(),
        LocalBridgePendingAction::ImportBundle(_) => None,
        LocalBridgePendingAction::RollbackBundleImport(_) => None,
    }
}

fn local_bridge_client_can_read_pending_action(
    action: &LocalBridgePendingAction,
    can_read_send_results: bool,
    can_read_import_results: bool,
) -> bool {
    match action {
        LocalBridgePendingAction::SendBundle(_) => can_read_send_results,
        LocalBridgePendingAction::ImportBundle(_)
        | LocalBridgePendingAction::RollbackBundleImport(_) => can_read_import_results,
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

fn local_bridge_authorization_matches_client(
    record: &LocalBridgeAuthorizationRecord,
    client: &LocalBridgeClientIdentity,
) -> bool {
    record.client_id == client.client_id && record.app_kind == client.app_kind
}

fn sort_local_bridge_authorizations(records: &mut [LocalBridgeAuthorizationRecord]) {
    records.sort_by(|left, right| {
        right
            .last_used_at_ms
            .cmp(&left.last_used_at_ms)
            .then_with(|| right.granted_at_ms.cmp(&left.granted_at_ms))
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
        local_bridge_authorization_matches_client(record, client)
            && local_bridge_authorization_is_active(record, now_ms)
            && record.scopes.contains(&scope)
    })
}

fn mark_local_bridge_authorization_used(
    runtime: &LocalBridgeRuntimeState,
    client: Option<&LocalBridgeClientIdentity>,
    scope: LocalBridgePermissionScope,
    now_ms: u128,
) -> Result<(), String> {
    let Some(client) = client else {
        return Ok(());
    };
    let mut authorizations = runtime
        .authorizations
        .lock()
        .map_err(|error| error.to_string())?;
    for record in authorizations.iter_mut().filter(|record| {
        local_bridge_authorization_matches_client(record, client)
            && local_bridge_authorization_is_active(record, now_ms)
            && record.scopes.contains(&scope)
    }) {
        record.last_used_at_ms = now_ms;
    }
    Ok(())
}

fn local_bridge_request_client(request: &LocalBridgeRequest) -> Option<&LocalBridgeClientIdentity> {
    match request {
        LocalBridgeRequest::ListDevices(request) => request.client.as_ref(),
        LocalBridgeRequest::SendBundle(request) => request.client.as_ref(),
        LocalBridgeRequest::BundleDetail(request) => request.client.as_ref(),
        LocalBridgeRequest::ImportBundle(request) => request.client.as_ref(),
        LocalBridgeRequest::RollbackBundleImport(request) => request.client.as_ref(),
        LocalBridgeRequest::AuthorizationRequest(request) => Some(&request.client),
        LocalBridgeRequest::TransferStatus(request) => request.client.as_ref(),
        LocalBridgeRequest::PollEvents(request) => request.client.as_ref(),
        LocalBridgeRequest::ActionResults(request) => request.client.as_ref(),
    }
}

fn local_bridge_authorized_scopes_used_by_response(
    request: &LocalBridgeRequest,
    response: &LocalBridgeResponseDto,
) -> Vec<LocalBridgePermissionScope> {
    if response.status == "pending_auth" {
        return Vec::new();
    }
    let mut scopes = Vec::new();
    match request {
        LocalBridgeRequest::ListDevices(_) => {
            scopes.push(LocalBridgePermissionScope::DeviceRead);
            if !response.staged_bundles.is_empty() {
                push_local_bridge_scope_once(&mut scopes, LocalBridgePermissionScope::BundleRead);
            }
        }
        LocalBridgeRequest::TransferStatus(_) => {
            scopes.push(LocalBridgePermissionScope::TransferStatusRead);
        }
        LocalBridgeRequest::BundleDetail(_) => {
            if !response.staged_bundles.is_empty() || response.status == "unsupported" {
                scopes.push(LocalBridgePermissionScope::BundleRead);
            }
        }
        LocalBridgeRequest::PollEvents(_) => {
            for event in &response.events {
                match event.get("kind").and_then(serde_json::Value::as_str) {
                    Some("bundle.received") => {
                        push_local_bridge_scope_once(
                            &mut scopes,
                            LocalBridgePermissionScope::BundleRead,
                        );
                    }
                    Some("transfer.updated") => {
                        push_local_bridge_scope_once(
                            &mut scopes,
                            LocalBridgePermissionScope::TransferStatusRead,
                        );
                    }
                    Some("bundle.send.preflight") => {
                        push_local_bridge_scope_once(
                            &mut scopes,
                            LocalBridgePermissionScope::BundleSend,
                        );
                    }
                    Some("action.updated") => {
                        match event
                            .get("payload")
                            .and_then(|payload| payload.get("action_kind"))
                            .and_then(serde_json::Value::as_str)
                        {
                            Some("bundle.send") => push_local_bridge_scope_once(
                                &mut scopes,
                                LocalBridgePermissionScope::BundleSend,
                            ),
                            Some("bundle.import" | "bundle.rollback") => {
                                push_local_bridge_scope_once(
                                    &mut scopes,
                                    LocalBridgePermissionScope::BundleImportRequest,
                                );
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
        LocalBridgeRequest::ActionResults(_) => {
            for result in &response.action_results {
                match result.action_kind.as_str() {
                    "bundle.send" => push_local_bridge_scope_once(
                        &mut scopes,
                        LocalBridgePermissionScope::BundleSend,
                    ),
                    "bundle.import" | "bundle.rollback" => push_local_bridge_scope_once(
                        &mut scopes,
                        LocalBridgePermissionScope::BundleImportRequest,
                    ),
                    _ => {}
                }
            }
        }
        LocalBridgeRequest::SendBundle(_) => scopes.push(LocalBridgePermissionScope::BundleSend),
        LocalBridgeRequest::ImportBundle(_) | LocalBridgeRequest::RollbackBundleImport(_) => {
            scopes.push(LocalBridgePermissionScope::BundleImportRequest);
        }
        LocalBridgeRequest::AuthorizationRequest(_) => {}
    }
    scopes
}

fn push_local_bridge_scope_once(
    scopes: &mut Vec<LocalBridgePermissionScope>,
    scope: LocalBridgePermissionScope,
) {
    if !scopes.contains(&scope) {
        scopes.push(scope);
    }
}

fn mark_local_bridge_authorization_used_for_response(
    runtime: &LocalBridgeRuntimeState,
    client: Option<&LocalBridgeClientIdentity>,
    request: &LocalBridgeRequest,
    response: &LocalBridgeResponseDto,
    now_ms: u128,
) -> Result<(), String> {
    let Some(client) = client else {
        return Ok(());
    };
    for scope in local_bridge_authorized_scopes_used_by_response(request, response) {
        mark_local_bridge_authorization_used(runtime, Some(client), scope, now_ms)?;
    }
    Ok(())
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
        scopes: dedupe_local_bridge_permission_scopes(&pending.requested_scopes),
        granted_at_ms: now_ms,
        last_used_at_ms: now_ms,
        expires_at_ms: Some(pending.expires_at_ms),
    })
}

fn dedupe_local_bridge_permission_scopes(
    scopes: &[LocalBridgePermissionScope],
) -> Vec<LocalBridgePermissionScope> {
    let mut output = Vec::new();
    for scope in scopes {
        push_local_bridge_scope_once(&mut output, *scope);
    }
    output
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

fn wait_for_local_bridge_events(
    runtime: &LocalBridgeRuntimeState,
    request: nekolink_protocol::LocalBridgePollEventsRequest,
    trusted_devices: &[TrustedDeviceRecord],
    transfer_status: Option<&TransferStatusState>,
    staging_root: &std::path::Path,
    import_root: &std::path::Path,
    authorizations: &[LocalBridgeAuthorizationRecord],
    now_ms: u128,
    timeout: Duration,
) -> Result<LocalBridgeResponseDto, String> {
    let mut events = runtime.events.lock().map_err(|error| error.to_string())?;
    let baseline_last_event_id = events.last().map(local_bridge_event_id).map(str::to_string);
    let deadline = Instant::now()
        .checked_add(timeout)
        .unwrap_or_else(Instant::now);
    while events.last().map(local_bridge_event_id).map(str::to_string) == baseline_last_event_id {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match runtime.events_signal.wait_timeout(events, remaining) {
            Ok((next_events, wait_result)) => {
                events = next_events;
                if wait_result.timed_out() {
                    break;
                }
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    let events = events.clone();
    let action_results = runtime
        .pending_action_results
        .lock()
        .map_err(|error| error.to_string())?
        .clone();

    handle_validated_local_bridge_request_with_auth_at(
        LocalBridgeRequest::PollEvents(request),
        trusted_devices,
        transfer_status,
        staging_root,
        import_root,
        authorizations,
        &events,
        &[],
        &action_results,
        now_ms,
    )
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
        action_results: Vec::new(),
        events: Vec::new(),
        events_last_id: None,
        events_next_after_id: None,
        events_has_more: false,
        events_cursor_state: "empty".to_string(),
        events_visible_first_id: None,
        events_visible_last_id: None,
        events_visible_count: 0,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::transfer_dtos::RECEIVE_FILE_PREVIEW_LIMIT;
    use nekolink_protocol::{
        BundleChecksums, BundleCompatibility, BundleFile, BundleManifest, BundlePermissionScope,
        BundlePermissions, BundleSecretsPolicy, BundleSender, BundleSummary, BundleType,
        BundleWriteMode, BundleWritePermission, Capability, BUNDLE_CHECKSUM_SHA256,
        BUNDLE_SCHEMA_V1, PROTOCOL_VERSION,
    };
    use std::collections::BTreeMap;

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
    fn current_utc_timestamp_uses_utc_iso_8601_shape() {
        let timestamp = current_utc_timestamp();

        assert!(timestamp.contains('T'));
        assert!(timestamp.ends_with('Z'));
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
        let import_root = dir.join("bundle_imports");
        let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
        nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();

        let bundles = list_staged_bundle_dtos_at(&staging_root, &import_root).unwrap();

        assert_eq!(bundles.len(), 1);
        assert_eq!(bundles[0].bundle_id, "bundle_1234567890");
        assert_eq!(bundles[0].staging_status, "saved");
        assert!(bundles[0].can_import_now);
        assert!(!bundles[0].has_import_receipt);
        assert!(!bundles[0].can_request_rollback);
        assert!(!bundles[0].import_conflict);
        assert_eq!(
            bundles[0].import_destination.as_deref(),
            Some(
                import_root
                    .join("bundle_1234567890")
                    .to_string_lossy()
                    .as_ref()
            )
        );
        assert_eq!(bundles[0].import_conflict_count, 0);
        assert_eq!(bundles[0].import_plan_files.len(), 2);
        assert_eq!(
            bundles[0].import_plan_files[0].manifest_path,
            "files/manifest.json"
        );
        assert!(bundles[0]
            .import_plan_files
            .iter()
            .all(|file| !file.destination_exists));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn staged_bundle_dto_marks_import_destination_conflict() {
        let dir = unique_bundle_temp_dir("desktop-bundle-list-conflict");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
        let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
        nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();
        fs::create_dir_all(import_root.join("bundle_1234567890")).unwrap();

        let bundles = list_staged_bundle_dtos_at(&staging_root, &import_root).unwrap();

        assert_eq!(bundles.len(), 1);
        assert_eq!(bundles[0].bundle_id, "bundle_1234567890");
        assert!(!bundles[0].can_import_now);
        assert!(bundles[0].import_conflict);
        assert_eq!(
            bundles[0].import_blocking_reason.as_deref(),
            Some("destination_exists")
        );
        assert_eq!(bundles[0].import_conflict_count, 0);
        assert_eq!(bundles[0].import_plan_files.len(), 2);
        assert!(bundles[0]
            .import_plan_files
            .iter()
            .all(|file| !file.destination_exists));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn staged_bundle_dto_includes_conflicting_import_files() {
        let dir = unique_bundle_temp_dir("desktop-bundle-list-file-conflict");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
        let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
        nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();
        fs::create_dir_all(import_root.join("bundle_1234567890")).unwrap();
        fs::write(
            import_root.join("bundle_1234567890").join("content.bin"),
            b"existing",
        )
        .unwrap();

        let bundles = list_staged_bundle_dtos_at(&staging_root, &import_root).unwrap();

        assert_eq!(bundles.len(), 1);
        assert!(!bundles[0].can_import_now);
        assert!(bundles[0].import_conflict);
        assert_eq!(bundles[0].import_conflict_count, 1);
        assert_eq!(bundles[0].import_plan_files.len(), 2);
        let conflicted = bundles[0]
            .import_plan_files
            .iter()
            .find(|file| file.destination_exists)
            .expect("one planned import file should conflict");
        assert_eq!(conflicted.manifest_path, "files/content.bin");

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
        assert!(imported.has_import_receipt);
        assert!(imported.can_request_rollback);
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
    fn staged_bundle_dto_keeps_imported_status_after_refresh() {
        let dir = unique_bundle_temp_dir("desktop-bundle-imported-refresh");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
        let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
        nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();
        import_staged_bundle_at(&staging_root, &import_root, "bundle_1234567890").unwrap();

        let bundles = list_staged_bundle_dtos_at(&staging_root, &import_root).unwrap();

        assert_eq!(bundles.len(), 1);
        assert_eq!(bundles[0].bundle_id, "bundle_1234567890");
        assert_eq!(bundles[0].staging_status, "imported");
        assert!(bundles[0].can_rollback_now);
        assert!(bundles[0].has_import_receipt);
        assert!(bundles[0].can_request_rollback);
        assert_eq!(bundles[0].rollback_file_count, 2);
        assert!(bundles[0].import_receipt_path.is_none());

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
        let import_root = dir.join("bundle_imports");
        let expired_root = create_desktop_test_bundle(&dir, "expired", "bundle_expired");
        let fresh_root = create_desktop_test_bundle(&dir, "fresh", "bundle_fresh");
        nekodrop_storage::stage_bundle_directory(&expired_root, &staging_root).unwrap();
        let cutoff = std::time::SystemTime::now();
        nekodrop_storage::stage_bundle_directory(&fresh_root, &staging_root).unwrap();

        let pruned = prune_staged_bundle_dtos_at(&staging_root, cutoff).unwrap();

        assert_eq!(pruned, vec!["bundle_expired"]);
        let remaining = list_staged_bundle_dtos_at(&staging_root, &import_root).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].bundle_id, "bundle_fresh");

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_devices_list_returns_trusted_devices_without_bundle_scope() {
        let dir = unique_bundle_temp_dir("local-bridge-devices");
        let staging_root = dir.join("bundle_staging");
        let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
        nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();
        let trusted = vec![trusted_record("device-a", "MacBook", "sha256:device-a")];
        let request = serde_json::json!({
            "kind": "devices.list",
            "payload": {
                "request_id": "bridge-request-1",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "trusted_only": true
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_auth_at(
            &request,
            &trusted,
            None,
            &staging_root,
            &dir.join("bundle_imports"),
            &[local_bridge_authorization(
                "local-agent-app",
                &[LocalBridgePermissionScope::DeviceRead],
                1_000,
                5_000,
            )],
            2_000,
        )
        .unwrap();

        assert_eq!(response.request_id, "bridge-request-1");
        assert_eq!(response.status, "ok");
        assert_eq!(response.devices.len(), 1);
        assert_eq!(response.devices[0].device_id, "device-a");
        assert!(response.staged_bundles.is_empty());
        assert!(response.transfer_status.is_none());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_devices_list_includes_staged_bundles_with_bundle_scope() {
        let dir = unique_bundle_temp_dir("local-bridge-devices-with-bundle-scope");
        let staging_root = dir.join("bundle_staging");
        let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
        nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();
        let trusted = vec![trusted_record("device-a", "MacBook", "sha256:device-a")];
        let request = serde_json::json!({
            "kind": "devices.list",
            "payload": {
                "request_id": "bridge-request-1",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "trusted_only": true
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_auth_at(
            &request,
            &trusted,
            None,
            &staging_root,
            &dir.join("bundle_imports"),
            &[local_bridge_authorization(
                "local-agent-app",
                &[
                    LocalBridgePermissionScope::DeviceRead,
                    LocalBridgePermissionScope::BundleRead,
                ],
                1_000,
                5_000,
            )],
            2_000,
        )
        .unwrap();

        assert_eq!(response.request_id, "bridge-request-1");
        assert_eq!(response.status, "ok");
        assert_eq!(response.devices.len(), 1);
        assert_eq!(response.devices[0].device_id, "device-a");
        assert_eq!(response.staged_bundles.len(), 1);
        assert_eq!(response.staged_bundles[0].bundle_id, "bundle_1234567890");
        assert!(response.staged_bundles[0].staging_path.is_empty());
        assert!(response.staged_bundles[0].import_destination.is_none());
        assert!(response.transfer_status.is_none());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_read_only_requests_require_matching_scope() {
        let dir = unique_bundle_temp_dir("local-bridge-read-only-security");
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

        let response = handle_local_bridge_request_with_auth_at(
            &request,
            &[],
            None,
            &staging_root,
            &dir.join("bundle_imports"),
            &[local_bridge_authorization(
                "local-agent-app",
                &[LocalBridgePermissionScope::TransferStatusRead],
                1_000,
                5_000,
            )],
            2_000,
        )
        .unwrap();

        assert_eq!(response.status, "ok");
        assert_eq!(response.security_state, "read_only");
        assert!(!response.requires_user_confirmation);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_read_only_requests_without_scope_require_authorization() {
        let dir = unique_bundle_temp_dir("local-bridge-read-only-requires-scope");
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

        let response = handle_local_bridge_request_with_auth_at(
            &request,
            &[],
            None,
            &staging_root,
            &dir.join("bundle_imports"),
            &[local_bridge_authorization(
                "local-agent-app",
                &[LocalBridgePermissionScope::DeviceRead],
                1_000,
                5_000,
            )],
            2_000,
        )
        .unwrap();

        assert_eq!(response.status, "pending_auth");
        assert_eq!(response.security_state, "requires_user_confirmation");
        assert!(response.requires_user_confirmation);
        assert!(response.transfer_status.is_none());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_unauthorized_read_only_response_marks_anonymous_client() {
        let dir = unique_bundle_temp_dir("local-bridge-client-anonymous-pending");
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

        assert_eq!(response.status, "pending_auth");
        assert_eq!(response.client_state, "anonymous");
        assert!(response.client_id.is_none());
        assert!(response.client_display_name.is_none());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_authorized_read_only_response_echoes_identified_client() {
        let dir = unique_bundle_temp_dir("local-bridge-client-read-identified");
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

        let response = handle_local_bridge_request_with_auth_at(
            &request,
            &[],
            None,
            &staging_root,
            &dir.join("bundle_imports"),
            &[local_bridge_authorization(
                "local-agent-app",
                &[LocalBridgePermissionScope::TransferStatusRead],
                1_000,
                5_000,
            )],
            2_000,
        )
        .unwrap();

        assert_eq!(response.client_state, "identified");
        assert_eq!(response.client_id.as_deref(), Some("local-agent-app"));
        assert_eq!(
            response.client_display_name.as_deref(),
            Some("Local Agent App")
        );

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

        let response = handle_local_bridge_request_with_auth_at(
            &request,
            &[],
            None,
            &staging_root,
            &dir.join("bundle_imports"),
            &[local_bridge_authorization(
                "local-agent-app",
                &[LocalBridgePermissionScope::TransferStatusRead],
                1_000,
                5_000,
            )],
            2_000,
        )
        .unwrap();

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
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "staged_bundle_id": "bundle_1234567890"
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_auth_at(
            &request,
            &[],
            None,
            &staging_root,
            &dir.join("bundle_imports"),
            &[local_bridge_authorization(
                "local-agent-app",
                &[LocalBridgePermissionScope::BundleRead],
                1_000,
                5_000,
            )],
            2_000,
        )
        .unwrap();

        assert_eq!(response.request_id, "bridge-request-detail");
        assert_eq!(response.status, "ok");
        assert_eq!(response.security_state, "read_only");
        assert_eq!(response.staged_bundles.len(), 1);
        assert_eq!(response.staged_bundles[0].bundle_id, "bundle_1234567890");
        assert!(response.staged_bundles[0].staging_path.is_empty());
        assert!(response.staged_bundles[0].import_destination.is_none());
        assert!(response.staged_bundles[0].import_receipt_path.is_none());
        assert!(!response.staged_bundles[0].has_import_receipt);
        assert!(!response.staged_bundles[0].can_request_rollback);
        assert!(response.staged_bundles[0]
            .import_plan_files
            .iter()
            .all(|file| file.destination_path.is_empty()));
        assert!(!response.requires_user_confirmation);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_bundle_detail_requires_bundle_read_scope() {
        let dir = unique_bundle_temp_dir("local-bridge-bundle-detail-scope");
        let staging_root = dir.join("bundle_staging");
        let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
        nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();
        let request = serde_json::json!({
            "kind": "bundle.detail",
            "payload": {
                "request_id": "bridge-request-detail-no-scope",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "staged_bundle_id": "bundle_1234567890"
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_auth_at(
            &request,
            &[],
            None,
            &staging_root,
            &dir.join("bundle_imports"),
            &[local_bridge_authorization(
                "local-agent-app",
                &[LocalBridgePermissionScope::BundleSend],
                1_000,
                5_000,
            )],
            2_000,
        )
        .unwrap();

        assert_eq!(response.request_id, "bridge-request-detail-no-scope");
        assert_eq!(response.status, "pending_auth");
        assert!(response.requires_user_confirmation);
        assert!(response.staged_bundles.is_empty());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_bundle_detail_returns_imported_status_without_local_paths() {
        let dir = unique_bundle_temp_dir("local-bridge-bundle-detail-imported");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
        let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
        nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();
        import_staged_bundle_at(&staging_root, &import_root, "bundle_1234567890").unwrap();
        let request = serde_json::json!({
            "kind": "bundle.detail",
            "payload": {
                "request_id": "bridge-request-detail-imported",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "staged_bundle_id": "bundle_1234567890"
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_auth_at(
            &request,
            &[],
            None,
            &staging_root,
            &import_root,
            &[local_bridge_authorization(
                "local-agent-app",
                &[LocalBridgePermissionScope::BundleRead],
                1_000,
                5_000,
            )],
            2_000,
        )
        .unwrap();

        assert_eq!(response.request_id, "bridge-request-detail-imported");
        assert_eq!(response.status, "ok");
        assert_eq!(response.security_state, "read_only");
        assert_eq!(response.staged_bundles.len(), 1);
        let bundle = &response.staged_bundles[0];
        assert_eq!(bundle.bundle_id, "bundle_1234567890");
        assert_eq!(bundle.staging_status, "imported");
        assert!(bundle.staging_path.is_empty());
        assert!(bundle.import_path.is_none());
        assert!(bundle.import_destination.is_none());
        assert!(bundle.import_receipt_path.is_none());
        assert!(bundle.has_import_receipt);
        assert_eq!(bundle.rollback_file_count, 2);
        assert!(bundle.can_rollback_now);
        assert!(bundle.can_request_rollback);
        assert!(bundle.rollback_blocking_reason.is_none());
        assert_eq!(bundle.rolled_back_file_count, 0);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_bundle_detail_returns_rolled_back_status_without_local_paths() {
        let dir = unique_bundle_temp_dir("local-bridge-bundle-detail-rolled-back");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
        let bundle_root = create_desktop_test_bundle(&dir, "source", "bundle_1234567890");
        nekodrop_storage::stage_bundle_directory(&bundle_root, &staging_root).unwrap();
        import_staged_bundle_at(&staging_root, &import_root, "bundle_1234567890").unwrap();
        rollback_imported_bundle_at(&import_root, "bundle_1234567890").unwrap();
        let request = serde_json::json!({
            "kind": "bundle.detail",
            "payload": {
                "request_id": "bridge-request-detail-rolled-back",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "staged_bundle_id": "bundle_1234567890"
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_auth_at(
            &request,
            &[],
            None,
            &staging_root,
            &import_root,
            &[local_bridge_authorization(
                "local-agent-app",
                &[LocalBridgePermissionScope::BundleRead],
                1_000,
                5_000,
            )],
            2_000,
        )
        .unwrap();

        assert_eq!(response.request_id, "bridge-request-detail-rolled-back");
        assert_eq!(response.status, "ok");
        assert_eq!(response.staged_bundles.len(), 1);
        let bundle = &response.staged_bundles[0];
        assert_eq!(bundle.bundle_id, "bundle_1234567890");
        assert_eq!(bundle.staging_status, "rolled_back");
        assert!(bundle.staging_path.is_empty());
        assert!(bundle.import_path.is_none());
        assert!(bundle.import_destination.is_none());
        assert!(bundle.import_receipt_path.is_none());
        assert!(bundle.has_import_receipt);
        assert_eq!(bundle.rollback_file_count, 2);
        assert!(!bundle.can_rollback_now);
        assert!(!bundle.can_request_rollback);
        assert_eq!(
            bundle.rollback_blocking_reason.as_deref(),
            Some("destination_missing")
        );
        assert_eq!(bundle.rolled_back_file_count, 2);

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
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "staged_bundle_id": "bundle_1234567890"
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_auth_at(
            &request,
            &[],
            None,
            &staging_root,
            &dir.join("bundle_imports"),
            &[local_bridge_authorization(
                "local-agent-app",
                &[LocalBridgePermissionScope::BundleRead],
                1_000,
                5_000,
            )],
            2_000,
        )
        .unwrap();

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
        let import_root = dir.join("bundle_imports");
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
            &import_root,
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
        let import_root = dir.join("bundle_imports");
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
            &import_root,
            &authorizations,
            1_500,
        )
        .unwrap();

        assert_eq!(response.status, "pending_auth");
        assert_eq!(response.security_state, "requires_user_confirmation");

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_authorization_is_bound_to_client_app_kind() {
        let dir = unique_bundle_temp_dir("local-bridge-app-kind-auth");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
        let authorizations = vec![local_bridge_authorization(
            "local-agent-app",
            &[LocalBridgePermissionScope::BundleImportRequest],
            1_000,
            5_000,
        )];
        let import_request = serde_json::json!({
            "kind": "bundle.import",
            "payload": {
                "request_id": "bridge-request-import",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "automation"
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
            &import_root,
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
        let import_root = dir.join("bundle_imports");
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
            &import_root,
            &runtime,
            false,
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
        let import_root = dir.join("bundle_imports");
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
            &import_root,
            &runtime,
            false,
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
            &import_root,
            &runtime,
            false,
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
        let import_root = dir.join("bundle_imports");
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
            &import_root,
            &runtime,
            false,
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
        drop(actions);
        let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].request_id, "bridge-request-send");
        assert_eq!(results[0].status, "queued");
        assert_eq!(results[0].lifecycle_status.as_deref(), Some("queued"));
        assert_eq!(results[0].bundle_type.as_deref(), Some("skill"));
        assert_eq!(results[0].target_device_id.as_deref(), Some("device-a"));
        assert!(results[0].bundle_root.is_none());
        let events = runtime.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            nekolink_protocol::LocalBridgeEvent::ActionUpdated(event) => {
                assert_eq!(event.request_id, "bridge-request-send");
                assert_eq!(
                    event.status,
                    nekolink_protocol::LocalBridgeActionLifecycleStatus::Queued
                );
                assert_eq!(event.bundle_type, Some(BundleType::Skill));
                assert_eq!(event.target_device_id.as_deref(), Some("device-a"));
            }
            other => panic!("expected action.updated event, got {other:?}"),
        }
        assert_eq!(
            runtime.authorizations.lock().unwrap()[0].last_used_at_ms,
            1_500
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unauthorized_local_bridge_request_does_not_update_last_used_at() {
        let dir = unique_bundle_temp_dir("local-bridge-unauthorized-last-used");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
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
                    "app_kind": "automation"
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
            &import_root,
            &runtime,
            false,
            1_500,
        )
        .unwrap();

        assert_eq!(response.status, "pending_auth");
        assert_eq!(
            runtime.authorizations.lock().unwrap()[0].last_used_at_ms,
            1_000
        );
        assert!(runtime.pending_actions.lock().unwrap().is_empty());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn authorized_local_bridge_bundle_import_is_queued_as_pending_action() {
        let dir = unique_bundle_temp_dir("local-bridge-runtime-pending-import-action");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
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
            &import_root,
            &runtime,
            false,
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
        drop(actions);
        let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].request_id, "bridge-request-import");
        assert_eq!(results[0].status, "queued");
        assert_eq!(results[0].bundle_id.as_deref(), Some("bundle_1234567890"));
        assert_eq!(results[0].bundle_type.as_deref(), Some("skill"));
        assert!(results[0].import_receipt_path.is_none());
        let events = runtime.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            nekolink_protocol::LocalBridgeEvent::ActionUpdated(event) => {
                assert_eq!(event.request_id, "bridge-request-import");
                assert_eq!(event.bundle_id.as_deref(), Some("bundle_1234567890"));
                assert_eq!(event.bundle_type, Some(BundleType::Skill));
            }
            other => panic!("expected action.updated event, got {other:?}"),
        }

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn authorized_local_bridge_bundle_rollback_is_queued_as_pending_action() {
        let dir = unique_bundle_temp_dir("local-bridge-runtime-pending-rollback-action");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
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
        let rollback_request = serde_json::json!({
            "kind": "bundle.rollback",
            "payload": {
                "request_id": "bridge-request-rollback",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "bundle_id": "bundle_1234567890"
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_runtime_at(
            &rollback_request,
            &[],
            None,
            &staging_root,
            &import_root,
            &runtime,
            false,
            1_500,
        )
        .unwrap();

        assert_eq!(response.status, "pending_runtime");
        let actions = runtime.pending_actions.lock().unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            crate::app_state::LocalBridgePendingAction::RollbackBundleImport(action) => {
                assert_eq!(action.request_id, "bridge-request-rollback");
                assert_eq!(action.client.client_id, "local-agent-app");
                assert_eq!(action.bundle_id, "bundle_1234567890");
                assert_eq!(action.requested_at_ms, 1_500);
            }
            other => panic!("expected rollback bundle action, got {other:?}"),
        }
        drop(actions);
        let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].request_id, "bridge-request-rollback");
        assert_eq!(results[0].status, "queued");
        assert_eq!(results[0].bundle_id.as_deref(), Some("bundle_1234567890"));
        let events = runtime.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            nekolink_protocol::LocalBridgeEvent::ActionUpdated(event) => {
                assert_eq!(event.request_id, "bridge-request-rollback");
                assert_eq!(event.bundle_id.as_deref(), Some("bundle_1234567890"));
                assert_eq!(event.bundle_type, None);
            }
            other => panic!("expected action.updated event, got {other:?}"),
        }

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unauthorized_local_bridge_bundle_mutation_is_not_queued() {
        let dir = unique_bundle_temp_dir("local-bridge-runtime-no-pending-action");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
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
            &import_root,
            &runtime,
            false,
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
                conflict_strategy: "reject".to_string(),
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
                conflict_strategy: "reject".to_string(),
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
        let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].request_id, "bridge-send-1");
        assert_eq!(results[0].status, "cancelled");
        assert_eq!(results[0].lifecycle_status.as_deref(), Some("cancelled"));
        assert!(results[0].bundle_root.is_none());
        let events = runtime.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            nekolink_protocol::LocalBridgeEvent::ActionUpdated(event) => {
                assert_eq!(event.request_id, "bridge-send-1");
                assert_eq!(
                    event.status,
                    nekolink_protocol::LocalBridgeActionLifecycleStatus::Cancelled
                );
            }
            other => panic!("expected action.updated event, got {other:?}"),
        }
    }

    #[test]
    fn local_bridge_pending_action_consumer_takes_next_action_fifo() {
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
                bundle_root: "/tmp/exported/bundle-a".to_string(),
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
                conflict_strategy: "reject".to_string(),
                requested_at_ms: 1_600,
            }),
        ]);

        let claimed = take_next_local_bridge_pending_action_at(&runtime)
            .unwrap()
            .expect("first action should be claimed");
        let remaining = list_local_bridge_pending_actions_at(&runtime).unwrap();

        assert_eq!(claimed.request_id, "bridge-send-1");
        assert_eq!(claimed.action_kind, "bundle.send");
        assert_eq!(
            claimed.bundle_root.as_deref(),
            Some("/tmp/exported/bundle-a")
        );
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].request_id, "bridge-import-1");
    }

    #[test]
    fn local_bridge_pending_action_consumer_returns_none_for_empty_queue() {
        let runtime = LocalBridgeRuntimeState::default();

        let claimed = take_next_local_bridge_pending_action_at(&runtime).unwrap();

        assert!(claimed.is_none());
    }

    #[test]
    fn local_bridge_bundle_import_execution_imports_staged_bundle_and_records_result() {
        let dir = unique_bundle_temp_dir("local-bridge-import-execution");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
        create_desktop_test_bundle(&staging_root, "bundle_1234567890", "bundle_1234567890");
        let runtime = LocalBridgeRuntimeState::default();
        runtime
            .pending_actions
            .lock()
            .unwrap()
            .push(LocalBridgePendingAction::ImportBundle(
                LocalBridgePendingImportBundleAction {
                    request_id: "bridge-import-1".to_string(),
                    client: LocalBridgeClientIdentity {
                        client_id: "local-agent-app".to_string(),
                        display_name: "Local Agent App".to_string(),
                        app_kind: Some("agent".to_string()),
                    },
                    staged_bundle_id: "bundle_1234567890".to_string(),
                    expected_bundle_type: Some(BundleType::Skill),
                    conflict_strategy: "reject".to_string(),
                    requested_at_ms: 1_500,
                },
            ));

        let result = execute_next_local_bridge_bundle_import_at(
            &runtime,
            &staging_root,
            &import_root,
            2_000,
        )
        .unwrap()
        .expect("pending bundle.import action should be executed");
        let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();

        assert_eq!(result.request_id, "bridge-import-1");
        assert_eq!(result.action_kind, "bundle.import");
        assert_eq!(result.status, "completed");
        assert_eq!(result.lifecycle_status.as_deref(), Some("succeeded"));
        assert_eq!(result.bundle_id.as_deref(), Some("bundle_1234567890"));
        assert_eq!(result.bundle_type.as_deref(), Some("skill"));
        assert_eq!(result.requested_at_ms, 1_500);
        assert_eq!(result.claimed_at_ms, 2_000);
        assert!(result.bundle_root.is_none());
        assert!(runtime.pending_actions.lock().unwrap().is_empty());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].request_id, "bridge-import-1");
        assert_eq!(results[0].status, "completed");
        assert_eq!(results[0].lifecycle_status.as_deref(), Some("succeeded"));
        assert!(results[0].bundle_root.is_none());
        assert!(import_root
            .join("bundle_1234567890")
            .join("content.bin")
            .exists());
        let events = runtime.events.lock().unwrap();
        let statuses: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                nekolink_protocol::LocalBridgeEvent::ActionUpdated(event) => Some(event.status),
                _ => None,
            })
            .collect();
        assert_eq!(
            statuses,
            vec![
                nekolink_protocol::LocalBridgeActionLifecycleStatus::Running,
                nekolink_protocol::LocalBridgeActionLifecycleStatus::Succeeded,
            ]
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_bundle_rollback_execution_removes_imported_files_and_records_result() {
        let dir = unique_bundle_temp_dir("local-bridge-rollback-execution");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
        create_desktop_test_bundle(&staging_root, "bundle_1234567890", "bundle_1234567890");
        let imported =
            import_staged_bundle_at(&staging_root, &import_root, "bundle_1234567890").unwrap();
        assert!(
            std::path::Path::new(imported.import_path.as_deref().unwrap())
                .join("content.bin")
                .exists()
        );
        let action = LocalBridgePendingRollbackBundleImportAction {
            request_id: "bridge-rollback-1".to_string(),
            client: LocalBridgeClientIdentity {
                client_id: "local-agent-app".to_string(),
                display_name: "Local Agent App".to_string(),
                app_kind: Some("agent".to_string()),
            },
            bundle_id: "bundle_1234567890".to_string(),
            requested_at_ms: 1_500,
        };
        let result =
            execute_local_bridge_bundle_rollback_action(action, &import_root, 2_000).unwrap();

        assert_eq!(result.request_id, "bridge-rollback-1");
        assert_eq!(result.action_kind, "bundle.rollback");
        assert_eq!(result.status, "completed");
        assert_eq!(result.lifecycle_status.as_deref(), Some("succeeded"));
        assert_eq!(result.bundle_id.as_deref(), Some("bundle_1234567890"));
        assert_eq!(result.rolled_back_file_count, 2);
        assert!(result.rollback_blocking_reason.is_none());
        assert!(!std::path::Path::new(imported.import_path.as_deref().unwrap()).exists());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_bundle_rollback_execution_records_blocking_reason() {
        let dir = unique_bundle_temp_dir("local-bridge-rollback-blocked");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
        create_desktop_test_bundle(&staging_root, "bundle_1234567890", "bundle_1234567890");
        let imported =
            import_staged_bundle_at(&staging_root, &import_root, "bundle_1234567890").unwrap();
        fs::remove_file(
            std::path::Path::new(imported.import_path.as_deref().unwrap()).join("content.bin"),
        )
        .unwrap();
        let action = LocalBridgePendingRollbackBundleImportAction {
            request_id: "bridge-rollback-blocked-1".to_string(),
            client: LocalBridgeClientIdentity {
                client_id: "local-agent-app".to_string(),
                display_name: "Local Agent App".to_string(),
                app_kind: Some("agent".to_string()),
            },
            bundle_id: "bundle_1234567890".to_string(),
            requested_at_ms: 1_500,
        };
        let result =
            execute_local_bridge_bundle_rollback_action(action, &import_root, 2_000).unwrap();

        assert_eq!(result.request_id, "bridge-rollback-blocked-1");
        assert_eq!(result.action_kind, "bundle.rollback");
        assert_eq!(result.status, "failed");
        assert_eq!(result.lifecycle_status.as_deref(), Some("failed"));
        assert_eq!(result.reason.as_deref(), Some("bundle_rollback_blocked"));
        assert_eq!(
            result.rollback_blocking_reason.as_deref(),
            Some("imported_file_missing")
        );
        assert_eq!(result.rolled_back_file_count, 0);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_bundle_import_execution_rejects_expected_type_mismatch() {
        let dir = unique_bundle_temp_dir("local-bridge-import-execution-type-mismatch");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
        create_desktop_test_bundle(&staging_root, "bundle_1234567890", "bundle_1234567890");
        let runtime = LocalBridgeRuntimeState::default();
        runtime
            .pending_actions
            .lock()
            .unwrap()
            .push(LocalBridgePendingAction::ImportBundle(
                LocalBridgePendingImportBundleAction {
                    request_id: "bridge-import-type".to_string(),
                    client: LocalBridgeClientIdentity {
                        client_id: "local-agent-app".to_string(),
                        display_name: "Local Agent App".to_string(),
                        app_kind: Some("agent".to_string()),
                    },
                    staged_bundle_id: "bundle_1234567890".to_string(),
                    expected_bundle_type: Some(BundleType::Workspace),
                    conflict_strategy: "reject".to_string(),
                    requested_at_ms: 1_500,
                },
            ));

        let result = execute_next_local_bridge_bundle_import_at(
            &runtime,
            &staging_root,
            &import_root,
            2_000,
        )
        .unwrap()
        .expect("pending bundle.import action should be consumed");
        let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();

        assert_eq!(result.status, "failed");
        assert_eq!(result.reason.as_deref(), Some("bundle_type_mismatch"));
        assert_eq!(result.bundle_id.as_deref(), Some("bundle_1234567890"));
        assert_eq!(result.bundle_type.as_deref(), Some("skill"));
        assert!(runtime.pending_actions.lock().unwrap().is_empty());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].request_id, "bridge-import-type");
        assert_eq!(results[0].status, "failed");
        assert_eq!(results[0].reason.as_deref(), Some("bundle_type_mismatch"));
        assert!(!import_root.join("bundle_1234567890").exists());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_bundle_import_execution_reports_name_conflict() {
        let dir = unique_bundle_temp_dir("local-bridge-import-execution-conflict");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
        create_desktop_test_bundle(&staging_root, "bundle_1234567890", "bundle_1234567890");
        fs::create_dir_all(import_root.join("bundle_1234567890")).unwrap();
        let runtime = LocalBridgeRuntimeState::default();
        runtime
            .pending_actions
            .lock()
            .unwrap()
            .push(LocalBridgePendingAction::ImportBundle(
                LocalBridgePendingImportBundleAction {
                    request_id: "bridge-import-conflict".to_string(),
                    client: LocalBridgeClientIdentity {
                        client_id: "local-agent-app".to_string(),
                        display_name: "Local Agent App".to_string(),
                        app_kind: Some("agent".to_string()),
                    },
                    staged_bundle_id: "bundle_1234567890".to_string(),
                    expected_bundle_type: Some(BundleType::Skill),
                    conflict_strategy: "reject".to_string(),
                    requested_at_ms: 1_500,
                },
            ));

        let result = execute_next_local_bridge_bundle_import_at(
            &runtime,
            &staging_root,
            &import_root,
            2_000,
        )
        .unwrap()
        .expect("pending bundle.import action should be consumed");

        assert_eq!(result.status, "failed");
        assert_eq!(result.reason.as_deref(), Some("bundle_import_conflict"));
        assert_eq!(result.lifecycle_status.as_deref(), Some("conflict"));
        assert!(!import_root.join("bundle_1234567890.importing").exists());
        let events = runtime.events.lock().unwrap();
        let last_status = events.iter().rev().find_map(|event| match event {
            nekolink_protocol::LocalBridgeEvent::ActionUpdated(event) => Some(event.status),
            _ => None,
        });
        assert_eq!(
            last_status,
            Some(nekolink_protocol::LocalBridgeActionLifecycleStatus::Conflict)
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_bundle_import_execution_skips_non_import_queue_head() {
        let dir = unique_bundle_temp_dir("local-bridge-import-execution-skip");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
        let runtime = LocalBridgeRuntimeState::default();
        runtime
            .pending_actions
            .lock()
            .unwrap()
            .push(LocalBridgePendingAction::SendBundle(
                LocalBridgePendingSendBundleAction {
                    request_id: "bridge-send-1".to_string(),
                    client: LocalBridgeClientIdentity {
                        client_id: "local-agent-app".to_string(),
                        display_name: "Local Agent App".to_string(),
                        app_kind: Some("agent".to_string()),
                    },
                    target_device_id: Some("device-a".to_string()),
                    bundle_root: "/tmp/exported/bundle".to_string(),
                    bundle_type: BundleType::Skill,
                    require_trusted_device: true,
                    requested_at_ms: 1_500,
                },
            ));

        let result = execute_next_local_bridge_bundle_import_at(
            &runtime,
            &staging_root,
            &import_root,
            2_000,
        )
        .unwrap();
        let actions = list_local_bridge_pending_actions_at(&runtime).unwrap();
        let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();

        assert!(result.is_none());
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].request_id, "bridge-send-1");
        assert!(results.is_empty());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_bundle_send_preflight_accepts_valid_trusted_target() {
        let dir = unique_bundle_temp_dir("local-bridge-send-preflight-ready");
        let bundle_root = create_desktop_test_bundle(&dir, "bundle", "bundle_preflight_ready");
        let runtime = LocalBridgeRuntimeState::default();
        runtime
            .pending_actions
            .lock()
            .unwrap()
            .push(LocalBridgePendingAction::SendBundle(
                LocalBridgePendingSendBundleAction {
                    request_id: "bridge-send-1".to_string(),
                    client: LocalBridgeClientIdentity {
                        client_id: "local-agent-app".to_string(),
                        display_name: "Local Agent App".to_string(),
                        app_kind: Some("agent".to_string()),
                    },
                    target_device_id: Some("device-a".to_string()),
                    bundle_root: bundle_root.display().to_string(),
                    bundle_type: BundleType::Skill,
                    require_trusted_device: true,
                    requested_at_ms: 1_500,
                },
            ));
        let trusted = vec![trusted_record("device-a", "MacBook", "sha256:device-a")];

        let result = preflight_next_local_bridge_bundle_send_at(&runtime, &trusted, 2_000).unwrap();

        assert_eq!(result.status, "ready");
        assert_eq!(result.request_id.as_deref(), Some("bridge-send-1"));
        assert_eq!(result.bundle_id.as_deref(), Some("bundle_preflight_ready"));
        assert_eq!(result.bundle_type.as_deref(), Some("skill"));
        assert_eq!(result.target_device_id.as_deref(), Some("device-a"));
        assert_eq!(result.claimed_at_ms, Some(2_000));
        assert!(result.message.contains("ready"));
        assert!(runtime.pending_actions.lock().unwrap().is_empty());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_bundle_send_preflight_rejects_missing_bundle_root() {
        let runtime = LocalBridgeRuntimeState::default();
        runtime
            .pending_actions
            .lock()
            .unwrap()
            .push(LocalBridgePendingAction::SendBundle(
                LocalBridgePendingSendBundleAction {
                    request_id: "bridge-send-missing".to_string(),
                    client: LocalBridgeClientIdentity {
                        client_id: "local-agent-app".to_string(),
                        display_name: "Local Agent App".to_string(),
                        app_kind: Some("agent".to_string()),
                    },
                    target_device_id: Some("device-a".to_string()),
                    bundle_root: "/tmp/nekodrop-missing-bundle-root".to_string(),
                    bundle_type: BundleType::Skill,
                    require_trusted_device: false,
                    requested_at_ms: 1_500,
                },
            ));

        let result = preflight_next_local_bridge_bundle_send_at(&runtime, &[], 2_000).unwrap();

        assert_eq!(result.status, "failed_preflight");
        assert_eq!(result.request_id.as_deref(), Some("bridge-send-missing"));
        assert_eq!(result.reason.as_deref(), Some("bundle_root_missing"));
        assert!(result.message.contains("bundle_root"));
        assert!(runtime.pending_actions.lock().unwrap().is_empty());
    }

    #[test]
    fn local_bridge_bundle_send_preflight_rejects_bundle_type_mismatch() {
        let dir = unique_bundle_temp_dir("local-bridge-send-preflight-type-mismatch");
        let bundle_root = create_desktop_test_bundle(&dir, "bundle", "bundle_preflight_type");
        let runtime = LocalBridgeRuntimeState::default();
        runtime
            .pending_actions
            .lock()
            .unwrap()
            .push(LocalBridgePendingAction::SendBundle(
                LocalBridgePendingSendBundleAction {
                    request_id: "bridge-send-type".to_string(),
                    client: LocalBridgeClientIdentity {
                        client_id: "local-agent-app".to_string(),
                        display_name: "Local Agent App".to_string(),
                        app_kind: Some("agent".to_string()),
                    },
                    target_device_id: None,
                    bundle_root: bundle_root.display().to_string(),
                    bundle_type: BundleType::Workspace,
                    require_trusted_device: false,
                    requested_at_ms: 1_500,
                },
            ));

        let result = preflight_next_local_bridge_bundle_send_at(&runtime, &[], 2_000).unwrap();

        assert_eq!(result.status, "failed_preflight");
        assert_eq!(result.reason.as_deref(), Some("bundle_type_mismatch"));
        assert_eq!(result.bundle_id.as_deref(), Some("bundle_preflight_type"));
        assert_eq!(result.bundle_type.as_deref(), Some("skill"));
        assert!(runtime.pending_actions.lock().unwrap().is_empty());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_bundle_send_preflight_rejects_sensitive_bundles_without_trusted_session_target()
    {
        let dir = unique_bundle_temp_dir("local-bridge-send-preflight-sensitive-policy");
        let trusted = vec![trusted_record("device-a", "MacBook", "sha256:device-a")];

        for bundle_type in [
            BundleType::Skill,
            BundleType::Session,
            BundleType::Workspace,
            BundleType::AgentProfile,
        ] {
            let label = bundle_type_label(bundle_type);
            let bundle_id = format!("bundle_preflight_sensitive_{label}");
            let bundle_root = create_desktop_test_bundle_with_type(
                &dir,
                format!("bundle_{label}").as_str(),
                &bundle_id,
                bundle_type,
            );
            let runtime = LocalBridgeRuntimeState::default();
            runtime
                .pending_actions
                .lock()
                .unwrap()
                .push(LocalBridgePendingAction::SendBundle(
                    LocalBridgePendingSendBundleAction {
                        request_id: format!("bridge-send-sensitive-{label}"),
                        client: LocalBridgeClientIdentity {
                            client_id: "local-agent-app".to_string(),
                            display_name: "Local Agent App".to_string(),
                            app_kind: Some("agent".to_string()),
                        },
                        target_device_id: Some("device-a".to_string()),
                        bundle_root: bundle_root.display().to_string(),
                        bundle_type,
                        require_trusted_device: false,
                        requested_at_ms: 1_500,
                    },
                ));

            let result =
                preflight_next_local_bridge_bundle_send_at(&runtime, &trusted, 2_000).unwrap();

            assert_eq!(result.status, "failed_preflight");
            assert_eq!(
                result.reason.as_deref(),
                Some("sensitive_bundle_requires_trusted_device")
            );
            assert_eq!(result.bundle_id.as_deref(), Some(bundle_id.as_str()));
            assert_eq!(result.bundle_type.as_deref(), Some(label));
            assert!(runtime.pending_actions.lock().unwrap().is_empty());
        }

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_bundle_send_preflight_rejects_missing_trusted_target() {
        let dir = unique_bundle_temp_dir("local-bridge-send-preflight-untrusted-target");
        let bundle_root = create_desktop_test_bundle(&dir, "bundle", "bundle_preflight_trusted");
        let runtime = LocalBridgeRuntimeState::default();
        runtime
            .pending_actions
            .lock()
            .unwrap()
            .push(LocalBridgePendingAction::SendBundle(
                LocalBridgePendingSendBundleAction {
                    request_id: "bridge-send-untrusted".to_string(),
                    client: LocalBridgeClientIdentity {
                        client_id: "local-agent-app".to_string(),
                        display_name: "Local Agent App".to_string(),
                        app_kind: Some("agent".to_string()),
                    },
                    target_device_id: Some("device-a".to_string()),
                    bundle_root: bundle_root.display().to_string(),
                    bundle_type: BundleType::Skill,
                    require_trusted_device: true,
                    requested_at_ms: 1_500,
                },
            ));

        let result = preflight_next_local_bridge_bundle_send_at(&runtime, &[], 2_000).unwrap();

        assert_eq!(result.status, "failed_preflight");
        assert_eq!(result.reason.as_deref(), Some("trusted_target_missing"));
        assert_eq!(result.target_device_id.as_deref(), Some("device-a"));
        assert!(runtime.pending_actions.lock().unwrap().is_empty());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_worker_executes_bundle_send_action_and_records_result() {
        let dir = unique_bundle_temp_dir("local-bridge-worker-send");
        let bundle_root = create_desktop_test_bundle(&dir, "bundle", "bundle_worker_send");
        let runtime = LocalBridgeRuntimeState::default();
        runtime
            .pending_actions
            .lock()
            .unwrap()
            .push(LocalBridgePendingAction::SendBundle(
                LocalBridgePendingSendBundleAction {
                    request_id: "bridge-send-worker".to_string(),
                    client: LocalBridgeClientIdentity {
                        client_id: "local-agent-app".to_string(),
                        display_name: "Local Agent App".to_string(),
                        app_kind: Some("agent".to_string()),
                    },
                    target_device_id: Some("device-a".to_string()),
                    bundle_root: bundle_root.display().to_string(),
                    bundle_type: BundleType::Skill,
                    require_trusted_device: true,
                    requested_at_ms: 1_500,
                },
            ));
        let trusted = vec![trusted_record("device-a", "MacBook", "sha256:device-a")];
        let mut sent_bundle_root = None;

        let result =
            execute_next_local_bridge_bundle_send_with(&runtime, &trusted, 2_000, |action| {
                sent_bundle_root = Some(action.bundle_root.clone());
                Ok(())
            })
            .unwrap()
            .expect("worker should execute queued bundle.send");
        let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();

        assert_eq!(
            sent_bundle_root.as_deref(),
            Some(bundle_root.to_str().unwrap())
        );
        assert_eq!(result.request_id, "bridge-send-worker");
        assert_eq!(result.status, "completed");
        assert_eq!(result.bundle_id.as_deref(), Some("bundle_worker_send"));
        assert!(result.bundle_root.is_none());
        assert!(runtime.pending_actions.lock().unwrap().is_empty());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, "completed");

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_bundle_send_preflight_records_result_without_sensitive_path() {
        let dir = unique_bundle_temp_dir("local-bridge-send-preflight-result-history");
        let bundle_root = create_desktop_test_bundle_with_type(
            &dir,
            "bundle",
            "bundle_preflight_result",
            BundleType::ConfigSnapshot,
        );
        let runtime = LocalBridgeRuntimeState::default();
        runtime
            .pending_actions
            .lock()
            .unwrap()
            .push(LocalBridgePendingAction::SendBundle(
                LocalBridgePendingSendBundleAction {
                    request_id: "bridge-send-result".to_string(),
                    client: LocalBridgeClientIdentity {
                        client_id: "local-agent-app".to_string(),
                        display_name: "Local Agent App".to_string(),
                        app_kind: Some("agent".to_string()),
                    },
                    target_device_id: None,
                    bundle_root: bundle_root.display().to_string(),
                    bundle_type: BundleType::ConfigSnapshot,
                    require_trusted_device: false,
                    requested_at_ms: 1_500,
                },
            ));

        let preflight = preflight_next_local_bridge_bundle_send_at(&runtime, &[], 2_000).unwrap();
        let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();

        assert_eq!(preflight.status, "ready");
        assert_eq!(preflight.bundle_type.as_deref(), Some("config_snapshot"));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].request_id, "bridge-send-result");
        assert_eq!(results[0].action_kind, "bundle.send");
        assert_eq!(results[0].status, "ready");
        assert_eq!(
            results[0].bundle_id.as_deref(),
            Some("bundle_preflight_result")
        );
        assert!(results[0].bundle_root.is_none());
        assert_eq!(results[0].claimed_at_ms, 2_000);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_bundle_send_preflight_records_failure_reason() {
        let runtime = LocalBridgeRuntimeState::default();
        runtime
            .pending_actions
            .lock()
            .unwrap()
            .push(LocalBridgePendingAction::SendBundle(
                LocalBridgePendingSendBundleAction {
                    request_id: "bridge-send-failed-result".to_string(),
                    client: LocalBridgeClientIdentity {
                        client_id: "local-agent-app".to_string(),
                        display_name: "Local Agent App".to_string(),
                        app_kind: Some("agent".to_string()),
                    },
                    target_device_id: Some("device-a".to_string()),
                    bundle_root: "/tmp/nekodrop-missing-bundle-result".to_string(),
                    bundle_type: BundleType::Skill,
                    require_trusted_device: false,
                    requested_at_ms: 1_500,
                },
            ));

        let preflight = preflight_next_local_bridge_bundle_send_at(&runtime, &[], 2_000).unwrap();
        let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();

        assert_eq!(preflight.status, "failed_preflight");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].request_id, "bridge-send-failed-result");
        assert_eq!(results[0].status, "failed_preflight");
        assert_eq!(results[0].reason.as_deref(), Some("bundle_root_missing"));
        assert!(results[0].message.contains("bundle_root"));
        assert!(results[0].bundle_root.is_none());
    }

    #[test]
    fn local_bridge_bundle_send_execution_records_completed_result_without_sensitive_path() {
        let dir = unique_bundle_temp_dir("local-bridge-send-execution-completed");
        let bundle_root = create_desktop_test_bundle(&dir, "bundle", "bundle_send_execution");
        let runtime = LocalBridgeRuntimeState::default();
        runtime
            .pending_actions
            .lock()
            .unwrap()
            .push(LocalBridgePendingAction::SendBundle(
                LocalBridgePendingSendBundleAction {
                    request_id: "bridge-send-execute".to_string(),
                    client: LocalBridgeClientIdentity {
                        client_id: "local-agent-app".to_string(),
                        display_name: "Local Agent App".to_string(),
                        app_kind: Some("agent".to_string()),
                    },
                    target_device_id: Some("device-a".to_string()),
                    bundle_root: bundle_root.display().to_string(),
                    bundle_type: BundleType::Skill,
                    require_trusted_device: true,
                    requested_at_ms: 1_500,
                },
            ));
        let trusted = vec![trusted_record("device-a", "MacBook", "sha256:device-a")];

        let result =
            execute_next_local_bridge_bundle_send_with(&runtime, &trusted, 2_000, |action| {
                assert_eq!(action.request_id, "bridge-send-execute");
                assert_eq!(action.bundle_root, bundle_root.display().to_string());
                Ok(())
            })
            .unwrap()
            .expect("pending bundle.send action should be executed");
        let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();

        assert_eq!(result.request_id, "bridge-send-execute");
        assert_eq!(result.action_kind, "bundle.send");
        assert_eq!(result.status, "completed");
        assert_eq!(result.lifecycle_status.as_deref(), Some("succeeded"));
        assert_eq!(result.bundle_id.as_deref(), Some("bundle_send_execution"));
        assert_eq!(result.bundle_type.as_deref(), Some("skill"));
        assert_eq!(result.target_device_id.as_deref(), Some("device-a"));
        assert_eq!(result.requested_at_ms, 1_500);
        assert_eq!(result.claimed_at_ms, 2_000);
        assert!(result.bundle_root.is_none());
        assert!(runtime.pending_actions.lock().unwrap().is_empty());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].request_id, "bridge-send-execute");
        assert_eq!(results[0].status, "completed");
        assert_eq!(results[0].lifecycle_status.as_deref(), Some("succeeded"));
        assert!(results[0].bundle_root.is_none());
        let events = runtime.events.lock().unwrap();
        let statuses: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                nekolink_protocol::LocalBridgeEvent::ActionUpdated(event) => Some(event.status),
                _ => None,
            })
            .collect();
        assert_eq!(
            statuses,
            vec![
                nekolink_protocol::LocalBridgeActionLifecycleStatus::Running,
                nekolink_protocol::LocalBridgeActionLifecycleStatus::Succeeded,
            ]
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_bundle_send_execution_requires_target_device() {
        let dir = unique_bundle_temp_dir("local-bridge-send-execution-target-required");
        let bundle_root = create_desktop_test_bundle(&dir, "bundle", "bundle_send_no_target");
        let runtime = LocalBridgeRuntimeState::default();
        runtime
            .pending_actions
            .lock()
            .unwrap()
            .push(LocalBridgePendingAction::SendBundle(
                LocalBridgePendingSendBundleAction {
                    request_id: "bridge-send-no-target".to_string(),
                    client: LocalBridgeClientIdentity {
                        client_id: "local-agent-app".to_string(),
                        display_name: "Local Agent App".to_string(),
                        app_kind: Some("agent".to_string()),
                    },
                    target_device_id: None,
                    bundle_root: bundle_root.display().to_string(),
                    bundle_type: BundleType::Skill,
                    require_trusted_device: false,
                    requested_at_ms: 1_500,
                },
            ));
        let mut called = false;

        let result = execute_next_local_bridge_bundle_send_with(&runtime, &[], 2_000, |_| {
            called = true;
            Ok(())
        })
        .unwrap()
        .expect("pending bundle.send action should be consumed");
        let results = list_local_bridge_pending_action_results_at(&runtime).unwrap();

        assert!(!called);
        assert_eq!(result.status, "failed");
        assert_eq!(result.lifecycle_status.as_deref(), Some("failed"));
        assert_eq!(
            result.reason.as_deref(),
            Some("sensitive_bundle_requires_trusted_device")
        );
        assert_eq!(result.bundle_id.as_deref(), Some("bundle_send_no_target"));
        assert_eq!(result.bundle_type.as_deref(), Some("skill"));
        assert!(result.bundle_root.is_none());
        assert!(runtime.pending_actions.lock().unwrap().is_empty());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].request_id, "bridge-send-no-target");
        assert_eq!(results[0].status, "failed");
        assert_eq!(results[0].lifecycle_status.as_deref(), Some("failed"));
        assert_eq!(
            results[0].reason.as_deref(),
            Some("sensitive_bundle_requires_trusted_device")
        );
        assert!(results[0].bundle_root.is_none());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn authorized_local_bridge_client_can_poll_bundle_send_preflight_events() {
        let runtime = LocalBridgeRuntimeState::default();
        let dir = unique_bundle_temp_dir("local-bridge-send-preflight-event");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
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
        runtime
            .pending_actions
            .lock()
            .unwrap()
            .push(LocalBridgePendingAction::SendBundle(
                LocalBridgePendingSendBundleAction {
                    request_id: "bridge-send-event".to_string(),
                    client: LocalBridgeClientIdentity {
                        client_id: "local-agent-app".to_string(),
                        display_name: "Local Agent App".to_string(),
                        app_kind: Some("agent".to_string()),
                    },
                    target_device_id: Some("device-a".to_string()),
                    bundle_root: "/tmp/nekodrop-missing-bundle-event".to_string(),
                    bundle_type: BundleType::Skill,
                    require_trusted_device: false,
                    requested_at_ms: 1_500,
                },
            ));
        preflight_next_local_bridge_bundle_send_at(&runtime, &[], 2_000).unwrap();
        let poll_request = serde_json::json!({
            "kind": "events.poll",
            "payload": {
                "request_id": "bridge-events-send",
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
            &import_root,
            &runtime,
            false,
            2_500,
        )
        .unwrap();

        assert_eq!(response.status, "ok");
        assert_eq!(response.events.len(), 1);
        assert_eq!(
            response.events[0]["kind"].as_str(),
            Some("bundle.send.preflight")
        );
        assert_eq!(
            response.events[0]["payload"]["request_id"].as_str(),
            Some("bridge-send-event")
        );
        assert_eq!(
            response.events[0]["payload"]["status"].as_str(),
            Some("failed_preflight")
        );
        assert!(response.events[0]["payload"].get("bundle_root").is_none());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn authorized_local_bridge_client_can_poll_action_updated_events_by_scope() {
        let dir = unique_bundle_temp_dir("local-bridge-action-updated-poll");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
        let runtime = LocalBridgeRuntimeState::default();
        runtime.authorizations.lock().unwrap().extend([
            local_bridge_authorization(
                "sender-app",
                &[LocalBridgePermissionScope::BundleSend],
                1_000,
                5_000,
            ),
            local_bridge_authorization(
                "importer-app",
                &[LocalBridgePermissionScope::BundleImportRequest],
                1_000,
                5_000,
            ),
        ]);
        push_local_bridge_runtime_event(
            &runtime,
            nekolink_protocol::LocalBridgeEvent::ActionUpdated(
                nekolink_protocol::LocalBridgeActionUpdatedEvent {
                    event_id: "bridge-action-send-running".to_string(),
                    request_id: "bridge-send-1".to_string(),
                    action_kind: "bundle.send".to_string(),
                    client_id: "sender-app".to_string(),
                    client_app_kind: Some("agent".to_string()),
                    status: nekolink_protocol::LocalBridgeActionLifecycleStatus::Running,
                    reason: None,
                    message: "send running".to_string(),
                    bundle_id: Some("bundle_send".to_string()),
                    bundle_type: Some(BundleType::Skill),
                    target_device_id: Some("device-a".to_string()),
                    updated_at_ms: 2_000,
                },
            ),
        )
        .unwrap();
        push_local_bridge_runtime_event(
            &runtime,
            nekolink_protocol::LocalBridgeEvent::ActionUpdated(
                nekolink_protocol::LocalBridgeActionUpdatedEvent {
                    event_id: "bridge-action-import-running".to_string(),
                    request_id: "bridge-import-1".to_string(),
                    action_kind: "bundle.import".to_string(),
                    client_id: "importer-app".to_string(),
                    client_app_kind: Some("agent".to_string()),
                    status: nekolink_protocol::LocalBridgeActionLifecycleStatus::Running,
                    reason: None,
                    message: "import running".to_string(),
                    bundle_id: Some("bundle_1234567890".to_string()),
                    bundle_type: Some(BundleType::Skill),
                    target_device_id: None,
                    updated_at_ms: 2_100,
                },
            ),
        )
        .unwrap();
        push_local_bridge_runtime_event(
            &runtime,
            nekolink_protocol::LocalBridgeEvent::ActionUpdated(
                nekolink_protocol::LocalBridgeActionUpdatedEvent {
                    event_id: "bridge-action-send-automation-running".to_string(),
                    request_id: "bridge-send-automation".to_string(),
                    action_kind: "bundle.send".to_string(),
                    client_id: "sender-app".to_string(),
                    client_app_kind: Some("automation".to_string()),
                    status: nekolink_protocol::LocalBridgeActionLifecycleStatus::Running,
                    reason: None,
                    message: "automation send running".to_string(),
                    bundle_id: Some("bundle_automation".to_string()),
                    bundle_type: Some(BundleType::Skill),
                    target_device_id: Some("device-a".to_string()),
                    updated_at_ms: 2_200,
                },
            ),
        )
        .unwrap();
        let poll_request = serde_json::json!({
            "kind": "events.poll",
            "payload": {
                "request_id": "bridge-events-action",
                "client": {
                    "client_id": "sender-app",
                    "display_name": "Sender App",
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
            &import_root,
            &runtime,
            false,
            2_500,
        )
        .unwrap();

        assert_eq!(response.status, "ok");
        assert_eq!(response.events.len(), 1);
        assert_eq!(response.events[0]["kind"].as_str(), Some("action.updated"));
        assert_eq!(
            response.events[0]["payload"]["request_id"].as_str(),
            Some("bridge-send-1")
        );
        assert_eq!(
            response.events[0]["payload"]["status"].as_str(),
            Some("running")
        );
        assert!(response.events[0]["payload"].get("bundle_root").is_none());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_event_poll_treats_hidden_cursor_as_missing() {
        let dir = unique_bundle_temp_dir("local-bridge-hidden-event-cursor");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
        let runtime = LocalBridgeRuntimeState::default();
        runtime.authorizations.lock().unwrap().extend([
            local_bridge_authorization(
                "sender-app",
                &[LocalBridgePermissionScope::BundleSend],
                1_000,
                5_000,
            ),
            local_bridge_authorization(
                "importer-app",
                &[LocalBridgePermissionScope::BundleImportRequest],
                1_000,
                5_000,
            ),
        ]);
        push_local_bridge_runtime_event(
            &runtime,
            nekolink_protocol::LocalBridgeEvent::ActionUpdated(
                nekolink_protocol::LocalBridgeActionUpdatedEvent {
                    event_id: "bridge-action-import-hidden".to_string(),
                    request_id: "bridge-import-1".to_string(),
                    action_kind: "bundle.import".to_string(),
                    client_id: "importer-app".to_string(),
                    client_app_kind: Some("agent".to_string()),
                    status: nekolink_protocol::LocalBridgeActionLifecycleStatus::Running,
                    reason: None,
                    message: "import running".to_string(),
                    bundle_id: Some("bundle_1234567890".to_string()),
                    bundle_type: Some(BundleType::Skill),
                    target_device_id: None,
                    updated_at_ms: 2_000,
                },
            ),
        )
        .unwrap();
        push_local_bridge_runtime_event(
            &runtime,
            nekolink_protocol::LocalBridgeEvent::ActionUpdated(
                nekolink_protocol::LocalBridgeActionUpdatedEvent {
                    event_id: "bridge-action-send-visible".to_string(),
                    request_id: "bridge-send-1".to_string(),
                    action_kind: "bundle.send".to_string(),
                    client_id: "sender-app".to_string(),
                    client_app_kind: Some("agent".to_string()),
                    status: nekolink_protocol::LocalBridgeActionLifecycleStatus::Running,
                    reason: None,
                    message: "send running".to_string(),
                    bundle_id: Some("bundle_send".to_string()),
                    bundle_type: Some(BundleType::Skill),
                    target_device_id: Some("device-a".to_string()),
                    updated_at_ms: 2_100,
                },
            ),
        )
        .unwrap();
        let poll_request = serde_json::json!({
            "kind": "events.poll",
            "payload": {
                "request_id": "bridge-events-hidden-cursor",
                "client": {
                    "client_id": "sender-app",
                    "display_name": "Sender App",
                    "app_kind": "agent"
                },
                "after_event_id": "bridge-action-import-hidden",
                "limit": 10
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_runtime_at(
            &poll_request,
            &[],
            None,
            &staging_root,
            &import_root,
            &runtime,
            false,
            2_500,
        )
        .unwrap();

        assert_eq!(response.status, "ok");
        assert!(response.events.is_empty());
        assert_eq!(response.events_cursor_state, "missing");
        assert_eq!(response.events_last_id, None);
        assert_eq!(response.events_next_after_id, None);
        assert!(!response.events_has_more);
        assert_eq!(
            response.events_visible_first_id.as_deref(),
            Some("bridge-action-send-visible")
        );
        assert_eq!(
            response.events_visible_last_id.as_deref(),
            Some("bridge-action-send-visible")
        );
        assert_eq!(response.events_visible_count, 1);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_event_poll_can_filter_action_updates_by_request_id() {
        let dir = unique_bundle_temp_dir("local-bridge-action-event-filter");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
        let runtime = LocalBridgeRuntimeState::default();
        runtime
            .authorizations
            .lock()
            .unwrap()
            .push(local_bridge_authorization(
                "sender-app",
                &[LocalBridgePermissionScope::BundleSend],
                1_000,
                5_000,
            ));
        push_local_bridge_runtime_event(
            &runtime,
            nekolink_protocol::LocalBridgeEvent::ActionUpdated(
                nekolink_protocol::LocalBridgeActionUpdatedEvent {
                    event_id: "bridge-action-send-a-running".to_string(),
                    request_id: "bridge-send-a".to_string(),
                    action_kind: "bundle.send".to_string(),
                    client_id: "sender-app".to_string(),
                    client_app_kind: Some("agent".to_string()),
                    status: nekolink_protocol::LocalBridgeActionLifecycleStatus::Running,
                    reason: None,
                    message: "send a running".to_string(),
                    bundle_id: Some("bundle_a".to_string()),
                    bundle_type: Some(BundleType::Skill),
                    target_device_id: Some("device-a".to_string()),
                    updated_at_ms: 2_000,
                },
            ),
        )
        .unwrap();
        push_local_bridge_runtime_event(
            &runtime,
            nekolink_protocol::LocalBridgeEvent::TransferUpdated(
                nekolink_protocol::LocalBridgeTransferUpdatedEvent {
                    event_id: "bridge-transfer-noise".to_string(),
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
            nekolink_protocol::LocalBridgeEvent::ActionUpdated(
                nekolink_protocol::LocalBridgeActionUpdatedEvent {
                    event_id: "bridge-action-send-b-running".to_string(),
                    request_id: "bridge-send-b".to_string(),
                    action_kind: "bundle.send".to_string(),
                    client_id: "sender-app".to_string(),
                    client_app_kind: Some("agent".to_string()),
                    status: nekolink_protocol::LocalBridgeActionLifecycleStatus::Running,
                    reason: None,
                    message: "send b running".to_string(),
                    bundle_id: Some("bundle_b".to_string()),
                    bundle_type: Some(BundleType::Skill),
                    target_device_id: Some("device-b".to_string()),
                    updated_at_ms: 2_100,
                },
            ),
        )
        .unwrap();
        let poll_request = serde_json::json!({
            "kind": "events.poll",
            "payload": {
                "request_id": "bridge-events-filter-action",
                "client": {
                    "client_id": "sender-app",
                    "display_name": "Sender App",
                    "app_kind": "agent"
                },
                "after_event_id": null,
                "action_request_id": "bridge-send-b",
                "limit": 10
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_runtime_at(
            &poll_request,
            &[],
            None,
            &staging_root,
            &import_root,
            &runtime,
            false,
            2_500,
        )
        .unwrap();

        assert_eq!(response.status, "ok");
        assert_eq!(response.events.len(), 1);
        assert_eq!(response.events[0]["kind"].as_str(), Some("action.updated"));
        assert_eq!(
            response.events[0]["payload"]["request_id"].as_str(),
            Some("bridge-send-b")
        );
        assert_eq!(
            response.events_last_id.as_deref(),
            Some("bridge-action-send-b-running")
        );
        assert_eq!(response.events_cursor_state, "ok");
        assert_eq!(
            response.events_visible_first_id.as_deref(),
            Some("bridge-action-send-b-running")
        );
        assert_eq!(
            response.events_visible_last_id.as_deref(),
            Some("bridge-action-send-b-running")
        );
        assert_eq!(response.events_visible_count, 1);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn authorized_local_bridge_client_can_poll_runtime_events() {
        let dir = unique_bundle_temp_dir("local-bridge-events-poll");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
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
            &import_root,
            &runtime,
            false,
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
    fn authorized_local_bridge_client_can_poll_action_results() {
        let dir = unique_bundle_temp_dir("local-bridge-action-results-poll");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
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
        runtime
            .pending_action_results
            .lock()
            .unwrap()
            .push(LocalBridgePendingActionResult {
                request_id: "bridge-import-1".to_string(),
                action_kind: "bundle.import".to_string(),
                client_id: "local-agent-app".to_string(),
                client_display_name: "Local Agent App".to_string(),
                client_app_kind: Some("agent".to_string()),
                status: "completed".to_string(),
                lifecycle_status: Some("succeeded".to_string()),
                reason: None,
                message: "local bridge staged bundle was imported".to_string(),
                bundle_id: Some("bundle_1234567890".to_string()),
                bundle_type: Some("skill".to_string()),
                bundle_root: None,
                target_device_id: None,
                require_trusted_device: None,
                conflict_strategy: None,
                skipped_file_count: 0,
                import_receipt_path: Some("/tmp/private/receipt.json".to_string()),
                rollback_file_count: 2,
                rollback_blocking_reason: None,
                rolled_back_file_count: 0,
                requested_at_ms: 1_500,
                claimed_at_ms: 2_000,
            });
        let request = serde_json::json!({
            "kind": "actions.results",
            "payload": {
                "request_id": "bridge-results-1",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "after_claimed_at_ms": null,
                "limit": 10
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_runtime_at(
            &request,
            &[],
            None,
            &staging_root,
            &import_root,
            &runtime,
            false,
            2_500,
        )
        .unwrap();

        assert_eq!(response.status, "ok");
        assert_eq!(response.security_state, "authorized");
        assert_eq!(response.events.len(), 0);
        assert!(response.staged_bundles.is_empty());
        assert!(response.transfer_status.is_none());
        assert_eq!(response.action_results.len(), 1);
        assert_eq!(response.action_results[0].request_id, "bridge-import-1");
        assert_eq!(response.action_results[0].action_kind, "bundle.import");
        assert_eq!(response.action_results[0].status, "completed");
        assert_eq!(
            response.action_results[0].bundle_id.as_deref(),
            Some("bundle_1234567890")
        );
        assert!(response.action_results[0].bundle_root.is_none());
        assert!(response.action_results[0].import_receipt_path.is_none());
        assert!(response.action_results[0].has_import_receipt);
        assert_eq!(response.action_results[0].rollback_file_count, 2);
        assert!(response.action_results[0].can_request_rollback);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_action_results_are_scoped_to_client_and_permission() {
        let dir = unique_bundle_temp_dir("local-bridge-action-results-scope");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
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
        runtime.pending_action_results.lock().unwrap().extend([
            LocalBridgePendingActionResult {
                request_id: "bridge-import-1".to_string(),
                action_kind: "bundle.import".to_string(),
                client_id: "local-agent-app".to_string(),
                client_display_name: "Local Agent App".to_string(),
                client_app_kind: Some("agent".to_string()),
                status: "completed".to_string(),
                lifecycle_status: Some("succeeded".to_string()),
                reason: None,
                message: "imported".to_string(),
                bundle_id: Some("bundle_1234567890".to_string()),
                bundle_type: Some("skill".to_string()),
                bundle_root: None,
                target_device_id: None,
                require_trusted_device: None,
                conflict_strategy: None,
                skipped_file_count: 0,
                import_receipt_path: None,
                rollback_file_count: 0,
                rollback_blocking_reason: None,
                rolled_back_file_count: 0,
                requested_at_ms: 1_500,
                claimed_at_ms: 2_000,
            },
            LocalBridgePendingActionResult {
                request_id: "bridge-send-1".to_string(),
                action_kind: "bundle.send".to_string(),
                client_id: "local-agent-app".to_string(),
                client_display_name: "Local Agent App".to_string(),
                client_app_kind: Some("agent".to_string()),
                status: "ready".to_string(),
                lifecycle_status: None,
                reason: None,
                message: "send ready".to_string(),
                bundle_id: Some("bundle_send".to_string()),
                bundle_type: Some("skill".to_string()),
                bundle_root: Some("/tmp/private/bundle".to_string()),
                target_device_id: Some("device-a".to_string()),
                require_trusted_device: Some(true),
                conflict_strategy: None,
                skipped_file_count: 0,
                import_receipt_path: None,
                rollback_file_count: 0,
                rollback_blocking_reason: None,
                rolled_back_file_count: 0,
                requested_at_ms: 1_600,
                claimed_at_ms: 2_100,
            },
            LocalBridgePendingActionResult {
                request_id: "bridge-import-automation".to_string(),
                action_kind: "bundle.import".to_string(),
                client_id: "local-agent-app".to_string(),
                client_display_name: "Local Automation App".to_string(),
                client_app_kind: Some("automation".to_string()),
                status: "completed".to_string(),
                lifecycle_status: Some("succeeded".to_string()),
                reason: None,
                message: "automation imported".to_string(),
                bundle_id: Some("bundle_automation".to_string()),
                bundle_type: Some("skill".to_string()),
                bundle_root: None,
                target_device_id: None,
                require_trusted_device: None,
                conflict_strategy: None,
                skipped_file_count: 0,
                import_receipt_path: None,
                rollback_file_count: 0,
                rollback_blocking_reason: None,
                rolled_back_file_count: 0,
                requested_at_ms: 1_650,
                claimed_at_ms: 2_150,
            },
            LocalBridgePendingActionResult {
                request_id: "bridge-import-other".to_string(),
                action_kind: "bundle.import".to_string(),
                client_id: "other-app".to_string(),
                client_display_name: "Other App".to_string(),
                client_app_kind: Some("agent".to_string()),
                status: "completed".to_string(),
                lifecycle_status: Some("succeeded".to_string()),
                reason: None,
                message: "other imported".to_string(),
                bundle_id: Some("bundle_other".to_string()),
                bundle_type: Some("skill".to_string()),
                bundle_root: None,
                target_device_id: None,
                require_trusted_device: None,
                conflict_strategy: None,
                skipped_file_count: 0,
                import_receipt_path: None,
                rollback_file_count: 0,
                rollback_blocking_reason: None,
                rolled_back_file_count: 0,
                requested_at_ms: 1_700,
                claimed_at_ms: 2_200,
            },
            LocalBridgePendingActionResult {
                request_id: "bridge-rollback-1".to_string(),
                action_kind: "bundle.rollback".to_string(),
                client_id: "local-agent-app".to_string(),
                client_display_name: "Local Agent App".to_string(),
                client_app_kind: Some("agent".to_string()),
                status: "completed".to_string(),
                lifecycle_status: Some("succeeded".to_string()),
                reason: None,
                message: "rollback completed".to_string(),
                bundle_id: Some("bundle_1234567890".to_string()),
                bundle_type: None,
                bundle_root: None,
                target_device_id: None,
                require_trusted_device: None,
                conflict_strategy: None,
                skipped_file_count: 0,
                import_receipt_path: None,
                rollback_file_count: 0,
                rollback_blocking_reason: None,
                rolled_back_file_count: 2,
                requested_at_ms: 1_800,
                claimed_at_ms: 2_300,
            },
        ]);
        let request = serde_json::json!({
            "kind": "actions.results",
            "payload": {
                "request_id": "bridge-results-scope",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "after_claimed_at_ms": 1_999,
                "limit": 10
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_runtime_at(
            &request,
            &[],
            None,
            &staging_root,
            &import_root,
            &runtime,
            false,
            2_500,
        )
        .unwrap();

        assert_eq!(response.status, "ok");
        assert_eq!(response.action_results.len(), 2);
        assert_eq!(response.action_results[0].request_id, "bridge-import-1");
        assert!(response.action_results[0].bundle_root.is_none());
        assert_eq!(response.action_results[1].request_id, "bridge-rollback-1");
        assert_eq!(response.action_results[1].action_kind, "bundle.rollback");
        assert_eq!(response.action_results[1].rolled_back_file_count, 2);

        let exact_request = serde_json::json!({
            "kind": "actions.results",
            "payload": {
                "request_id": "bridge-results-exact",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "action_request_id": "bridge-rollback-1",
                "after_claimed_at_ms": null,
                "limit": 10
            }
        })
        .to_string();
        let exact_response = handle_local_bridge_request_with_runtime_at(
            &exact_request,
            &[],
            None,
            &staging_root,
            &import_root,
            &runtime,
            false,
            2_500,
        )
        .unwrap();

        assert_eq!(exact_response.status, "ok");
        assert_eq!(exact_response.action_results.len(), 1);
        assert_eq!(
            exact_response.action_results[0].request_id,
            "bridge-rollback-1"
        );
        assert_eq!(
            exact_response.action_results[0].action_kind,
            "bundle.rollback"
        );
        assert_eq!(exact_response.action_results[0].rolled_back_file_count, 2);

        let automation_exact_request = serde_json::json!({
            "kind": "actions.results",
            "payload": {
                "request_id": "bridge-results-automation",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "action_request_id": "bridge-import-automation",
                "after_claimed_at_ms": null,
                "limit": 10
            }
        })
        .to_string();
        let automation_exact_response = handle_local_bridge_request_with_runtime_at(
            &automation_exact_request,
            &[],
            None,
            &staging_root,
            &import_root,
            &runtime,
            false,
            2_500,
        )
        .unwrap();

        assert_eq!(automation_exact_response.status, "ok");
        assert!(automation_exact_response.action_results.is_empty());

        let send_without_scope_request = serde_json::json!({
            "kind": "actions.results",
            "payload": {
                "request_id": "bridge-results-send-without-scope",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "action_request_id": "bridge-send-1",
                "after_claimed_at_ms": null,
                "limit": 10
            }
        })
        .to_string();
        let send_without_scope_response = handle_local_bridge_request_with_runtime_at(
            &send_without_scope_request,
            &[],
            None,
            &staging_root,
            &import_root,
            &runtime,
            false,
            2_500,
        )
        .unwrap();

        assert_eq!(send_without_scope_response.status, "ok");
        assert!(send_without_scope_response.action_results.is_empty());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_action_results_returns_pending_queue_status_for_exact_lookup() {
        let dir = unique_bundle_temp_dir("local-bridge-action-results-pending-lookup");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
        let runtime = LocalBridgeRuntimeState::default();
        runtime.authorizations.lock().unwrap().extend([
            local_bridge_authorization(
                "local-agent-app",
                &[
                    LocalBridgePermissionScope::BundleSend,
                    LocalBridgePermissionScope::BundleImportRequest,
                ],
                1_000,
                5_000,
            ),
            local_bridge_authorization(
                "other-app",
                &[LocalBridgePermissionScope::BundleSend],
                1_000,
                5_000,
            ),
            local_bridge_authorization(
                "import-only-app",
                &[LocalBridgePermissionScope::BundleImportRequest],
                1_000,
                5_000,
            ),
        ]);
        runtime.pending_actions.lock().unwrap().extend([
            LocalBridgePendingAction::SendBundle(LocalBridgePendingSendBundleAction {
                request_id: "bridge-send-pending".to_string(),
                client: LocalBridgeClientIdentity {
                    client_id: "local-agent-app".to_string(),
                    display_name: "Local Agent App".to_string(),
                    app_kind: Some("agent".to_string()),
                },
                target_device_id: Some("device-a".to_string()),
                bundle_root: "/tmp/private/bundle".to_string(),
                bundle_type: BundleType::Skill,
                require_trusted_device: true,
                requested_at_ms: 1_500,
            }),
            LocalBridgePendingAction::SendBundle(LocalBridgePendingSendBundleAction {
                request_id: "bridge-send-import-only".to_string(),
                client: LocalBridgeClientIdentity {
                    client_id: "import-only-app".to_string(),
                    display_name: "Import Only App".to_string(),
                    app_kind: Some("agent".to_string()),
                },
                target_device_id: Some("device-b".to_string()),
                bundle_root: "/tmp/private/import-only-bundle".to_string(),
                bundle_type: BundleType::Workspace,
                require_trusted_device: true,
                requested_at_ms: 1_600,
            }),
            LocalBridgePendingAction::ImportBundle(LocalBridgePendingImportBundleAction {
                request_id: "bridge-import-pending".to_string(),
                client: LocalBridgeClientIdentity {
                    client_id: "local-agent-app".to_string(),
                    display_name: "Local Agent App".to_string(),
                    app_kind: Some("agent".to_string()),
                },
                staged_bundle_id: "bundle_pending_import".to_string(),
                expected_bundle_type: Some(BundleType::Workspace),
                conflict_strategy: "rename".to_string(),
                requested_at_ms: 1_700,
            }),
            LocalBridgePendingAction::RollbackBundleImport(
                LocalBridgePendingRollbackBundleImportAction {
                    request_id: "bridge-rollback-pending".to_string(),
                    client: LocalBridgeClientIdentity {
                        client_id: "local-agent-app".to_string(),
                        display_name: "Local Agent App".to_string(),
                        app_kind: Some("agent".to_string()),
                    },
                    bundle_id: "bundle_pending_rollback".to_string(),
                    requested_at_ms: 1_800,
                },
            ),
        ]);
        let request = serde_json::json!({
            "kind": "actions.results",
            "payload": {
                "request_id": "bridge-results-pending",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "action_request_id": "bridge-send-pending",
                "after_claimed_at_ms": null,
                "limit": 10
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_runtime_at(
            &request,
            &[],
            None,
            &staging_root,
            &import_root,
            &runtime,
            false,
            2_500,
        )
        .unwrap();

        assert_eq!(response.status, "ok");
        assert_eq!(response.action_results.len(), 1);
        assert_eq!(response.action_results[0].request_id, "bridge-send-pending");
        assert_eq!(response.action_results[0].action_kind, "bundle.send");
        assert_eq!(response.action_results[0].status, "queued");
        assert_eq!(
            response.action_results[0].lifecycle_status.as_deref(),
            Some("queued")
        );
        assert_eq!(
            response.action_results[0].bundle_type.as_deref(),
            Some("skill")
        );
        assert_eq!(
            response.action_results[0].target_device_id.as_deref(),
            Some("device-a")
        );
        assert!(response.action_results[0].bundle_root.is_none());

        let import_request = serde_json::json!({
            "kind": "actions.results",
            "payload": {
                "request_id": "bridge-results-import-pending",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "action_request_id": "bridge-import-pending",
                "after_claimed_at_ms": null,
                "limit": 10
            }
        })
        .to_string();
        let import_response = handle_local_bridge_request_with_runtime_at(
            &import_request,
            &[],
            None,
            &staging_root,
            &import_root,
            &runtime,
            false,
            2_500,
        )
        .unwrap();
        assert_eq!(import_response.status, "ok");
        assert_eq!(import_response.action_results.len(), 1);
        assert_eq!(
            import_response.action_results[0].request_id,
            "bridge-import-pending"
        );
        assert_eq!(
            import_response.action_results[0].action_kind,
            "bundle.import"
        );
        assert_eq!(import_response.action_results[0].status, "queued");
        assert_eq!(
            import_response.action_results[0].bundle_id.as_deref(),
            Some("bundle_pending_import")
        );
        assert_eq!(
            import_response.action_results[0].bundle_type.as_deref(),
            Some("workspace")
        );
        assert_eq!(
            import_response.action_results[0]
                .conflict_strategy
                .as_deref(),
            Some("rename")
        );
        assert!(import_response.action_results[0]
            .import_receipt_path
            .is_none());

        let rollback_request = serde_json::json!({
            "kind": "actions.results",
            "payload": {
                "request_id": "bridge-results-rollback-pending",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "action_request_id": "bridge-rollback-pending",
                "after_claimed_at_ms": null,
                "limit": 10
            }
        })
        .to_string();
        let rollback_response = handle_local_bridge_request_with_runtime_at(
            &rollback_request,
            &[],
            None,
            &staging_root,
            &import_root,
            &runtime,
            false,
            2_500,
        )
        .unwrap();
        assert_eq!(rollback_response.status, "ok");
        assert_eq!(rollback_response.action_results.len(), 1);
        assert_eq!(
            rollback_response.action_results[0].request_id,
            "bridge-rollback-pending"
        );
        assert_eq!(
            rollback_response.action_results[0].action_kind,
            "bundle.rollback"
        );
        assert_eq!(rollback_response.action_results[0].status, "queued");
        assert_eq!(
            rollback_response.action_results[0].bundle_id.as_deref(),
            Some("bundle_pending_rollback")
        );

        let other_client_request = serde_json::json!({
            "kind": "actions.results",
            "payload": {
                "request_id": "bridge-results-other-client",
                "client": {
                    "client_id": "other-app",
                    "display_name": "Other App",
                    "app_kind": "agent"
                },
                "action_request_id": "bridge-send-pending",
                "after_claimed_at_ms": null,
                "limit": 10
            }
        })
        .to_string();
        let other_client_response = handle_local_bridge_request_with_runtime_at(
            &other_client_request,
            &[],
            None,
            &staging_root,
            &import_root,
            &runtime,
            false,
            2_500,
        )
        .unwrap();
        assert_eq!(other_client_response.status, "ok");
        assert!(other_client_response.action_results.is_empty());

        let wrong_scope_request = serde_json::json!({
            "kind": "actions.results",
            "payload": {
                "request_id": "bridge-results-wrong-scope",
                "client": {
                    "client_id": "import-only-app",
                    "display_name": "Import Only App",
                    "app_kind": "agent"
                },
                "action_request_id": "bridge-send-import-only",
                "after_claimed_at_ms": null,
                "limit": 10
            }
        })
        .to_string();
        let wrong_scope_response = handle_local_bridge_request_with_runtime_at(
            &wrong_scope_request,
            &[],
            None,
            &staging_root,
            &import_root,
            &runtime,
            false,
            2_500,
        )
        .unwrap();
        assert_eq!(wrong_scope_response.status, "ok");
        assert!(wrong_scope_response.action_results.is_empty());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_action_results_returns_running_lifecycle_status_for_exact_lookup() {
        let dir = unique_bundle_temp_dir("local-bridge-action-results-running-lookup");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
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
        runtime
            .pending_action_results
            .lock()
            .unwrap()
            .push(LocalBridgePendingActionResult {
                request_id: "bridge-import-running".to_string(),
                action_kind: "bundle.import".to_string(),
                client_id: "local-agent-app".to_string(),
                client_display_name: "Local Agent App".to_string(),
                client_app_kind: Some("agent".to_string()),
                status: "running".to_string(),
                lifecycle_status: Some("running".to_string()),
                reason: None,
                message: "local bridge bundle import is running".to_string(),
                bundle_id: Some("bundle_1234567890".to_string()),
                bundle_type: Some("skill".to_string()),
                bundle_root: None,
                target_device_id: None,
                require_trusted_device: None,
                conflict_strategy: Some("reject".to_string()),
                skipped_file_count: 0,
                import_receipt_path: None,
                rollback_file_count: 0,
                rollback_blocking_reason: None,
                rolled_back_file_count: 0,
                requested_at_ms: 1_500,
                claimed_at_ms: 2_000,
            });
        let request = serde_json::json!({
            "kind": "actions.results",
            "payload": {
                "request_id": "bridge-results-running",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "action_request_id": "bridge-import-running",
                "after_claimed_at_ms": null,
                "limit": 10
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_runtime_at(
            &request,
            &[],
            None,
            &staging_root,
            &import_root,
            &runtime,
            false,
            2_500,
        )
        .unwrap();

        assert_eq!(response.status, "ok");
        assert_eq!(response.action_results.len(), 1);
        assert_eq!(
            response.action_results[0].request_id,
            "bridge-import-running"
        );
        assert_eq!(response.action_results[0].status, "running");
        assert_eq!(
            response.action_results[0].lifecycle_status.as_deref(),
            Some("running")
        );
        assert!(response.action_results[0].import_receipt_path.is_none());
        assert!(!response.action_results[0].has_import_receipt);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_action_results_updates_only_scope_used_by_returned_results() {
        let dir = unique_bundle_temp_dir("local-bridge-action-results-last-used-scope");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
        let runtime = LocalBridgeRuntimeState::default();
        runtime.authorizations.lock().unwrap().extend([
            local_bridge_authorization(
                "local-agent-app",
                &[LocalBridgePermissionScope::BundleSend],
                1_000,
                5_000,
            ),
            local_bridge_authorization(
                "local-agent-app",
                &[LocalBridgePermissionScope::BundleImportRequest],
                1_100,
                5_000,
            ),
        ]);
        runtime
            .pending_action_results
            .lock()
            .unwrap()
            .push(LocalBridgePendingActionResult {
                request_id: "bridge-send-completed".to_string(),
                action_kind: "bundle.send".to_string(),
                client_id: "local-agent-app".to_string(),
                client_display_name: "Local Agent App".to_string(),
                client_app_kind: Some("agent".to_string()),
                status: "completed".to_string(),
                lifecycle_status: Some("succeeded".to_string()),
                reason: None,
                message: "send completed".to_string(),
                bundle_id: Some("bundle_1234567890".to_string()),
                bundle_type: Some("skill".to_string()),
                bundle_root: Some("/tmp/private/bundle".to_string()),
                target_device_id: Some("device-a".to_string()),
                require_trusted_device: Some(true),
                conflict_strategy: None,
                skipped_file_count: 0,
                import_receipt_path: None,
                rollback_file_count: 0,
                rollback_blocking_reason: None,
                rolled_back_file_count: 0,
                requested_at_ms: 1_500,
                claimed_at_ms: 1_900,
            });
        let request = serde_json::json!({
            "kind": "actions.results",
            "payload": {
                "request_id": "bridge-results-send",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "action_request_id": "bridge-send-completed",
                "after_claimed_at_ms": null,
                "limit": 10
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_runtime_at(
            &request,
            &[],
            None,
            &staging_root,
            &import_root,
            &runtime,
            false,
            2_000,
        )
        .unwrap();

        assert_eq!(response.status, "ok");
        assert_eq!(response.action_results.len(), 1);
        let authorizations = runtime.authorizations.lock().unwrap();
        let last_used_for_scope = |scope| {
            authorizations
                .iter()
                .find(|record| record.scopes == vec![scope])
                .map(|record| record.last_used_at_ms)
                .unwrap()
        };
        assert_eq!(
            last_used_for_scope(LocalBridgePermissionScope::BundleSend),
            2_000
        );
        assert_eq!(
            last_used_for_scope(LocalBridgePermissionScope::BundleImportRequest),
            1_100
        );
        drop(authorizations);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_event_poll_returns_only_events_after_cursor() {
        let dir = unique_bundle_temp_dir("local-bridge-events-after");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
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
            &import_root,
            &runtime,
            false,
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
    fn local_bridge_event_poll_returns_cursor_metadata_for_paging() {
        let dir = unique_bundle_temp_dir("local-bridge-events-cursor-page");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
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
        for (event_id, bytes_transferred) in [
            ("bridge-event-1", 10_u64),
            ("bridge-event-2", 20_u64),
            ("bridge-event-3", 30_u64),
        ] {
            push_local_bridge_runtime_event(
                &runtime,
                nekolink_protocol::LocalBridgeEvent::TransferUpdated(
                    nekolink_protocol::LocalBridgeTransferUpdatedEvent {
                        event_id: event_id.to_string(),
                        transfer_id: "transfer-1".to_string(),
                        phase: nekolink_protocol::LocalBridgeTransferPhase::Sending,
                        bytes_transferred,
                        total_bytes: 100,
                    },
                ),
            )
            .unwrap();
        }
        let first_page_request = serde_json::json!({
            "kind": "events.poll",
            "payload": {
                "request_id": "bridge-events-page-1",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "after_event_id": null,
                "limit": 1
            }
        })
        .to_string();

        let first_page = handle_local_bridge_request_with_runtime_at(
            &first_page_request,
            &[],
            None,
            &staging_root,
            &import_root,
            &runtime,
            false,
            1_500,
        )
        .unwrap();
        let second_page_request = serde_json::json!({
            "kind": "events.poll",
            "payload": {
                "request_id": "bridge-events-page-2",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "after_event_id": first_page.events_next_after_id,
                "limit": 2
            }
        })
        .to_string();
        let second_page = handle_local_bridge_request_with_runtime_at(
            &second_page_request,
            &[],
            None,
            &staging_root,
            &import_root,
            &runtime,
            false,
            1_600,
        )
        .unwrap();

        assert_eq!(first_page.events.len(), 1);
        assert_eq!(first_page.events_last_id.as_deref(), Some("bridge-event-1"));
        assert_eq!(
            first_page.events_next_after_id.as_deref(),
            Some("bridge-event-1")
        );
        assert!(first_page.events_has_more);
        assert_eq!(first_page.events_cursor_state, "ok");
        assert_eq!(
            first_page.events_visible_first_id.as_deref(),
            Some("bridge-event-1")
        );
        assert_eq!(
            first_page.events_visible_last_id.as_deref(),
            Some("bridge-event-3")
        );
        assert_eq!(first_page.events_visible_count, 3);
        assert_eq!(second_page.events.len(), 2);
        assert_eq!(
            second_page.events[0]["payload"]["event_id"].as_str(),
            Some("bridge-event-2")
        );
        assert_eq!(
            second_page.events_last_id.as_deref(),
            Some("bridge-event-3")
        );
        assert!(!second_page.events_has_more);
        assert_eq!(second_page.events_cursor_state, "ok");
        assert_eq!(
            second_page.events_visible_first_id.as_deref(),
            Some("bridge-event-1")
        );
        assert_eq!(
            second_page.events_visible_last_id.as_deref(),
            Some("bridge-event-3")
        );
        assert_eq!(second_page.events_visible_count, 3);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_event_poll_reports_missing_cursor() {
        let dir = unique_bundle_temp_dir("local-bridge-events-missing-cursor");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
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
                    event_id: "bridge-event-current".to_string(),
                    transfer_id: "transfer-1".to_string(),
                    phase: nekolink_protocol::LocalBridgeTransferPhase::Sending,
                    bytes_transferred: 10,
                    total_bytes: 100,
                },
            ),
        )
        .unwrap();
        let poll_request = serde_json::json!({
            "kind": "events.poll",
            "payload": {
                "request_id": "bridge-events-missing-cursor",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "after_event_id": "bridge-event-pruned",
                "limit": 10
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_runtime_at(
            &poll_request,
            &[],
            None,
            &staging_root,
            &import_root,
            &runtime,
            false,
            1_500,
        )
        .unwrap();

        assert_eq!(response.status, "ok");
        assert_eq!(response.events.len(), 0);
        assert_eq!(response.events_cursor_state, "missing");
        assert_eq!(response.events_last_id, None);
        assert_eq!(response.events_next_after_id, None);
        assert!(!response.events_has_more);
        assert_eq!(
            response.events_visible_first_id.as_deref(),
            Some("bridge-event-current")
        );
        assert_eq!(
            response.events_visible_last_id.as_deref(),
            Some("bridge-event-current")
        );
        assert_eq!(response.events_visible_count, 1);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_event_poll_reports_empty_cursor_state() {
        let dir = unique_bundle_temp_dir("local-bridge-events-empty-cursor");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
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
        let poll_request = serde_json::json!({
            "kind": "events.poll",
            "payload": {
                "request_id": "bridge-events-empty-cursor",
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
            &import_root,
            &runtime,
            false,
            1_500,
        )
        .unwrap();

        assert_eq!(response.status, "ok");
        assert_eq!(response.events.len(), 0);
        assert_eq!(response.events_cursor_state, "empty");

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_event_poll_updates_only_scopes_used_by_returned_events() {
        let dir = unique_bundle_temp_dir("local-bridge-events-last-used-scope");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
        let runtime = LocalBridgeRuntimeState::default();
        runtime.authorizations.lock().unwrap().extend([
            local_bridge_authorization(
                "local-agent-app",
                &[LocalBridgePermissionScope::TransferStatusRead],
                1_000,
                5_000,
            ),
            local_bridge_authorization(
                "local-agent-app",
                &[LocalBridgePermissionScope::BundleRead],
                1_100,
                5_000,
            ),
            local_bridge_authorization(
                "local-agent-app",
                &[LocalBridgePermissionScope::BundleSend],
                1_200,
                5_000,
            ),
            local_bridge_authorization(
                "local-agent-app",
                &[LocalBridgePermissionScope::BundleImportRequest],
                1_300,
                5_000,
            ),
        ]);
        push_local_bridge_runtime_event(
            &runtime,
            nekolink_protocol::LocalBridgeEvent::TransferUpdated(
                nekolink_protocol::LocalBridgeTransferUpdatedEvent {
                    event_id: "bridge-event-transfer".to_string(),
                    transfer_id: "transfer-1".to_string(),
                    phase: nekolink_protocol::LocalBridgeTransferPhase::Sending,
                    bytes_transferred: 10,
                    total_bytes: 100,
                },
            ),
        )
        .unwrap();
        let poll_request = serde_json::json!({
            "kind": "events.poll",
            "payload": {
                "request_id": "bridge-events-last-used",
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
            &import_root,
            &runtime,
            false,
            2_000,
        )
        .unwrap();

        assert_eq!(response.status, "ok");
        assert_eq!(response.events.len(), 1);
        let authorizations = runtime.authorizations.lock().unwrap();
        let last_used_for_scope = |scope| {
            authorizations
                .iter()
                .find(|record| record.scopes == vec![scope])
                .map(|record| record.last_used_at_ms)
                .unwrap()
        };
        assert_eq!(
            last_used_for_scope(LocalBridgePermissionScope::TransferStatusRead),
            2_000
        );
        assert_eq!(
            last_used_for_scope(LocalBridgePermissionScope::BundleRead),
            1_100
        );
        assert_eq!(
            last_used_for_scope(LocalBridgePermissionScope::BundleSend),
            1_200
        );
        assert_eq!(
            last_used_for_scope(LocalBridgePermissionScope::BundleImportRequest),
            1_300
        );
        drop(authorizations);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_event_poll_can_wait_for_new_events() {
        let dir = unique_bundle_temp_dir("local-bridge-events-long-poll");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
        let runtime = Arc::new(LocalBridgeRuntimeState::default());
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
        let producer_runtime = runtime.clone();
        let producer = thread::spawn(move || {
            thread::sleep(Duration::from_millis(20));
            push_local_bridge_runtime_event(
                &producer_runtime,
                nekolink_protocol::LocalBridgeEvent::TransferUpdated(
                    nekolink_protocol::LocalBridgeTransferUpdatedEvent {
                        event_id: "bridge-event-waited".to_string(),
                        transfer_id: "transfer-1".to_string(),
                        phase: nekolink_protocol::LocalBridgeTransferPhase::Sending,
                        bytes_transferred: 10,
                        total_bytes: 100,
                    },
                ),
            )
            .unwrap();
        });
        let poll_request = serde_json::json!({
            "kind": "events.poll",
            "payload": {
                "request_id": "bridge-events-wait",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "after_event_id": null,
                "limit": 10,
                "timeout_ms": 500
            }
        })
        .to_string();

        let response = handle_local_bridge_request_with_runtime_at(
            &poll_request,
            &[],
            None,
            &staging_root,
            &import_root,
            &runtime,
            true,
            1_500,
        )
        .unwrap();
        producer.join().unwrap();

        assert_eq!(response.status, "ok");
        assert_eq!(response.events.len(), 1);
        assert_eq!(
            response.events[0]["payload"]["event_id"].as_str(),
            Some("bridge-event-waited")
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_event_poll_requires_authorized_client_scope() {
        let dir = unique_bundle_temp_dir("local-bridge-events-auth");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
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
            &import_root,
            &runtime,
            false,
            1_500,
        )
        .unwrap();

        assert_eq!(response.status, "pending_auth");
        assert_eq!(response.security_state, "requires_user_confirmation");
        assert!(response.events.is_empty());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn local_bridge_event_poll_timeout_does_not_delay_pending_auth() {
        let dir = unique_bundle_temp_dir("local-bridge-events-pending-auth-no-wait");
        let staging_root = dir.join("bundle_staging");
        let import_root = dir.join("bundle_imports");
        let runtime = LocalBridgeRuntimeState::default();
        let poll_request = serde_json::json!({
            "kind": "events.poll",
            "payload": {
                "request_id": "bridge-events-auth-timeout",
                "client": {
                    "client_id": "local-agent-app",
                    "display_name": "Local Agent App",
                    "app_kind": "agent"
                },
                "after_event_id": null,
                "limit": 10,
                "timeout_ms": 500
            }
        })
        .to_string();
        let started = Instant::now();

        let response = handle_local_bridge_request_with_runtime_at(
            &poll_request,
            &[],
            None,
            &staging_root,
            &import_root,
            &runtime,
            true,
            1_500,
        )
        .unwrap();

        assert_eq!(response.status, "pending_auth");
        assert!(started.elapsed() < Duration::from_millis(100));

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
        let import_root = dir.join("bundle_imports");
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
            &import_root,
            &runtime,
            false,
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
        let import_root = dir.join("bundle_imports");
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
            &import_root,
            &runtime,
            false,
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
        assert_eq!(authorization.last_used_at_ms, 2_000);
        assert_eq!(authorization.expires_at_ms, Some(11_000));
        assert!(local_bridge_client_has_scope(
            Some(&pending.client),
            &[authorization],
            LocalBridgePermissionScope::BundleImportRequest,
            3_000,
        ));
    }

    #[test]
    fn confirmed_local_bridge_authorization_dedupes_requested_scopes() {
        let pending = PendingLocalBridgeAuthorization {
            request_id: "bridge-auth-1".to_string(),
            client: LocalBridgeClientIdentity {
                client_id: "local-agent-app".to_string(),
                display_name: "Local Agent App".to_string(),
                app_kind: Some("agent".to_string()),
            },
            requested_scopes: vec![
                LocalBridgePermissionScope::BundleRead,
                LocalBridgePermissionScope::BundleRead,
                LocalBridgePermissionScope::TransferStatusRead,
            ],
            reason: "Read local bridge state".to_string(),
            authorization_code: "ABC-123".to_string(),
            requested_at_ms: 1_000,
            expires_at_ms: 11_000,
        };

        let authorization =
            confirm_pending_local_bridge_authorization(&pending, "ABC-123", 2_000).unwrap();

        assert_eq!(
            authorization.scopes,
            vec![
                LocalBridgePermissionScope::BundleRead,
                LocalBridgePermissionScope::TransferStatusRead,
            ]
        );
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
            last_used_at_ms: granted_at_ms,
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
        create_desktop_test_bundle_with_type(dir, directory_name, bundle_id, BundleType::Skill)
    }

    fn create_desktop_test_bundle_with_type(
        dir: &std::path::Path,
        directory_name: &str,
        bundle_id: &str,
        bundle_type: BundleType,
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
        manifest.bundle_type = bundle_type;
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
