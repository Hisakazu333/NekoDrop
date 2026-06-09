use std::sync::{atomic::AtomicBool, Arc, Condvar, Mutex};

use nekodrop_core::{AppConfig, Device, TransferJob};
use nekodrop_service::TransferReceiveReport;

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
    pub files: Vec<PendingReceiveFile>,
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
    pub config: Mutex<AppConfig>,
    pub nearby_devices: Mutex<Vec<Device>>,
    pub transfers: Mutex<Vec<TransferJob>>,
    pub receive_status: Arc<Mutex<Option<String>>>,
    pub receive_session: Arc<Mutex<Option<ActiveReceiveSession>>>,
    pub pending_receive_offer: Arc<Mutex<Option<PendingReceiveOffer>>>,
    pub transfer_status: Arc<Mutex<Option<TransferStatusState>>>,
    pub last_receive_report: Arc<Mutex<Option<TransferReceiveReport>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            config: Mutex::new(AppConfig::default()),
            nearby_devices: Mutex::new(Vec::new()),
            transfers: Mutex::new(Vec::new()),
            receive_status: Arc::new(Mutex::new(None)),
            receive_session: Arc::new(Mutex::new(None)),
            pending_receive_offer: Arc::new(Mutex::new(None)),
            transfer_status: Arc::new(Mutex::new(None)),
            last_receive_report: Arc::new(Mutex::new(None)),
        }
    }
}
