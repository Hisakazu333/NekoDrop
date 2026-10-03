use super::super::*;
use super::*;

pub(crate) const LOCAL_BRIDGE_PENDING_ACTION_QUEUE_LIMIT: usize = 128;

pub(crate) const LOCAL_BRIDGE_PENDING_ACTION_RESULT_LIMIT: usize = 128;

#[tauri::command(async)]
pub fn list_local_bridge_pending_actions(
    state: State<'_, AppState>,
) -> Result<LocalBridgePendingActionListDto, String> {
    Ok(LocalBridgePendingActionListDto {
        actions: list_local_bridge_pending_actions_at(&state.local_bridge_runtime)?,
    })
}

#[tauri::command(async)]
pub fn remove_local_bridge_pending_action(
    state: State<'_, AppState>,
    request_id: String,
) -> Result<LocalBridgePendingActionRemoveDto, String> {
    let removed = remove_local_bridge_pending_action_at(&state.local_bridge_runtime, &request_id)?;
    Ok(LocalBridgePendingActionRemoveDto {
        removed,
        actions: list_local_bridge_pending_actions_at(&state.local_bridge_runtime)?,
    })
}

#[tauri::command(async)]
pub fn respond_local_bridge_pending_action(
    state: State<'_, AppState>,
    request_id: String,
    accept: bool,
) -> Result<LocalBridgePendingActionRespondDto, String> {
    let runtime = &state.local_bridge_runtime;

    if !accept {
        let action = take_local_bridge_pending_action_by_request_id(runtime, &request_id)?;
        let declined = action.is_some();
        if let Some(action) = action {
            // The protocol has no dedicated "declined" lifecycle status; the
            // message distinguishes a user rejection from a plain removal.
            push_local_bridge_action_lifecycle_result(
                runtime,
                local_bridge_action_lifecycle_result(
                    &action,
                    LocalBridgeActionLifecycleStatus::Cancelled,
                    None,
                    "local bridge action was declined by the desktop user",
                    None,
                    match &action {
                        LocalBridgePendingAction::SendBundle(action) => Some(action.bundle_type),
                        LocalBridgePendingAction::ImportBundle(action) => {
                            action.expected_bundle_type
                        }
                        LocalBridgePendingAction::RollbackBundleImport(_) => None,
                    },
                    match &action {
                        LocalBridgePendingAction::SendBundle(action) => {
                            action.target_device_id.as_deref()
                        }
                        LocalBridgePendingAction::ImportBundle(_) => None,
                        LocalBridgePendingAction::RollbackBundleImport(_) => None,
                    },
                    now_ms(),
                ),
            )?;
        }
        return Ok(LocalBridgePendingActionRespondDto {
            handled: declined,
            accepted: false,
            result: None,
            actions: list_local_bridge_pending_actions_at(runtime)?,
        });
    }

    let action = take_local_bridge_pending_action_by_request_id(runtime, &request_id)?;
    let Some(action) = action else {
        return Ok(LocalBridgePendingActionRespondDto {
            handled: false,
            accepted: true,
            result: None,
            actions: list_local_bridge_pending_actions_at(runtime)?,
        });
    };

    // On failure the action is already gone from the queue, so record a
    // failed lifecycle result here; otherwise the client would wait forever
    // on a "running" action that will never finish.
    let pending_action = action.clone();
    let result = match action {
        LocalBridgePendingAction::SendBundle(action) => {
            execute_local_bridge_bundle_send_action_at(&state, action, now_ms()).map(Some)
        }
        LocalBridgePendingAction::ImportBundle(action) => {
            let staging_root = bundle_staging_root()?;
            let import_root = bundle_import_root()?;
            let result = execute_local_bridge_bundle_import_action(
                action,
                &staging_root,
                &import_root,
                now_ms(),
            );
            match result {
                Ok(result) => {
                    push_local_bridge_pending_action_result_record(runtime, result.clone())?;
                    Ok(Some(local_bridge_pending_action_result_to_dto(
                        &result, false,
                    )))
                }
                Err(error) => {
                    push_failed_local_bridge_action_result(runtime, &pending_action, &error)?;
                    Err(error)
                }
            }
        }
        LocalBridgePendingAction::RollbackBundleImport(action) => {
            let import_root = bundle_import_root()?;
            let result =
                execute_local_bridge_bundle_rollback_action(action, &import_root, now_ms());
            match result {
                Ok(result) => {
                    push_local_bridge_action_lifecycle_result(runtime, result.clone())?;
                    Ok(Some(local_bridge_pending_action_result_to_dto(
                        &result, false,
                    )))
                }
                Err(error) => {
                    push_failed_local_bridge_action_result(runtime, &pending_action, &error)?;
                    Err(error)
                }
            }
        }
    }?;

    Ok(LocalBridgePendingActionRespondDto {
        handled: true,
        accepted: true,
        result,
        actions: list_local_bridge_pending_actions_at(runtime)?,
    })
}

pub(crate) fn push_failed_local_bridge_action_result(
    runtime: &LocalBridgeRuntimeState,
    action: &LocalBridgePendingAction,
    error: &str,
) -> Result<(), String> {
    push_local_bridge_action_lifecycle_result(
        runtime,
        local_bridge_action_lifecycle_result(
            action,
            LocalBridgeActionLifecycleStatus::Failed,
            None,
            error,
            None,
            match action {
                LocalBridgePendingAction::SendBundle(action) => Some(action.bundle_type),
                LocalBridgePendingAction::ImportBundle(action) => action.expected_bundle_type,
                LocalBridgePendingAction::RollbackBundleImport(_) => None,
            },
            match action {
                LocalBridgePendingAction::SendBundle(action) => action.target_device_id.as_deref(),
                LocalBridgePendingAction::ImportBundle(_) => None,
                LocalBridgePendingAction::RollbackBundleImport(_) => None,
            },
            now_ms(),
        ),
    )
}

#[tauri::command(async)]
pub fn list_local_bridge_pending_action_results(
    state: State<'_, AppState>,
) -> Result<LocalBridgePendingActionResultListDto, String> {
    Ok(LocalBridgePendingActionResultListDto {
        results: list_local_bridge_pending_action_results_at(&state.local_bridge_runtime)?,
    })
}

#[tauri::command(async)]
pub fn take_next_local_bridge_pending_action(
    state: State<'_, AppState>,
) -> Result<LocalBridgePendingActionTakeDto, String> {
    let action = take_next_local_bridge_pending_action_at(&state.local_bridge_runtime)?;
    let remaining_count = state
        .local_bridge_runtime
        .pending_actions
        .lock()
        .map_err(|error| error.to_string())?
        .len();
    Ok(LocalBridgePendingActionTakeDto {
        action,
        remaining_count,
    })
}

#[tauri::command(async)]
pub fn preflight_next_local_bridge_bundle_send(
    state: State<'_, AppState>,
) -> Result<LocalBridgeBundleSendPreflightDto, String> {
    let trusted_devices = state
        .trusted_devices
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    preflight_next_local_bridge_bundle_send_at(
        &state.local_bridge_runtime,
        &trusted_devices,
        now_ms(),
    )
}

#[tauri::command(async)]
pub fn execute_next_local_bridge_bundle_import(
    state: State<'_, AppState>,
) -> Result<Option<LocalBridgePendingActionResultDto>, String> {
    let staging_root = bundle_staging_root()?;
    let import_root = bundle_import_root()?;
    execute_next_local_bridge_bundle_import_at(
        &state.local_bridge_runtime,
        &staging_root,
        &import_root,
        now_ms(),
    )
}

#[tauri::command(async)]
pub fn execute_next_local_bridge_bundle_send(
    state: State<'_, AppState>,
) -> Result<Option<LocalBridgePendingActionResultDto>, String> {
    let trusted_devices = state
        .trusted_devices
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    execute_next_local_bridge_bundle_send_at(&state, &trusted_devices, now_ms())
}

#[tauri::command(async)]
pub fn run_local_bridge_runtime_worker_once(
    state: State<'_, AppState>,
) -> Result<Option<LocalBridgePendingActionResultDto>, String> {
    run_local_bridge_runtime_worker_once_at(&state, now_ms())
}

pub(crate) fn start_local_bridge_runtime_worker(app: AppHandle) {
    thread::spawn(move || loop {
        let state = app.state::<AppState>();
        {
            // 锁中毒时恢复数据继续执行，而非静默跳过（跳过会让状态永久卡住）
            let mut actions = state
                .local_bridge_runtime
                .pending_actions
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            while actions.is_empty() {
                match state
                    .local_bridge_runtime
                    .pending_actions_signal
                    .wait_timeout(actions, Duration::from_secs(30))
                {
                    Ok((next_actions, wait_result)) => {
                        actions = next_actions;
                        if !actions.is_empty() || !wait_result.timed_out() {
                            break;
                        }
                    }
                    Err(error) => {
                        eprintln!("local bridge worker wait failed: {error}");
                        break;
                    }
                }
            }
        }
        if let Err(error) = run_local_bridge_runtime_worker_once_at(&state, now_ms()) {
            eprintln!("local bridge worker failed: {error}");
            thread::sleep(Duration::from_millis(500));
        }
    });
}

pub(crate) fn run_local_bridge_runtime_worker_once_at(
    state: &AppState,
    now_ms: u128,
) -> Result<Option<LocalBridgePendingActionResultDto>, String> {
    let action = take_next_local_bridge_pending_action_raw(&state.local_bridge_runtime)?;
    match action {
        Some(LocalBridgePendingAction::SendBundle(action)) => {
            execute_local_bridge_bundle_send_action_at(state, action, now_ms).map(Some)
        }
        Some(LocalBridgePendingAction::ImportBundle(action)) => {
            let staging_root = bundle_staging_root()?;
            let import_root = bundle_import_root()?;
            let result = execute_local_bridge_bundle_import_action(
                action,
                &staging_root,
                &import_root,
                now_ms,
            )?;
            push_local_bridge_pending_action_result_record(
                &state.local_bridge_runtime,
                result.clone(),
            )?;
            Ok(Some(local_bridge_pending_action_result_to_dto(
                &result, false,
            )))
        }
        Some(LocalBridgePendingAction::RollbackBundleImport(action)) => {
            let import_root = bundle_import_root()?;
            let result = execute_local_bridge_bundle_rollback_action(action, &import_root, now_ms)?;
            push_local_bridge_action_lifecycle_result(&state.local_bridge_runtime, result.clone())?;
            Ok(Some(local_bridge_pending_action_result_to_dto(
                &result, false,
            )))
        }
        None => Ok(None),
    }
}

pub(crate) fn push_local_bridge_pending_action_queued(
    runtime: &LocalBridgeRuntimeState,
    action: LocalBridgePendingAction,
    now_ms: u128,
) -> Result<(), String> {
    let result = local_bridge_action_lifecycle_result(
        &action,
        LocalBridgeActionLifecycleStatus::Queued,
        None,
        "local bridge action is queued for the desktop runtime",
        local_bridge_pending_action_bundle_id(&action),
        local_bridge_pending_action_bundle_type(&action),
        local_bridge_pending_action_target_device_id(&action),
        now_ms,
    );
    let mut actions = runtime
        .pending_actions
        .lock()
        .map_err(|error| error.to_string())?;
    actions.retain(|existing| !local_bridge_pending_actions_are_same_request(existing, &action));
    actions.push(action);
    if actions.len() > LOCAL_BRIDGE_PENDING_ACTION_QUEUE_LIMIT {
        let excess = actions.len() - LOCAL_BRIDGE_PENDING_ACTION_QUEUE_LIMIT;
        actions.drain(0..excess);
    }
    runtime.pending_actions_signal.notify_one();
    push_local_bridge_action_lifecycle_result(runtime, result)?;
    Ok(())
}

pub(crate) fn local_bridge_retry_send_response(
    request: &nekolink_protocol::LocalBridgeSendBundleRequest,
    pending_actions: &[LocalBridgePendingAction],
    action_results: &[LocalBridgePendingActionResult],
) -> Option<LocalBridgeResponseDto> {
    let request_client = request.client.as_ref()?;
    let request_kind = "bundle.send";
    let request_id = request.request_id.as_str();

    if let Some(pending_action) = pending_actions.iter().find(|pending_action| {
        local_bridge_pending_action_request_id(pending_action) == request_id
            && local_bridge_pending_action_kind(pending_action) == request_kind
            && local_bridge_pending_action_client(pending_action).client_id
                == request_client.client_id
            && local_bridge_pending_action_client(pending_action).app_kind
                == request_client.app_kind
    }) {
        if local_bridge_send_request_matches_pending_action(request, pending_action) {
            return Some(local_bridge_pending_action_retry_response(
                request_id,
                Some(request_client.clone()),
                pending_action,
            ));
        }
        return Some(local_bridge_retry_payload_conflict_response(
            request_id,
            Some(request_client.clone()),
            pending_action,
        ));
    }

    let Some(existing_result) = action_results.iter().rev().find(|result| {
        result.request_id == request_id
            && result.action_kind == request_kind
            && result.client_id == request_client.client_id
            && result.client_app_kind == request_client.app_kind
            && local_bridge_send_result_matches_request(result, request)
    }) else {
        if action_results.iter().rev().any(|result| {
            result.request_id == request_id
                && result.action_kind == request_kind
                && result.client_id == request_client.client_id
                && result.client_app_kind == request_client.app_kind
        }) {
            return Some(local_bridge_retry_result_payload_conflict_response(
                request_id,
                Some(request_client.clone()),
                action_results.iter().rev().find(|result| {
                    result.request_id == request_id
                        && result.action_kind == request_kind
                        && result.client_id == request_client.client_id
                        && result.client_app_kind == request_client.app_kind
                })?,
            ));
        }
        return None;
    };

    Some(local_bridge_action_results_response(
        request_id.to_string(),
        Some(request_client.clone()),
        vec![local_bridge_pending_action_result_to_dto(
            existing_result,
            false,
        )],
    ))
}

pub(crate) fn local_bridge_retry_import_response(
    request: &nekolink_protocol::LocalBridgeImportBundleRequest,
    pending_actions: &[LocalBridgePendingAction],
    action_results: &[LocalBridgePendingActionResult],
) -> Option<LocalBridgeResponseDto> {
    let request_client = request.client.as_ref()?;
    let request_kind = "bundle.import";
    let request_id = request.request_id.as_str();

    if let Some(pending_action) = pending_actions.iter().find(|pending_action| {
        local_bridge_pending_action_request_id(pending_action) == request_id
            && local_bridge_pending_action_kind(pending_action) == request_kind
            && local_bridge_pending_action_client(pending_action).client_id
                == request_client.client_id
            && local_bridge_pending_action_client(pending_action).app_kind
                == request_client.app_kind
    }) {
        if local_bridge_import_request_matches_pending_action(request, pending_action) {
            return Some(local_bridge_pending_action_retry_response(
                request_id,
                Some(request_client.clone()),
                pending_action,
            ));
        }
        return Some(local_bridge_retry_payload_conflict_response(
            request_id,
            Some(request_client.clone()),
            pending_action,
        ));
    }

    let Some(existing_result) = action_results.iter().rev().find(|result| {
        result.request_id == request_id
            && result.action_kind == request_kind
            && result.client_id == request_client.client_id
            && result.client_app_kind == request_client.app_kind
            && local_bridge_import_result_matches_request(result, request)
    }) else {
        if action_results.iter().rev().any(|result| {
            result.request_id == request_id
                && result.action_kind == request_kind
                && result.client_id == request_client.client_id
                && result.client_app_kind == request_client.app_kind
        }) {
            return Some(local_bridge_retry_result_payload_conflict_response(
                request_id,
                Some(request_client.clone()),
                action_results.iter().rev().find(|result| {
                    result.request_id == request_id
                        && result.action_kind == request_kind
                        && result.client_id == request_client.client_id
                        && result.client_app_kind == request_client.app_kind
                })?,
            ));
        }
        return None;
    };

    Some(local_bridge_action_results_response(
        request_id.to_string(),
        Some(request_client.clone()),
        vec![local_bridge_pending_action_result_to_dto(
            existing_result,
            false,
        )],
    ))
}

pub(crate) fn local_bridge_retry_rollback_response(
    request: &nekolink_protocol::LocalBridgeRollbackBundleImportRequest,
    pending_actions: &[LocalBridgePendingAction],
    action_results: &[LocalBridgePendingActionResult],
) -> Option<LocalBridgeResponseDto> {
    let request_client = request.client.as_ref()?;
    let request_kind = "bundle.rollback";
    let request_id = request.request_id.as_str();

    if let Some(pending_action) = pending_actions.iter().find(|pending_action| {
        local_bridge_pending_action_request_id(pending_action) == request_id
            && local_bridge_pending_action_kind(pending_action) == request_kind
            && local_bridge_pending_action_client(pending_action).client_id
                == request_client.client_id
            && local_bridge_pending_action_client(pending_action).app_kind
                == request_client.app_kind
    }) {
        if local_bridge_rollback_request_matches_pending_action(request, pending_action) {
            return Some(local_bridge_pending_action_retry_response(
                request_id,
                Some(request_client.clone()),
                pending_action,
            ));
        }
        return Some(local_bridge_retry_payload_conflict_response(
            request_id,
            Some(request_client.clone()),
            pending_action,
        ));
    }

    let Some(existing_result) = action_results.iter().rev().find(|result| {
        result.request_id == request_id
            && result.action_kind == request_kind
            && result.client_id == request_client.client_id
            && result.client_app_kind == request_client.app_kind
            && local_bridge_rollback_result_matches_request(result, request)
    }) else {
        if action_results.iter().rev().any(|result| {
            result.request_id == request_id
                && result.action_kind == request_kind
                && result.client_id == request_client.client_id
                && result.client_app_kind == request_client.app_kind
        }) {
            return Some(local_bridge_retry_result_payload_conflict_response(
                request_id,
                Some(request_client.clone()),
                action_results.iter().rev().find(|result| {
                    result.request_id == request_id
                        && result.action_kind == request_kind
                        && result.client_id == request_client.client_id
                        && result.client_app_kind == request_client.app_kind
                })?,
            ));
        }
        return None;
    };

    Some(local_bridge_action_results_response(
        request_id.to_string(),
        Some(request_client.clone()),
        vec![local_bridge_pending_action_result_to_dto(
            existing_result,
            false,
        )],
    ))
}

pub(crate) fn local_bridge_existing_action_result_for_retry(
    pending_actions: &[LocalBridgePendingAction],
    action_results: &[LocalBridgePendingActionResult],
    action: &LocalBridgePendingAction,
) -> Option<LocalBridgePendingActionResultDto> {
    if pending_actions
        .iter()
        .any(|pending_action| local_bridge_pending_actions_are_same_request(pending_action, action))
    {
        return None;
    }
    action_results
        .iter()
        .rev()
        .find(|result| local_bridge_action_result_matches_action(result, action))
        .map(|result| local_bridge_pending_action_result_to_dto(result, false))
}

pub(crate) fn local_bridge_retry_payload_conflict_response(
    request_id: &str,
    client: Option<LocalBridgeClientIdentity>,
    existing_action: &LocalBridgePendingAction,
) -> LocalBridgeResponseDto {
    let action_results = vec![local_bridge_pending_action_result_to_dto(
        &local_bridge_queued_result_for_pending_action(existing_action),
        false,
    )];
    let mut response =
        local_bridge_action_results_response(request_id.to_string(), client, action_results);
    response.status = "conflict".to_string();
    response.message = "local bridge request_id already belongs to a different payload".to_string();
    response
}

pub(crate) fn local_bridge_retry_result_payload_conflict_response(
    request_id: &str,
    client: Option<LocalBridgeClientIdentity>,
    existing_result: &LocalBridgePendingActionResult,
) -> LocalBridgeResponseDto {
    let mut response = local_bridge_action_results_response(
        request_id.to_string(),
        client,
        vec![local_bridge_pending_action_result_to_dto(
            existing_result,
            false,
        )],
    );
    response.status = "conflict".to_string();
    response.message = "local bridge request_id already belongs to a different payload".to_string();
    response
}

pub(crate) fn local_bridge_pending_action_retry_response(
    request_id: &str,
    client: Option<LocalBridgeClientIdentity>,
    pending_action: &LocalBridgePendingAction,
) -> LocalBridgeResponseDto {
    let mut response = local_bridge_authorized_runtime_pending_response(
        request_id.to_string(),
        client,
        "local bridge action is already queued for the desktop runtime",
    );
    response.action_results = vec![local_bridge_pending_action_result_to_dto(
        &local_bridge_queued_result_for_pending_action(pending_action),
        false,
    )];
    response
}

pub(crate) fn local_bridge_queued_result_for_pending_action(
    pending_action: &LocalBridgePendingAction,
) -> LocalBridgePendingActionResult {
    local_bridge_action_lifecycle_result(
        pending_action,
        LocalBridgeActionLifecycleStatus::Queued,
        None,
        "local bridge action is queued for the desktop runtime",
        local_bridge_pending_action_bundle_id(pending_action),
        local_bridge_pending_action_bundle_type(pending_action),
        local_bridge_pending_action_target_device_id(pending_action),
        local_bridge_pending_action_requested_at_ms(pending_action),
    )
}

pub(crate) fn local_bridge_pending_actions_are_same_request(
    left: &LocalBridgePendingAction,
    right: &LocalBridgePendingAction,
) -> bool {
    local_bridge_pending_action_kind(left) == local_bridge_pending_action_kind(right)
        && local_bridge_pending_action_request_id(left)
            == local_bridge_pending_action_request_id(right)
        && local_bridge_client_identity_matches(
            local_bridge_pending_action_client(left),
            local_bridge_pending_action_client(right),
        )
        && match (left, right) {
            (
                LocalBridgePendingAction::SendBundle(left),
                LocalBridgePendingAction::SendBundle(right),
            ) => local_bridge_send_request_matches_send_action_payload(left, right),
            (
                LocalBridgePendingAction::ImportBundle(left),
                LocalBridgePendingAction::ImportBundle(right),
            ) => local_bridge_import_request_matches_import_action_payload(left, right),
            (
                LocalBridgePendingAction::RollbackBundleImport(left),
                LocalBridgePendingAction::RollbackBundleImport(right),
            ) => local_bridge_rollback_request_matches_rollback_action_payload(left, right),
            _ => false,
        }
}

pub(crate) fn local_bridge_pending_action_kind(action: &LocalBridgePendingAction) -> &'static str {
    match action {
        LocalBridgePendingAction::SendBundle(_) => "bundle.send",
        LocalBridgePendingAction::ImportBundle(_) => "bundle.import",
        LocalBridgePendingAction::RollbackBundleImport(_) => "bundle.rollback",
    }
}

pub(crate) fn local_bridge_pending_action_client(
    action: &LocalBridgePendingAction,
) -> &LocalBridgeClientIdentity {
    match action {
        LocalBridgePendingAction::SendBundle(action) => &action.client,
        LocalBridgePendingAction::ImportBundle(action) => &action.client,
        LocalBridgePendingAction::RollbackBundleImport(action) => &action.client,
    }
}

pub(crate) fn list_local_bridge_pending_actions_at(
    runtime: &LocalBridgeRuntimeState,
) -> Result<Vec<LocalBridgePendingActionDto>, String> {
    let actions = runtime
        .pending_actions
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(actions
        .iter()
        .map(|action| local_bridge_pending_action_to_dto(action, false))
        .collect())
}

pub(crate) fn take_next_local_bridge_pending_action_at(
    runtime: &LocalBridgeRuntimeState,
) -> Result<Option<LocalBridgePendingActionDto>, String> {
    Ok(take_next_local_bridge_pending_action_raw(runtime)?
        .map(|action| local_bridge_pending_action_to_dto(&action, true)))
}

pub(crate) fn take_next_local_bridge_pending_action_raw(
    runtime: &LocalBridgeRuntimeState,
) -> Result<Option<LocalBridgePendingAction>, String> {
    let mut actions = runtime
        .pending_actions
        .lock()
        .map_err(|error| error.to_string())?;
    if actions.is_empty() {
        return Ok(None);
    }
    Ok(Some(actions.remove(0)))
}

pub(crate) fn preflight_next_local_bridge_bundle_send_at(
    runtime: &LocalBridgeRuntimeState,
    trusted_devices: &[TrustedDeviceRecord],
    now_ms: u128,
) -> Result<LocalBridgeBundleSendPreflightDto, String> {
    let action = {
        let mut actions = runtime
            .pending_actions
            .lock()
            .map_err(|error| error.to_string())?;
        let Some(LocalBridgePendingAction::SendBundle(_)) = actions.first() else {
            return Ok(LocalBridgeBundleSendPreflightDto {
                status: "skipped".to_string(),
                request_id: None,
                reason: Some("no_bundle_send_action".to_string()),
                message: "no pending local bridge bundle send action".to_string(),
                client_id: None,
                client_display_name: None,
                client_app_kind: None,
                bundle_id: None,
                bundle_type: None,
                bundle_root: None,
                target_device_id: None,
                require_trusted_device: None,
                requested_at_ms: None,
                claimed_at_ms: Some(now_ms),
            });
        };
        match actions.remove(0) {
            LocalBridgePendingAction::SendBundle(action) => action,
            LocalBridgePendingAction::ImportBundle(_) => unreachable!("first action checked above"),
            LocalBridgePendingAction::RollbackBundleImport(_) => {
                unreachable!("first action checked above")
            }
        }
    };

    let result = preflight_local_bridge_bundle_send_action(action, trusted_devices, now_ms)?;
    push_local_bridge_pending_action_result(runtime, &result)?;
    Ok(result)
}

pub(crate) fn execute_next_local_bridge_bundle_import_at(
    runtime: &LocalBridgeRuntimeState,
    staging_root: &std::path::Path,
    import_root: &std::path::Path,
    now_ms: u128,
) -> Result<Option<LocalBridgePendingActionResultDto>, String> {
    let action = match take_next_local_bridge_pending_action_raw(runtime)? {
        Some(LocalBridgePendingAction::ImportBundle(action)) => action,
        Some(LocalBridgePendingAction::SendBundle(action)) => {
            push_front_local_bridge_pending_action(
                runtime,
                LocalBridgePendingAction::SendBundle(action),
            )?;
            return Ok(None);
        }
        Some(LocalBridgePendingAction::RollbackBundleImport(action)) => {
            push_front_local_bridge_pending_action(
                runtime,
                LocalBridgePendingAction::RollbackBundleImport(action),
            )?;
            return Ok(None);
        }
        None => return Ok(None),
    };

    push_local_bridge_action_lifecycle_result(
        runtime,
        local_bridge_action_lifecycle_result(
            &LocalBridgePendingAction::ImportBundle(action.clone()),
            LocalBridgeActionLifecycleStatus::Running,
            None,
            "local bridge bundle import is running",
            None,
            action.expected_bundle_type,
            None,
            now_ms,
        ),
    )?;
    let result =
        execute_local_bridge_bundle_import_action(action, staging_root, import_root, now_ms)?;
    push_local_bridge_action_lifecycle_result(runtime, result.clone())?;
    Ok(Some(local_bridge_pending_action_result_to_dto(
        &result, false,
    )))
}

pub(crate) fn execute_next_local_bridge_bundle_send_at(
    state: &AppState,
    trusted_devices: &[TrustedDeviceRecord],
    now_ms: u128,
) -> Result<Option<LocalBridgePendingActionResultDto>, String> {
    execute_next_local_bridge_bundle_send_with(
        &state.local_bridge_runtime,
        trusted_devices,
        now_ms,
        |action| {
            let target_device_id = action
                .target_device_id
                .as_deref()
                .ok_or_else(|| "local bridge bundle send requires target_device_id".to_string())?;
            let (endpoint, peer) = endpoint_and_peer_for_device_id(state, target_device_id)?;
            send_paths_to_endpoint(state, endpoint, action.bundle_root.clone(), peer).map(|_| ())
        },
    )
}

pub(crate) fn execute_local_bridge_bundle_send_action_at(
    state: &AppState,
    action: LocalBridgePendingSendBundleAction,
    now_ms: u128,
) -> Result<LocalBridgePendingActionResultDto, String> {
    let trusted_devices = state
        .trusted_devices
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    execute_local_bridge_bundle_send_action_with(
        &state.local_bridge_runtime,
        &trusted_devices,
        action,
        now_ms,
        |action| {
            let target_device_id = action
                .target_device_id
                .as_deref()
                .ok_or_else(|| "local bridge bundle send requires target_device_id".to_string())?;
            let (endpoint, peer) = endpoint_and_peer_for_device_id(state, target_device_id)?;
            send_paths_to_endpoint(state, endpoint, action.bundle_root.clone(), peer).map(|_| ())
        },
    )
}

pub(crate) fn execute_next_local_bridge_bundle_send_with<S>(
    runtime: &LocalBridgeRuntimeState,
    trusted_devices: &[TrustedDeviceRecord],
    now_ms: u128,
    mut send_bundle: S,
) -> Result<Option<LocalBridgePendingActionResultDto>, String>
where
    S: FnMut(&LocalBridgePendingSendBundleAction) -> Result<(), String>,
{
    let action = match take_next_local_bridge_pending_action_raw(runtime)? {
        Some(LocalBridgePendingAction::SendBundle(action)) => action,
        Some(LocalBridgePendingAction::ImportBundle(action)) => {
            push_front_local_bridge_pending_action(
                runtime,
                LocalBridgePendingAction::ImportBundle(action),
            )?;
            return Ok(None);
        }
        Some(LocalBridgePendingAction::RollbackBundleImport(action)) => {
            push_front_local_bridge_pending_action(
                runtime,
                LocalBridgePendingAction::RollbackBundleImport(action),
            )?;
            return Ok(None);
        }
        None => return Ok(None),
    };

    execute_local_bridge_bundle_send_action_with(
        runtime,
        trusted_devices,
        action,
        now_ms,
        &mut send_bundle,
    )
    .map(Some)
}

pub(crate) fn execute_local_bridge_bundle_send_action_with<S>(
    runtime: &LocalBridgeRuntimeState,
    trusted_devices: &[TrustedDeviceRecord],
    action: LocalBridgePendingSendBundleAction,
    now_ms: u128,
    mut send_bundle: S,
) -> Result<LocalBridgePendingActionResultDto, String>
where
    S: FnMut(&LocalBridgePendingSendBundleAction) -> Result<(), String>,
{
    push_local_bridge_action_lifecycle_result(
        runtime,
        local_bridge_action_lifecycle_result(
            &LocalBridgePendingAction::SendBundle(action.clone()),
            LocalBridgeActionLifecycleStatus::Running,
            None,
            "local bridge bundle send is running",
            None,
            Some(action.bundle_type),
            action.target_device_id.as_deref(),
            now_ms,
        ),
    )?;
    let preflight =
        preflight_local_bridge_bundle_send_action(action.clone(), trusted_devices, now_ms)?;
    let result = if preflight.status != "ready" {
        local_bridge_bundle_send_result_from_preflight("failed", &preflight, &action, now_ms)
    } else if action.target_device_id.as_deref().is_none() {
        local_bridge_bundle_send_result(
            "failed",
            &action,
            preflight.bundle_id.as_deref(),
            preflight
                .bundle_type
                .as_deref()
                .and_then(bundle_type_from_label),
            Some("target_device_required"),
            "local bridge bundle send requires target_device_id before desktop execution",
            now_ms,
        )
    } else {
        match send_bundle(&action) {
            Ok(()) => local_bridge_bundle_send_result(
                "completed",
                &action,
                preflight.bundle_id.as_deref(),
                preflight
                    .bundle_type
                    .as_deref()
                    .and_then(bundle_type_from_label),
                None,
                "local bridge bundle was sent by the desktop runtime",
                now_ms,
            ),
            Err(error) => local_bridge_bundle_send_result(
                "failed",
                &action,
                preflight.bundle_id.as_deref(),
                preflight
                    .bundle_type
                    .as_deref()
                    .and_then(bundle_type_from_label),
                Some("bundle_send_failed"),
                &format!(
                    "local bridge bundle send failed: {}",
                    friendly_transfer_error(&error)
                ),
                now_ms,
            ),
        }
    };
    push_local_bridge_action_lifecycle_result(runtime, result.clone())?;
    Ok(local_bridge_pending_action_result_to_dto(&result, false))
}

pub(crate) fn execute_local_bridge_bundle_import_action(
    action: LocalBridgePendingImportBundleAction,
    staging_root: &std::path::Path,
    import_root: &std::path::Path,
    now_ms: u128,
) -> Result<LocalBridgePendingActionResult, String> {
    validate_safe_bundle_id(&action.staged_bundle_id)?;
    let staged_path = staging_root.join(&action.staged_bundle_id);
    let detected = match detect_bundle_directory(&staged_path) {
        Ok(Some(detected)) => detected,
        Ok(None) => {
            return Ok(local_bridge_bundle_import_result(
                "failed",
                &action,
                Some("bundle_manifest_missing"),
                "local bridge staged bundle does not contain bundle.json",
                None,
                None,
                0,
                None,
                0,
                now_ms,
            ));
        }
        Err(error) => {
            return Ok(local_bridge_bundle_import_result(
                "failed",
                &action,
                Some("bundle_invalid"),
                &format!("local bridge staged bundle validation failed: {error}"),
                None,
                None,
                0,
                None,
                0,
                now_ms,
            ));
        }
    };

    let bundle_id = detected.manifest.bundle_id.clone();
    let bundle_type = detected.manifest.bundle_type;
    if let Some(expected_bundle_type) = action.expected_bundle_type {
        if bundle_type != expected_bundle_type {
            return Ok(local_bridge_bundle_import_result(
                "failed",
                &action,
                Some("bundle_type_mismatch"),
                "local bridge expected bundle_type does not match the staged bundle manifest",
                Some(bundle_id.as_str()),
                Some(bundle_type),
                0,
                None,
                0,
                now_ms,
            ));
        }
    }

    let conflict_strategy =
        parse_import_conflict_strategy(Some(action.conflict_strategy.as_str()))?;
    match import_staged_bundle_with_strategy_at(
        staging_root,
        import_root,
        &action.staged_bundle_id,
        conflict_strategy,
    ) {
        Ok(imported) => Ok(local_bridge_bundle_import_result(
            "completed",
            &action,
            None,
            "local bridge staged bundle was imported",
            Some(imported.bundle_id.as_str()),
            bundle_type_from_label(&imported.bundle_type).or(Some(bundle_type)),
            imported.import_skipped_file_count,
            imported.import_receipt_path.as_deref(),
            imported.rollback_file_count,
            now_ms,
        )),
        Err(error) => {
            let reason = local_bridge_bundle_import_failure_reason(&error);
            Ok(local_bridge_bundle_import_result(
                "failed",
                &action,
                Some(reason),
                &format!("local bridge staged bundle import failed: {error}"),
                Some(bundle_id.as_str()),
                Some(bundle_type),
                0,
                None,
                0,
                now_ms,
            ))
        }
    }
}

pub(crate) fn execute_local_bridge_bundle_rollback_action(
    action: LocalBridgePendingRollbackBundleImportAction,
    import_root: &std::path::Path,
    now_ms: u128,
) -> Result<LocalBridgePendingActionResult, String> {
    validate_safe_bundle_id(&action.bundle_id)?;
    match rollback_imported_bundle_at(import_root, &action.bundle_id) {
        Ok(rolled_back) => Ok(local_bridge_bundle_rollback_result(
            "completed",
            &action,
            None,
            None,
            "local bridge bundle import was rolled back",
            rolled_back.rolled_back_file_count,
            now_ms,
        )),
        Err(error) => {
            let reason = local_bridge_bundle_rollback_failure_reason(&error);
            let rollback_blocking_reason = local_bridge_bundle_rollback_blocking_reason(&error);
            Ok(local_bridge_bundle_rollback_result(
                "failed",
                &action,
                Some(reason),
                rollback_blocking_reason,
                &format!("local bridge bundle rollback failed: {error}"),
                0,
                now_ms,
            ))
        }
    }
}

pub(crate) fn local_bridge_bundle_import_failure_reason(error: &str) -> &'static str {
    if error.contains("destination already exists") {
        return "bundle_import_conflict";
    }
    "bundle_import_failed"
}

pub(crate) fn local_bridge_bundle_rollback_failure_reason(error: &str) -> &'static str {
    if error.contains("没有找到资料包导入记录") {
        return "bundle_import_receipt_missing";
    }
    if error.contains("destination_missing")
        || error.contains("imported_file_missing")
        || error.contains("already_rolled_back")
    {
        return "bundle_rollback_blocked";
    }
    "bundle_rollback_failed"
}

pub(crate) fn local_bridge_bundle_rollback_blocking_reason(error: &str) -> Option<&'static str> {
    if error.contains("destination_missing") {
        return Some("destination_missing");
    }
    if error.contains("imported_file_missing") {
        return Some("imported_file_missing");
    }
    if error.contains("already_rolled_back") {
        return Some("already_rolled_back");
    }
    None
}

pub(crate) fn preflight_local_bridge_bundle_send_action(
    action: LocalBridgePendingSendBundleAction,
    trusted_devices: &[TrustedDeviceRecord],
    now_ms: u128,
) -> Result<LocalBridgeBundleSendPreflightDto, String> {
    let bundle_root = Path::new(&action.bundle_root);
    if !bundle_root.exists() || !bundle_root.is_dir() {
        return Ok(local_bridge_bundle_send_preflight_result(
            "failed_preflight",
            &action,
            None,
            None,
            Some("bundle_root_missing"),
            "local bridge bundle_root is missing or is not a directory",
            now_ms,
        ));
    }

    let detected = match detect_bundle_directory(bundle_root) {
        Ok(Some(detected)) => detected,
        Ok(None) => {
            return Ok(local_bridge_bundle_send_preflight_result(
                "failed_preflight",
                &action,
                None,
                None,
                Some("bundle_manifest_missing"),
                "local bridge bundle_root does not contain bundle.json",
                now_ms,
            ));
        }
        Err(error) => {
            return Ok(local_bridge_bundle_send_preflight_result(
                "failed_preflight",
                &action,
                None,
                None,
                Some("bundle_invalid"),
                &format!("local bridge bundle validation failed: {error}"),
                now_ms,
            ));
        }
    };

    let detected_type = detected.manifest.bundle_type;
    if detected_type != action.bundle_type {
        return Ok(local_bridge_bundle_send_preflight_result(
            "failed_preflight",
            &action,
            Some(detected.manifest.bundle_id.as_str()),
            Some(detected_type),
            Some("bundle_type_mismatch"),
            "local bridge bundle_type does not match the detected bundle manifest",
            now_ms,
        ));
    }

    if detected_type.requires_authenticated_encrypted_session() && !action.require_trusted_device {
        return Ok(local_bridge_bundle_send_preflight_result(
            "failed_preflight",
            &action,
            Some(detected.manifest.bundle_id.as_str()),
            Some(detected_type),
            Some("sensitive_bundle_requires_trusted_device"),
            "local bridge sensitive bundle send requires a trusted authenticated session target",
            now_ms,
        ));
    }

    if action.require_trusted_device {
        let Some(target_device_id) = action.target_device_id.as_deref() else {
            return Ok(local_bridge_bundle_send_preflight_result(
                "failed_preflight",
                &action,
                Some(detected.manifest.bundle_id.as_str()),
                Some(detected_type),
                Some("trusted_target_required"),
                "local bridge bundle send requires a trusted target device",
                now_ms,
            ));
        };
        if !trusted_devices
            .iter()
            .any(|device| device.device_id == target_device_id)
        {
            return Ok(local_bridge_bundle_send_preflight_result(
                "failed_preflight",
                &action,
                Some(detected.manifest.bundle_id.as_str()),
                Some(detected_type),
                Some("trusted_target_missing"),
                "local bridge bundle send target is not a trusted device",
                now_ms,
            ));
        }
    }

    Ok(local_bridge_bundle_send_preflight_result(
        "ready",
        &action,
        Some(detected.manifest.bundle_id.as_str()),
        Some(detected_type),
        None,
        "local bridge bundle send is ready for the desktop send worker",
        now_ms,
    ))
}

pub(crate) fn local_bridge_bundle_send_preflight_result(
    status: &str,
    action: &LocalBridgePendingSendBundleAction,
    bundle_id: Option<&str>,
    bundle_type: Option<BundleType>,
    reason: Option<&str>,
    message: &str,
    now_ms: u128,
) -> LocalBridgeBundleSendPreflightDto {
    LocalBridgeBundleSendPreflightDto {
        status: status.to_string(),
        request_id: Some(action.request_id.clone()),
        reason: reason.map(str::to_string),
        message: message.to_string(),
        client_id: Some(action.client.client_id.clone()),
        client_display_name: Some(action.client.display_name.clone()),
        client_app_kind: action.client.app_kind.clone(),
        bundle_id: bundle_id.map(str::to_string),
        bundle_type: bundle_type.map(bundle_type_label).map(str::to_string),
        bundle_root: Some(action.bundle_root.clone()),
        target_device_id: action.target_device_id.clone(),
        require_trusted_device: Some(action.require_trusted_device),
        requested_at_ms: Some(action.requested_at_ms),
        claimed_at_ms: Some(now_ms),
    }
}

pub(crate) fn push_local_bridge_pending_action_result(
    runtime: &LocalBridgeRuntimeState,
    result: &LocalBridgeBundleSendPreflightDto,
) -> Result<(), String> {
    let Some(request_id) = result.request_id.clone() else {
        return Ok(());
    };
    let Some(client_id) = result.client_id.clone() else {
        return Ok(());
    };
    let Some(client_display_name) = result.client_display_name.clone() else {
        return Ok(());
    };
    let client_app_kind = result.client_app_kind.clone();
    let Some(requested_at_ms) = result.requested_at_ms else {
        return Ok(());
    };
    let Some(claimed_at_ms) = result.claimed_at_ms else {
        return Ok(());
    };
    let event_id = format!("bridge-action-{request_id}-{claimed_at_ms}");

    let result = LocalBridgePendingActionResult {
        request_id: request_id.clone(),
        action_kind: "bundle.send".to_string(),
        client_id: client_id.clone(),
        client_display_name,
        client_app_kind,
        status: result.status.clone(),
        lifecycle_status: None,
        reason: result.reason.clone(),
        message: result.message.clone(),
        bundle_id: result.bundle_id.clone(),
        bundle_type: result.bundle_type.clone(),
        bundle_root: result.bundle_root.clone(),
        target_device_id: result.target_device_id.clone(),
        require_trusted_device: result.require_trusted_device,
        conflict_strategy: None,
        skipped_file_count: 0,
        import_receipt_path: None,
        rollback_file_count: 0,
        rollback_blocking_reason: None,
        rolled_back_file_count: 0,
        requested_at_ms,
        claimed_at_ms,
    };
    push_local_bridge_pending_action_result_record(runtime, result.clone())?;

    let status = match result.status.as_str() {
        "ready" => LocalBridgeBundleSendPreflightStatus::Ready,
        "failed_preflight" => LocalBridgeBundleSendPreflightStatus::FailedPreflight,
        _ => return Ok(()),
    };
    push_local_bridge_runtime_event(
        runtime,
        LocalBridgeEvent::BundleSendPreflight(LocalBridgeBundleSendPreflightEvent {
            event_id,
            request_id,
            client_id,
            client_app_kind: result.client_app_kind.clone(),
            status,
            reason: result.reason.clone(),
            bundle_id: result.bundle_id.clone(),
            bundle_type: result
                .bundle_type
                .as_deref()
                .and_then(bundle_type_from_label),
            target_device_id: result.target_device_id.clone(),
        }),
    )?;
    Ok(())
}

pub(crate) fn push_local_bridge_pending_action_result_record(
    runtime: &LocalBridgeRuntimeState,
    result: LocalBridgePendingActionResult,
) -> Result<(), String> {
    let mut results = runtime
        .pending_action_results
        .lock()
        .map_err(|error| error.to_string())?;
    results.retain(|existing| {
        existing.request_id != result.request_id
            || existing.action_kind != result.action_kind
            || existing.client_id != result.client_id
            || existing.client_app_kind != result.client_app_kind
    });
    results.push(result);
    if results.len() > LOCAL_BRIDGE_PENDING_ACTION_RESULT_LIMIT {
        let excess = results.len() - LOCAL_BRIDGE_PENDING_ACTION_RESULT_LIMIT;
        results.drain(0..excess);
    }
    runtime.events_signal.notify_all();
    Ok(())
}

pub(crate) fn push_local_bridge_action_lifecycle_result(
    runtime: &LocalBridgeRuntimeState,
    result: LocalBridgePendingActionResult,
) -> Result<(), String> {
    push_local_bridge_pending_action_result_record(runtime, result.clone())?;
    push_local_bridge_runtime_event(
        runtime,
        LocalBridgeEvent::ActionUpdated(LocalBridgeActionUpdatedEvent {
            event_id: format!(
                "bridge-action-{}-{}-{}",
                result.request_id,
                result
                    .lifecycle_status
                    .as_deref()
                    .unwrap_or(result.status.as_str()),
                result.claimed_at_ms
            ),
            request_id: result.request_id,
            action_kind: result.action_kind,
            client_id: result.client_id,
            client_app_kind: result.client_app_kind,
            status: local_bridge_lifecycle_status_from_label(
                result
                    .lifecycle_status
                    .as_deref()
                    .unwrap_or(result.status.as_str()),
            ),
            reason: result.reason,
            message: result.message,
            bundle_id: result.bundle_id,
            bundle_type: result
                .bundle_type
                .as_deref()
                .and_then(bundle_type_from_label),
            target_device_id: result.target_device_id,
            updated_at_ms: result.claimed_at_ms,
        }),
    )
}

pub(crate) fn local_bridge_lifecycle_status_from_label(
    label: &str,
) -> LocalBridgeActionLifecycleStatus {
    match label {
        "queued" => LocalBridgeActionLifecycleStatus::Queued,
        "running" => LocalBridgeActionLifecycleStatus::Running,
        "succeeded" => LocalBridgeActionLifecycleStatus::Succeeded,
        "conflict" => LocalBridgeActionLifecycleStatus::Conflict,
        "cancelled" => LocalBridgeActionLifecycleStatus::Cancelled,
        _ => LocalBridgeActionLifecycleStatus::Failed,
    }
}

pub(crate) fn push_front_local_bridge_pending_action(
    runtime: &LocalBridgeRuntimeState,
    action: LocalBridgePendingAction,
) -> Result<(), String> {
    let mut actions = runtime
        .pending_actions
        .lock()
        .map_err(|error| error.to_string())?;
    actions.insert(0, action);
    runtime.pending_actions_signal.notify_one();
    Ok(())
}

pub(crate) fn list_local_bridge_pending_action_results_at(
    runtime: &LocalBridgeRuntimeState,
) -> Result<Vec<LocalBridgePendingActionResultDto>, String> {
    let results = runtime
        .pending_action_results
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(results
        .iter()
        .map(|result| local_bridge_pending_action_result_to_dto(result, false))
        .collect())
}

pub(crate) fn take_local_bridge_pending_action_by_request_id(
    runtime: &LocalBridgeRuntimeState,
    request_id: &str,
) -> Result<Option<LocalBridgePendingAction>, String> {
    if request_id.trim().is_empty() {
        return Err("request_id 不能为空".to_string());
    }
    let mut actions = runtime
        .pending_actions
        .lock()
        .map_err(|error| error.to_string())?;
    let Some(position) = actions
        .iter()
        .position(|action| local_bridge_pending_action_request_id(action) == request_id)
    else {
        return Ok(None);
    };
    Ok(Some(actions.remove(position)))
}

pub(crate) fn remove_local_bridge_pending_action_at(
    runtime: &LocalBridgeRuntimeState,
    request_id: &str,
) -> Result<bool, String> {
    let Some(action) = take_local_bridge_pending_action_by_request_id(runtime, request_id)? else {
        return Ok(false);
    };
    push_local_bridge_action_lifecycle_result(
        runtime,
        local_bridge_action_lifecycle_result(
            &action,
            LocalBridgeActionLifecycleStatus::Cancelled,
            None,
            "local bridge action was cancelled before execution",
            None,
            match &action {
                LocalBridgePendingAction::SendBundle(action) => Some(action.bundle_type),
                LocalBridgePendingAction::ImportBundle(action) => action.expected_bundle_type,
                LocalBridgePendingAction::RollbackBundleImport(_) => None,
            },
            match &action {
                LocalBridgePendingAction::SendBundle(action) => action.target_device_id.as_deref(),
                LocalBridgePendingAction::ImportBundle(_) => None,
                LocalBridgePendingAction::RollbackBundleImport(_) => None,
            },
            now_ms(),
        ),
    )?;
    Ok(true)
}

pub(crate) fn local_bridge_pending_action_request_id(action: &LocalBridgePendingAction) -> &str {
    match action {
        LocalBridgePendingAction::SendBundle(action) => &action.request_id,
        LocalBridgePendingAction::ImportBundle(action) => &action.request_id,
        LocalBridgePendingAction::RollbackBundleImport(action) => &action.request_id,
    }
}

pub(crate) fn local_bridge_pending_send_action_from_request(
    request: &nekolink_protocol::LocalBridgeSendBundleRequest,
    now_ms: u128,
) -> Result<LocalBridgePendingSendBundleAction, String> {
    let client = request
        .client
        .clone()
        .ok_or_else(|| "authorized local bridge send requires a client identity".to_string())?;
    Ok(LocalBridgePendingSendBundleAction {
        request_id: request.request_id.clone(),
        client,
        target_device_id: request.target_device_id.clone(),
        bundle_root: request.bundle_root.clone(),
        bundle_type: request.bundle_type,
        require_trusted_device: request.require_trusted_device,
        requested_at_ms: now_ms,
    })
}

pub(crate) fn local_bridge_pending_import_action_from_request(
    request: &nekolink_protocol::LocalBridgeImportBundleRequest,
    now_ms: u128,
) -> Result<LocalBridgePendingImportBundleAction, String> {
    let client = request
        .client
        .clone()
        .ok_or_else(|| "authorized local bridge import requires a client identity".to_string())?;
    Ok(LocalBridgePendingImportBundleAction {
        request_id: request.request_id.clone(),
        client,
        staged_bundle_id: request.staged_bundle_id.clone(),
        expected_bundle_type: request.expected_bundle_type,
        conflict_strategy: request
            .conflict_strategy
            .clone()
            .unwrap_or_else(|| "reject".to_string()),
        requested_at_ms: now_ms,
    })
}

pub(crate) fn local_bridge_pending_rollback_action_from_request(
    request: &nekolink_protocol::LocalBridgeRollbackBundleImportRequest,
    now_ms: u128,
) -> Result<LocalBridgePendingRollbackBundleImportAction, String> {
    let client = request
        .client
        .clone()
        .ok_or_else(|| "authorized local bridge rollback requires a client identity".to_string())?;
    Ok(LocalBridgePendingRollbackBundleImportAction {
        request_id: request.request_id.clone(),
        client,
        bundle_id: request.bundle_id.clone(),
        requested_at_ms: now_ms,
    })
}
