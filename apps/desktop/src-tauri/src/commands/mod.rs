use std::fs;
use std::io::ErrorKind;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nekodrop_core::{
    Device, FileManifest, ManifestItem, ManifestItemKind, NekoDropError, TransferJob,
};
use nekodrop_network::{ConnectionTicket, Endpoint, TransferOffer, TransferProgress};
use nekodrop_service::{
    accept_transfer_stream_with_decision, create_transfer_plan as create_service_transfer_plan,
    endpoint_from_connection_code, send_plan_with_progress, TransferProgressEvent,
    TransferReceiveReport, TransferSendReport, TransferSourceFile, TransferSourcePlan,
};
use nekolink_protocol::DeviceIdentity;
use serde::Serialize;
use tauri::State;

use crate::app_state::{
    ActiveReceiveSession, AppState, PendingReceiveFile, PendingReceiveOffer, ReceiveDecision,
    TransferStatusState,
};
use crate::network::primary_lan_ip;

#[derive(Debug, Clone, Serialize)]
pub struct AppSnapshot {
    pub device_name: String,
    pub receive_dir: String,
    pub discovery_enabled: bool,
    pub tray_enabled: bool,
    pub device_identity: DeviceIdentityDto,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceIdentityDto {
    pub device_id: String,
    pub device_name: String,
    pub device_kind: String,
    pub platform: String,
    pub public_key_fingerprint: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceDto {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub host: String,
    pub port: u16,
    pub trust_state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransferDto {
    pub id: String,
    pub peer_device_id: String,
    pub direction: String,
    pub status: String,
    pub file_count: usize,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub progress: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManifestItemDto {
    pub path: String,
    pub kind: String,
    pub size: u64,
    pub modified_at: Option<String>,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransferSourceFileDto {
    pub manifest_path: String,
    pub source_path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransferPlanDto {
    pub root_name: String,
    pub file_count: usize,
    pub total_bytes: u64,
    pub items: Vec<ManifestItemDto>,
    pub files: Vec<TransferSourceFileDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReceiveSessionDto {
    pub bind_addr: String,
    pub receive_dir: String,
    pub connection_code: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SentFileDto {
    pub manifest_path: String,
    pub bytes_sent: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SendReportDto {
    pub root_name: String,
    pub file_count: usize,
    pub total_bytes: u64,
    pub sent_files: Vec<SentFileDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReceivedFileDto {
    pub path: String,
    pub manifest_path: String,
    pub bytes_written: u64,
    pub sha256: String,
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReceiveReportDto {
    pub files: Vec<ReceivedFileDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingReceiveFileDto {
    pub manifest_path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingReceiveOfferDto {
    pub transfer_id: String,
    pub root_name: String,
    pub file_count: usize,
    pub total_bytes: u64,
    pub files: Vec<PendingReceiveFileDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransferStatusDto {
    pub direction: String,
    pub phase: String,
    pub root_name: Option<String>,
    pub file_count: usize,
    pub file_index: usize,
    pub current_file: Option<String>,
    pub bytes_transferred: u64,
    pub total_bytes: u64,
    pub progress: f32,
    pub message: String,
    pub updated_at_ms: u128,
}

#[tauri::command]
pub fn get_app_snapshot(state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    let config = state.config.lock().map_err(|error| error.to_string())?;
    let identity = state.device_identity.public_identity();
    Ok(AppSnapshot {
        device_name: config.device_name.clone(),
        receive_dir: config.receive_dir.clone(),
        discovery_enabled: config.discovery_enabled,
        tray_enabled: config.tray_enabled,
        device_identity: device_identity_to_dto(&identity),
    })
}

#[tauri::command]
pub fn list_nearby_devices(state: State<'_, AppState>) -> Result<Vec<DeviceDto>, String> {
    let devices = state
        .nearby_devices
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(devices.iter().map(device_to_dto).collect())
}

#[tauri::command]
pub fn list_transfers(state: State<'_, AppState>) -> Result<Vec<TransferDto>, String> {
    let transfers = state.transfers.lock().map_err(|error| error.to_string())?;
    Ok(transfers.iter().map(transfer_to_dto).collect())
}

#[tauri::command]
pub fn create_transfer_plan(paths: Vec<String>) -> Result<TransferPlanDto, String> {
    let paths = string_paths_to_path_bufs(paths)?;
    let plan = create_service_transfer_plan(&paths).map_err(|error| error.to_string())?;
    Ok(source_plan_to_dto(&plan))
}

#[tauri::command]
pub fn create_transfer_plan_from_text(paths_text: String) -> Result<TransferPlanDto, String> {
    let paths = parse_paths_text(&paths_text)?;
    let plan = create_service_transfer_plan(&paths).map_err(|error| error.to_string())?;
    Ok(source_plan_to_dto(&plan))
}

#[tauri::command]
pub fn send_paths_to_code(
    state: State<'_, AppState>,
    connection_code: String,
    paths_text: String,
) -> Result<SendReportDto, String> {
    let endpoint =
        endpoint_from_connection_code(&connection_code).map_err(|error| error.to_string())?;
    let paths = parse_paths_text(&paths_text)?;
    let plan = create_service_transfer_plan(&paths).map_err(|error| error.to_string())?;
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
    let report = send_plan_with_progress(&endpoint, plan.clone(), move |event| {
        if let Some(status) = status_from_progress_event("send", None, event) {
            set_transfer_status(&transfer_status, status);
        }
    })
    .map_err(|error| {
        set_transfer_status(
            &state.transfer_status,
            TransferStatusState {
                direction: "send".to_string(),
                phase: "failed".to_string(),
                root_name: Some(plan.manifest.root_name.clone()),
                file_count: plan.file_count(),
                file_index: 0,
                current_file: None,
                bytes_transferred: 0,
                total_bytes: plan.total_bytes(),
                message: friendly_transfer_error(&error.to_string()),
                updated_at_ms: now_ms(),
            },
        );
        error.to_string()
    })?;
    set_transfer_status(
        &state.transfer_status,
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
    Ok(send_report_to_dto(&report))
}

#[tauri::command]
pub fn select_send_files() -> Result<Vec<String>, String> {
    choose_paths(PathDialogKind::Files)
}

#[tauri::command]
pub fn select_send_folders() -> Result<Vec<String>, String> {
    choose_paths(PathDialogKind::Folders)
}

#[tauri::command]
pub fn select_receive_dir() -> Result<Option<String>, String> {
    Ok(choose_paths(PathDialogKind::SingleFolder)?
        .into_iter()
        .next())
}

#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    let target = expand_home_dir(path.trim());
    if !target.exists() {
        return Err(format!("路径不存在：{}", target.display()));
    }

    open_path_with_system(target)
}

#[tauri::command]
pub fn start_receive_once(
    state: State<'_, AppState>,
    bind_host: Option<String>,
    port: Option<u16>,
    receive_dir: Option<String>,
) -> Result<ReceiveSessionDto, String> {
    let bind_host = bind_host
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "0.0.0.0".to_string());
    let port = port.unwrap_or(45821);
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

    let listener = bind_available_listener(&bind_host, port)?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("无法设置收件监听状态: {error}"))?;
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
    let transfer_status = state.transfer_status.clone();
    let last_receive_report = state.last_receive_report.clone();
    let receive_dir_for_thread = receive_dir_path.clone();
    thread::spawn(move || {
        let pending_for_decision = pending_receive_offer.clone();
        let status_for_decision = transfer_status.clone();
        let status_for_progress = transfer_status.clone();
        let result = loop {
            if cancel.load(Ordering::SeqCst) {
                break None;
            }

            match listener.accept() {
                Ok((mut stream, _)) => {
                    if let Err(error) = stream.set_nonblocking(false) {
                        break Some(Err(NekoDropError::Network(format!(
                            "failed to prepare TCP stream: {error}"
                        ))));
                    }
                    break Some(accept_transfer_stream_with_decision(
                        &mut stream,
                        &receive_dir_for_thread,
                        move |offer| {
                            wait_for_receive_decision(
                                offer,
                                &pending_for_decision,
                                &status_for_decision,
                            )
                        },
                        move |event| {
                            if let Some(status) = status_from_progress_event("receive", None, event)
                            {
                                set_transfer_status(&status_for_progress, status);
                            }
                        },
                    ));
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(120));
                }
                Err(error) => {
                    break Some(Err(NekoDropError::Network(format!(
                        "failed to accept TCP connection: {error}"
                    ))));
                }
            }
        };
        let Some(result) = result else {
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
        };
        if let Ok(mut status) = receive_status.lock() {
            *status = Some(match &result {
                Ok(report) => format!("接收完成：{} 个文件", report.files.len()),
                Err(_) if is_receive_terminal_offer_status(&transfer_status, "declined") => {
                    "已拒绝这次传输".to_string()
                }
                Err(_) if is_receive_terminal_offer_status(&transfer_status, "expired") => {
                    "等待确认超时，已自动拒绝".to_string()
                }
                Err(_) if is_receive_terminal_offer_status(&transfer_status, "closed") => {
                    "收件已关闭".to_string()
                }
                Err(error) => format!("接收失败：{error}"),
            });
        }
        if let Ok(mut pending) = pending_receive_offer.lock() {
            *pending = None;
        }
        if let Ok(report) = result {
            let total_bytes = report.files.iter().map(|file| file.bytes_written).sum();
            set_transfer_status(
                &transfer_status,
                TransferStatusState {
                    direction: "receive".to_string(),
                    phase: "completed".to_string(),
                    root_name: None,
                    file_count: report.files.len(),
                    file_index: report.files.len(),
                    current_file: None,
                    bytes_transferred: total_bytes,
                    total_bytes,
                    message: "接收完成，校验通过".to_string(),
                    updated_at_ms: now_ms(),
                },
            );
            if let Ok(mut last_report) = last_receive_report.lock() {
                *last_report = Some(report);
            }
        } else if !is_receive_terminal_offer_status(&transfer_status, "declined")
            && !is_receive_terminal_offer_status(&transfer_status, "expired")
            && !is_receive_terminal_offer_status(&transfer_status, "closed")
        {
            if let Ok(status) = receive_status.lock() {
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
                        message: status.clone().unwrap_or_else(|| "接收失败".to_string()),
                        updated_at_ms: now_ms(),
                    },
                );
            }
        }
        if let Ok(mut active_session) = receive_session.lock() {
            *active_session = None;
        }
    });

    Ok(receive_session_to_dto(&session))
}

#[tauri::command]
pub fn stop_receive_once(state: State<'_, AppState>) -> Result<(), String> {
    if is_receive_transfer_active(&state.transfer_status) {
        return Err("正在接收文件，当前版本还不能中途取消。".to_string());
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

    {
        let mut receive_status = state
            .receive_status
            .lock()
            .map_err(|error| error.to_string())?;
        *receive_status = Some("收件已关闭".to_string());
    }
    set_transfer_status(
        &state.transfer_status,
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
    Ok(())
}

#[tauri::command]
pub fn get_receive_status(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let status = state
        .receive_status
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(status.clone())
}

#[tauri::command]
pub fn get_receive_session(
    state: State<'_, AppState>,
) -> Result<Option<ReceiveSessionDto>, String> {
    let session = state
        .receive_session
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(session.as_ref().map(receive_session_to_dto))
}

#[tauri::command]
pub fn get_last_receive_report(
    state: State<'_, AppState>,
) -> Result<Option<ReceiveReportDto>, String> {
    let report = state
        .last_receive_report
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(report.as_ref().map(receive_report_to_dto))
}

#[tauri::command]
pub fn get_pending_receive_offer(
    state: State<'_, AppState>,
) -> Result<Option<PendingReceiveOfferDto>, String> {
    let offer = state
        .pending_receive_offer
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(offer.as_ref().map(pending_offer_to_dto))
}

#[tauri::command]
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

#[tauri::command]
pub fn get_transfer_status(
    state: State<'_, AppState>,
) -> Result<Option<TransferStatusDto>, String> {
    let status = state
        .transfer_status
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(status.as_ref().map(transfer_status_to_dto))
}

fn receive_session_to_dto(session: &ActiveReceiveSession) -> ReceiveSessionDto {
    ReceiveSessionDto {
        bind_addr: session.bind_addr.clone(),
        receive_dir: session.receive_dir.clone(),
        connection_code: session.connection_code.clone(),
    }
}

fn device_to_dto(device: &Device) -> DeviceDto {
    DeviceDto {
        id: device.id.as_str().to_string(),
        name: device.name.clone(),
        platform: format!("{:?}", device.platform),
        host: device.host.clone(),
        port: device.port,
        trust_state: format!("{:?}", device.trust_state),
    }
}

fn device_identity_to_dto(identity: &DeviceIdentity) -> DeviceIdentityDto {
    DeviceIdentityDto {
        device_id: identity.device_id.clone(),
        device_name: identity.device_name.clone(),
        device_kind: identity.device_kind.as_str().to_string(),
        platform: identity.platform.as_str().to_string(),
        public_key_fingerprint: identity.public_key_fingerprint.clone(),
        capabilities: identity
            .capabilities
            .iter()
            .map(|capability| capability.as_str().to_string())
            .collect(),
    }
}

fn source_plan_to_dto(plan: &TransferSourcePlan) -> TransferPlanDto {
    TransferPlanDto {
        root_name: plan.manifest.root_name.clone(),
        file_count: plan.file_count(),
        total_bytes: plan.total_bytes(),
        items: manifest_items_to_dto(&plan.manifest),
        files: plan.files.iter().map(source_file_to_dto).collect(),
    }
}

fn manifest_items_to_dto(manifest: &FileManifest) -> Vec<ManifestItemDto> {
    manifest.items.iter().map(manifest_item_to_dto).collect()
}

fn manifest_item_to_dto(item: &ManifestItem) -> ManifestItemDto {
    ManifestItemDto {
        path: item.path.clone(),
        kind: match item.kind {
            ManifestItemKind::File => "file",
            ManifestItemKind::Directory => "directory",
        }
        .to_string(),
        size: item.size,
        modified_at: item.modified_at.clone(),
        sha256: item.sha256.clone(),
    }
}

fn source_file_to_dto(file: &TransferSourceFile) -> TransferSourceFileDto {
    TransferSourceFileDto {
        manifest_path: file.manifest_path.clone(),
        source_path: file.source_path.display().to_string(),
        size: file.size,
        sha256: file.sha256.clone(),
    }
}

fn send_report_to_dto(report: &TransferSendReport) -> SendReportDto {
    SendReportDto {
        root_name: report.plan.manifest.root_name.clone(),
        file_count: report.plan.file_count(),
        total_bytes: report.plan.total_bytes(),
        sent_files: report
            .sent_files
            .iter()
            .map(|file| SentFileDto {
                manifest_path: file.manifest_path.clone(),
                bytes_sent: file.bytes_sent,
            })
            .collect(),
    }
}

fn receive_report_to_dto(report: &TransferReceiveReport) -> ReceiveReportDto {
    ReceiveReportDto {
        files: report
            .files
            .iter()
            .map(|file| ReceivedFileDto {
                path: file.path.display().to_string(),
                manifest_path: file.manifest_path.clone(),
                bytes_written: file.bytes_written,
                sha256: file.sha256.clone(),
                verified: file.verified,
            })
            .collect(),
    }
}

fn pending_offer_to_dto(offer: &PendingReceiveOffer) -> PendingReceiveOfferDto {
    PendingReceiveOfferDto {
        transfer_id: offer.transfer_id.clone(),
        root_name: offer.root_name.clone(),
        file_count: offer.file_count,
        total_bytes: offer.total_bytes,
        files: offer
            .files
            .iter()
            .map(|file| PendingReceiveFileDto {
                manifest_path: file.manifest_path.clone(),
                size: file.size,
                sha256: file.sha256.clone(),
            })
            .collect(),
    }
}

fn transfer_status_to_dto(status: &TransferStatusState) -> TransferStatusDto {
    let progress = if status.total_bytes == 0 {
        0.0
    } else {
        (status.bytes_transferred as f32 / status.total_bytes as f32).clamp(0.0, 1.0)
    };
    TransferStatusDto {
        direction: status.direction.clone(),
        phase: status.phase.clone(),
        root_name: status.root_name.clone(),
        file_count: status.file_count,
        file_index: status.file_index,
        current_file: status.current_file.clone(),
        bytes_transferred: status.bytes_transferred,
        total_bytes: status.total_bytes,
        progress,
        message: status.message.clone(),
        updated_at_ms: status.updated_at_ms,
    }
}

fn transfer_to_dto(job: &TransferJob) -> TransferDto {
    TransferDto {
        id: job.id.as_str().to_string(),
        peer_device_id: job.peer_device_id.as_str().to_string(),
        direction: format!("{:?}", job.direction),
        status: format!("{:?}", job.status),
        file_count: job.manifest.file_count(),
        total_bytes: job.manifest.total_bytes(),
        transferred_bytes: job.transferred_bytes,
        progress: job.progress(),
    }
}

fn wait_for_receive_decision(
    offer: &TransferOffer,
    pending_receive_offer: &Arc<Mutex<Option<PendingReceiveOffer>>>,
    transfer_status: &Arc<Mutex<Option<TransferStatusState>>>,
) -> bool {
    let decision = Arc::new((Mutex::new(None), Condvar::new()));
    let pending = PendingReceiveOffer {
        transfer_id: offer.transfer_id.clone(),
        root_name: offer.root_name.clone(),
        file_count: offer.file_count,
        total_bytes: offer.total_bytes,
        files: offer
            .files
            .iter()
            .map(|file| PendingReceiveFile {
                manifest_path: file.manifest_path.clone(),
                size: file.size,
                sha256: file.sha256.clone(),
            })
            .collect(),
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

fn is_receive_terminal_offer_status(
    transfer_status: &Arc<Mutex<Option<TransferStatusState>>>,
    phase: &str,
) -> bool {
    transfer_status
        .lock()
        .ok()
        .and_then(|status| status.as_ref().map(|status| status.phase.clone()))
        .is_some_and(|current_phase| current_phase == phase)
}

fn is_receive_transfer_active(transfer_status: &Arc<Mutex<Option<TransferStatusState>>>) -> bool {
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

fn status_from_progress_event(
    direction: &str,
    root_name: Option<String>,
    event: TransferProgressEvent,
) -> Option<TransferStatusState> {
    match event {
        TransferProgressEvent::AwaitingApproval {
            root_name: event_root_name,
            file_count,
            total_bytes,
        } => Some(TransferStatusState {
            direction: direction.to_string(),
            phase: "awaiting_approval".to_string(),
            root_name: root_name.or(Some(event_root_name)),
            file_count,
            file_index: 0,
            current_file: None,
            bytes_transferred: 0,
            total_bytes,
            message: "已发送传输请求，等待对方确认".to_string(),
            updated_at_ms: now_ms(),
        }),
        TransferProgressEvent::Sending(progress) => Some(status_from_transfer_progress(
            direction,
            "transferring",
            "正在发送文件",
            root_name,
            progress,
        )),
        TransferProgressEvent::Receiving(progress) => Some(status_from_transfer_progress(
            direction,
            "transferring",
            "正在接收文件",
            root_name,
            progress,
        )),
        TransferProgressEvent::Verifying {
            manifest_path,
            bytes_transferred,
            total_bytes,
        } => Some(TransferStatusState {
            direction: direction.to_string(),
            phase: "verifying".to_string(),
            root_name,
            file_count: 0,
            file_index: 0,
            current_file: Some(manifest_path),
            bytes_transferred,
            total_bytes,
            message: "正在校验文件".to_string(),
            updated_at_ms: now_ms(),
        }),
    }
}

fn status_from_transfer_progress(
    direction: &str,
    phase: &str,
    message: &str,
    root_name: Option<String>,
    progress: TransferProgress,
) -> TransferStatusState {
    TransferStatusState {
        direction: direction.to_string(),
        phase: phase.to_string(),
        root_name,
        file_count: progress.file_count,
        file_index: progress.file_index,
        current_file: Some(progress.manifest_path),
        bytes_transferred: progress.bytes_transferred,
        total_bytes: progress.total_bytes,
        message: message.to_string(),
        updated_at_ms: now_ms(),
    }
}

fn set_transfer_status(
    transfer_status: &Arc<Mutex<Option<TransferStatusState>>>,
    status: TransferStatusState,
) {
    if let Ok(mut slot) = transfer_status.lock() {
        *slot = Some(status);
    }
}

fn friendly_transfer_error(error: &str) -> String {
    if error.contains("receiver declined") {
        return "对方拒绝了这次传输".to_string();
    }
    if error.contains("Connection refused") || error.contains("failed to connect") {
        return "无法连接对方电脑，请确认对方已打开收件、防火墙允许端口访问，且两台设备网络互通。"
            .to_string();
    }
    if error.contains("unsupported connection code") || error.contains("connection code") {
        return "连接码无效，请重新复制对方生成的连接码。".to_string();
    }
    error.to_string()
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn string_paths_to_path_bufs(paths: Vec<String>) -> Result<Vec<PathBuf>, String> {
    if paths.is_empty() {
        return Err("请至少输入一个文件或文件夹路径".into());
    }

    paths
        .into_iter()
        .map(|path| normalize_user_path(&path))
        .collect()
}

fn parse_paths_text(paths_text: &str) -> Result<Vec<PathBuf>, String> {
    let paths = paths_text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.trim_matches('"').trim_matches('\'').to_string())
        .collect::<Vec<_>>();

    string_paths_to_path_bufs(paths)
}

fn normalize_user_path(path: &str) -> Result<PathBuf, String> {
    let expanded = expand_home_dir(path);
    if !expanded.exists() {
        return Err(format!("路径不存在：{}", expanded.display()));
    }
    Ok(expanded)
}

fn expand_home_dir(path: &str) -> PathBuf {
    if path == "~" {
        return home_dir().unwrap_or_else(|| PathBuf::from(path));
    }

    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            return home.join(rest);
        }
    }

    PathBuf::from(path)
}

fn default_receive_dir() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Downloads")
        .join("NekoDrop")
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn bind_available_listener(bind_host: &str, requested_port: u16) -> Result<TcpListener, String> {
    let mut last_error = None;

    for offset in 0..20 {
        let Some(port) = requested_port.checked_add(offset) else {
            break;
        };
        match TcpListener::bind((bind_host, port)) {
            Ok(listener) => return Ok(listener),
            Err(error) => last_error = Some(format!("{bind_host}:{port}: {error}")),
        }
    }

    Err(format!(
        "无法监听端口，从 {requested_port} 起连续尝试失败：{}",
        last_error.unwrap_or_else(|| "没有可用端口".to_string())
    ))
}

#[derive(Debug, Clone, Copy)]
enum PathDialogKind {
    Files,
    Folders,
    SingleFolder,
}

fn parse_dialog_output(output: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(output)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(target_os = "macos")]
fn choose_paths(kind: PathDialogKind) -> Result<Vec<String>, String> {
    let script = match kind {
        PathDialogKind::Files => {
            r#"
set pickedItems to choose file with prompt "选择要发送的文件" with multiple selections allowed
set outputText to ""
repeat with pickedItem in pickedItems
  set outputText to outputText & POSIX path of pickedItem & linefeed
end repeat
return outputText
"#
        }
        PathDialogKind::Folders => {
            r#"
set pickedItems to choose folder with prompt "选择要发送的文件夹" with multiple selections allowed
set outputText to ""
repeat with pickedItem in pickedItems
  set outputText to outputText & POSIX path of pickedItem & linefeed
end repeat
return outputText
"#
        }
        PathDialogKind::SingleFolder => {
            r#"
set pickedItem to choose folder with prompt "选择接收目录"
return POSIX path of pickedItem
"#
        }
    };

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|error| format!("无法打开系统选择窗口：{error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("User canceled") || stderr.contains("-128") {
            return Ok(Vec::new());
        }
        return Err(format!("系统选择窗口失败：{}", stderr.trim()));
    }

    Ok(parse_dialog_output(&output.stdout))
}

#[cfg(target_os = "windows")]
fn choose_paths(kind: PathDialogKind) -> Result<Vec<String>, String> {
    let script = match kind {
        PathDialogKind::Files => {
            r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.OpenFileDialog
$dialog.Multiselect = $true
$dialog.Title = '选择要发送的文件'
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  $dialog.FileNames -join "`n"
}
"#
        }
        PathDialogKind::Folders => {
            r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.FolderBrowserDialog
$dialog.Description = '选择要发送的文件夹'
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  $dialog.SelectedPath
}
"#
        }
        PathDialogKind::SingleFolder => {
            r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.FolderBrowserDialog
$dialog.Description = '选择接收目录'
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  $dialog.SelectedPath
}
"#
        }
    };

    let output = Command::new("powershell")
        .args(["-NoProfile", "-STA", "-Command", script])
        .output()
        .map_err(|error| format!("无法打开系统选择窗口：{error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("系统选择窗口失败：{}", stderr.trim()));
    }

    Ok(parse_dialog_output(&output.stdout))
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn choose_paths(kind: PathDialogKind) -> Result<Vec<String>, String> {
    let mut args = vec!["--file-selection".to_string()];
    match kind {
        PathDialogKind::Files => {
            args.push("--multiple".to_string());
            args.push("--separator=\n".to_string());
            args.push("--title=选择要发送的文件".to_string());
        }
        PathDialogKind::Folders => {
            args.push("--directory".to_string());
            args.push("--multiple".to_string());
            args.push("--separator=\n".to_string());
            args.push("--title=选择要发送的文件夹".to_string());
        }
        PathDialogKind::SingleFolder => {
            args.push("--directory".to_string());
            args.push("--title=选择接收目录".to_string());
        }
    }

    let output = Command::new("zenity")
        .args(args)
        .output()
        .map_err(|error| format!("无法打开系统选择窗口：{error}"))?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    Ok(parse_dialog_output(&output.stdout))
}

fn open_path_with_system(path: PathBuf) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(&path);
        command
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer");
        command.arg(&path);
        command
    };

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(&path);
        command
    };

    command
        .spawn()
        .map_err(|error| format!("无法打开 {}：{error}", path.display()))?;
    Ok(())
}
