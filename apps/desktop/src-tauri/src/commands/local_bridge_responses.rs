use nekolink_protocol::LocalBridgeClientIdentity;

use crate::app_state::PendingLocalBridgeAuthorization;

use super::dto::{
    LocalBridgePendingActionResultDto, LocalBridgeResponseDto, ReceivedBundleDto,
    TransferStatusDto, TrustedDeviceDto,
};
use super::local_bridge_permission_scope_label;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct LocalBridgeEventPage {
    pub(super) events: Vec<serde_json::Value>,
    pub(super) last_event_id: Option<String>,
    pub(super) next_after_event_id: Option<String>,
    pub(super) has_more: bool,
}

pub(super) fn local_bridge_read_only_response(
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
        action_results: Vec::new(),
        events: Vec::new(),
        events_last_id: None,
        events_next_after_id: None,
        events_has_more: false,
    }
}

pub(super) fn local_bridge_read_only_unsupported_response(
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
        action_results: Vec::new(),
        events: Vec::new(),
        events_last_id: None,
        events_next_after_id: None,
        events_has_more: false,
    }
}

pub(super) fn local_bridge_pending_confirmation_response(
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
        action_results: Vec::new(),
        events: Vec::new(),
        events_last_id: None,
        events_next_after_id: None,
        events_has_more: false,
    }
}

pub(super) fn local_bridge_authorized_runtime_pending_response(
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
        action_results: Vec::new(),
        events: Vec::new(),
        events_last_id: None,
        events_next_after_id: None,
        events_has_more: false,
    }
}

pub(super) fn local_bridge_events_response(
    request_id: String,
    client: Option<LocalBridgeClientIdentity>,
    page: LocalBridgeEventPage,
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
        action_results: Vec::new(),
        events: page.events,
        events_last_id: page.last_event_id,
        events_next_after_id: page.next_after_event_id,
        events_has_more: page.has_more,
    }
}

pub(super) fn local_bridge_action_results_response(
    request_id: String,
    client: Option<LocalBridgeClientIdentity>,
    action_results: Vec<LocalBridgePendingActionResultDto>,
) -> LocalBridgeResponseDto {
    let client_metadata = local_bridge_client_metadata(client);
    LocalBridgeResponseDto {
        request_id,
        status: "ok".to_string(),
        message: "local bridge action result snapshot".to_string(),
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
        action_results,
        events: Vec::new(),
        events_last_id: None,
        events_next_after_id: None,
        events_has_more: false,
    }
}

pub(super) fn local_bridge_pending_authorization_response_from_pending(
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
        action_results: Vec::new(),
        events: Vec::new(),
        events_last_id: None,
        events_next_after_id: None,
        events_has_more: false,
    }
}

pub(super) fn local_bridge_client_metadata(
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
