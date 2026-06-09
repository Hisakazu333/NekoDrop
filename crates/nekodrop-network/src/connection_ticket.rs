use std::collections::BTreeMap;

use nekodrop_core::{NekoDropError, NekoDropResult};

use crate::{Endpoint, TransportKind};

const PREFIX: &str = "nekodrop-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionTicket {
    pub endpoint: Endpoint,
    pub device_name: Option<String>,
    pub fingerprint: Option<String>,
}

impl ConnectionTicket {
    pub fn new(endpoint: Endpoint) -> NekoDropResult<Self> {
        validate_tcp_endpoint(&endpoint)?;
        Ok(Self {
            endpoint,
            device_name: None,
            fingerprint: None,
        })
    }

    pub fn with_device_name(mut self, device_name: impl Into<String>) -> Self {
        let device_name = device_name.into();
        if !device_name.trim().is_empty() {
            self.device_name = Some(device_name);
        }
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
        validate_tcp_endpoint(&self.endpoint)?;

        let mut parts = vec![
            PREFIX.to_string(),
            "transport=tcp".to_string(),
            format!("host={}", encode_field(&self.endpoint.host)),
            format!("port={}", self.endpoint.port),
        ];

        if let Some(device_name) = &self.device_name {
            parts.push(format!("name={}", encode_field(device_name)));
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
            return Err(NekoDropError::Network(format!(
                "unsupported connection code prefix: {prefix}"
            )));
        }

        let mut fields = BTreeMap::new();
        for part in parts {
            let (key, value) = part.split_once('=').ok_or_else(|| {
                NekoDropError::Network(format!("invalid connection code field: {part}"))
            })?;
            fields.insert(key.to_string(), decode_field(value)?);
        }

        let transport = fields
            .get("transport")
            .ok_or_else(|| NekoDropError::Network("connection code missing transport".into()))?;
        if transport != "tcp" {
            return Err(NekoDropError::Network(format!(
                "unsupported connection transport: {transport}"
            )));
        }

        let host = fields
            .get("host")
            .ok_or_else(|| NekoDropError::Network("connection code missing host".into()))?
            .to_string();
        let port = fields
            .get("port")
            .ok_or_else(|| NekoDropError::Network("connection code missing port".into()))?
            .parse::<u16>()
            .map_err(|error| NekoDropError::Network(format!("invalid connection port: {error}")))?;

        let ticket = Self {
            endpoint: Endpoint::tcp(host, port),
            device_name: fields.get("name").cloned(),
            fingerprint: fields.get("fingerprint").cloned(),
        };
        validate_tcp_endpoint(&ticket.endpoint)?;
        Ok(ticket)
    }
}

fn validate_tcp_endpoint(endpoint: &Endpoint) -> NekoDropResult<()> {
    if endpoint.transport != TransportKind::Tcp {
        return Err(NekoDropError::Network(format!(
            "connection ticket only supports TCP, got {:?}",
            endpoint.transport
        )));
    }
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
            .with_device_name("Hisakazu Mac")
            .with_fingerprint("sha256:abc123");

        let code = ticket.to_code().unwrap();
        let parsed = ConnectionTicket::parse(&code).unwrap();

        assert_eq!(parsed.endpoint, Endpoint::tcp("192.168.1.24", 45821));
        assert_eq!(parsed.device_name.as_deref(), Some("Hisakazu Mac"));
        assert_eq!(parsed.fingerprint.as_deref(), Some("sha256:abc123"));
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
