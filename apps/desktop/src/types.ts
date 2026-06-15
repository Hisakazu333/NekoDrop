export type PageId = "home" | "devices" | "transfers" | "settings";

export interface AppSnapshot {
  device_name: string;
  receive_dir: string;
  receive_port: number;
  receive_policy: string;
  discovery_enabled: boolean;
  tray_enabled: boolean;
  device_identity: DeviceIdentityDto;
}

export interface DeviceIdentityDto {
  device_id: string;
  device_name: string;
  device_kind: string;
  platform: string;
  public_key_fingerprint: string;
  capabilities: string[];
}

export interface DeviceDto {
  id: string;
  name: string;
  platform: string;
  host: string;
  port: number;
  trust_state: string;
  public_key_fingerprint: string | null;
  pairing_code: string | null;
}

export interface TrustedDeviceDto {
  device_id: string;
  device_name: string;
  platform: string;
  host: string;
  port: number;
  public_key_fingerprint: string;
  pairing_code: string;
  paired_at_ms: number;
  last_seen_at_ms: number;
}

export type LocalBridgeResponseStatus = "ok" | "pending_auth" | "unsupported" | string;
export type LocalBridgeSecurityState = "read_only" | "requires_user_confirmation" | string;
export type LocalBridgeClientState = "anonymous" | "identified" | string;
export type LocalBridgePermissionScope =
  | "device.read"
  | "transfer.status.read"
  | "bundle.read"
  | "bundle.send"
  | "bundle.import.request";

export interface LocalBridgeResponseDto {
  request_id: string;
  status: LocalBridgeResponseStatus;
  message: string;
  security_state: LocalBridgeSecurityState;
  requires_user_confirmation: boolean;
  client_state: LocalBridgeClientState;
  client_id: string | null;
  client_display_name: string | null;
  authorization_scopes: LocalBridgePermissionScope[];
  authorization_reason: string | null;
  authorization_ttl_seconds: number | null;
  authorization_code: string | null;
  authorization_expires_at_ms: number | null;
  devices: TrustedDeviceDto[];
  staged_bundles: ReceivedBundleDto[];
  transfer_status: TransferStatusDto | null;
}

export interface LocalBridgeAuthorizationDto {
  client_id: string;
  display_name: string;
  app_kind: string | null;
  scopes: LocalBridgePermissionScope[];
  granted_at_ms: number;
  expires_at_ms: number | null;
}

export interface LocalBridgeRuntimeStatusDto {
  active: boolean;
  bind_host: string;
  port: number;
  request_path: string;
  max_request_bytes: number;
  pending_authorization_client: string | null;
  authorization_count: number;
  last_error: string | null;
}

export interface DiscoveryStatusDto {
  phase: string;
  message: string;
  service_type: string;
  advertised: boolean;
  lan_ip: string | null;
  port: number | null;
  device_count: number;
  last_seen_seconds_ago: number | null;
  last_error: string | null;
}

export interface DesktopRealtimeSnapshotDto {
  receive_status: string | null;
  receive_session: ReceiveSessionDto | null;
  receive_report: ReceiveReportDto | null;
  pending_receive_offer: PendingReceiveOfferDto | null;
  pending_pairing_request: PendingPairingRequestDto | null;
  transfer_status: TransferStatusDto | null;
  discovery_status: DiscoveryStatusDto;
}

export interface TransferDto {
  id: string;
  root_name: string;
  peer_device_id: string | null;
  peer_name: string | null;
  target_host: string | null;
  source_paths: string[];
  received_paths: string[];
  direction: string;
  status: string;
  file_count: number;
  total_bytes: number;
  transferred_bytes: number;
  progress: number;
  receive_dir: string | null;
  error_message: string | null;
  security_mode: TransferSecurityMode | null;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface ManifestItemDto {
  path: string;
  kind: "file" | "directory";
  size: number;
  modified_at: string | null;
  sha256: string | null;
}

export interface TransferSourceFileDto {
  manifest_path: string;
  source_path: string;
  size: number;
  sha256: string;
}

export interface TransferPlanDto {
  root_name: string;
  file_count: number;
  total_bytes: number;
  items: ManifestItemDto[];
  files: TransferSourceFileDto[];
}

export interface TransferScanProgressDto {
  phase: "started" | "scanning" | "hashing" | "completed";
  current_path: string | null;
  files_found: number;
  directories_found: number;
  bytes_found: number;
}

export interface ReceiveSessionDto {
  bind_addr: string;
  receive_dir: string;
  connection_code: string;
}

export interface ReceivePortDiagnosticsDto {
  phase: "closed" | "listening" | "no_lan_ip" | "invalid_bind_addr";
  listening: boolean;
  bind_addr: string | null;
  advertised_host: string | null;
  port: number | null;
  lan_ips: string[];
  message: string;
  checks: string[];
}

export interface SentFileDto {
  manifest_path: string;
  bytes_sent: number;
}

export interface SendReportDto {
  root_name: string;
  file_count: number;
  total_bytes: number;
  sent_files: SentFileDto[];
}

export interface ReceivedFileDto {
  path: string;
  manifest_path: string;
  bytes_written: number;
  sha256: string;
  verified: boolean;
}

export interface ReceivedBundleDto {
  bundle_id: string;
  bundle_type: "skill" | "session" | "workspace" | "agent_profile" | "config_snapshot" | string;
  display_name: string;
  source_app: string;
  file_count: number;
  total_bytes: number;
  staging_path: string;
  import_allowed: boolean;
  staging_status: "saved" | "imported" | "deleted" | "import_failed" | "expired" | string;
  can_import_now: boolean;
  import_path: string | null;
}

export interface ManualBundleCreateDto {
  bundle_id: string;
  bundle_type: "skill" | "session" | "workspace" | "agent_profile" | "config_snapshot" | string;
  display_name: string;
  source_app: string;
  staging_path: string;
  file_count: number;
  total_bytes: number;
}

export interface ReceiveReportDto {
  transfer_id: string;
  root_name: string;
  security_mode: TransferSecurityMode;
  sender_device_id: string | null;
  sender_device_name: string | null;
  sender_public_key_fingerprint: string | null;
  file_count: number;
  bundle: ReceivedBundleDto | null;
  files: ReceivedFileDto[];
}

export type TransferSecurityMode =
  | "legacy_plain"
  | "encrypted_session"
  | "authenticated_encrypted_session";

export interface PendingReceiveFileDto {
  manifest_path: string;
  size: number;
  sha256: string;
}

export interface ReceiveResumeSummaryDto {
  resumable_file_count: number;
  completed_file_count: number;
  partial_file_count: number;
  received_bytes: number;
}

export interface PendingReceiveOfferDto {
  transfer_id: string;
  root_name: string;
  file_count: number;
  total_bytes: number;
  sender_device_id: string | null;
  sender_device_name: string | null;
  sender_public_key_fingerprint: string | null;
  preview_file_count: number;
  files: PendingReceiveFileDto[];
  resume_summary: ReceiveResumeSummaryDto | null;
}

export interface PendingPairingRequestDto {
  request_id: string;
  device_id: string;
  device_name: string;
  platform: string;
  host: string;
  port: number;
  public_key_fingerprint: string;
  pairing_code: string;
}

export interface TransferStatusDto {
  direction: "send" | "receive" | string;
  phase: string;
  root_name: string | null;
  file_count: number;
  file_index: number;
  current_file: string | null;
  bytes_transferred: number;
  total_bytes: number;
  progress: number;
  message: string;
  updated_at_ms: number;
}
