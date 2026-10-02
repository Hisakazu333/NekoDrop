use super::super::*;
use super::*;

pub(crate) fn local_bridge_action_results_for_client(
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

pub(crate) fn local_bridge_action_result_matches_client(
    result: &LocalBridgePendingActionResult,
    client: &LocalBridgeClientIdentity,
) -> bool {
    result.client_id == client.client_id && result.client_app_kind == client.app_kind
}

pub(crate) fn local_bridge_action_result_matches_action(
    result: &LocalBridgePendingActionResult,
    action: &LocalBridgePendingAction,
) -> bool {
    result.request_id == local_bridge_pending_action_request_id(action)
        && result.action_kind == local_bridge_pending_action_kind(action)
        && result.client_id == local_bridge_pending_action_client(action).client_id
        && result.client_app_kind == local_bridge_pending_action_client(action).app_kind
        && match action {
            LocalBridgePendingAction::SendBundle(action) => {
                local_bridge_send_result_matches_action(result, action)
            }
            LocalBridgePendingAction::ImportBundle(action) => {
                local_bridge_import_result_matches_action(result, action)
            }
            LocalBridgePendingAction::RollbackBundleImport(action) => {
                local_bridge_rollback_result_matches_action(result, action)
            }
        }
}

pub(crate) fn local_bridge_send_request_matches_pending_action(
    request: &nekolink_protocol::LocalBridgeSendBundleRequest,
    action: &LocalBridgePendingAction,
) -> bool {
    match action {
        LocalBridgePendingAction::SendBundle(action) => {
            local_bridge_send_request_matches_send_action(request, action)
        }
        _ => false,
    }
}

pub(crate) fn local_bridge_import_request_matches_pending_action(
    request: &nekolink_protocol::LocalBridgeImportBundleRequest,
    action: &LocalBridgePendingAction,
) -> bool {
    match action {
        LocalBridgePendingAction::ImportBundle(action) => {
            local_bridge_import_request_matches_import_action(request, action)
        }
        _ => false,
    }
}

pub(crate) fn local_bridge_rollback_request_matches_pending_action(
    request: &nekolink_protocol::LocalBridgeRollbackBundleImportRequest,
    action: &LocalBridgePendingAction,
) -> bool {
    match action {
        LocalBridgePendingAction::RollbackBundleImport(action) => {
            local_bridge_rollback_request_matches_rollback_action(request, action)
        }
        _ => false,
    }
}

pub(crate) fn local_bridge_send_request_matches_send_action(
    request: &nekolink_protocol::LocalBridgeSendBundleRequest,
    action: &LocalBridgePendingSendBundleAction,
) -> bool {
    request.target_device_id == action.target_device_id
        && request.bundle_root == action.bundle_root
        && request.bundle_type == action.bundle_type
        && request.require_trusted_device == action.require_trusted_device
}

pub(crate) fn local_bridge_import_request_matches_import_action(
    request: &nekolink_protocol::LocalBridgeImportBundleRequest,
    action: &LocalBridgePendingImportBundleAction,
) -> bool {
    request.staged_bundle_id == action.staged_bundle_id
        && request.expected_bundle_type == action.expected_bundle_type
        && request.conflict_strategy.as_deref().unwrap_or("reject") == action.conflict_strategy
}

pub(crate) fn local_bridge_rollback_request_matches_rollback_action(
    request: &nekolink_protocol::LocalBridgeRollbackBundleImportRequest,
    action: &LocalBridgePendingRollbackBundleImportAction,
) -> bool {
    request.bundle_id == action.bundle_id
}

pub(crate) fn local_bridge_send_result_matches_action(
    result: &LocalBridgePendingActionResult,
    action: &LocalBridgePendingSendBundleAction,
) -> bool {
    result.bundle_root.as_deref() == Some(action.bundle_root.as_str())
        && result.target_device_id.as_deref() == action.target_device_id.as_deref()
        && result.bundle_type.as_deref() == Some(bundle_type_label(action.bundle_type))
        && result.require_trusted_device == Some(action.require_trusted_device)
}

pub(crate) fn local_bridge_import_result_matches_action(
    result: &LocalBridgePendingActionResult,
    action: &LocalBridgePendingImportBundleAction,
) -> bool {
    result.bundle_id.as_deref() == Some(action.staged_bundle_id.as_str())
        && match action.expected_bundle_type {
            Some(expected_bundle_type) => {
                result.bundle_type.as_deref() == Some(bundle_type_label(expected_bundle_type))
            }
            None => true,
        }
        && result.conflict_strategy.as_deref() == Some(action.conflict_strategy.as_str())
}

pub(crate) fn local_bridge_rollback_result_matches_action(
    result: &LocalBridgePendingActionResult,
    action: &LocalBridgePendingRollbackBundleImportAction,
) -> bool {
    result.bundle_id.as_deref() == Some(action.bundle_id.as_str())
}

pub(crate) fn local_bridge_send_result_matches_request(
    result: &LocalBridgePendingActionResult,
    request: &nekolink_protocol::LocalBridgeSendBundleRequest,
) -> bool {
    result.bundle_root.as_deref() == Some(request.bundle_root.as_str())
        && result.target_device_id.as_deref() == request.target_device_id.as_deref()
        && result.bundle_type.as_deref() == Some(bundle_type_label(request.bundle_type))
        && result.require_trusted_device == Some(request.require_trusted_device)
}

pub(crate) fn local_bridge_import_result_matches_request(
    result: &LocalBridgePendingActionResult,
    request: &nekolink_protocol::LocalBridgeImportBundleRequest,
) -> bool {
    result.bundle_id.as_deref() == Some(request.staged_bundle_id.as_str())
        && match request.expected_bundle_type {
            Some(expected_bundle_type) => {
                result.bundle_type.as_deref() == Some(bundle_type_label(expected_bundle_type))
            }
            None => true,
        }
        && result.conflict_strategy.as_deref()
            == Some(request.conflict_strategy.as_deref().unwrap_or("reject"))
}

pub(crate) fn local_bridge_rollback_result_matches_request(
    result: &LocalBridgePendingActionResult,
    request: &nekolink_protocol::LocalBridgeRollbackBundleImportRequest,
) -> bool {
    result.bundle_id.as_deref() == Some(request.bundle_id.as_str())
}

pub(crate) fn local_bridge_send_request_matches_send_action_payload(
    left: &LocalBridgePendingSendBundleAction,
    right: &LocalBridgePendingSendBundleAction,
) -> bool {
    left.target_device_id == right.target_device_id
        && left.bundle_root == right.bundle_root
        && left.bundle_type == right.bundle_type
        && left.require_trusted_device == right.require_trusted_device
}

pub(crate) fn local_bridge_import_request_matches_import_action_payload(
    left: &LocalBridgePendingImportBundleAction,
    right: &LocalBridgePendingImportBundleAction,
) -> bool {
    left.staged_bundle_id == right.staged_bundle_id
        && left.expected_bundle_type == right.expected_bundle_type
        && left.conflict_strategy == right.conflict_strategy
}

pub(crate) fn local_bridge_rollback_request_matches_rollback_action_payload(
    left: &LocalBridgePendingRollbackBundleImportAction,
    right: &LocalBridgePendingRollbackBundleImportAction,
) -> bool {
    left.bundle_id == right.bundle_id
}

pub(crate) fn local_bridge_pending_action_matches_client(
    action: &LocalBridgePendingAction,
    client: &LocalBridgeClientIdentity,
) -> bool {
    match action {
        LocalBridgePendingAction::SendBundle(action) => {
            local_bridge_client_identity_matches(&action.client, client)
        }
        LocalBridgePendingAction::ImportBundle(action) => {
            local_bridge_client_identity_matches(&action.client, client)
        }
        LocalBridgePendingAction::RollbackBundleImport(action) => {
            local_bridge_client_identity_matches(&action.client, client)
        }
    }
}

pub(crate) fn local_bridge_pending_action_requested_at_ms(
    action: &LocalBridgePendingAction,
) -> u128 {
    match action {
        LocalBridgePendingAction::SendBundle(action) => action.requested_at_ms,
        LocalBridgePendingAction::ImportBundle(action) => action.requested_at_ms,
        LocalBridgePendingAction::RollbackBundleImport(action) => action.requested_at_ms,
    }
}

pub(crate) fn local_bridge_pending_action_bundle_type(
    action: &LocalBridgePendingAction,
) -> Option<BundleType> {
    match action {
        LocalBridgePendingAction::SendBundle(action) => Some(action.bundle_type),
        LocalBridgePendingAction::ImportBundle(action) => action.expected_bundle_type,
        LocalBridgePendingAction::RollbackBundleImport(_) => None,
    }
}

pub(crate) fn local_bridge_pending_action_bundle_id(
    action: &LocalBridgePendingAction,
) -> Option<&str> {
    match action {
        LocalBridgePendingAction::SendBundle(_) => None,
        LocalBridgePendingAction::ImportBundle(action) => Some(action.staged_bundle_id.as_str()),
        LocalBridgePendingAction::RollbackBundleImport(action) => Some(action.bundle_id.as_str()),
    }
}

pub(crate) fn local_bridge_pending_action_target_device_id(
    action: &LocalBridgePendingAction,
) -> Option<&str> {
    match action {
        LocalBridgePendingAction::SendBundle(action) => action.target_device_id.as_deref(),
        LocalBridgePendingAction::ImportBundle(_) => None,
        LocalBridgePendingAction::RollbackBundleImport(_) => None,
    }
}

pub(crate) fn local_bridge_client_can_read_pending_action(
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
