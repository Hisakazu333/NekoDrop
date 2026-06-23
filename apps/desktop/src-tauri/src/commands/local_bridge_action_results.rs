use nekolink_protocol::{BundleType, LocalBridgeActionLifecycleStatus};

use crate::app_state::{
    LocalBridgePendingAction, LocalBridgePendingActionResult, LocalBridgePendingImportBundleAction,
    LocalBridgePendingRollbackBundleImportAction, LocalBridgePendingSendBundleAction,
};

use super::bundle_helpers::{bundle_type_from_label, bundle_type_label};
use super::dto::LocalBridgeBundleSendPreflightDto;

pub(super) fn local_bridge_bundle_import_result(
    status: &str,
    action: &LocalBridgePendingImportBundleAction,
    reason: Option<&str>,
    message: &str,
    bundle_id: Option<&str>,
    bundle_type: Option<BundleType>,
    skipped_file_count: usize,
    import_receipt_path: Option<&str>,
    rollback_file_count: usize,
    now_ms: u128,
) -> LocalBridgePendingActionResult {
    LocalBridgePendingActionResult {
        request_id: action.request_id.clone(),
        action_kind: "bundle.import".to_string(),
        client_id: action.client.client_id.clone(),
        client_display_name: action.client.display_name.clone(),
        status: status.to_string(),
        lifecycle_status: Some(
            local_bridge_lifecycle_status_from_result(status, reason).to_string(),
        ),
        reason: reason.map(str::to_string),
        message: message.to_string(),
        bundle_id: bundle_id.map(str::to_string),
        bundle_type: bundle_type.map(bundle_type_label).map(str::to_string),
        bundle_root: None,
        target_device_id: None,
        require_trusted_device: None,
        conflict_strategy: Some(action.conflict_strategy.clone()),
        skipped_file_count,
        import_receipt_path: import_receipt_path.map(str::to_string),
        rollback_file_count,
        rollback_blocking_reason: None,
        rolled_back_file_count: 0,
        requested_at_ms: action.requested_at_ms,
        claimed_at_ms: now_ms,
    }
}

pub(super) fn local_bridge_bundle_rollback_result(
    status: &str,
    action: &LocalBridgePendingRollbackBundleImportAction,
    reason: Option<&str>,
    rollback_blocking_reason: Option<&str>,
    message: &str,
    rolled_back_file_count: usize,
    now_ms: u128,
) -> LocalBridgePendingActionResult {
    LocalBridgePendingActionResult {
        request_id: action.request_id.clone(),
        action_kind: "bundle.rollback".to_string(),
        client_id: action.client.client_id.clone(),
        client_display_name: action.client.display_name.clone(),
        status: status.to_string(),
        lifecycle_status: Some(
            local_bridge_lifecycle_status_from_result(status, reason).to_string(),
        ),
        reason: reason.map(str::to_string),
        message: message.to_string(),
        bundle_id: Some(action.bundle_id.clone()),
        bundle_type: None,
        bundle_root: None,
        target_device_id: None,
        require_trusted_device: None,
        conflict_strategy: None,
        skipped_file_count: 0,
        import_receipt_path: None,
        rollback_file_count: 0,
        rollback_blocking_reason: rollback_blocking_reason.map(str::to_string),
        rolled_back_file_count,
        requested_at_ms: action.requested_at_ms,
        claimed_at_ms: now_ms,
    }
}

pub(super) fn local_bridge_bundle_send_result_from_preflight(
    status: &str,
    preflight: &LocalBridgeBundleSendPreflightDto,
    action: &LocalBridgePendingSendBundleAction,
    now_ms: u128,
) -> LocalBridgePendingActionResult {
    local_bridge_bundle_send_result(
        status,
        action,
        preflight.bundle_id.as_deref(),
        preflight
            .bundle_type
            .as_deref()
            .and_then(bundle_type_from_label),
        preflight.reason.as_deref(),
        &preflight.message,
        now_ms,
    )
}

pub(super) fn local_bridge_bundle_send_result(
    status: &str,
    action: &LocalBridgePendingSendBundleAction,
    bundle_id: Option<&str>,
    bundle_type: Option<BundleType>,
    reason: Option<&str>,
    message: &str,
    now_ms: u128,
) -> LocalBridgePendingActionResult {
    LocalBridgePendingActionResult {
        request_id: action.request_id.clone(),
        action_kind: "bundle.send".to_string(),
        client_id: action.client.client_id.clone(),
        client_display_name: action.client.display_name.clone(),
        status: status.to_string(),
        lifecycle_status: Some(
            local_bridge_lifecycle_status_from_result(status, reason).to_string(),
        ),
        reason: reason.map(str::to_string),
        message: message.to_string(),
        bundle_id: bundle_id.map(str::to_string),
        bundle_type: bundle_type.map(bundle_type_label).map(str::to_string),
        bundle_root: Some(action.bundle_root.clone()),
        target_device_id: action.target_device_id.clone(),
        require_trusted_device: Some(action.require_trusted_device),
        conflict_strategy: None,
        skipped_file_count: 0,
        import_receipt_path: None,
        rollback_file_count: 0,
        rollback_blocking_reason: None,
        rolled_back_file_count: 0,
        requested_at_ms: action.requested_at_ms,
        claimed_at_ms: now_ms,
    }
}

pub(super) fn local_bridge_action_lifecycle_result(
    action: &LocalBridgePendingAction,
    lifecycle_status: LocalBridgeActionLifecycleStatus,
    reason: Option<&str>,
    message: &str,
    bundle_id: Option<&str>,
    bundle_type: Option<BundleType>,
    target_device_id: Option<&str>,
    now_ms: u128,
) -> LocalBridgePendingActionResult {
    match action {
        LocalBridgePendingAction::SendBundle(action) => LocalBridgePendingActionResult {
            request_id: action.request_id.clone(),
            action_kind: "bundle.send".to_string(),
            client_id: action.client.client_id.clone(),
            client_display_name: action.client.display_name.clone(),
            status: local_bridge_lifecycle_status_label(lifecycle_status).to_string(),
            lifecycle_status: Some(
                local_bridge_lifecycle_status_label(lifecycle_status).to_string(),
            ),
            reason: reason.map(str::to_string),
            message: message.to_string(),
            bundle_id: bundle_id.map(str::to_string),
            bundle_type: bundle_type.map(bundle_type_label).map(str::to_string),
            bundle_root: Some(action.bundle_root.clone()),
            target_device_id: target_device_id
                .map(str::to_string)
                .or_else(|| action.target_device_id.clone()),
            require_trusted_device: Some(action.require_trusted_device),
            conflict_strategy: None,
            skipped_file_count: 0,
            import_receipt_path: None,
            rollback_file_count: 0,
            rollback_blocking_reason: None,
            rolled_back_file_count: 0,
            requested_at_ms: action.requested_at_ms,
            claimed_at_ms: now_ms,
        },
        LocalBridgePendingAction::ImportBundle(action) => LocalBridgePendingActionResult {
            request_id: action.request_id.clone(),
            action_kind: "bundle.import".to_string(),
            client_id: action.client.client_id.clone(),
            client_display_name: action.client.display_name.clone(),
            status: local_bridge_lifecycle_status_label(lifecycle_status).to_string(),
            lifecycle_status: Some(
                local_bridge_lifecycle_status_label(lifecycle_status).to_string(),
            ),
            reason: reason.map(str::to_string),
            message: message.to_string(),
            bundle_id: bundle_id.map(str::to_string),
            bundle_type: bundle_type.map(bundle_type_label).map(str::to_string),
            bundle_root: None,
            target_device_id: None,
            require_trusted_device: None,
            conflict_strategy: Some(action.conflict_strategy.clone()),
            skipped_file_count: 0,
            import_receipt_path: None,
            rollback_file_count: 0,
            rollback_blocking_reason: None,
            rolled_back_file_count: 0,
            requested_at_ms: action.requested_at_ms,
            claimed_at_ms: now_ms,
        },
        LocalBridgePendingAction::RollbackBundleImport(action) => LocalBridgePendingActionResult {
            request_id: action.request_id.clone(),
            action_kind: "bundle.rollback".to_string(),
            client_id: action.client.client_id.clone(),
            client_display_name: action.client.display_name.clone(),
            status: local_bridge_lifecycle_status_label(lifecycle_status).to_string(),
            lifecycle_status: Some(
                local_bridge_lifecycle_status_label(lifecycle_status).to_string(),
            ),
            reason: reason.map(str::to_string),
            message: message.to_string(),
            bundle_id: Some(action.bundle_id.clone()),
            bundle_type: bundle_type.map(bundle_type_label).map(str::to_string),
            bundle_root: None,
            target_device_id: None,
            require_trusted_device: None,
            conflict_strategy: None,
            skipped_file_count: 0,
            import_receipt_path: None,
            rollback_file_count: 0,
            rollback_blocking_reason: None,
            rolled_back_file_count: 0,
            requested_at_ms: action.requested_at_ms,
            claimed_at_ms: now_ms,
        },
    }
}

pub(super) fn local_bridge_lifecycle_status_from_result(
    status: &str,
    reason: Option<&str>,
) -> &'static str {
    if reason == Some("bundle_import_conflict") {
        return LocalBridgeActionLifecycleStatus::Conflict.as_str();
    }
    match status {
        "queued" => LocalBridgeActionLifecycleStatus::Queued.as_str(),
        "running" => LocalBridgeActionLifecycleStatus::Running.as_str(),
        "completed" => LocalBridgeActionLifecycleStatus::Succeeded.as_str(),
        "cancelled" => LocalBridgeActionLifecycleStatus::Cancelled.as_str(),
        "failed" | "failed_preflight" => LocalBridgeActionLifecycleStatus::Failed.as_str(),
        _ => LocalBridgeActionLifecycleStatus::Failed.as_str(),
    }
}

fn local_bridge_lifecycle_status_label(status: LocalBridgeActionLifecycleStatus) -> &'static str {
    status.as_str()
}
