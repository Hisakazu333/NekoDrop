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
    TransferProgress, TransportStream, TCP_IO_STALL_TIMEOUT,
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

#[cfg(test)]
mod tests;

mod bundles;
mod devices;
mod history;
mod local_bridge;
mod receive;
mod send;
mod settings;

pub(crate) use bundles::*;
pub(crate) use devices::*;
pub(crate) use history::*;
pub(crate) use local_bridge::*;
pub(crate) use receive::*;
pub(crate) use send::*;
pub(crate) use settings::*;

pub(crate) fn status_from_progress_event(
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

pub(crate) fn status_from_transfer_progress(
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

pub(crate) fn set_transfer_status(
    transfer_status: &Arc<Mutex<Option<TransferStatusState>>>,
    status: TransferStatusState,
) {
    if let Ok(mut slot) = transfer_status.lock() {
        *slot = Some(status);
    }
}

pub(crate) fn current_transfer_progress(
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

pub(crate) fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}
