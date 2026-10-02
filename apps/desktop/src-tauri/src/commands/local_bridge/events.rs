use super::super::*;
use super::*;

pub(crate) const LOCAL_BRIDGE_EVENT_QUEUE_LIMIT: usize = 256;

pub(crate) fn push_local_bridge_runtime_event(
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

pub(crate) fn push_local_bridge_transfer_status_event(
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

pub(crate) fn push_local_bridge_bundle_received_event(
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

pub(crate) fn set_transfer_status_and_push_bridge_event(
    transfer_status: &Arc<Mutex<Option<TransferStatusState>>>,
    runtime: &LocalBridgeRuntimeState,
    transfer_id: &str,
    status: TransferStatusState,
) {
    set_transfer_status(transfer_status, status.clone());
    let _ = push_local_bridge_transfer_status_event(runtime, transfer_id, &status);
}

pub(crate) fn local_bridge_transfer_phase_from_status(
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

pub(crate) fn wait_for_local_bridge_events(
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
