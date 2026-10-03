use std::collections::BTreeMap;

use nekodrop_core::{NekoDropError, NekoDropResult};
use nekolink_protocol::{DeviceIdentity, DeviceKind, PlatformKind};

use crate::{Endpoint, TransportKind};

const PREFIX: &str = "nekodrop-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionTicket {
    pub endpoint: Endpoint,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub device_kind: Option<DeviceKind>,
    pub platform: Option<PlatformKind>,
    pub fingerprint: Option<String>,
}

impl ConnectionTicket {
    pub fn new(endpoint: Endpoint) -> NekoDropResult<Self> {
        validate_endpoint(&endpoint)?;
        Ok(Self {
            endpoint,
            device_id: None,
            device_name: None,
            device_kind: None,
            platform: None,
            fingerprint: None,
        })
    }

    pub fn with_device_identity(mut self, identity: &DeviceIdentity) -> Self {
        self.device_id = Some(identity.device_id.clone());
        self.device_name = Some(identity.device_name.clone());
        self.device_kind = Some(identity.device_kind);
        self.platform = Some(identity.platform);
        self.fingerprint = Some(identity.public_key_fingerprint.clone());
        self
    }

    pub fn with_device_id(mut self, device_id: impl Into<String>) -> Self {
        let device_id = device_id.into();
        if !device_id.trim().is_empty() {
            self.device_id = Some(device_id);
        }
        self
    }

    pub fn with_device_name(mut self, device_name: impl Into<String>) -> Self {
        let device_name = device_name.into();
        if !device_name.trim().is_empty() {
            self.device_name = Some(device_name);
        }
        self
    }

    pub fn with_device_kind(mut self, device_kind: DeviceKind) -> Self {
        self.device_kind = Some(device_kind);
        self
    }

    pub fn with_platform(mut self, platform: PlatformKind) -> Self {
        self.platform = Some(platform);
        self
    }

    pub fn with_fingerprint(mut self, fingerprint: impl Into<String>) -> Self {
        let fingerprint = fingerprint.into();
        if !fingerprint.trim().is_empty() {
            self.fingerprint = Some(fingerprint);
        }
        self
    }

    pub fn to_code(&self) -> NekoDropResult<String> {
        validate_endpoint(&self.endpoint)?;

        let mut parts = match self.endpoint.transport {
            TransportKind::Tcp => vec![
                PREFIX.to_string(),
                "transport=tcp".to_string(),
                format!("host={}", encode_field(&self.endpoint.host)),
                format!("port={}", self.endpoint.port),
            ],
            TransportKind::Iroh => {
                let mut parts = vec![
                    PREFIX.to_string(),
                    "transport=iroh".to_string(),
                    format!("node={}", encode_field(&self.endpoint.host)),
                ];
                if let Some(relay) = &self.endpoint.relay_url {
                    parts.push(format!("relay={}", encode_field(relay)));
                }
                if !self.endpoint.direct_addrs.is_empty() {
                    parts.push(format!(
                        "addrs={}",
                        encode_field(&self.endpoint.direct_addrs.join(","))
                    ));
                }
                parts
            }
            other => {
                return Err(NekoDropError::Network(format!(
                    "connection ticket does not support transport {}",
                    other.as_str()
                )));
            }
        };

        if let Some(device_id) = &self.device_id {
            parts.push(format!("device_id={}", encode_field(device_id)));
        }

        if let Some(device_name) = &self.device_name {
            parts.push(format!("name={}", encode_field(device_name)));
        }

        if let Some(device_kind) = self.device_kind {
            parts.push(format!("kind={}", device_kind.as_str()));
        }

        if let Some(platform) = self.platform {
            parts.push(format!("platform={}", platform.as_str()));
        }

        if let Some(fingerprint) = &self.fingerprint {
            parts.push(format!("fingerprint={}", encode_field(fingerprint)));
        }

        Ok(parts.join(";"))
    }

    pub fn parse(code: &str) -> NekoDropResult<Self> {
        let mut parts = code.trim().split(';');
        let Some(prefix) = parts.next() else {
            return Err(NekoDropError::Network("empty connection code".into()));
        };
        if prefix != PREFIX {
            return Err(NekoDropError::InvalidConnectionCode(format!(
                "unsupported prefix: {prefix}"
            )));
        }

        let mut fields = BTreeMap::new();
        for part in parts {
            let (key, value) = part.split_once('=').ok_or_else(|| {
                NekoDropError::InvalidConnectionCode(format!("invalid field: {part}"))
            })?;
            fields.insert(key.to_string(), decode_field(value)?);
        }

        let transport = fields
            .get("transport")
            .ok_or_else(|| NekoDropError::Network("connection code missing transport".into()))?;
        let transport = TransportKind::parse(transport).ok_or_else(|| {
            NekoDropError::Network(format!("unsupported connection transport: {transport}"))
        })?;

        let endpoint = match transport {
            TransportKind::Tcp => {
                let host = fields
                    .get("host")
                    .ok_or_else(|| NekoDropError::Network("connection code missing host".into()))?
                    .to_string();
                let port = fields
                    .get("port")
                    .ok_or_else(|| NekoDropError::Network("connection code missing port".into()))?
                    .parse::<u16>()
                    .map_err(|error| {
                        NekoDropError::Network(format!("invalid connection port: {error}"))
                    })?;
                Endpoint::tcp(host, port)
            }
            TransportKind::Iroh => {
                let node = fields
                    .get("node")
                    .ok_or_else(|| {
                        NekoDropError::InvalidConnectionCode(
                            "iroh connection code missing node".into(),
                        )
                    })?
                    .clone();
                let relay = fields.get("relay").cloned();
                let direct_addrs = fields
                    .get("addrs")
                    .map(|value| {
                        value
                            .split(',')
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                crate::iroh_transport::endpoint_from_iroh_fields(&node, relay, direct_addrs)?
            }
            other => {
                return Err(NekoDropError::Network(format!(
                    "connection ticket does not support transport {}",
                    other.as_str()
                )));
            }
        };

        let ticket = Self {
            endpoint,
            device_id: fields.get("device_id").cloned(),
            device_name: fields.get("name").cloned(),
            device_kind: fields
                .get("kind")
                .map(|value| DeviceKind::parse(value.as_str())),
            platform: fields
                .get("platform")
                .map(|value| PlatformKind::parse(value.as_str())),
            fingerprint: fields.get("fingerprint").cloned(),
        };
        validate_endpoint(&ticket.endpoint)?;
        Ok(ticket)
    }
}

fn validate_endpoint(endpoint: &Endpoint) -> NekoDropResult<()> {
    match endpoint.transport {
        TransportKind::Tcp => {
            if endpoint.host.trim().is_empty() {
                return Err(NekoDropError::Network(
                    "connection ticket host cannot be empty".into(),
                ));
            }
            if endpoint.port == 0 {
                return Err(NekoDropError::Network(
                    "connection ticket port cannot be 0".into(),
                ));
            }
        }
        TransportKind::Iroh => {
            if endpoint.host.trim().is_empty() {
                return Err(NekoDropError::InvalidConnectionCode(
                    "iroh connection code missing node id".into(),
                ));
            }
        }
        other => {
            return Err(NekoDropError::Network(format!(
                "connection ticket does not support transport {}",
                other.as_str()
            )));
        }
    }
    Ok(())
}

fn encode_field(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':' | b'[' | b']') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
}

fn decode_field(value: &str) -> NekoDropResult<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }

        if index + 2 >= bytes.len() {
            return Err(NekoDropError::Network(format!(
                "invalid percent encoding in connection field: {value}"
            )));
        }
        let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).map_err(|error| {
            NekoDropError::Network(format!("invalid percent encoding bytes: {error}"))
        })?;
        let byte = u8::from_str_radix(hex, 16).map_err(|error| {
            NekoDropError::Network(format!("invalid percent encoding value {hex}: {error}"))
        })?;
        decoded.push(byte);
        index += 3;
    }

    String::from_utf8(decoded)
        .map_err(|error| NekoDropError::Network(format!("connection field is not UTF-8: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_connection_ticket() {
        let ticket = ConnectionTicket::new(Endpoint::tcp("192.168.1.24", 45821))
            .unwrap()
            .with_device_id("neko-device-abc123")
            .with_device_name("Hisakazu Mac")
            .with_device_kind(DeviceKind::Desktop)
            .with_platform(PlatformKind::Macos)
            .with_fingerprint("sha256:abc123");

        let code = ticket.to_code().unwrap();
        let parsed = ConnectionTicket::parse(&code).unwrap();

        assert_eq!(parsed.endpoint, Endpoint::tcp("192.168.1.24", 45821));
        assert_eq!(parsed.device_id.as_deref(), Some("neko-device-abc123"));
        assert_eq!(parsed.device_name.as_deref(), Some("Hisakazu Mac"));
        assert_eq!(parsed.device_kind, Some(DeviceKind::Desktop));
        assert_eq!(parsed.platform, Some(PlatformKind::Macos));
        assert_eq!(parsed.fingerprint.as_deref(), Some("sha256:abc123"));
    }

    #[test]
    fn includes_device_identity_in_connection_ticket() {
        let identity = DeviceIdentity::new(
            "neko-device-def456",
            "Windows Workstation",
            DeviceKind::Desktop,
            PlatformKind::Windows,
            "sha256:def456",
            [],
        );
        let ticket = ConnectionTicket::new(Endpoint::tcp("10.0.0.8", 45821))
            .unwrap()
            .with_device_identity(&identity);

        let parsed = ConnectionTicket::parse(&ticket.to_code().unwrap()).unwrap();

        assert_eq!(parsed.device_id.as_deref(), Some("neko-device-def456"));
        assert_eq!(parsed.device_name.as_deref(), Some("Windows Workstation"));
        assert_eq!(parsed.device_kind, Some(DeviceKind::Desktop));
        assert_eq!(parsed.platform, Some(PlatformKind::Windows));
        assert_eq!(parsed.fingerprint.as_deref(), Some("sha256:def456"));
    }

    #[test]
    fn rejects_unsupported_connection_code() {
        assert!(ConnectionTicket::parse("bad-prefix;host=127.0.0.1;port=45821").is_err());
        assert!(ConnectionTicket::parse("nekodrop-v1;transport=tcp;host=;port=45821").is_err());
        assert!(
            ConnectionTicket::parse("nekodrop-v1;transport=tcp;host=127.0.0.1;port=0").is_err()
        );
    }
}
