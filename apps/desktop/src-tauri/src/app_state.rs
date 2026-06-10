use std::collections::HashMap;
use std::sync::{atomic::AtomicBool, Arc, Condvar, Mutex};
use std::time::Instant;

use nekodrop_core::{AppConfig, Device};
use nekodrop_service::TransferReceiveReport;

use crate::app_config::load_app_config;
use crate::device_identity::{load_or_create_device_identity, LocalDeviceIdentity};
use crate::transfer_history::{load_transfer_history, TransferHistoryRecord};
use crate::trusted_devices::{load_trusted_devices, TrustedDeviceRecord};

#[derive(Debug, Clone)]
pub struct DiscoveryStatusState {
    pub phase: String,
    pub message: String,
    pub service_type: String,
    pub advertised: bool,
    pub lan_ip: Option<String>,
    pub port: Option<u16>,
    pub last_seen_at: Option<Instant>,
    pub last_error: Option<String>,
}

impl DiscoveryStatusState {
    pub fn starting() -> Self {
        Self {
            phase: "starting".to_string(),
            message: "正在启动自动发现".to_string(),
            service_type: "_nekodrop._tcp.local.".to_string(),
            advertised: false,
            lan_ip: None,
            port: None,
            last_seen_at: None,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ActiveReceiveSession {
    pub bind_addr: String,
    pub receive_dir: String,
    pub connection_code: String,
    pub cancel: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
pub struct PendingReceiveFile {
    pub manifest_path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiveDecision {
    Accept,
    Decline,
}

#[derive(Debug, Clone)]
pub struct PendingReceiveOffer {
    pub transfer_id: String,
    pub root_name: String,
    pub file_count: usize,
    pub total_bytes: u64,
    pub sender_device_id: Option<String>,
    pub sender_device_name: Option<String>,
    pub sender_public_key_fingerprint: Option<String>,
    pub files: Vec<PendingReceiveFile>,
    pub decision: Arc<(Mutex<Option<ReceiveDecision>>, Condvar)>,
}

#[derive(Debug, Clone)]
pub struct PendingPairingRequest {
    pub request_id: String,
    pub device_id: String,
    pub device_name: String,
    pub platform: String,
    pub host: String,
    pub port: u16,
    pub public_key_fingerprint: String,
    pub pairing_code: String,
    pub decision: Arc<(Mutex<Option<ReceiveDecision>>, Condvar)>,
}

#[derive(Debug, Clone)]
pub struct TransferStatusState {
    pub direction: String,
    pub phase: String,
    pub root_name: Option<String>,
    pub file_count: usize,
    pub file_index: usize,
    pub current_file: Option<String>,
    pub bytes_transferred: u64,
    pub total_bytes: u64,
    pub message: String,
    pub updated_at_ms: u128,
}

#[derive(Debug)]
pub struct AppState {
    pub config: Arc<Mutex<AppConfig>>,
    pub device_identity: LocalDeviceIdentity,
    pub nearby_devices: Arc<Mutex<Vec<Device>>>,
    pub nearby_devices_seen_at: Arc<Mutex<HashMap<String, Instant>>>,
    pub discovery_status: Arc<Mutex<DiscoveryStatusState>>,
    pub trusted_devices: Arc<Mutex<Vec<TrustedDeviceRecord>>>,
    pub transfer_history: Arc<Mutex<Vec<TransferHistoryRecord>>>,
    pub receive_status: Arc<Mutex<Option<String>>>,
    pub receive_session: Arc<Mutex<Option<ActiveReceiveSession>>>,
    pub pending_receive_offer: Arc<Mutex<Option<PendingReceiveOffer>>>,
    pub pending_pairing_request: Arc<Mutex<Option<PendingPairingRequest>>>,
    pub active_send_cancel: Arc<Mutex<Option<Arc<AtomicBool>>>>,
    pub active_receive_cancel: Arc<Mutex<Option<Arc<AtomicBool>>>>,
    pub transfer_status: Arc<Mutex<Option<TransferStatusState>>>,
    pub last_receive_report: Arc<Mutex<Option<TransferReceiveReport>>>,
}

impl AppState {
    pub fn new() -> Result<Self, String> {
        let device_identity = load_or_create_device_identity()?;
        let trusted_devices = load_trusted_devices()?;
        let transfer_history = load_transfer_history()?;
        let config = load_app_config(device_identity.device_name())?;

        Ok(Self {
            config: Arc::new(Mutex::new(config)),
            device_identity,
            nearby_devices: Arc::new(Mutex::new(Vec::new())),
            nearby_devices_seen_at: Arc::new(Mutex::new(HashMap::new())),
            discovery_status: Arc::new(Mutex::new(DiscoveryStatusState::starting())),
            trusted_devices: Arc::new(Mutex::new(trusted_devices)),
            transfer_history: Arc::new(Mutex::new(transfer_history)),
            receive_status: Arc::new(Mutex::new(None)),
            receive_session: Arc::new(Mutex::new(None)),
            pending_receive_offer: Arc::new(Mutex::new(None)),
            pending_pairing_request: Arc::new(Mutex::new(None)),
            active_send_cancel: Arc::new(Mutex::new(None)),
            active_receive_cancel: Arc::new(Mutex::new(None)),
            transfer_status: Arc::new(Mutex::new(None)),
            last_receive_report: Arc::new(Mutex::new(None)),
        })
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new().expect("failed to initialize NekoDrop app state")
    }
}
