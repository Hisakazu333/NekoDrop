use crate::errors::{NekoDropError, NekoDropResult};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeviceId(String);

impl DeviceId {
    pub fn new(value: impl Into<String>) -> NekoDropResult<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(NekoDropError::InvalidDeviceName);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevicePlatform {
    MacOS,
    Windows,
    Linux,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceTrustState {
    Local,
    Untrusted,
    Pairing,
    Trusted,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: DeviceId,
    pub name: String,
    pub platform: DevicePlatform,
    pub host: String,
    pub port: u16,
    pub public_key_fingerprint: Option<String>,
    pub trust_state: DeviceTrustState,
}

impl Device {
    pub fn new(
        id: DeviceId,
        name: impl Into<String>,
        platform: DevicePlatform,
        host: impl Into<String>,
        port: u16,
    ) -> NekoDropResult<Self> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(NekoDropError::InvalidDeviceName);
        }

        Ok(Self {
            id,
            name,
            platform,
            host: host.into(),
            port,
            public_key_fingerprint: None,
            trust_state: DeviceTrustState::Untrusted,
        })
    }

    pub fn is_trusted(&self) -> bool {
        matches!(
            self.trust_state,
            DeviceTrustState::Local | DeviceTrustState::Trusted
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedDevice {
    pub id: DeviceId,
    pub name: String,
    pub platform: DevicePlatform,
    pub public_key: String,
    pub fingerprint: String,
    pub paired_at: String,
    pub last_seen_at: Option<String>,
    pub auto_accept: bool,
}
