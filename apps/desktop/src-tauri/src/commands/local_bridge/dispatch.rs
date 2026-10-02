use super::super::*;
use super::*;

#[tauri::command(async)]
pub fn handle_local_bridge_request(
    state: State<'_, AppState>,
    request_json: String,
) -> Result<LocalBridgeResponseDto, String> {
    handle_local_bridge_request_for_runtime(
        &state.trusted_devices,
        &state.transfer_status,
        &state.local_bridge_runtime,
        &request_json,
    )
}

pub(crate) fn handle_local_bridge_request_at(
    request_json: &str,
    trusted_devices: &[TrustedDeviceRecord],
    transfer_status: Option<&TransferStatusState>,
    staging_root: &std::path::Path,
) -> Result<LocalBridgeResponseDto, String> {
    let import_root = bundle_import_root()?;
    handle_local_bridge_request_with_auth_at(
        request_json,
        trusted_devices,
        transfer_status,
        staging_root,
        &import_root,
        &[],
        now_ms(),
    )
}

pub(crate) fn handle_local_bridge_request_for_runtime(
    trusted_devices: &Arc<Mutex<Vec<TrustedDeviceRecord>>>,
    transfer_status: &Arc<Mutex<Option<TransferStatusState>>>,
    runtime: &LocalBridgeRuntimeState,
    request_json: &str,
) -> Result<LocalBridgeResponseDto, String> {
    let trusted_devices = trusted_devices
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let transfer_status = transfer_status
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let staging_root = bundle_staging_root()?;
    let import_root = bundle_import_root()?;
    handle_local_bridge_request_with_runtime_at(
        request_json,
        &trusted_devices,
        transfer_status.as_ref(),
        &staging_root,
        &import_root,
        runtime,
        true,
        now_ms(),
    )
}

pub(crate) fn handle_local_bridge_request_with_runtime_at(
    request_json: &str,
    trusted_devices: &[TrustedDeviceRecord],
    transfer_status: Option<&TransferStatusState>,
    staging_root: &std::path::Path,
    import_root: &std::path::Path,
    runtime: &LocalBridgeRuntimeState,
    allow_wait: bool,
    now_ms: u128,
) -> Result<LocalBridgeResponseDto, String> {
    let request: LocalBridgeRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid bridge request JSON: {error}"))?;
    request.validate().map_err(|error| error.message)?;

    if let LocalBridgeRequest::AuthorizationRequest(request) = &request {
        let pending = pending_local_bridge_authorization_from_request(request, now_ms)?;
        let response = local_bridge_pending_authorization_response_from_pending(&pending);
        *runtime
            .pending_authorization
            .lock()
            .map_err(|error| error.to_string())? = Some(pending);
        return Ok(response);
    }

    let authorizations = runtime
        .authorizations
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let events = runtime
        .events
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let action_results = runtime
        .pending_action_results
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let pending_actions = runtime
        .pending_actions
        .lock()
        .map_err(|error| error.to_string())?
        .clone();

    if let LocalBridgeRequest::SendBundle(request) = &request {
        if local_bridge_client_has_scope(
            request.client.as_ref(),
            &authorizations,
            LocalBridgePermissionScope::BundleSend,
            now_ms,
        ) {
            if let Some(retry_response) =
                local_bridge_retry_send_response(request, &pending_actions, &action_results)
            {
                mark_local_bridge_authorization_used(
                    runtime,
                    request.client.as_ref(),
                    LocalBridgePermissionScope::BundleSend,
                    now_ms,
                )?;
                return Ok(retry_response);
            }
            let action = LocalBridgePendingAction::SendBundle(
                local_bridge_pending_send_action_from_request(request, now_ms)?,
            );
            if let Some(result) = local_bridge_existing_action_result_for_retry(
                &pending_actions,
                &action_results,
                &action,
            ) {
                mark_local_bridge_authorization_used(
                    runtime,
                    request.client.as_ref(),
                    LocalBridgePermissionScope::BundleSend,
                    now_ms,
                )?;
                return Ok(local_bridge_action_results_response(
                    request.request_id.clone(),
                    request.client.clone(),
                    vec![result],
                ));
            }
            push_local_bridge_pending_action_queued(runtime, action, now_ms)?;
            mark_local_bridge_authorization_used(
                runtime,
                request.client.as_ref(),
                LocalBridgePermissionScope::BundleSend,
                now_ms,
            )?;
            return Ok(local_bridge_authorized_runtime_pending_response(
                request.request_id.clone(),
                request.client.clone(),
                "local bridge bundle send is authorized and waiting for the desktop runtime",
            ));
        }
    }

    if let LocalBridgeRequest::ImportBundle(request) = &request {
        if local_bridge_client_has_scope(
            request.client.as_ref(),
            &authorizations,
            LocalBridgePermissionScope::BundleImportRequest,
            now_ms,
        ) {
            if let Some(retry_response) =
                local_bridge_retry_import_response(request, &pending_actions, &action_results)
            {
                mark_local_bridge_authorization_used(
                    runtime,
                    request.client.as_ref(),
                    LocalBridgePermissionScope::BundleImportRequest,
                    now_ms,
                )?;
                return Ok(retry_response);
            }
            let action = LocalBridgePendingAction::ImportBundle(
                local_bridge_pending_import_action_from_request(request, now_ms)?,
            );
            if let Some(result) = local_bridge_existing_action_result_for_retry(
                &pending_actions,
                &action_results,
                &action,
            ) {
                mark_local_bridge_authorization_used(
                    runtime,
                    request.client.as_ref(),
                    LocalBridgePermissionScope::BundleImportRequest,
                    now_ms,
                )?;
                return Ok(local_bridge_action_results_response(
                    request.request_id.clone(),
                    request.client.clone(),
                    vec![result],
                ));
            }
            push_local_bridge_pending_action_queued(runtime, action, now_ms)?;
            mark_local_bridge_authorization_used(
                runtime,
                request.client.as_ref(),
                LocalBridgePermissionScope::BundleImportRequest,
                now_ms,
            )?;
            return Ok(local_bridge_authorized_runtime_pending_response(
                request.request_id.clone(),
                request.client.clone(),
                "local bridge bundle import is authorized and waiting for the desktop runtime",
            ));
        }
    }

    if let LocalBridgeRequest::RollbackBundleImport(request) = &request {
        if local_bridge_client_has_scope(
            request.client.as_ref(),
            &authorizations,
            LocalBridgePermissionScope::BundleImportRequest,
            now_ms,
        ) {
            if let Some(retry_response) =
                local_bridge_retry_rollback_response(request, &pending_actions, &action_results)
            {
                mark_local_bridge_authorization_used(
                    runtime,
                    request.client.as_ref(),
                    LocalBridgePermissionScope::BundleImportRequest,
                    now_ms,
                )?;
                return Ok(retry_response);
            }
            let action = LocalBridgePendingAction::RollbackBundleImport(
                local_bridge_pending_rollback_action_from_request(request, now_ms)?,
            );
            if let Some(result) = local_bridge_existing_action_result_for_retry(
                &pending_actions,
                &action_results,
                &action,
            ) {
                mark_local_bridge_authorization_used(
                    runtime,
                    request.client.as_ref(),
                    LocalBridgePermissionScope::BundleImportRequest,
                    now_ms,
                )?;
                return Ok(local_bridge_action_results_response(
                    request.request_id.clone(),
                    request.client.clone(),
                    vec![result],
                ));
            }
            push_local_bridge_pending_action_queued(runtime, action, now_ms)?;
            mark_local_bridge_authorization_used(
                runtime,
                request.client.as_ref(),
                LocalBridgePermissionScope::BundleImportRequest,
                now_ms,
            )?;
            return Ok(local_bridge_authorized_runtime_pending_response(
                request.request_id.clone(),
                request.client.clone(),
                "local bridge bundle rollback is authorized and waiting for the desktop runtime",
            ));
        }
    }

    let request_for_usage = request.clone();
    let used_client = local_bridge_request_client(&request).cloned();
    let response = handle_validated_local_bridge_request_with_auth_at(
        request,
        trusted_devices,
        transfer_status,
        staging_root,
        import_root,
        &authorizations,
        &events,
        &pending_actions,
        &action_results,
        now_ms,
    )?;

    if !allow_wait || response.status != "ok" || !response.events.is_empty() {
        mark_local_bridge_authorization_used_for_response(
            runtime,
            used_client.as_ref(),
            &request_for_usage,
            &response,
            now_ms,
        )?;
        return Ok(response);
    }

    let request: LocalBridgeRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid bridge request JSON: {error}"))?;
    let LocalBridgeRequest::PollEvents(request) = request else {
        return Ok(response);
    };
    let Some(timeout_ms) = request.timeout_ms else {
        return Ok(response);
    };
    if timeout_ms == 0 {
        mark_local_bridge_authorization_used_for_response(
            runtime,
            used_client.as_ref(),
            &request_for_usage,
            &response,
            now_ms,
        )?;
        return Ok(response);
    }

    let response = wait_for_local_bridge_events(
        runtime,
        request,
        trusted_devices,
        transfer_status,
        staging_root,
        import_root,
        &authorizations,
        now_ms,
        Duration::from_millis(timeout_ms.min(30_000)),
    )?;
    mark_local_bridge_authorization_used_for_response(
        runtime,
        used_client.as_ref(),
        &request_for_usage,
        &response,
        now_ms,
    )?;
    Ok(response)
}

pub(crate) fn handle_local_bridge_request_with_auth_at(
    request_json: &str,
    trusted_devices: &[TrustedDeviceRecord],
    transfer_status: Option<&TransferStatusState>,
    staging_root: &std::path::Path,
    import_root: &std::path::Path,
    authorizations: &[LocalBridgeAuthorizationRecord],
    now_ms: u128,
) -> Result<LocalBridgeResponseDto, String> {
    let request: LocalBridgeRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("invalid bridge request JSON: {error}"))?;
    request.validate().map_err(|error| error.message)?;
    handle_validated_local_bridge_request_with_auth_at(
        request,
        trusted_devices,
        transfer_status,
        staging_root,
        import_root,
        authorizations,
        &[],
        &[],
        &[],
        now_ms,
    )
}

pub(crate) fn handle_validated_local_bridge_request_with_auth_at(
    request: LocalBridgeRequest,
    trusted_devices: &[TrustedDeviceRecord],
    transfer_status: Option<&TransferStatusState>,
    staging_root: &std::path::Path,
    import_root: &std::path::Path,
    authorizations: &[LocalBridgeAuthorizationRecord],
    events: &[LocalBridgeEvent],
    pending_actions: &[LocalBridgePendingAction],
    action_results: &[LocalBridgePendingActionResult],
    now_ms: u128,
) -> Result<LocalBridgeResponseDto, String> {
    match request {
        LocalBridgeRequest::ListDevices(request) => {
            if !local_bridge_client_has_scope(
                request.client.as_ref(),
                authorizations,
                LocalBridgePermissionScope::DeviceRead,
                now_ms,
            ) {
                return Ok(local_bridge_pending_confirmation_response(
                    request.request_id,
                    request.client,
                ));
            }
            let can_read_bundles = local_bridge_client_has_scope(
                request.client.as_ref(),
                authorizations,
                LocalBridgePermissionScope::BundleRead,
                now_ms,
            );
            let client = request.client.clone();
            Ok(local_bridge_read_only_response(
                request.request_id,
                client,
                "local bridge read-only snapshot",
                trusted_devices.iter().map(trusted_device_to_dto).collect(),
                if can_read_bundles {
                    list_staged_bundle_dtos_at(staging_root, import_root)?
                } else {
                    Vec::new()
                },
                None,
            ))
        }
        LocalBridgeRequest::TransferStatus(request) => {
            if !local_bridge_client_has_scope(
                request.client.as_ref(),
                authorizations,
                LocalBridgePermissionScope::TransferStatusRead,
                now_ms,
            ) {
                return Ok(local_bridge_pending_confirmation_response(
                    request.request_id,
                    request.client,
                ));
            }
            let client = request.client.clone();
            Ok(local_bridge_read_only_response(
                request.request_id,
                client,
                "local bridge transfer status snapshot",
                Vec::new(),
                Vec::new(),
                transfer_status.map(transfer_status_to_dto),
            ))
        }
        LocalBridgeRequest::BundleDetail(request) => {
            if !local_bridge_client_has_scope(
                request.client.as_ref(),
                authorizations,
                LocalBridgePermissionScope::BundleRead,
                now_ms,
            ) {
                return Ok(local_bridge_pending_confirmation_response(
                    request.request_id,
                    request.client,
                ));
            }
            let client = request.client.clone();
            let bundle =
                find_staged_bundle_dto_at(staging_root, import_root, &request.staged_bundle_id)?;
            let bundle = match bundle {
                Some(bundle) => Some(bundle),
                None => {
                    latest_bundle_import_receipt_dto_at(import_root, &request.staged_bundle_id)?
                }
            };
            match bundle {
                Some(bundle) => Ok(local_bridge_read_only_response(
                    request.request_id,
                    client,
                    "local bridge staged bundle detail",
                    Vec::new(),
                    vec![bundle],
                    None,
                )),
                None => Ok(local_bridge_read_only_unsupported_response(
                    request.request_id,
                    client,
                    "staged bundle not found",
                )),
            }
        }
        LocalBridgeRequest::PollEvents(request) => {
            let can_read_bundles = local_bridge_client_has_scope(
                request.client.as_ref(),
                authorizations,
                LocalBridgePermissionScope::BundleRead,
                now_ms,
            );
            let can_read_transfers = local_bridge_client_has_scope(
                request.client.as_ref(),
                authorizations,
                LocalBridgePermissionScope::TransferStatusRead,
                now_ms,
            );
            let can_send_bundles = local_bridge_client_has_scope(
                request.client.as_ref(),
                authorizations,
                LocalBridgePermissionScope::BundleSend,
                now_ms,
            );
            let can_import_bundles = local_bridge_client_has_scope(
                request.client.as_ref(),
                authorizations,
                LocalBridgePermissionScope::BundleImportRequest,
                now_ms,
            );
            if !can_read_bundles && !can_read_transfers && !can_send_bundles && !can_import_bundles
            {
                return Ok(local_bridge_pending_confirmation_response(
                    request.request_id,
                    request.client,
                ));
            }
            let bridge_events = local_bridge_events_after(
                events,
                request.client.as_ref(),
                request.after_event_id.as_deref(),
                request.action_request_id.as_deref(),
                request.limit.unwrap_or(50),
                can_read_bundles,
                can_read_transfers,
                can_send_bundles,
                can_import_bundles,
            )?;
            Ok(local_bridge_events_response(
                request.request_id,
                request.client,
                bridge_events,
            ))
        }
        LocalBridgeRequest::ActionResults(request) => {
            let action_results = local_bridge_action_results_for_client(
                request.client.as_ref(),
                authorizations,
                pending_actions,
                action_results,
                request.action_request_id.as_deref(),
                request.after_claimed_at_ms,
                request.limit.unwrap_or(50),
                now_ms,
            )?;
            if action_results.is_none() {
                return Ok(local_bridge_pending_confirmation_response(
                    request.request_id,
                    request.client,
                ));
            }
            Ok(local_bridge_action_results_response(
                request.request_id,
                request.client,
                action_results.unwrap_or_default(),
            ))
        }
        LocalBridgeRequest::SendBundle(request) => {
            if local_bridge_client_has_scope(
                request.client.as_ref(),
                authorizations,
                LocalBridgePermissionScope::BundleSend,
                now_ms,
            ) {
                Ok(local_bridge_authorized_runtime_pending_response(
                    request.request_id,
                    request.client,
                    "local bridge bundle send is authorized, but the send runtime is not connected yet",
                ))
            } else {
                Ok(local_bridge_pending_confirmation_response(
                    request.request_id,
                    request.client,
                ))
            }
        }
        LocalBridgeRequest::ImportBundle(request) => {
            if local_bridge_client_has_scope(
                request.client.as_ref(),
                authorizations,
                LocalBridgePermissionScope::BundleImportRequest,
                now_ms,
            ) {
                Ok(local_bridge_authorized_runtime_pending_response(
                    request.request_id,
                    request.client,
                    "local bridge bundle import is authorized, but the import runtime is not connected yet",
                ))
            } else {
                Ok(local_bridge_pending_confirmation_response(
                    request.request_id,
                    request.client,
                ))
            }
        }
        LocalBridgeRequest::RollbackBundleImport(request) => {
            if local_bridge_client_has_scope(
                request.client.as_ref(),
                authorizations,
                LocalBridgePermissionScope::BundleImportRequest,
                now_ms,
            ) {
                Ok(local_bridge_authorized_runtime_pending_response(
                    request.request_id,
                    request.client,
                    "local bridge bundle rollback is authorized, but the rollback runtime is not connected yet",
                ))
            } else {
                Ok(local_bridge_pending_confirmation_response(
                    request.request_id,
                    request.client,
                ))
            }
        }
        LocalBridgeRequest::AuthorizationRequest(request) => {
            Ok(local_bridge_pending_authorization_response(
                request.request_id,
                request.client,
                request.requested_scopes,
                request.reason,
                request.ttl_seconds,
                now_ms,
            ))
        }
    }
}

pub(crate) fn local_bridge_request_client(
    request: &LocalBridgeRequest,
) -> Option<&LocalBridgeClientIdentity> {
    match request {
        LocalBridgeRequest::ListDevices(request) => request.client.as_ref(),
        LocalBridgeRequest::SendBundle(request) => request.client.as_ref(),
        LocalBridgeRequest::BundleDetail(request) => request.client.as_ref(),
        LocalBridgeRequest::ImportBundle(request) => request.client.as_ref(),
        LocalBridgeRequest::RollbackBundleImport(request) => request.client.as_ref(),
        LocalBridgeRequest::AuthorizationRequest(request) => Some(&request.client),
        LocalBridgeRequest::TransferStatus(request) => request.client.as_ref(),
        LocalBridgeRequest::PollEvents(request) => request.client.as_ref(),
        LocalBridgeRequest::ActionResults(request) => request.client.as_ref(),
    }
}
