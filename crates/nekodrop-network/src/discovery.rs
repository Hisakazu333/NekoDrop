use nekodrop_core::{DeviceId, DevicePlatform};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryState {
    Disabled,
    Searching,
    Online,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryAdvertisement {
    pub device_id: DeviceId,
    pub device_name: String,
    pub platform: DevicePlatform,
    pub app_version: String,
    pub host: String,
    pub port: u16,
    pub public_key_fingerprint: String,
}
