use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReceiveTrustContext {
    Untrusted,
    AuthenticatedTrusted,
}

pub(crate) fn clear_active_receive_cancel(
    active_receive_cancel: &Arc<Mutex<Option<Arc<AtomicBool>>>>,
    cancel: &Arc<AtomicBool>,
) {
    if let Ok(mut active) = active_receive_cancel.lock() {
        if active
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, cancel))
        {
            *active = None;
        }
    }
}

#[tauri::command(async)]
pub fn start_receive_once(
    state: State<'_, AppState>,
    bind_host: Option<String>,
    port: Option<u16>,
    receive_dir: Option<String>,
) -> Result<ReceiveSessionDto, String> {
    let bind_host = bind_host
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "0.0.0.0".to_string());
    let port = match port {
        Some(port) => port,
        None => state
            .config
            .lock()
            .map(|config| config.receive_port)
            .unwrap_or(45821),
    };
    if port == 0 {
        return Err("端口不能为 0".into());
    }

    if let Some(session) = state
        .receive_session
        .lock()
        .map_err(|error| error.to_string())?
        .clone()
    {
        return Ok(receive_session_to_dto(&session));
    }

    let receive_dir = receive_dir
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            state
                .config
                .lock()
                .map(|config| expand_home_dir(&config.receive_dir).display().to_string())
                .unwrap_or_else(|_| default_receive_dir().display().to_string())
        });
    let receive_dir_path = expand_home_dir(&receive_dir);
    fs::create_dir_all(&receive_dir_path)
        .map_err(|error| format!("无法创建接收目录 {}: {error}", receive_dir_path.display()))?;
    persist_receive_dir_path(&state, &receive_dir_path)?;

    let listener = bind_available_listener(&bind_host, port)?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("无法设置收件监听状态: {error}"))?;
    let bundle_staging_root = bundle_staging_root()?;
    fs::create_dir_all(&bundle_staging_root).map_err(|error| {
        format!(
            "无法创建 bundle 暂存目录 {}: {error}",
            bundle_staging_root.display()
        )
    })?;
    let local_addr = listener
        .local_addr()
        .map_err(|error| format!("无法读取监听地址: {error}"))?;
    let code_host = if local_addr.ip().is_unspecified() {
        primary_lan_ip()
            .map(|ip| ip.to_string())
            .ok_or_else(|| {
                "无法找到可用于其他设备连接的局域网地址，请确认已连接到同一局域网，或关闭代理/虚拟网卡后重试。".to_string()
            })?
    } else {
        local_addr.ip().to_string()
    };
    let identity = state.device_identity.public_identity();
    let connection_code = ConnectionTicket::new(Endpoint::tcp(code_host, local_addr.port()))
        .map(|ticket| ticket.with_device_identity(&identity))
        .and_then(|ticket| ticket.to_code())
        .map_err(|error| error.to_string())?;
    let bind_addr = local_addr.to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    let session = ActiveReceiveSession {
        bind_addr: bind_addr.clone(),
        receive_dir: receive_dir_path.display().to_string(),
        connection_code,
        cancel: cancel.clone(),
    };

    {
        let mut receive_status = state
            .receive_status
            .lock()
            .map_err(|error| error.to_string())?;
        *receive_status = Some("等待接收中".to_string());
    }
    {
        let mut last_receive_report = state
            .last_receive_report
            .lock()
            .map_err(|error| error.to_string())?;
        *last_receive_report = None;
    }
    {
        let mut receive_session = state
            .receive_session
            .lock()
            .map_err(|error| error.to_string())?;
        if let Some(existing) = receive_session.as_ref() {
            // Another start_receive_once call won the race while we were
            // binding; drop our listener so its port is released immediately.
            drop(listener);
            return Ok(receive_session_to_dto(existing));
        }
        *receive_session = Some(session.clone());
    }
    set_transfer_status(
        &state.transfer_status,
        TransferStatusState {
            direction: "receive".to_string(),
            phase: "listening".to_string(),
            root_name: None,
            file_count: 0,
            file_index: 0,
            current_file: None,
            bytes_transferred: 0,
            total_bytes: 0,
            message: "收件已打开，等待连接".to_string(),
            updated_at_ms: now_ms(),
        },
    );

    let receive_status = state.receive_status.clone();
    let receive_session = state.receive_session.clone();
    let pending_receive_offer = state.pending_receive_offer.clone();
    let pending_pairing_request = state.pending_pairing_request.clone();
    let config = state.config.clone();
    let transfer_status = state.transfer_status.clone();
    let last_receive_report = state.last_receive_report.clone();
    let trusted_devices = state.trusted_devices.clone();
    let transfer_history = state.transfer_history.clone();
    let active_receive_cancel = state.active_receive_cancel.clone();
    let local_bridge_runtime = state.local_bridge_runtime.clone();
    let local_device_identity = state.device_identity.clone();
    let local_identity = state.device_identity.public_identity();
    let receive_dir_for_thread = receive_dir_path.clone();
    let bundle_staging_root_for_thread = bundle_staging_root.clone();
    thread::spawn(move || loop {
        if cancel.load(Ordering::SeqCst) {
            if let Ok(mut status) = receive_status.lock() {
                *status = Some("收件已关闭".to_string());
            }
            if let Ok(mut active_session) = receive_session.lock() {
                *active_session = None;
            }
            set_transfer_status(
                &transfer_status,
                TransferStatusState {
                    direction: "receive".to_string(),
                    phase: "closed".to_string(),
                    root_name: None,
                    file_count: 0,
                    file_index: 0,
                    current_file: None,
                    bytes_transferred: 0,
                    total_bytes: 0,
                    message: "收件已关闭".to_string(),
                    updated_at_ms: now_ms(),
                },
            );
            return;
        }

        match listener.accept() {
            Ok((mut stream, peer_addr)) => {
                if let Err(error) = stream.set_nonblocking(false) {
                    set_transfer_status(
                        &transfer_status,
                        TransferStatusState {
                            direction: "receive".to_string(),
                            phase: "failed".to_string(),
                            root_name: None,
                            file_count: 0,
                            file_index: 0,
                            current_file: None,
                            bytes_transferred: 0,
                            total_bytes: 0,
                            message: format!("接收连接准备失败：{error}"),
                            updated_at_ms: now_ms(),
                        },
                    );
                    continue;
                }
                if let Err(error) = stream.set_io_timeout(TCP_IO_STALL_TIMEOUT) {
                    eprintln!("nekodrop: failed to set receive socket timeout: {error}");
                }
                let peer_host = peer_addr.ip().to_string();
                let receive_policy = config
                    .lock()
                    .map(|config| config.receive_policy)
                    .unwrap_or(ReceivePolicy::AlwaysAsk);
                let pending_for_decision = pending_receive_offer.clone();
                let trusted_for_decision = trusted_devices.clone();
                let trusted_for_session = trusted_devices.clone();
                let receive_trust_context = Arc::new(Mutex::new(ReceiveTrustContext::Untrusted));
                let receive_trust_for_session = receive_trust_context.clone();
                let receive_trust_for_decision = receive_trust_context.clone();
                let pending_for_pairing = pending_pairing_request.clone();
                let status_for_decision = transfer_status.clone();
                let status_for_progress = transfer_status.clone();
                let runtime_for_progress = local_bridge_runtime.clone();
                let receive_dir_for_decision = receive_dir_for_thread.clone();
                let trusted_for_pairing = trusted_devices.clone();
                let local_for_pairing = local_identity.clone();
                let local_for_signing = local_device_identity.clone();
                let peer_host_for_pairing = peer_host.clone();
                let current_receive_cancel = Arc::new(AtomicBool::new(false));
                if let Ok(mut active_cancel) = active_receive_cancel.lock() {
                    *active_cancel = Some(current_receive_cancel.clone());
                }
                let result =
                    accept_incoming_stream_with_authenticated_control_bundle_staging_peer_verifier_and_cancel(
                        &mut stream,
                        &receive_dir_for_thread,
                        &bundle_staging_root_for_thread,
                        &local_identity,
                        move |binding| {
                            local_for_signing
                                .sign_session_identity_binding(binding)
                                .map_err(NekoDropError::Network)
                        },
                        move |identity, signed_binding| {
                            let trusted_devices = trusted_for_session
                                .lock()
                                .map_err(|error| NekoDropError::Network(error.to_string()))?;
                            let trust_context = verify_incoming_peer_against_trusted_devices(
                                &trusted_devices,
                                identity,
                                signed_binding,
                            )
                            .map_err(NekoDropError::Network)?;
                            if let Ok(mut slot) = receive_trust_for_session.lock() {
                                *slot = trust_context;
                            }
                            Ok(())
                        },
                        move |offer| {
                            let resume_summary =
                                pending_resume_summary_from_offer(&receive_dir_for_decision, offer);
                            let trust_context = receive_trust_for_decision
                                .lock()
                                .map(|context| *context)
                                .unwrap_or(ReceiveTrustContext::Untrusted);
                            wait_for_receive_decision(
                                offer,
                                &pending_for_decision,
                                &status_for_decision,
                                receive_policy,
                                &trusted_for_decision,
                                trust_context,
                                resume_summary,
                            )
                        },
                        move |request| {
                            wait_for_pairing_decision(
                                request,
                                &peer_host_for_pairing,
                                &pending_for_pairing,
                                &trusted_for_pairing,
                                &local_for_pairing,
                            )
                        },
                        move |event| {
                            if let Some(status) = status_from_progress_event("receive", None, event)
                            {
                                let transfer_id_for_event = status
                                    .root_name
                                    .as_deref()
                                    .unwrap_or("receive")
                                    .to_string();
                                set_transfer_status_and_push_bridge_event(
                                    &status_for_progress,
                                    &runtime_for_progress,
                                    &transfer_id_for_event,
                                    status,
                                );
                            }
                        },
                        || {
                            cancel.load(Ordering::SeqCst)
                                || current_receive_cancel.load(Ordering::SeqCst)
                        },
                    );
                clear_active_receive_cancel(&active_receive_cancel, &current_receive_cancel);
                if let Ok(mut status) = receive_status.lock() {
                    *status = Some(match &result {
                        Ok(IncomingSessionReport::Transfer(report)) => {
                            format!("接收完成：{} 个文件", report.files.len())
                        }
                        Ok(IncomingSessionReport::Pairing(decision)) if decision.accepted => {
                            "配对完成".to_string()
                        }
                        Ok(IncomingSessionReport::Pairing(_)) => "已拒绝配对".to_string(),
                        Err(_)
                            if is_receive_terminal_offer_status(&transfer_status, "declined") =>
                        {
                            "已拒绝这次传输".to_string()
                        }
                        Err(_) if is_receive_terminal_offer_status(&transfer_status, "expired") => {
                            "等待确认超时，已自动拒绝".to_string()
                        }
                        Err(_) if is_receive_terminal_offer_status(&transfer_status, "closed") => {
                            "收件已关闭".to_string()
                        }
                        Err(_) if is_receive_terminal_offer_status(&transfer_status, "blocked") => {
                            "已阻止这次传输".to_string()
                        }
                        Err(_)
                            if is_receive_terminal_offer_status(&transfer_status, "cancelled") =>
                        {
                            "接收已取消".to_string()
                        }
                        Err(error) => {
                            format!("接收失败：{}", friendly_transfer_error(&error.to_string()))
                        }
                    });
                }
                if let Ok(mut pending) = pending_receive_offer.lock() {
                    *pending = None;
                }
                if let Ok(mut pending) = pending_pairing_request.lock() {
                    *pending = None;
                }
                if let Ok(report) = result {
                    match report {
                        IncomingSessionReport::Transfer(report) => {
                            let total_bytes =
                                report.files.iter().map(|file| file.bytes_written).sum();
                            set_transfer_status_and_push_bridge_event(
                                &transfer_status,
                                &local_bridge_runtime,
                                &report.transfer_id,
                                TransferStatusState {
                                    direction: "receive".to_string(),
                                    phase: "completed".to_string(),
                                    root_name: None,
                                    file_count: report.files.len(),
                                    file_index: report.files.len(),
                                    current_file: None,
                                    bytes_transferred: total_bytes,
                                    total_bytes,
                                    message: "接收完成，继续等待下一次连接".to_string(),
                                    updated_at_ms: now_ms(),
                                },
                            );
                            let mut record = new_transfer_history_record(
                                format!("receive-{}", now_ms()),
                                "receive",
                                "completed",
                                received_root_name(&report),
                                report.files.len(),
                                total_bytes,
                                total_bytes,
                                now_ms(),
                            );
                            record.peer_device_id = report.sender_device_id.clone();
                            record.peer_name = report.sender_device_name.clone();
                            record.target_host = Some(peer_host.clone());
                            record.receive_dir = Some(receive_dir_for_thread.display().to_string());
                            record.security_mode = Some(
                                transfer_security_mode_label(report.security_mode).to_string(),
                            );
                            record.received_paths = report
                                .files
                                .iter()
                                .map(|file| file.path.display().to_string())
                                .collect();
                            refresh_trusted_device_contact_from_receive_report(
                                &trusted_devices,
                                &report,
                            );
                            if let Err(error) =
                                push_transfer_history_record(&transfer_history, record)
                            {
                                eprintln!("nekodrop: failed to persist transfer history: {error}");
                            }
                            if let Some(bundle) = report.bundle.as_ref() {
                                let _ = push_local_bridge_bundle_received_event(
                                    &local_bridge_runtime,
                                    &report.transfer_id,
                                    bundle,
                                );
                            }
                            if let Ok(mut last_report) = last_receive_report.lock() {
                                *last_report = Some(report);
                            }
                        }
                        IncomingSessionReport::Pairing(decision) => {
                            set_transfer_status(
                                &transfer_status,
                                TransferStatusState {
                                    direction: "receive".to_string(),
                                    phase: if decision.accepted {
                                        "completed"
                                    } else {
                                        "declined"
                                    }
                                    .to_string(),
                                    root_name: None,
                                    file_count: 0,
                                    file_index: 0,
                                    current_file: None,
                                    bytes_transferred: 0,
                                    total_bytes: 0,
                                    message: if decision.accepted {
                                        "配对完成，继续等待下一次连接"
                                    } else {
                                        "已拒绝配对"
                                    }
                                    .to_string(),
                                    updated_at_ms: now_ms(),
                                },
                            );
                        }
                    }
                } else if !is_receive_terminal_offer_status(&transfer_status, "declined")
                    && !is_receive_terminal_offer_status(&transfer_status, "expired")
                    && !is_receive_terminal_offer_status(&transfer_status, "closed")
                    && !is_receive_terminal_offer_status(&transfer_status, "blocked")
                    && !is_receive_terminal_offer_status(&transfer_status, "cancelled")
                {
                    if let Ok(status) = receive_status.lock() {
                        let failure_message = status
                            .clone()
                            .unwrap_or_else(|| "接收失败，继续等待下一次连接".to_string());
                        set_transfer_status(
                            &transfer_status,
                            TransferStatusState {
                                direction: "receive".to_string(),
                                phase: "failed".to_string(),
                                root_name: None,
                                file_count: 0,
                                file_index: 0,
                                current_file: None,
                                bytes_transferred: 0,
                                total_bytes: 0,
                                message: failure_message.clone(),
                                updated_at_ms: now_ms(),
                            },
                        );
                        push_receive_failure_history(
                            &transfer_history,
                            &transfer_status,
                            &peer_host,
                            &receive_dir_for_thread,
                            failure_message,
                        );
                    }
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(120));
            }
            Err(error) => {
                if let Ok(mut status) = receive_status.lock() {
                    *status = Some(format!("接收监听异常：{error}"));
                }
                set_transfer_status(
                    &transfer_status,
                    TransferStatusState {
                        direction: "receive".to_string(),
                        phase: "failed".to_string(),
                        root_name: None,
                        file_count: 0,
                        file_index: 0,
                        current_file: None,
                        bytes_transferred: 0,
                        total_bytes: 0,
                        message: format!("接收监听异常：{error}"),
                        updated_at_ms: now_ms(),
                    },
                );
                thread::sleep(Duration::from_millis(500));
            }
        }
    });

    Ok(receive_session_to_dto(&session))
}

#[tauri::command(async)]
pub fn stop_receive_once(state: State<'_, AppState>) -> Result<(), String> {
    let receive_was_active = is_receive_transfer_active(&state.transfer_status);
    if let Some(cancel) = state
        .active_receive_cancel
        .lock()
        .map_err(|error| error.to_string())?
        .clone()
    {
        cancel.store(true, Ordering::SeqCst);
    }

    let session = state
        .receive_session
        .lock()
        .map_err(|error| error.to_string())?
        .take();
    if let Some(session) = session {
        session.cancel.store(true, Ordering::SeqCst);
    }

    let pending = state
        .pending_receive_offer
        .lock()
        .map_err(|error| error.to_string())?
        .take();
    if let Some(offer) = pending {
        let (decision_lock, decision_cvar) = &*offer.decision;
        if let Ok(mut decision) = decision_lock.lock() {
            *decision = Some(ReceiveDecision::Decline);
            decision_cvar.notify_all();
        }
    }
    let pending_pairing = state
        .pending_pairing_request
        .lock()
        .map_err(|error| error.to_string())?
        .take();
    if let Some(request) = pending_pairing {
        let (decision_lock, decision_cvar) = &*request.decision;
        if let Ok(mut decision) = decision_lock.lock() {
            *decision = Some(ReceiveDecision::Decline);
            decision_cvar.notify_all();
        }
    }

    {
        let mut receive_status = state
            .receive_status
            .lock()
            .map_err(|error| error.to_string())?;
        *receive_status = Some(if receive_was_active {
            "正在取消接收".to_string()
        } else {
            "收件已关闭".to_string()
        });
    }
    let (file_index, current_file, bytes_transferred, total_bytes) =
        current_transfer_progress(&state.transfer_status);
    set_transfer_status(
        &state.transfer_status,
        TransferStatusState {
            direction: "receive".to_string(),
            phase: if receive_was_active {
                "cancelled"
            } else {
                "closed"
            }
            .to_string(),
            root_name: None,
            file_count: 0,
            file_index,
            current_file,
            bytes_transferred,
            total_bytes,
            message: if receive_was_active {
                "正在取消接收"
            } else {
                "收件已关闭"
            }
            .to_string(),
            updated_at_ms: now_ms(),
        },
    );
    Ok(())
}

#[tauri::command(async)]
pub fn cancel_current_transfer(state: State<'_, AppState>) -> Result<(), String> {
    let cancel = state
        .active_send_cancel
        .lock()
        .map_err(|error| error.to_string())?
        .clone()
        .ok_or_else(|| "当前没有可取消的发送任务".to_string())?;
    cancel.store(true, Ordering::SeqCst);

    let mut transfer_status = state
        .transfer_status
        .lock()
        .map_err(|error| error.to_string())?;
    if let Some(status) = transfer_status.as_mut() {
        if status.direction == "send"
            && !matches!(status.phase.as_str(), "completed" | "failed" | "cancelled")
        {
            status.phase = "cancelled".to_string();
            status.message = "正在取消发送".to_string();
            status.updated_at_ms = now_ms();
        }
    }

    Ok(())
}

#[tauri::command(async)]
pub fn get_receive_status(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let status = state
        .receive_status
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(status.clone())
}

#[tauri::command(async)]
pub fn get_receive_session(
    state: State<'_, AppState>,
) -> Result<Option<ReceiveSessionDto>, String> {
    let session = state
        .receive_session
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(session.as_ref().map(receive_session_to_dto))
}

#[tauri::command(async)]
pub fn get_receive_port_diagnostics(
    state: State<'_, AppState>,
) -> Result<ReceivePortDiagnosticsDto, String> {
    let session = state
        .receive_session
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    if session.is_none() {
        return Ok(receive_port_diagnostics_from_session(None, Vec::new()));
    }
    Ok(receive_port_diagnostics_from_session(
        session.as_ref(),
        local_lan_ips(),
    ))
}

#[tauri::command(async)]
pub fn get_last_receive_report(
    state: State<'_, AppState>,
) -> Result<Option<ReceiveReportDto>, String> {
    let report = state
        .last_receive_report
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(report.as_ref().map(receive_report_to_dto))
}

#[tauri::command(async)]
pub fn get_pending_receive_offer(
    state: State<'_, AppState>,
) -> Result<Option<PendingReceiveOfferDto>, String> {
    let offer = state
        .pending_receive_offer
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(offer.as_ref().map(pending_offer_to_dto))
}

#[tauri::command(async)]
pub fn respond_receive_offer(state: State<'_, AppState>, accept: bool) -> Result<(), String> {
    let offer = state
        .pending_receive_offer
        .lock()
        .map_err(|error| error.to_string())?
        .clone()
        .ok_or_else(|| "当前没有等待确认的接收请求".to_string())?;
    let (decision_lock, decision_cvar) = &*offer.decision;
    let mut decision = decision_lock.lock().map_err(|error| error.to_string())?;
    *decision = Some(if accept {
        ReceiveDecision::Accept
    } else {
        ReceiveDecision::Decline
    });
    decision_cvar.notify_all();
    if let Ok(mut pending) = state.pending_receive_offer.lock() {
        *pending = None;
    }
    if accept {
        set_transfer_status(
            &state.transfer_status,
            TransferStatusState {
                direction: "receive".to_string(),
                phase: "accepted".to_string(),
                root_name: Some(offer.root_name),
                file_count: offer.file_count,
                file_index: 0,
                current_file: None,
                bytes_transferred: 0,
                total_bytes: offer.total_bytes,
                message: "已接受，等待对方开始发送".to_string(),
                updated_at_ms: now_ms(),
            },
        );
    } else {
        set_transfer_status(
            &state.transfer_status,
            TransferStatusState {
                direction: "receive".to_string(),
                phase: "declined".to_string(),
                root_name: Some(offer.root_name),
                file_count: offer.file_count,
                file_index: 0,
                current_file: None,
                bytes_transferred: 0,
                total_bytes: offer.total_bytes,
                message: "已拒绝这次传输".to_string(),
                updated_at_ms: now_ms(),
            },
        );
    }
    Ok(())
}

#[tauri::command(async)]
pub fn get_transfer_status(
    state: State<'_, AppState>,
) -> Result<Option<TransferStatusDto>, String> {
    let status = state
        .transfer_status
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(status.as_ref().map(transfer_status_to_dto))
}

pub(crate) fn push_receive_failure_history(
    transfer_history: &Arc<Mutex<Vec<TransferHistoryRecord>>>,
    transfer_status: &Arc<Mutex<Option<TransferStatusState>>>,
    peer_host: &str,
    receive_dir: &PathBuf,
    error_message: String,
) {
    let status = transfer_status
        .lock()
        .ok()
        .and_then(|status| status.clone());
    let now = now_ms();
    let mut record = new_transfer_history_record(
        format!("receive-{now}"),
        "receive",
        "failed",
        status
            .as_ref()
            .and_then(|status| status.root_name.clone())
            .unwrap_or_else(|| "接收失败".to_string()),
        status.as_ref().map(|status| status.file_count).unwrap_or(0),
        status
            .as_ref()
            .map(|status| status.total_bytes)
            .unwrap_or(0),
        status
            .as_ref()
            .map(|status| status.bytes_transferred)
            .unwrap_or(0),
        now,
    );
    record.target_host = Some(peer_host.to_string());
    record.receive_dir = Some(receive_dir.display().to_string());
    record.error_message = Some(error_message);
    if let Err(error) = push_transfer_history_record(transfer_history, record) {
        eprintln!("nekodrop: failed to persist transfer history: {error}");
    }
}

pub(crate) fn wait_for_receive_decision(
    offer: &TransferOffer,
    pending_receive_offer: &Arc<Mutex<Option<PendingReceiveOffer>>>,
    transfer_status: &Arc<Mutex<Option<TransferStatusState>>>,
    receive_policy: ReceivePolicy,
    trusted_devices: &Arc<Mutex<Vec<TrustedDeviceRecord>>>,
    trust_context: ReceiveTrustContext,
    resume_summary: Option<PendingReceiveResumeSummary>,
) -> bool {
    if receive_policy == ReceivePolicy::BlockAll {
        set_transfer_status(
            transfer_status,
            TransferStatusState {
                direction: "receive".to_string(),
                phase: "blocked".to_string(),
                root_name: Some(offer.root_name.clone()),
                file_count: offer.file_count,
                file_index: 0,
                current_file: None,
                bytes_transferred: 0,
                total_bytes: offer.total_bytes,
                message: "当前接收策略已阻止传输请求".to_string(),
                updated_at_ms: now_ms(),
            },
        );
        return false;
    }

    if trust_context == ReceiveTrustContext::Untrusted
        && legacy_plain_offer_matches_trusted_device(offer, trusted_devices)
    {
        set_transfer_status(
            transfer_status,
            TransferStatusState {
                direction: "receive".to_string(),
                phase: "blocked".to_string(),
                root_name: Some(offer.root_name.clone()),
                file_count: offer.file_count,
                file_index: 0,
                current_file: None,
                bytes_transferred: 0,
                total_bytes: offer.total_bytes,
                message: "可信设备必须使用已认证加密传输，已拒绝兼容明文请求".to_string(),
                updated_at_ms: now_ms(),
            },
        );
        return false;
    }

    if should_auto_accept_receive_offer(offer, receive_policy, trusted_devices, trust_context) {
        set_transfer_status(
            transfer_status,
            TransferStatusState {
                direction: "receive".to_string(),
                phase: "auto_accepted".to_string(),
                root_name: Some(offer.root_name.clone()),
                file_count: offer.file_count,
                file_index: 0,
                current_file: None,
                bytes_transferred: 0,
                total_bytes: offer.total_bytes,
                message: "可信设备已自动接受".to_string(),
                updated_at_ms: now_ms(),
            },
        );
        return true;
    }

    let decision = Arc::new((Mutex::new(None), Condvar::new()));
    let pending = PendingReceiveOffer {
        transfer_id: offer.transfer_id.clone(),
        root_name: offer.root_name.clone(),
        file_count: offer.file_count,
        total_bytes: offer.total_bytes,
        sender_device_id: offer.sender_device_id.clone(),
        sender_device_name: offer.sender_device_name.clone(),
        sender_public_key_fingerprint: offer.sender_public_key_fingerprint.clone(),
        files: offer
            .files
            .iter()
            .map(|file| PendingReceiveFile {
                manifest_path: file.manifest_path.clone(),
                size: file.size,
                sha256: file.sha256.clone(),
            })
            .collect(),
        resume_summary,
        decision: decision.clone(),
    };

    if let Ok(mut offer_slot) = pending_receive_offer.lock() {
        *offer_slot = Some(pending);
    }
    set_transfer_status(
        transfer_status,
        TransferStatusState {
            direction: "receive".to_string(),
            phase: "awaiting_approval".to_string(),
            root_name: Some(offer.root_name.clone()),
            file_count: offer.file_count,
            file_index: 0,
            current_file: None,
            bytes_transferred: 0,
            total_bytes: offer.total_bytes,
            message: "收到传输请求，等待确认".to_string(),
            updated_at_ms: now_ms(),
        },
    );

    let (decision_lock, decision_cvar) = &*decision;
    let mut guard = match decision_lock.lock() {
        Ok(guard) => guard,
        Err(_) => return false,
    };
    while guard.is_none() {
        let next = decision_cvar.wait_timeout(guard, Duration::from_secs(300));
        let Ok((next_guard, timeout)) = next else {
            return false;
        };
        guard = next_guard;
        if timeout.timed_out() {
            set_transfer_status(
                transfer_status,
                TransferStatusState {
                    direction: "receive".to_string(),
                    phase: "expired".to_string(),
                    root_name: Some(offer.root_name.clone()),
                    file_count: offer.file_count,
                    file_index: 0,
                    current_file: None,
                    bytes_transferred: 0,
                    total_bytes: offer.total_bytes,
                    message: "等待确认超时，已自动拒绝".to_string(),
                    updated_at_ms: now_ms(),
                },
            );
            return false;
        }
    }

    matches!(*guard, Some(ReceiveDecision::Accept))
}

pub(crate) fn legacy_plain_offer_matches_trusted_device(
    offer: &TransferOffer,
    trusted_devices: &Arc<Mutex<Vec<TrustedDeviceRecord>>>,
) -> bool {
    let Some(sender_device_id) = offer.sender_device_id.as_deref() else {
        return false;
    };
    let Ok(trusted_devices) = trusted_devices.lock() else {
        return false;
    };
    trusted_devices
        .iter()
        .any(|record| record.device_id == sender_device_id)
}

pub(crate) fn should_auto_accept_receive_offer(
    offer: &TransferOffer,
    receive_policy: ReceivePolicy,
    trusted_devices: &Arc<Mutex<Vec<TrustedDeviceRecord>>>,
    trust_context: ReceiveTrustContext,
) -> bool {
    if receive_policy != ReceivePolicy::AutoAcceptTrusted
        || trust_context != ReceiveTrustContext::AuthenticatedTrusted
    {
        return false;
    }

    let Some(sender_device_id) = offer.sender_device_id.as_deref() else {
        return false;
    };
    let Some(sender_fingerprint) = offer.sender_public_key_fingerprint.as_deref() else {
        return false;
    };
    let Ok(trusted_devices) = trusted_devices.lock() else {
        return false;
    };
    trusted_devices.iter().any(|record| {
        record.device_id == sender_device_id && record.public_key_fingerprint == sender_fingerprint
    })
}

pub(crate) fn current_receive_session_port(state: &AppState) -> Result<Option<u16>, String> {
    let session = state
        .receive_session
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(session
        .as_ref()
        .and_then(|session| session.bind_addr.rsplit_once(':'))
        .and_then(|(_, port)| port.parse::<u16>().ok()))
}

pub(crate) fn is_receive_terminal_offer_status(
    transfer_status: &Arc<Mutex<Option<TransferStatusState>>>,
    phase: &str,
) -> bool {
    transfer_status
        .lock()
        .ok()
        .and_then(|status| status.as_ref().map(|status| status.phase.clone()))
        .is_some_and(|current_phase| current_phase == phase)
}

pub(crate) fn is_receive_transfer_active(
    transfer_status: &Arc<Mutex<Option<TransferStatusState>>>,
) -> bool {
    transfer_status
        .lock()
        .ok()
        .and_then(|status| {
            status
                .as_ref()
                .map(|status| (status.direction.clone(), status.phase.clone()))
        })
        .is_some_and(|(direction, phase)| {
            direction == "receive"
                && matches!(phase.as_str(), "accepted" | "transferring" | "verifying")
        })
}
