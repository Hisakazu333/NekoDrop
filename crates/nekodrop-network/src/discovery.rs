use nekodrop_core::{DeviceId, DevicePlatform};
use serde::{Deserialize, Serialize};

pub const DISCOVERY_PROTOCOL: &str = "nekodrop.discovery.v1";
pub const UDP_DISCOVERY_PORT: u16 = 47618;
pub const MAX_DISCOVERY_BEACON_BYTES: usize = 2048;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryBeaconError {
    PayloadTooLarge,
    InvalidJson,
    InvalidProtocol,
    InvalidDeviceId,
    InvalidField,
}

impl std::fmt::Display for DiscoveryBeaconError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PayloadTooLarge => write!(formatter, "discovery beacon payload is too large"),
            Self::InvalidJson => write!(formatter, "discovery beacon is not valid JSON"),
            Self::InvalidProtocol => write!(formatter, "unsupported discovery beacon protocol"),
            Self::InvalidDeviceId => write!(formatter, "discovery beacon has invalid device id"),
            Self::InvalidField => write!(formatter, "discovery beacon has invalid fields"),
        }
    }
}

impl std::error::Error for DiscoveryBeaconError {}

#[derive(Debug, Deserialize, Serialize)]
struct WireDiscoveryAdvertisement {
    protocol: String,
    device_id: String,
    device_name: String,
    platform: String,
    app_version: String,
    host: String,
    port: u16,
    public_key_fingerprint: String,
}

pub fn encode_discovery_beacon(
    advertisement: &DiscoveryAdvertisement,
) -> Result<Vec<u8>, DiscoveryBeaconError> {
    let wire = WireDiscoveryAdvertisement {
        protocol: DISCOVERY_PROTOCOL.to_string(),
        device_id: advertisement.device_id.as_str().to_string(),
        device_name: advertisement.device_name.clone(),
        platform: platform_wire_label(advertisement.platform).to_string(),
        app_version: advertisement.app_version.clone(),
        host: advertisement.host.clone(),
        port: advertisement.port,
        public_key_fingerprint: advertisement.public_key_fingerprint.clone(),
    };
    validate_wire_advertisement(&wire)?;
    let payload = serde_json::to_vec(&wire).map_err(|_| DiscoveryBeaconError::InvalidJson)?;
    if payload.len() > MAX_DISCOVERY_BEACON_BYTES {
        return Err(DiscoveryBeaconError::PayloadTooLarge);
    }
    Ok(payload)
}

pub fn decode_discovery_beacon(
    payload: &[u8],
) -> Result<DiscoveryAdvertisement, DiscoveryBeaconError> {
    if payload.len() > MAX_DISCOVERY_BEACON_BYTES {
        return Err(DiscoveryBeaconError::PayloadTooLarge);
    }

    let wire: WireDiscoveryAdvertisement =
        serde_json::from_slice(payload).map_err(|_| DiscoveryBeaconError::InvalidJson)?;
    if wire.protocol != DISCOVERY_PROTOCOL {
        return Err(DiscoveryBeaconError::InvalidProtocol);
    }
    validate_wire_advertisement(&wire)?;

    Ok(DiscoveryAdvertisement {
        device_id: DeviceId::new(wire.device_id)
            .map_err(|_| DiscoveryBeaconError::InvalidDeviceId)?,
        device_name: wire.device_name,
        platform: platform_from_wire_label(&wire.platform),
        app_version: wire.app_version,
        host: wire.host,
        port: wire.port,
        public_key_fingerprint: wire.public_key_fingerprint,
    })
}

fn validate_wire_advertisement(
    wire: &WireDiscoveryAdvertisement,
) -> Result<(), DiscoveryBeaconError> {
    if wire.device_name.trim().is_empty()
        || wire.host.trim().is_empty()
        || wire.port == 0
        || wire.public_key_fingerprint.trim().is_empty()
    {
        return Err(DiscoveryBeaconError::InvalidField);
    }
    Ok(())
}

fn platform_wire_label(platform: DevicePlatform) -> &'static str {
    match platform {
        DevicePlatform::MacOS => "macos",
        DevicePlatform::Windows => "windows",
        DevicePlatform::Linux => "linux",
        DevicePlatform::Unknown => "unknown",
    }
}

fn platform_from_wire_label(value: &str) -> DevicePlatform {
    match value {
        "macos" => DevicePlatform::MacOS,
        "windows" => DevicePlatform::Windows,
        "linux" => DevicePlatform::Linux,
        _ => DevicePlatform::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn udp_discovery_beacon_round_trips() {
        let advertisement = DiscoveryAdvertisement {
            device_id: DeviceId::new("neko-device-remote").unwrap(),
            device_name: "Windows Desk".to_string(),
            platform: DevicePlatform::Windows,
            app_version: "0.1.0".to_string(),
            host: "192.168.1.42".to_string(),
            port: 45821,
            public_key_fingerprint: "sha256:remote".to_string(),
        };

        let encoded = encode_discovery_beacon(&advertisement).unwrap();
        let decoded = decode_discovery_beacon(&encoded).unwrap();

        assert_eq!(decoded, advertisement);
    }

    #[test]
    fn udp_discovery_beacon_rejects_unknown_protocol() {
        let payload = br#"{
            "protocol":"nekodrop.discovery.v0",
            "device_id":"neko-device-remote",
            "device_name":"Windows Desk",
            "platform":"windows",
            "app_version":"0.1.0",
            "host":"192.168.1.42",
            "port":45821,
            "public_key_fingerprint":"sha256:remote"
        }"#;

        let error = decode_discovery_beacon(payload).unwrap_err();

        assert_eq!(error, DiscoveryBeaconError::InvalidProtocol);
    }

    #[test]
    fn udp_discovery_beacon_rejects_oversized_payloads() {
        let payload = vec![b'x'; MAX_DISCOVERY_BEACON_BYTES + 1];

        let error = decode_discovery_beacon(&payload).unwrap_err();

        assert_eq!(error, DiscoveryBeaconError::PayloadTooLarge);
    }

    #[test]
    fn udp_discovery_beacon_refuses_to_encode_oversized_payloads() {
        let advertisement = DiscoveryAdvertisement {
            device_id: DeviceId::new("neko-device-local").unwrap(),
            device_name: "x".repeat(MAX_DISCOVERY_BEACON_BYTES),
            platform: DevicePlatform::MacOS,
            app_version: "0.1.0".to_string(),
            host: "192.168.1.20".to_string(),
            port: 45821,
            public_key_fingerprint: "sha256:local".to_string(),
        };

        let error = encode_discovery_beacon(&advertisement).unwrap_err();

        assert_eq!(error, DiscoveryBeaconError::PayloadTooLarge);
    }

    #[test]
    fn udp_discovery_beacon_rejects_invalid_endpoint_fields() {
        let payload = br#"{
            "protocol":"nekodrop.discovery.v1",
            "device_id":"neko-device-remote",
            "device_name":"Windows Desk",
            "platform":"windows",
            "app_version":"0.1.0",
            "host":"192.168.1.42",
            "port":0,
            "public_key_fingerprint":"sha256:remote"
        }"#;

        let error = decode_discovery_beacon(payload).unwrap_err();

        assert_eq!(error, DiscoveryBeaconError::InvalidField);
    }
}
