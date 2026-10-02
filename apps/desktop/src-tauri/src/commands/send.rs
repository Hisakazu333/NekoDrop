use super::*;

#[tauri::command(async)]
pub fn create_transfer_plan(app: AppHandle, paths: Vec<String>) -> Result<TransferPlanDto, String> {
    let paths = string_paths_to_path_bufs(paths)?;
    let plan = create_transfer_plan_with_scan_progress(&paths, |progress| {
        emit_transfer_scan_progress(&app, progress);
    })
    .map_err(|error| error.to_string())?;
    Ok(source_plan_to_dto(&plan))
}

#[tauri::command(async)]
pub fn create_transfer_plan_from_text(
    app: AppHandle,
    paths_text: String,
) -> Result<TransferPlanDto, String> {
    let paths = parse_paths_text(&paths_text)?;
    let plan = create_transfer_plan_with_scan_progress(&paths, |progress| {
        emit_transfer_scan_progress(&app, progress);
    })
    .map_err(|error| error.to_string())?;
    Ok(source_plan_to_dto(&plan))
}

#[tauri::command(async)]
pub fn send_paths_to_code(
    state: State<'_, AppState>,
    connection_code: String,
    paths_text: String,
) -> Result<SendReportDto, String> {
    let (endpoint, peer) = endpoint_and_peer_from_connection_input(&connection_code)?;
    send_paths_to_endpoint(&state, endpoint, paths_text, peer)
}

#[tauri::command(async)]
pub fn send_paths_to_device(
    state: State<'_, AppState>,
    device_id: String,
    paths_text: String,
) -> Result<SendReportDto, String> {
    let (endpoint, peer) = endpoint_and_peer_for_device_id(&state, &device_id)?;
    send_paths_to_endpoint(&state, endpoint, paths_text, peer)
}

#[tauri::command(async)]
pub fn resend_transfer(
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<SendReportDto, String> {
    let record = transfer_history_record_by_id(&state, &transfer_id)?;
    if record.direction != "send" {
        return Err("接收记录不能重发".to_string());
    }
    if record.source_paths.is_empty() {
        return Err("这条历史没有可重发的源路径".to_string());
    }

    let (endpoint, peer) = endpoint_and_peer_for_history_record(&state, &record)?;
    send_paths_to_endpoint_with_history_id(
        &state,
        endpoint,
        record.source_paths.join("\n"),
        peer,
        Some(record.id.clone()),
    )
}

#[tauri::command(async)]
pub fn open_transfer_location(
    state: State<'_, AppState>,
    transfer_id: String,
) -> Result<(), String> {
    let record = transfer_history_record_by_id(&state, &transfer_id)?;
    let path = record
        .received_paths
        .first()
        .or_else(|| record.source_paths.first())
        .or(record.receive_dir.as_ref())
        .map(|value| expand_home_dir(value))
        .ok_or_else(|| "这条历史没有可打开的位置".to_string())?;
    let target = if path.exists() {
        path
    } else {
        path.parent()
            .filter(|parent| parent.exists())
            .map(PathBuf::from)
            .ok_or_else(|| format!("路径不存在：{}", path.display()))?
    };

    open_path_with_system(target)
}

pub(crate) fn clear_active_send_cancel(
    active_send_cancel: &Arc<Mutex<Option<Arc<AtomicBool>>>>,
    cancel: &Arc<AtomicBool>,
) {
    if let Ok(mut active) = active_send_cancel.lock() {
        if active
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, cancel))
        {
            *active = None;
        }
    }
}

pub(crate) fn send_paths_to_endpoint(
    state: &AppState,
    endpoint: Endpoint,
    paths_text: String,
    peer: TransferPeer,
) -> Result<SendReportDto, String> {
    send_paths_to_endpoint_with_history_id(state, endpoint, paths_text, peer, None)
}

pub(crate) fn send_paths_to_endpoint_with_history_id(
    state: &AppState,
    endpoint: Endpoint,
    paths_text: String,
    peer: TransferPeer,
    history_id_override: Option<String>,
) -> Result<SendReportDto, String> {
    validate_endpoint_for_desktop_send(&endpoint)?;
    let paths = parse_paths_text(&paths_text)?;
    let source_paths = path_bufs_to_strings(&paths);
    let plan = create_service_transfer_plan(&paths).map_err(|error| error.to_string())?;
    let sender_identity = state.device_identity.public_identity();
    let local_device_identity = state.device_identity.clone();
    reject_self_peer(&sender_identity, &peer)?;
    let started_at_ms = now_ms();
    let transfer_id = history_transfer_id(started_at_ms, history_id_override.as_deref());
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut active_cancel = state
            .active_send_cancel
            .lock()
            .map_err(|error| error.to_string())?;
        if active_cancel.is_some() {
            return Err("已有发送任务进行中".to_string());
        }
        *active_cancel = Some(cancel.clone());
    }
    set_transfer_status(
        &state.transfer_status,
        TransferStatusState {
            direction: "send".to_string(),
            phase: "connecting".to_string(),
            root_name: Some(plan.manifest.root_name.clone()),
            file_count: plan.file_count(),
            file_index: 0,
            current_file: None,
            bytes_transferred: 0,
            total_bytes: plan.total_bytes(),
            message: "正在连接对方电脑".to_string(),
            updated_at_ms: now_ms(),
        },
    );
    let transfer_status = state.transfer_status.clone();
    let local_bridge_runtime = state.local_bridge_runtime.clone();
    let cancel_for_send = cancel.clone();
    let report = send_with_auto_retry(
        || {
            let transfer_status = transfer_status.clone();
            let runtime_for_progress = local_bridge_runtime.clone();
            let transfer_id_for_progress = transfer_id.clone();
            let cancel_for_attempt = cancel_for_send.clone();
            let local_device_identity = local_device_identity.clone();
            let peer_for_verifier = peer.clone();
            send_plan_with_authenticated_session_peer_verifier_and_cancel(
                &endpoint,
                plan.clone(),
                &sender_identity,
                move |binding| {
                    local_device_identity
                        .sign_session_identity_binding(binding)
                        .map_err(NekoDropError::Network)
                },
                move |identity, signed_binding| {
                    verify_peer_matches_transfer_peer(&peer_for_verifier, identity, signed_binding)
                        .map_err(NekoDropError::Network)
                },
                move |event| {
                    if let Some(status) = status_from_progress_event("send", None, event) {
                        set_transfer_status_and_push_bridge_event(
                            &transfer_status,
                            &runtime_for_progress,
                            &transfer_id_for_progress,
                            status,
                        );
                    }
                },
                || cancel_for_attempt.load(Ordering::SeqCst),
            )
            .map_err(|error| error.to_string())
        },
        |retry_number, retry_limit, error| {
            let (file_index, current_file, bytes_transferred, _) =
                current_transfer_progress(&state.transfer_status);
            set_transfer_status(
                &state.transfer_status,
                TransferStatusState {
                    direction: "send".to_string(),
                    phase: "retrying".to_string(),
                    root_name: Some(plan.manifest.root_name.clone()),
                    file_count: plan.file_count(),
                    file_index,
                    current_file,
                    bytes_transferred,
                    total_bytes: plan.total_bytes(),
                    message: format!(
                        "连接中断，正在自动重试 {retry_number}/{retry_limit}：{}",
                        friendly_transfer_error(error)
                    ),
                    updated_at_ms: now_ms(),
                },
            );
        },
    )
    .map_err(|error| {
        let cancelled = cancel.load(Ordering::SeqCst) || error.contains("transfer cancelled");
        let message = if cancelled {
            "传输已取消".to_string()
        } else {
            friendly_transfer_error(&error)
        };
        let status_phase = if cancelled { "cancelled" } else { "failed" };
        let (file_index, current_file, bytes_transferred, _) =
            current_transfer_progress(&state.transfer_status);
        clear_active_send_cancel(&state.active_send_cancel, &cancel);
        set_transfer_status_and_push_bridge_event(
            &state.transfer_status,
            &state.local_bridge_runtime,
            &transfer_id,
            TransferStatusState {
                direction: "send".to_string(),
                phase: status_phase.to_string(),
                root_name: Some(plan.manifest.root_name.clone()),
                file_count: plan.file_count(),
                file_index,
                current_file,
                bytes_transferred,
                total_bytes: plan.total_bytes(),
                message: message.clone(),
                updated_at_ms: now_ms(),
            },
        );
        let mut record = new_transfer_history_record(
            transfer_id.clone(),
            "send",
            status_phase,
            plan.manifest.root_name.clone(),
            plan.file_count(),
            plan.total_bytes(),
            bytes_transferred,
            started_at_ms,
        );
        record.peer_device_id = peer.device_id.clone();
        record.peer_name = peer.name.clone();
        record.target_host = peer.target_host.clone();
        record.source_paths = source_paths.clone();
        if !cancelled {
            record.error_message = Some(message.clone());
        }
        record.updated_at_ms = now_ms();
        if let Err(error) = push_transfer_history_record(&state.transfer_history, record) {
            eprintln!("nekodrop: failed to persist transfer history: {error}");
        }
        message
    })?;
    clear_active_send_cancel(&state.active_send_cancel, &cancel);
    let transferred_bytes = report.plan.total_bytes();
    set_transfer_status_and_push_bridge_event(
        &state.transfer_status,
        &state.local_bridge_runtime,
        &transfer_id,
        TransferStatusState {
            direction: "send".to_string(),
            phase: "completed".to_string(),
            root_name: Some(report.plan.manifest.root_name.clone()),
            file_count: report.plan.file_count(),
            file_index: report.plan.file_count(),
            current_file: None,
            bytes_transferred: report.plan.total_bytes(),
            total_bytes: report.plan.total_bytes(),
            message: "发送完成，等待对方校验结果".to_string(),
            updated_at_ms: now_ms(),
        },
    );
    let mut record = new_transfer_history_record(
        transfer_id,
        "send",
        "completed",
        report.plan.manifest.root_name.clone(),
        report.plan.file_count(),
        report.plan.total_bytes(),
        transferred_bytes,
        started_at_ms,
    );
    record.peer_device_id = peer.device_id.clone();
    record.peer_name = peer.name.clone();
    record.target_host = peer.target_host.clone();
    record.source_paths = source_paths;
    record.updated_at_ms = now_ms();
    refresh_trusted_device_contact_from_peer(&state.trusted_devices, &peer);
    if let Err(error) = push_transfer_history_record(&state.transfer_history, record) {
        eprintln!("nekodrop: failed to persist transfer history: {error}");
    }
    Ok(send_report_to_dto(&report))
}

pub(crate) fn history_transfer_id(
    started_at_ms: u128,
    existing_transfer_id: Option<&str>,
) -> String {
    existing_transfer_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("send-{started_at_ms}"))
}

pub(crate) const SEND_AUTO_RETRY_LIMIT: usize = 1;

pub(crate) fn send_with_auto_retry<T, S, R>(mut send: S, mut on_retry: R) -> Result<T, String>
where
    S: FnMut() -> Result<T, String>,
    R: FnMut(usize, usize, &str),
{
    for attempt_index in 0..=SEND_AUTO_RETRY_LIMIT {
        match send() {
            Ok(result) => return Ok(result),
            Err(error)
                if attempt_index < SEND_AUTO_RETRY_LIMIT && is_retryable_send_error(&error) =>
            {
                on_retry(attempt_index + 1, SEND_AUTO_RETRY_LIMIT, &error);
            }
            Err(error) => return Err(error),
        }
    }

    unreachable!("send retry loop always returns from success or final error")
}

pub(crate) fn is_retryable_send_error(error: &str) -> bool {
    let lower = error.to_lowercase();

    if lower.contains("transfer cancelled")
        || lower.contains("receiver declined")
        || lower.contains("transfer declined by receiver")
        || lower.contains("checksum")
        || lower.contains("sha-256")
        || lower.contains("sha256")
        || lower.contains("does not match accepted offer")
        || lower.contains("no such file")
        || lower.contains("not found")
        || lower.contains("路径不存在")
        || lower.contains("permission denied")
        || lower.contains("access is denied")
        || lower.contains("operation not permitted")
        || lower.contains("unsupported connection code")
        || lower.contains("invalid connection code")
        || lower.contains("invalid endpoint")
        || lower.contains("transport is not available")
        || lower.contains("unsupported transport")
        || lower.contains("requested iroh")
        || lower.contains("requested relay")
        || lower.contains("requested quic")
    {
        return false;
    }

    lower.contains("failed to connect")
        || lower.contains("connection refused")
        || lower.contains("actively refused")
        || lower.contains("connection reset")
        || lower.contains("connection aborted")
        || lower.contains("broken pipe")
        || lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("network is unreachable")
        || lower.contains("no route to host")
        || lower.contains("host unreachable")
        || lower.contains("连接尝试失败")
        || lower.contains("由于目标计算机积极拒绝")
}
