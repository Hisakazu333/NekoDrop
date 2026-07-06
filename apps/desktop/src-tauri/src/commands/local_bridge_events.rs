use nekolink_protocol::{
    LocalBridgeClientIdentity, LocalBridgeEvent, LOCAL_BRIDGE_ACTION_RESULT_FOLLOWUP_ACTION,
    LOCAL_BRIDGE_CURSOR_HAS_MORE_RECOVERY_ACTION, LOCAL_BRIDGE_CURSOR_MISSING_RECOVERY_ACTION,
};

use super::local_bridge_responses::LocalBridgeEventPage;

pub(super) fn local_bridge_events_after(
    events: &[LocalBridgeEvent],
    client: Option<&LocalBridgeClientIdentity>,
    after_event_id: Option<&str>,
    action_request_id: Option<&str>,
    limit: usize,
    can_read_bundles: bool,
    can_read_transfers: bool,
    can_send_bundles: bool,
    can_import_bundles: bool,
) -> Result<LocalBridgeEventPage, String> {
    let mut after_cursor = after_event_id.is_none();
    let mut cursor_found = after_event_id.is_none();
    let mut output = Vec::new();
    let mut last_event_id = None;
    let mut next_after_event_id = None;
    let mut has_more = false;
    let mut visible_first_event_id = None;
    let mut visible_last_event_id = None;
    let mut visible_event_count = 0;
    let mut terminal_action_request_ids = Vec::new();
    for event in events {
        let is_allowed = local_bridge_event_is_allowed(
            event,
            can_read_bundles,
            can_read_transfers,
            can_send_bundles,
            can_import_bundles,
            client,
            action_request_id,
        );
        if is_allowed {
            let event_id = local_bridge_event_id(event).to_string();
            visible_first_event_id.get_or_insert_with(|| event_id.clone());
            visible_last_event_id = Some(event_id);
            visible_event_count += 1;
        }
        if !after_cursor {
            after_cursor =
                is_allowed && local_bridge_event_id(event) == after_event_id.unwrap_or_default();
            if after_cursor {
                cursor_found = true;
            }
            continue;
        }
        if !is_allowed {
            continue;
        }
        let event_id = local_bridge_event_id(event).to_string();
        if output.len() >= limit {
            has_more = true;
            continue;
        }
        if let LocalBridgeEvent::ActionUpdated(action) = event {
            if action.status.is_terminal()
                && !terminal_action_request_ids.contains(&action.request_id)
            {
                terminal_action_request_ids.push(action.request_id.clone());
            }
        }
        output.push(serde_json::to_value(event).map_err(|error| error.to_string())?);
        last_event_id = Some(event_id.clone());
        next_after_event_id = Some(event_id);
    }
    let cursor_state = if events.is_empty() {
        "empty"
    } else if cursor_found {
        "ok"
    } else {
        "missing"
    };
    let recovery_action = if cursor_state == "missing" {
        Some(LOCAL_BRIDGE_CURSOR_MISSING_RECOVERY_ACTION.to_string())
    } else if cursor_state == "ok" && has_more {
        Some(LOCAL_BRIDGE_CURSOR_HAS_MORE_RECOVERY_ACTION.to_string())
    } else {
        None
    };
    let result_followup_required = !terminal_action_request_ids.is_empty();
    let result_followup_action =
        result_followup_required.then(|| LOCAL_BRIDGE_ACTION_RESULT_FOLLOWUP_ACTION.to_string());
    Ok(LocalBridgeEventPage {
        events: output,
        last_event_id,
        next_after_event_id,
        has_more,
        cursor_state,
        visible_first_event_id,
        visible_last_event_id,
        visible_event_count,
        recovery_action,
        result_followup_required,
        result_followup_action,
        terminal_action_request_ids,
    })
}

pub(super) fn local_bridge_event_id(event: &LocalBridgeEvent) -> &str {
    match event {
        LocalBridgeEvent::BundleReceived(event) => &event.event_id,
        LocalBridgeEvent::BundleSendPreflight(event) => &event.event_id,
        LocalBridgeEvent::ActionUpdated(event) => &event.event_id,
        LocalBridgeEvent::TransferUpdated(event) => &event.event_id,
    }
}

fn local_bridge_event_is_allowed(
    event: &LocalBridgeEvent,
    can_read_bundles: bool,
    can_read_transfers: bool,
    can_send_bundles: bool,
    can_import_bundles: bool,
    client: Option<&LocalBridgeClientIdentity>,
    action_request_id: Option<&str>,
) -> bool {
    if let Some(action_request_id) = action_request_id {
        return match event {
            LocalBridgeEvent::ActionUpdated(event) if event.request_id == action_request_id => {
                local_bridge_action_event_is_allowed(
                    event,
                    can_send_bundles,
                    can_import_bundles,
                    client,
                )
            }
            _ => false,
        };
    }
    match event {
        LocalBridgeEvent::BundleReceived(_) => can_read_bundles,
        LocalBridgeEvent::BundleSendPreflight(event) => {
            can_send_bundles
                && client.is_some_and(|client| {
                    event.client_id == client.client_id && event.client_app_kind == client.app_kind
                })
        }
        LocalBridgeEvent::ActionUpdated(event) => local_bridge_action_event_is_allowed(
            event,
            can_send_bundles,
            can_import_bundles,
            client,
        ),
        LocalBridgeEvent::TransferUpdated(_) => can_read_transfers,
    }
}

fn local_bridge_action_event_is_allowed(
    event: &nekolink_protocol::LocalBridgeActionUpdatedEvent,
    can_send_bundles: bool,
    can_import_bundles: bool,
    client: Option<&LocalBridgeClientIdentity>,
) -> bool {
    match event.action_kind.as_str() {
        "bundle.send" => {
            can_send_bundles
                && client.is_some_and(|client| {
                    event.client_id == client.client_id && event.client_app_kind == client.app_kind
                })
        }
        "bundle.import" | "bundle.rollback" => {
            can_import_bundles
                && client.is_some_and(|client| {
                    event.client_id == client.client_id && event.client_app_kind == client.app_kind
                })
        }
        _ => false,
    }
}
