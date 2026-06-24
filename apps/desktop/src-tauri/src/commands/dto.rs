use serde::{Deserialize, Serialize};

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
    pub import_destination: Option<String>,
    pub import_conflict: bool,
    pub import_blocking_reason: Option<String>,
    pub import_plan_files: Vec<BundleImportPlanFileDto>,
    pub import_conflict_count: usize,
    pub import_conflict_strategies: Vec<String>,
    pub imported_with_strategy: Option<String>,
    pub import_skipped_file_count: usize,
    pub import_receipt_path: Option<String>,
    pub has_import_receipt: bool,
    pub imported_manifest_paths: Vec<String>,
    pub skipped_manifest_paths: Vec<String>,
    pub rollback_file_count: usize,
    pub can_rollback_now: bool,
    pub can_request_rollback: bool,
    pub rollback_blocking_reason: Option<String>,
    pub rolled_back_file_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct BundleImportPlanFileDto {
    pub manifest_path: String,
    pub size: u64,
    pub sha256: String,
    pub destination_path: String,
    pub destination_exists: bool,
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

#[derive(Debug, Clone, Deserialize)]
pub struct ImportStagedBundleRequestDto {
    pub bundle_id: String,
    pub conflict_strategy: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RollbackImportedBundleRequestDto {
    pub bundle_id: String,
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
    pub action_results: Vec<LocalBridgePendingActionResultDto>,
    pub events: Vec<serde_json::Value>,
    pub events_last_id: Option<String>,
    pub events_next_after_id: Option<String>,
    pub events_has_more: bool,
    pub events_cursor_state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalBridgeAuthorizationDto {
    pub client_id: String,
    pub display_name: String,
    pub app_kind: Option<String>,
    pub scopes: Vec<String>,
    pub granted_at_ms: u128,
    pub last_used_at_ms: u128,
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
    pub client_app_kind: Option<String>,
    pub bundle_type: Option<String>,
    pub target_device_id: Option<String>,
    pub staged_bundle_id: Option<String>,
    pub expected_bundle_type: Option<String>,
    pub conflict_strategy: Option<String>,
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
pub struct LocalBridgePendingActionTakeDto {
    pub action: Option<LocalBridgePendingActionDto>,
    pub remaining_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalBridgePendingActionResultDto {
    pub request_id: String,
    pub action_kind: String,
    pub client_id: String,
    pub client_display_name: String,
    pub client_app_kind: Option<String>,
    pub status: String,
    pub lifecycle_status: Option<String>,
    pub reason: Option<String>,
    pub message: String,
    pub bundle_id: Option<String>,
    pub bundle_type: Option<String>,
    pub bundle_root: Option<String>,
    pub target_device_id: Option<String>,
    pub require_trusted_device: Option<bool>,
    pub conflict_strategy: Option<String>,
    pub skipped_file_count: usize,
    pub import_receipt_path: Option<String>,
    pub has_import_receipt: bool,
    pub rollback_file_count: usize,
    pub can_request_rollback: bool,
    pub rollback_blocking_reason: Option<String>,
    pub rolled_back_file_count: usize,
    pub requested_at_ms: u128,
    pub claimed_at_ms: u128,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalBridgePendingActionResultListDto {
    pub results: Vec<LocalBridgePendingActionResultDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalBridgeBundleSendPreflightDto {
    pub status: String,
    pub request_id: Option<String>,
    pub reason: Option<String>,
    pub message: String,
    pub client_id: Option<String>,
    pub client_display_name: Option<String>,
    pub client_app_kind: Option<String>,
    pub bundle_id: Option<String>,
    pub bundle_type: Option<String>,
    pub bundle_root: Option<String>,
    pub target_device_id: Option<String>,
    pub require_trusted_device: Option<bool>,
    pub requested_at_ms: Option<u128>,
    pub claimed_at_ms: Option<u128>,
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
