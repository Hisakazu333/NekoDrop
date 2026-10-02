use super::super::*;
use super::*;

#[tauri::command(async)]
pub fn confirm_local_bridge_authorization(
    state: State<'_, AppState>,
    authorization_code: String,
) -> Result<LocalBridgeAuthorizationDto, String> {
    let now_ms = now_ms();
    let authorization = confirm_local_bridge_runtime_authorization_and_persist(
        &state.local_bridge_runtime,
        &authorization_code,
        now_ms,
    )?;
    Ok(local_bridge_authorization_to_dto(authorization))
}

#[tauri::command(async)]
pub fn get_local_bridge_runtime_status(
    state: State<'_, AppState>,
) -> Result<LocalBridgeRuntimeStatusDto, String> {
    Ok(local_bridge_runtime_status_to_dto(
        local_bridge_runtime::local_bridge_runtime_status(&state.local_bridge_runtime),
    ))
}

#[tauri::command(async)]
pub fn list_local_bridge_authorizations(
    state: State<'_, AppState>,
) -> Result<LocalBridgeAuthorizationListDto, String> {
    let now_ms = now_ms();
    let pruned_count =
        prune_local_bridge_authorizations_and_persist(&state.local_bridge_runtime, now_ms)?;
    Ok(LocalBridgeAuthorizationListDto {
        authorizations: local_bridge_authorizations_to_dtos(list_local_bridge_authorizations_at(
            &state.local_bridge_runtime,
            now_ms,
        )),
        pruned_count,
    })
}

#[tauri::command(async)]
pub fn revoke_local_bridge_authorization(
    state: State<'_, AppState>,
    client_id: String,
    scope: String,
) -> Result<LocalBridgeAuthorizationRevokeDto, String> {
    let now_ms = now_ms();
    let scope = parse_local_bridge_permission_scope(&scope)?;
    let revoked = revoke_local_bridge_authorization_and_persist(
        &state.local_bridge_runtime,
        &client_id,
        scope,
        now_ms,
    )?;
    Ok(LocalBridgeAuthorizationRevokeDto {
        revoked,
        authorizations: local_bridge_authorizations_to_dtos(list_local_bridge_authorizations_at(
            &state.local_bridge_runtime,
            now_ms,
        )),
    })
}

#[tauri::command(async)]
pub fn prune_local_bridge_authorizations(
    state: State<'_, AppState>,
) -> Result<LocalBridgeAuthorizationListDto, String> {
    let now_ms = now_ms();
    let pruned_count =
        prune_local_bridge_authorizations_and_persist(&state.local_bridge_runtime, now_ms)?;
    Ok(LocalBridgeAuthorizationListDto {
        authorizations: local_bridge_authorizations_to_dtos(list_local_bridge_authorizations_at(
            &state.local_bridge_runtime,
            now_ms,
        )),
        pruned_count,
    })
}

pub(crate) fn confirm_local_bridge_runtime_authorization_at(
    runtime: &LocalBridgeRuntimeState,
    authorization_code: &str,
    now_ms: u128,
) -> Result<LocalBridgeAuthorizationRecord, String> {
    let mut pending_guard = runtime
        .pending_authorization
        .lock()
        .map_err(|error| error.to_string())?;
    let pending = pending_guard
        .as_ref()
        .ok_or_else(|| "local bridge authorization request not found".to_string())?;
    let authorization =
        confirm_pending_local_bridge_authorization(pending, authorization_code, now_ms)?;
    runtime
        .authorizations
        .lock()
        .map_err(|error| error.to_string())?
        .push(authorization.clone());
    *pending_guard = None;
    Ok(authorization)
}

pub(crate) fn confirm_local_bridge_runtime_authorization_and_persist(
    runtime: &LocalBridgeRuntimeState,
    authorization_code: &str,
    now_ms: u128,
) -> Result<LocalBridgeAuthorizationRecord, String> {
    let authorization =
        confirm_local_bridge_runtime_authorization_at(runtime, authorization_code, now_ms)?;
    let authorizations = runtime
        .authorizations
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    save_local_bridge_authorizations(&authorizations, now_ms)?;
    Ok(authorization)
}

pub(crate) fn confirm_local_bridge_runtime_authorization_and_save_at(
    runtime: &LocalBridgeRuntimeState,
    authorization_code: &str,
    now_ms: u128,
    authorizations_path: &Path,
) -> Result<LocalBridgeAuthorizationRecord, String> {
    let authorization =
        confirm_local_bridge_runtime_authorization_at(runtime, authorization_code, now_ms)?;
    let authorizations = runtime
        .authorizations
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    save_local_bridge_authorizations_at(authorizations_path, &authorizations, now_ms)?;
    Ok(authorization)
}

pub(crate) fn list_local_bridge_authorizations_at(
    runtime: &LocalBridgeRuntimeState,
    now_ms: u128,
) -> Vec<LocalBridgeAuthorizationRecord> {
    let mut authorizations = runtime
        .authorizations
        .lock()
        .map(|authorizations| {
            authorizations
                .iter()
                .filter(|record| local_bridge_authorization_is_active(record, now_ms))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    sort_local_bridge_authorizations(&mut authorizations);
    authorizations
}

pub(crate) fn revoke_local_bridge_authorization_and_persist(
    runtime: &LocalBridgeRuntimeState,
    client_id: &str,
    scope: LocalBridgePermissionScope,
    now_ms: u128,
) -> Result<bool, String> {
    let path = local_bridge_authorizations_file_path()?;
    revoke_local_bridge_authorization_at(runtime, client_id, scope, now_ms, &path)
}

pub(crate) fn revoke_local_bridge_authorization_at(
    runtime: &LocalBridgeRuntimeState,
    client_id: &str,
    scope: LocalBridgePermissionScope,
    now_ms: u128,
    authorizations_path: &Path,
) -> Result<bool, String> {
    let mut authorizations = runtime
        .authorizations
        .lock()
        .map_err(|error| error.to_string())?;
    let mut revoked = false;
    for record in authorizations
        .iter_mut()
        .filter(|record| record.client_id == client_id)
    {
        let before_scope_count = record.scopes.len();
        record.scopes.retain(|candidate| *candidate != scope);
        revoked |= record.scopes.len() != before_scope_count;
    }
    authorizations.retain(|record| !record.scopes.is_empty());
    save_local_bridge_authorizations_at(authorizations_path, &authorizations, now_ms)?;
    Ok(revoked)
}

pub(crate) fn prune_local_bridge_authorizations_and_persist(
    runtime: &LocalBridgeRuntimeState,
    now_ms: u128,
) -> Result<usize, String> {
    let path = local_bridge_authorizations_file_path()?;
    prune_local_bridge_authorizations_at(runtime, now_ms, &path)
}

pub(crate) fn prune_local_bridge_authorizations_at(
    runtime: &LocalBridgeRuntimeState,
    now_ms: u128,
    authorizations_path: &Path,
) -> Result<usize, String> {
    let mut authorizations = runtime
        .authorizations
        .lock()
        .map_err(|error| error.to_string())?;
    let before_len = authorizations.len();
    authorizations.retain(|record| local_bridge_authorization_is_active(record, now_ms));
    let pruned_count = before_len.saturating_sub(authorizations.len());
    save_local_bridge_authorizations_at(authorizations_path, &authorizations, now_ms)?;
    Ok(pruned_count)
}

pub(crate) fn local_bridge_authorization_is_active(
    record: &LocalBridgeAuthorizationRecord,
    now_ms: u128,
) -> bool {
    record
        .expires_at_ms
        .is_none_or(|expires_at_ms| expires_at_ms >= now_ms)
}

pub(crate) fn local_bridge_authorization_matches_client(
    record: &LocalBridgeAuthorizationRecord,
    client: &LocalBridgeClientIdentity,
) -> bool {
    record.client_id == client.client_id && record.app_kind == client.app_kind
}

pub(crate) fn local_bridge_client_identity_matches(
    left: &LocalBridgeClientIdentity,
    right: &LocalBridgeClientIdentity,
) -> bool {
    left.client_id == right.client_id && left.app_kind == right.app_kind
}

pub(crate) fn sort_local_bridge_authorizations(records: &mut [LocalBridgeAuthorizationRecord]) {
    records.sort_by(|left, right| {
        right
            .last_used_at_ms
            .cmp(&left.last_used_at_ms)
            .then_with(|| right.granted_at_ms.cmp(&left.granted_at_ms))
            .then_with(|| left.client_id.cmp(&right.client_id))
            .then_with(|| {
                local_bridge_permission_scopes_label(&left.scopes)
                    .cmp(&local_bridge_permission_scopes_label(&right.scopes))
            })
    });
}

pub(crate) fn local_bridge_client_has_scope(
    client: Option<&LocalBridgeClientIdentity>,
    authorizations: &[LocalBridgeAuthorizationRecord],
    scope: LocalBridgePermissionScope,
    now_ms: u128,
) -> bool {
    let Some(client) = client else {
        return false;
    };
    authorizations.iter().any(|record| {
        local_bridge_authorization_matches_client(record, client)
            && local_bridge_authorization_is_active(record, now_ms)
            && record.scopes.contains(&scope)
    })
}

pub(crate) fn mark_local_bridge_authorization_used(
    runtime: &LocalBridgeRuntimeState,
    client: Option<&LocalBridgeClientIdentity>,
    scope: LocalBridgePermissionScope,
    now_ms: u128,
) -> Result<(), String> {
    let Some(client) = client else {
        return Ok(());
    };
    let mut authorizations = runtime
        .authorizations
        .lock()
        .map_err(|error| error.to_string())?;
    for record in authorizations.iter_mut().filter(|record| {
        local_bridge_authorization_matches_client(record, client)
            && local_bridge_authorization_is_active(record, now_ms)
            && record.scopes.contains(&scope)
    }) {
        record.last_used_at_ms = now_ms;
    }
    Ok(())
}

pub(crate) fn local_bridge_authorized_scopes_used_by_response(
    request: &LocalBridgeRequest,
    response: &LocalBridgeResponseDto,
) -> Vec<LocalBridgePermissionScope> {
    if response.status == "pending_auth" {
        return Vec::new();
    }
    let mut scopes = Vec::new();
    match request {
        LocalBridgeRequest::ListDevices(_) => {
            scopes.push(LocalBridgePermissionScope::DeviceRead);
            if !response.staged_bundles.is_empty() {
                push_local_bridge_scope_once(&mut scopes, LocalBridgePermissionScope::BundleRead);
            }
        }
        LocalBridgeRequest::TransferStatus(_) => {
            scopes.push(LocalBridgePermissionScope::TransferStatusRead);
        }
        LocalBridgeRequest::BundleDetail(_) => {
            if !response.staged_bundles.is_empty() || response.status == "unsupported" {
                scopes.push(LocalBridgePermissionScope::BundleRead);
            }
        }
        LocalBridgeRequest::PollEvents(_) => {
            for event in &response.events {
                match event.get("kind").and_then(serde_json::Value::as_str) {
                    Some("bundle.received") => {
                        push_local_bridge_scope_once(
                            &mut scopes,
                            LocalBridgePermissionScope::BundleRead,
                        );
                    }
                    Some("transfer.updated") => {
                        push_local_bridge_scope_once(
                            &mut scopes,
                            LocalBridgePermissionScope::TransferStatusRead,
                        );
                    }
                    Some("bundle.send.preflight") => {
                        push_local_bridge_scope_once(
                            &mut scopes,
                            LocalBridgePermissionScope::BundleSend,
                        );
                    }
                    Some("action.updated") => {
                        match event
                            .get("payload")
                            .and_then(|payload| payload.get("action_kind"))
                            .and_then(serde_json::Value::as_str)
                        {
                            Some("bundle.send") => push_local_bridge_scope_once(
                                &mut scopes,
                                LocalBridgePermissionScope::BundleSend,
                            ),
                            Some("bundle.import" | "bundle.rollback") => {
                                push_local_bridge_scope_once(
                                    &mut scopes,
                                    LocalBridgePermissionScope::BundleImportRequest,
                                );
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
        LocalBridgeRequest::ActionResults(_) => {
            for result in &response.action_results {
                match result.action_kind.as_str() {
                    "bundle.send" => push_local_bridge_scope_once(
                        &mut scopes,
                        LocalBridgePermissionScope::BundleSend,
                    ),
                    "bundle.import" | "bundle.rollback" => push_local_bridge_scope_once(
                        &mut scopes,
                        LocalBridgePermissionScope::BundleImportRequest,
                    ),
                    _ => {}
                }
            }
        }
        LocalBridgeRequest::SendBundle(_) => scopes.push(LocalBridgePermissionScope::BundleSend),
        LocalBridgeRequest::ImportBundle(_) | LocalBridgeRequest::RollbackBundleImport(_) => {
            scopes.push(LocalBridgePermissionScope::BundleImportRequest);
        }
        LocalBridgeRequest::AuthorizationRequest(_) => {}
    }
    scopes
}

pub(crate) fn push_local_bridge_scope_once(
    scopes: &mut Vec<LocalBridgePermissionScope>,
    scope: LocalBridgePermissionScope,
) {
    if !scopes.contains(&scope) {
        scopes.push(scope);
    }
}

pub(crate) fn mark_local_bridge_authorization_used_for_response(
    runtime: &LocalBridgeRuntimeState,
    client: Option<&LocalBridgeClientIdentity>,
    request: &LocalBridgeRequest,
    response: &LocalBridgeResponseDto,
    now_ms: u128,
) -> Result<(), String> {
    let Some(client) = client else {
        return Ok(());
    };
    for scope in local_bridge_authorized_scopes_used_by_response(request, response) {
        mark_local_bridge_authorization_used(runtime, Some(client), scope, now_ms)?;
    }
    Ok(())
}

pub(crate) fn pending_local_bridge_authorization_from_request(
    request: &LocalBridgeAuthorizationRequest,
    now_ms: u128,
) -> Result<PendingLocalBridgeAuthorization, String> {
    request.validate().map_err(|error| error.message)?;
    let ttl_ms = u128::from(request.ttl_seconds.unwrap_or(900)).saturating_mul(1_000);
    let expires_at_ms = now_ms.saturating_add(ttl_ms);
    Ok(PendingLocalBridgeAuthorization {
        request_id: request.request_id.clone(),
        client: request.client.clone(),
        requested_scopes: request.requested_scopes.clone(),
        reason: request.reason.clone(),
        authorization_code: local_bridge_authorization_code(request, now_ms),
        requested_at_ms: now_ms,
        expires_at_ms,
    })
}

pub(crate) fn confirm_pending_local_bridge_authorization(
    pending: &PendingLocalBridgeAuthorization,
    authorization_code: &str,
    now_ms: u128,
) -> Result<LocalBridgeAuthorizationRecord, String> {
    if now_ms > pending.expires_at_ms {
        return Err("local bridge authorization request expired".to_string());
    }
    if authorization_code.trim() != pending.authorization_code {
        return Err("local bridge authorization code mismatch".to_string());
    }
    Ok(LocalBridgeAuthorizationRecord {
        client_id: pending.client.client_id.clone(),
        display_name: pending.client.display_name.clone(),
        app_kind: pending.client.app_kind.clone(),
        scopes: dedupe_local_bridge_permission_scopes(&pending.requested_scopes),
        granted_at_ms: now_ms,
        last_used_at_ms: now_ms,
        expires_at_ms: Some(pending.expires_at_ms),
    })
}

pub(crate) fn dedupe_local_bridge_permission_scopes(
    scopes: &[LocalBridgePermissionScope],
) -> Vec<LocalBridgePermissionScope> {
    let mut output = Vec::new();
    for scope in scopes {
        push_local_bridge_scope_once(&mut output, *scope);
    }
    output
}

pub(crate) fn local_bridge_authorization_code(
    request: &LocalBridgeAuthorizationRequest,
    requested_at_ms: u128,
) -> String {
    // The code exists so a human can confirm which client is asking. It must
    // not be derivable from request fields an attacker controls, so draw it
    // from the CSPRNG; keep the legacy digest only as a fallback for the
    // practically-unreachable case of the system RNG failing.
    let mut bytes = [0_u8; 3];
    if getrandom::fill(&mut bytes).is_ok() {
        let hex = bytes
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<String>();
        return format!("{}-{}", &hex[..3], &hex[3..6]);
    }

    let mut material = String::new();
    material.push_str(&request.request_id);
    material.push('\n');
    material.push_str(&request.client.client_id);
    material.push('\n');
    material.push_str(&request.client.display_name);
    material.push('\n');
    if let Some(app_kind) = &request.client.app_kind {
        material.push_str(app_kind);
    }
    material.push('\n');
    for scope in &request.requested_scopes {
        material.push_str(local_bridge_permission_scope_label(*scope));
        material.push('\n');
    }
    material.push_str(&request.reason);
    material.push('\n');
    material.push_str(&requested_at_ms.to_string());
    let digest = sha256_hex(material.as_bytes()).to_ascii_uppercase();
    format!("{}-{}", &digest[..3], &digest[3..6])
}

pub(crate) fn local_bridge_pending_authorization_response(
    request_id: String,
    client: LocalBridgeClientIdentity,
    requested_scopes: Vec<LocalBridgePermissionScope>,
    reason: String,
    ttl_seconds: Option<u64>,
    now_ms: u128,
) -> LocalBridgeResponseDto {
    let request = LocalBridgeAuthorizationRequest {
        request_id: request_id.clone(),
        client: client.clone(),
        requested_scopes: requested_scopes.clone(),
        reason: reason.clone(),
        ttl_seconds,
    };
    let pending = pending_local_bridge_authorization_from_request(&request, now_ms).ok();
    let client_metadata = local_bridge_client_metadata(Some(client));
    LocalBridgeResponseDto {
        request_id,
        status: "pending_auth".to_string(),
        message: "local bridge authorization request is waiting for user confirmation".to_string(),
        security_state: "requires_user_confirmation".to_string(),
        requires_user_confirmation: true,
        client_state: client_metadata.0,
        client_id: client_metadata.1,
        client_display_name: client_metadata.2,
        authorization_scopes: requested_scopes
            .into_iter()
            .map(local_bridge_permission_scope_label)
            .map(str::to_string)
            .collect(),
        authorization_reason: Some(reason),
        authorization_ttl_seconds: ttl_seconds,
        authorization_code: pending
            .as_ref()
            .map(|authorization| authorization.authorization_code.clone()),
        authorization_expires_at_ms: pending.map(|authorization| authorization.expires_at_ms),
        devices: Vec::new(),
        staged_bundles: Vec::new(),
        transfer_status: None,
        action_results: Vec::new(),
        events: Vec::new(),
        events_last_id: None,
        events_next_after_id: None,
        events_has_more: false,
        events_cursor_state: "empty".to_string(),
        events_visible_first_id: None,
        events_visible_last_id: None,
        events_visible_count: 0,
    }
}

pub(crate) fn local_bridge_permission_scope_label(
    scope: LocalBridgePermissionScope,
) -> &'static str {
    match scope {
        LocalBridgePermissionScope::DeviceRead => "device.read",
        LocalBridgePermissionScope::TransferStatusRead => "transfer.status.read",
        LocalBridgePermissionScope::BundleRead => "bundle.read",
        LocalBridgePermissionScope::BundleSend => "bundle.send",
        LocalBridgePermissionScope::BundleImportRequest => "bundle.import.request",
    }
}

pub(crate) fn local_bridge_permission_scopes_label(
    scopes: &[LocalBridgePermissionScope],
) -> String {
    scopes
        .iter()
        .copied()
        .map(local_bridge_permission_scope_label)
        .collect::<Vec<_>>()
        .join(",")
}

pub(crate) fn parse_local_bridge_permission_scope(
    value: &str,
) -> Result<LocalBridgePermissionScope, String> {
    match value {
        "device.read" => Ok(LocalBridgePermissionScope::DeviceRead),
        "transfer.status.read" => Ok(LocalBridgePermissionScope::TransferStatusRead),
        "bundle.read" => Ok(LocalBridgePermissionScope::BundleRead),
        "bundle.send" => Ok(LocalBridgePermissionScope::BundleSend),
        "bundle.import.request" => Ok(LocalBridgePermissionScope::BundleImportRequest),
        _ => Err(format!("未知本机接入权限: {value}")),
    }
}
