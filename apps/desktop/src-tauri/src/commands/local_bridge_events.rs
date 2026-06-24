use nekolink_protocol::{LocalBridgeClientIdentity, LocalBridgeEvent};

use super::local_bridge_responses::LocalBridgeEventPage;

pub(super) fn local_bridge_events_after(
    events: &[LocalBridgeEvent],
    client: Option<&LocalBridgeClientIdentity>,
    after_event_id: Option<&str>,
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
    for event in events {
        if !after_cursor {
            after_cursor = local_bridge_event_id(event) == after_event_id.unwrap_or_default();
            if after_cursor {
                cursor_found = true;
            }
            continue;
        }
        if !local_bridge_event_is_allowed(
            event,
            can_read_bundles,
            can_read_transfers,
            can_send_bundles,
            can_import_bundles,
            client,
        ) {
            continue;
        }
        let event_id = local_bridge_event_id(event).to_string();
        if output.len() >= limit {
            has_more = true;
            break;
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
    Ok(LocalBridgeEventPage {
        events: output,
        last_event_id,
        next_after_event_id,
        has_more,
        cursor_state,
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
) -> bool {
    match event {
        LocalBridgeEvent::BundleReceived(_) => can_read_bundles,
        LocalBridgeEvent::BundleSendPreflight(event) => {
            can_send_bundles && client.is_some_and(|client| event.client_id == client.client_id)
        }
        LocalBridgeEvent::ActionUpdated(event) => match event.action_kind.as_str() {
            "bundle.send" => {
                can_send_bundles && client.is_some_and(|client| event.client_id == client.client_id)
            }
            "bundle.import" | "bundle.rollback" => {
                can_import_bundles
                    && client.is_some_and(|client| event.client_id == client.client_id)
            }
            _ => false,
        },
        LocalBridgeEvent::TransferUpdated(_) => can_read_transfers,
    }
}
