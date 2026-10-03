use super::*;

pub(crate) const TRANSFER_SCAN_PROGRESS_EVENT: &str = "transfer_scan_progress";

#[tauri::command(async)]
pub fn get_app_snapshot(state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    let config = state.config.lock().map_err(|error| error.to_string())?;
    let identity = state.device_identity.public_identity();
    Ok(AppSnapshot {
        device_name: config.device_name.clone(),
        receive_dir: config.receive_dir.clone(),
        receive_port: config.receive_port,
        receive_policy: receive_policy_label(config.receive_policy).to_string(),
        send_limit_kbps: config.send_limit_kbps,
        discovery_enabled: config.discovery_enabled,
        tray_enabled: config.tray_enabled,
        device_identity: device_identity_to_dto(&identity),
    })
}

#[tauri::command(async)]
pub fn list_nearby_devices(state: State<'_, AppState>) -> Result<Vec<DeviceDto>, String> {
    let devices = state
        .nearby_devices
        .lock()
        .map_err(|error| error.to_string())?;
    let trusted_devices = state
        .trusted_devices
        .lock()
        .map_err(|error| error.to_string())?;
    let local_identity = state.device_identity.public_identity();
    Ok(devices
        .iter()
        .map(|device| device_to_dto(device, &local_identity, &trusted_devices))
        .collect())
}

#[tauri::command(async)]
pub fn get_discovery_status(state: State<'_, AppState>) -> Result<DiscoveryStatusDto, String> {
    let device_count = state
        .nearby_devices
        .lock()
        .map_err(|error| error.to_string())?
        .len();

    discovery_status_snapshot(&state, device_count)
}

#[tauri::command(async)]
pub fn list_trusted_devices(state: State<'_, AppState>) -> Result<Vec<TrustedDeviceDto>, String> {
    let trusted_devices = state
        .trusted_devices
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(trusted_devices.iter().map(trusted_device_to_dto).collect())
}

#[tauri::command(async)]
pub fn trust_nearby_device(
    state: State<'_, AppState>,
    device_id: String,
) -> Result<TrustedDeviceDto, String> {
    let device = {
        let devices = state
            .nearby_devices
            .lock()
            .map_err(|error| error.to_string())?;
        devices
            .iter()
            .find(|device| device.id.as_str() == device_id)
            .cloned()
            .ok_or_else(|| "设备不在线或尚未被自动扫描到".to_string())?
    };

    let local_identity = state.device_identity.public_identity();
    let record = trust_device_record(&local_identity, &device)?;
    {
        let mut trusted_devices = state
            .trusted_devices
            .lock()
            .map_err(|error| error.to_string())?;
        let mut next_trusted_devices = trusted_devices.clone();
        upsert_trusted_device(&mut next_trusted_devices, record.clone());
        save_trusted_devices(&next_trusted_devices)?;
        *trusted_devices = next_trusted_devices;
    }
    {
        // 锁中毒时恢复数据继续执行，而非静默跳过（跳过会让状态永久卡住）
        let mut devices = state
            .nearby_devices
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(device) = devices
            .iter_mut()
            .find(|device| device.id.as_str() == device_id)
        {
            device.trust_state = DeviceTrustState::Trusted;
        }
    }

    Ok(trusted_device_to_dto(&record))
}

#[tauri::command(async)]
pub fn request_device_pairing(
    state: State<'_, AppState>,
    device_id: String,
) -> Result<TrustedDeviceDto, String> {
    let device = {
        let devices = state
            .nearby_devices
            .lock()
            .map_err(|error| error.to_string())?;
        devices
            .iter()
            .find(|device| device.id.as_str() == device_id)
            .cloned()
            .ok_or_else(|| "设备不在线或尚未被自动扫描到".to_string())?
    };
    let listen_port = current_receive_session_port(&state)?
        .ok_or_else(|| "请先打开后台收件，再发起配对。".to_string())?;
    let local_identity = state.device_identity.public_identity();
    let local_public_key = state.device_identity.public_key()?;
    let pairing_code = pairing_code_for_device(&local_identity, &device)
        .ok_or_else(|| "这个设备缺少公开指纹，当前不能发起配对。".to_string())?;
    let request = PairingRequestPayload {
        request_id: format!("pairing-{}", now_ms()),
        device_id: local_identity.device_id.clone(),
        device_name: local_identity.device_name.clone(),
        platform: local_identity.platform.as_str().to_string(),
        public_key: local_public_key,
        public_key_fingerprint: local_identity.public_key_fingerprint.clone(),
        pairing_code,
        listen_port,
    };
    let endpoint = Endpoint::tcp(device.host.clone(), device.port);
    validate_endpoint_for_desktop_send(&endpoint)?;
    let decision = send_pairing_request(&endpoint, request)
        .map_err(|error| friendly_transfer_error(&error.to_string()))?;
    if !decision.accepted {
        return Err(format!(
            "对方拒绝配对：{}",
            decision.reason.unwrap_or_else(|| "未提供原因".to_string())
        ));
    }

    let record = trust_device_record(&local_identity, &device)?;
    persist_trusted_device(&state, record.clone())?;
    {
        // 锁中毒时恢复数据继续执行，而非静默跳过（跳过会让状态永久卡住）
        let mut devices = state
            .nearby_devices
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(device) = devices
            .iter_mut()
            .find(|device| device.id.as_str() == device_id)
        {
            device.trust_state = DeviceTrustState::Trusted;
        }
    }
    Ok(trusted_device_to_dto(&record))
}

#[tauri::command(async)]
pub fn set_trusted_device_alias(
    state: State<'_, AppState>,
    device_id: String,
    alias: String,
) -> Result<TrustedDeviceDto, String> {
    let mut trusted_devices = state
        .trusted_devices
        .lock()
        .map_err(|error| error.to_string())?;
    let mut next_trusted_devices = trusted_devices.clone();
    let record = set_alias_on_trusted_device(&mut next_trusted_devices, &device_id, &alias)
        .ok_or_else(|| "没有这台已配对设备".to_string())?;
    save_trusted_devices(&next_trusted_devices)?;
    *trusted_devices = next_trusted_devices;
    Ok(trusted_device_to_dto(&record))
}

#[tauri::command(async)]
pub fn forget_trusted_device(state: State<'_, AppState>, device_id: String) -> Result<(), String> {
    {
        let mut trusted_devices = state
            .trusted_devices
            .lock()
            .map_err(|error| error.to_string())?;
        let mut next_trusted_devices = trusted_devices.clone();
        next_trusted_devices.retain(|device| device.device_id != device_id);
        save_trusted_devices(&next_trusted_devices)?;
        *trusted_devices = next_trusted_devices;
    }
    {
        // 锁中毒时恢复数据继续执行，而非静默跳过（跳过会让状态永久卡住）
        let mut devices = state
            .nearby_devices
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(device) = devices
            .iter_mut()
            .find(|device| device.id.as_str() == device_id)
        {
            device.trust_state = DeviceTrustState::Untrusted;
        }
    }
    Ok(())
}

#[tauri::command(async)]
pub fn get_pending_pairing_request(
    state: State<'_, AppState>,
) -> Result<Option<PendingPairingRequestDto>, String> {
    let request = state
        .pending_pairing_request
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(request.as_ref().map(pending_pairing_request_to_dto))
}

#[tauri::command(async)]
pub fn get_desktop_realtime_snapshot(
    state: State<'_, AppState>,
) -> Result<DesktopRealtimeSnapshotDto, String> {
    desktop_realtime_snapshot(&state)
}

#[tauri::command(async)]
pub fn respond_pairing_request(state: State<'_, AppState>, accept: bool) -> Result<(), String> {
    let request = state
        .pending_pairing_request
        .lock()
        .map_err(|error| error.to_string())?
        .clone()
        .ok_or_else(|| "当前没有等待确认的配对请求".to_string())?;
    let (decision_lock, decision_cvar) = &*request.decision;
    let mut decision = decision_lock.lock().map_err(|error| error.to_string())?;
    *decision = Some(if accept {
        ReceiveDecision::Accept
    } else {
        ReceiveDecision::Decline
    });
    decision_cvar.notify_all();
    {
        // 锁中毒时恢复数据继续执行，而非静默跳过（跳过会让状态永久卡住）
        let mut pending = state
            .pending_pairing_request
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *pending = None;
    }
    Ok(())
}

pub(crate) fn desktop_realtime_snapshot(
    state: &AppState,
) -> Result<DesktopRealtimeSnapshotDto, String> {
    let receive_status = state
        .receive_status
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let receive_session = state
        .receive_session
        .lock()
        .map_err(|error| error.to_string())?
        .as_ref()
        .map(receive_session_to_dto);
    let receive_report = state
        .last_receive_report
        .lock()
        .map_err(|error| error.to_string())?
        .as_ref()
        .map(receive_report_to_dto);
    let pending_receive_offer = state
        .pending_receive_offer
        .lock()
        .map_err(|error| error.to_string())?
        .as_ref()
        .map(pending_offer_to_dto);
    let pending_pairing_request = state
        .pending_pairing_request
        .lock()
        .map_err(|error| error.to_string())?
        .as_ref()
        .map(pending_pairing_request_to_dto);
    let transfer_status = state
        .transfer_status
        .lock()
        .map_err(|error| error.to_string())?
        .as_ref()
        .map(transfer_status_to_dto);
    let device_count = state
        .nearby_devices
        .lock()
        .map_err(|error| error.to_string())?
        .len();
    let discovery_status = discovery_status_snapshot(state, device_count)?;

    Ok(DesktopRealtimeSnapshotDto {
        receive_status,
        receive_session,
        receive_report,
        pending_receive_offer,
        pending_pairing_request,
        transfer_status,
        discovery_status,
    })
}

pub(crate) fn emit_transfer_scan_progress(app: &AppHandle, progress: TransferPlanScanProgress) {
    let _ = app.emit(
        TRANSFER_SCAN_PROGRESS_EVENT,
        transfer_scan_progress_to_dto(progress),
    );
}

pub(crate) fn wait_for_pairing_decision(
    request: &PairingRequestPayload,
    peer_host: &str,
    pending_pairing_request: &Arc<Mutex<Option<PendingPairingRequest>>>,
    trusted_devices: &Arc<Mutex<Vec<TrustedDeviceRecord>>>,
    local_identity: &DeviceIdentity,
) -> PairingDecisionPayload {
    let expected_code = pairing_code_for_values(
        &local_identity.device_id,
        &local_identity.public_key_fingerprint,
        &request.device_id,
        &request.public_key_fingerprint,
    );
    if expected_code != request.pairing_code {
        return PairingDecisionPayload::reject("配对码不匹配");
    }

    let decision = Arc::new((Mutex::new(None), Condvar::new()));
    let pending = PendingPairingRequest {
        request_id: request.request_id.clone(),
        device_id: request.device_id.clone(),
        device_name: request.device_name.clone(),
        platform: request.platform.clone(),
        host: peer_host.to_string(),
        port: request.listen_port,
        public_key: request.public_key.clone(),
        public_key_fingerprint: request.public_key_fingerprint.clone(),
        pairing_code: request.pairing_code.clone(),
        decision: decision.clone(),
    };

    {
        // 锁中毒时恢复数据继续执行，而非静默跳过（跳过会让状态永久卡住）
        let mut request_slot = pending_pairing_request
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *request_slot = Some(pending);
    }

    let (decision_lock, decision_cvar) = &*decision;
    let mut guard = match decision_lock.lock() {
        Ok(guard) => guard,
        Err(_) => return PairingDecisionPayload::reject("配对确认状态异常"),
    };
    while guard.is_none() {
        let next = decision_cvar.wait_timeout(guard, Duration::from_secs(300));
        let Ok((next_guard, timeout)) = next else {
            return PairingDecisionPayload::reject("配对确认状态异常");
        };
        guard = next_guard;
        if timeout.timed_out() {
            {
                // 锁中毒时恢复数据继续执行，而非静默跳过（跳过会让状态永久卡住）
                let mut request_slot = pending_pairing_request
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                *request_slot = None;
            }
            return PairingDecisionPayload::reject("等待确认超时");
        }
    }

    if !matches!(*guard, Some(ReceiveDecision::Accept)) {
        return PairingDecisionPayload::reject("用户拒绝配对");
    }

    let record = trusted_device_record_from_remote(
        local_identity,
        request.device_id.clone(),
        request.device_name.clone(),
        request.platform.clone(),
        peer_host.to_string(),
        request.listen_port,
        request.public_key.clone(),
        request.public_key_fingerprint.clone(),
    );
    let record = match record {
        Ok(record) => record,
        Err(error) => return PairingDecisionPayload::reject(error),
    };
    match persist_trusted_device_records(trusted_devices, record) {
        Ok(()) => PairingDecisionPayload::accept(),
        Err(error) => PairingDecisionPayload::reject(error),
    }
}

pub(crate) fn persist_trusted_device(
    state: &AppState,
    record: TrustedDeviceRecord,
) -> Result<(), String> {
    persist_trusted_device_records(&state.trusted_devices, record)
}

pub(crate) fn persist_trusted_device_records(
    trusted_devices: &Arc<Mutex<Vec<TrustedDeviceRecord>>>,
    record: TrustedDeviceRecord,
) -> Result<(), String> {
    let mut trusted_devices = trusted_devices.lock().map_err(|error| error.to_string())?;
    let mut next_trusted_devices = trusted_devices.clone();
    upsert_trusted_device(&mut next_trusted_devices, record);
    save_trusted_devices(&next_trusted_devices)?;
    *trusted_devices = next_trusted_devices;
    Ok(())
}

pub(crate) fn refresh_trusted_device_contact_from_receive_report(
    trusted_devices: &Arc<Mutex<Vec<TrustedDeviceRecord>>>,
    report: &TransferReceiveReport,
) {
    if report.security_mode != TransferSecurityMode::AuthenticatedEncryptedSession {
        return;
    }

    let Some(sender_device_id) = report.sender_device_id.as_deref() else {
        return;
    };
    let Some(sender_fingerprint) = report.sender_public_key_fingerprint.as_deref() else {
        return;
    };

    let Ok(mut trusted_devices) = trusted_devices.lock() else {
        return;
    };
    let mut next_trusted_devices = trusted_devices.clone();
    let Some(sender_public_key) = next_trusted_devices
        .iter()
        .find(|record| {
            record.device_id == sender_device_id
                && record.public_key_fingerprint == sender_fingerprint
        })
        .map(|record| record.public_key.clone())
    else {
        return;
    };
    let changed = refresh_trusted_device_contact(
        &mut next_trusted_devices,
        sender_device_id,
        &sender_public_key,
        sender_fingerprint,
        report.sender_device_name.as_deref(),
        now_ms(),
    );
    if !changed {
        return;
    }
    if save_trusted_devices(&next_trusted_devices).is_ok() {
        *trusted_devices = next_trusted_devices;
    }
}

pub(crate) fn refresh_trusted_device_contact_from_peer(
    trusted_devices: &Arc<Mutex<Vec<TrustedDeviceRecord>>>,
    peer: &TransferPeer,
) {
    let Some(device_id) = peer.device_id.as_deref() else {
        return;
    };
    let Some(fingerprint) = peer.fingerprint.as_deref() else {
        return;
    };

    let Ok(mut trusted_devices) = trusted_devices.lock() else {
        return;
    };
    let mut next_trusted_devices = trusted_devices.clone();
    let Some(public_key) = next_trusted_devices
        .iter()
        .find(|record| {
            record.device_id == device_id && record.public_key_fingerprint == fingerprint
        })
        .map(|record| record.public_key.clone())
    else {
        return;
    };
    let changed = refresh_trusted_device_contact(
        &mut next_trusted_devices,
        device_id,
        &public_key,
        fingerprint,
        peer.name.as_deref(),
        now_ms(),
    );
    if !changed {
        return;
    }
    if save_trusted_devices(&next_trusted_devices).is_ok() {
        *trusted_devices = next_trusted_devices;
    }
}
