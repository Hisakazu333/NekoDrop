use std::sync::Mutex;

use nekodrop_core::{AppConfig, Device, TransferJob};

#[derive(Debug)]
pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub nearby_devices: Mutex<Vec<Device>>,
    pub transfers: Mutex<Vec<TransferJob>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            config: Mutex::new(AppConfig::default()),
            nearby_devices: Mutex::new(Vec::new()),
            transfers: Mutex::new(Vec::new()),
        }
    }
}
