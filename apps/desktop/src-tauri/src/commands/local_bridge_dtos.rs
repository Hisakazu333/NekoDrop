use crate::app_state::{
    LocalBridgeAuthorizationRecord, LocalBridgePendingAction, LocalBridgePendingActionResult,
};
use crate::local_bridge_runtime::LocalBridgeRuntimeStatusSnapshot;

use super::bundle_helpers::bundle_type_label;
use super::dto::{
    LocalBridgeAuthorizationDto, LocalBridgePendingActionDto, LocalBridgePendingActionResultDto,
    LocalBridgeRuntimeStatusDto,
};
use super::local_bridge_permission_scope_label;

pub(super) fn local_bridge_pending_action_result_to_dto(
    result: &LocalBridgePendingActionResult,
    include_sensitive_paths: bool,
) -> LocalBridgePendingActionResultDto {
    LocalBridgePendingActionResultDto {
        request_id: result.request_id.clone(),
        action_kind: result.action_kind.clone(),
        client_id: result.client_id.clone(),
        client_display_name: result.client_display_name.clone(),
        status: result.status.clone(),
        lifecycle_status: result.lifecycle_status.clone(),
        reason: result.reason.clone(),
        message: result.message.clone(),
        bundle_id: result.bundle_id.clone(),
        bundle_type: result.bundle_type.clone(),
        bundle_root: include_sensitive_paths
            .then(|| result.bundle_root.clone())
            .flatten(),
        target_device_id: result.target_device_id.clone(),
        require_trusted_device: result.require_trusted_device,
        conflict_strategy: result.conflict_strategy.clone(),
        skipped_file_count: result.skipped_file_count,
        requested_at_ms: result.requested_at_ms,
        claimed_at_ms: result.claimed_at_ms,
    }
}

pub(super) fn local_bridge_pending_action_to_dto(
    action: &LocalBridgePendingAction,
    include_sensitive_paths: bool,
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
            conflict_strategy: None,
            require_trusted_device: Some(action.require_trusted_device),
            requested_at_ms: action.requested_at_ms,
            bundle_root: include_sensitive_paths.then(|| action.bundle_root.clone()),
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
            conflict_strategy: Some(action.conflict_strategy.clone()),
            require_trusted_device: None,
            requested_at_ms: action.requested_at_ms,
            bundle_root: None,
        },
    }
}

pub(super) fn local_bridge_authorization_to_dto(
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

pub(super) fn local_bridge_authorizations_to_dtos(
    authorizations: Vec<LocalBridgeAuthorizationRecord>,
) -> Vec<LocalBridgeAuthorizationDto> {
    authorizations
        .into_iter()
        .map(local_bridge_authorization_to_dto)
        .collect()
}

pub(super) fn local_bridge_runtime_status_to_dto(
    status: LocalBridgeRuntimeStatusSnapshot,
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
