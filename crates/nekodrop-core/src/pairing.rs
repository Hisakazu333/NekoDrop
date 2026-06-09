use crate::device::{DeviceId, DevicePlatform};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingState {
    Idle,
    Requested,
    AwaitingConfirmation,
    Accepted,
    Rejected,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingRequest {
    pub device_id: DeviceId,
    pub device_name: String,
    pub platform: DevicePlatform,
    pub public_key: String,
    pub short_code: String,
}

impl PairingRequest {
    pub fn display_label(&self) -> String {
        format!("{} ({})", self.device_name, self.short_code)
    }
}
