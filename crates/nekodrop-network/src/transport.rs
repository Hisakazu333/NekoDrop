use std::io::{Read, Write};
use std::net::TcpStream;

use nekodrop_core::{NekoDropError, NekoDropResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    Tcp,
    Iroh,
    Quic,
    Relay,
}

impl TransportKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Iroh => "iroh",
            Self::Quic => "quic",
            Self::Relay => "relay",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "tcp" => Some(Self::Tcp),
            "iroh" => Some(Self::Iroh),
            "quic" => Some(Self::Quic),
            "relay" => Some(Self::Relay),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub transport: TransportKind,
}

impl Endpoint {
    pub fn tcp(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            transport: TransportKind::Tcp,
        }
    }
}

pub trait TransportStream: Read + Write + Send {}

impl<T> TransportStream for T where T: Read + Write + Send {}

pub trait NekoLinkTransport {
    type Stream: TransportStream;

    fn kind(&self) -> TransportKind;

    fn connect(&self, endpoint: &Endpoint) -> NekoDropResult<Self::Stream>;
}

pub fn connect_endpoint(endpoint: &Endpoint) -> NekoDropResult<Box<dyn TransportStream>> {
    match endpoint.transport {
        TransportKind::Tcp => Ok(Box::new(TcpTransport.connect(endpoint)?)),
        TransportKind::Iroh => Err(transport_not_available_error(TransportKind::Iroh)),
        TransportKind::Quic => Err(transport_not_available_error(TransportKind::Quic)),
        TransportKind::Relay => Err(transport_not_available_error(TransportKind::Relay)),
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TcpTransport;

impl NekoLinkTransport for TcpTransport {
    type Stream = TcpStream;

    fn kind(&self) -> TransportKind {
        TransportKind::Tcp
    }

    fn connect(&self, endpoint: &Endpoint) -> NekoDropResult<Self::Stream> {
        if endpoint.transport != TransportKind::Tcp {
            return Err(unsupported_transport_error(endpoint.transport, self.kind()));
        }
        if endpoint.port == 0 {
            return Err(NekoDropError::Network("endpoint port cannot be 0".into()));
        }

        TcpStream::connect((endpoint.host.as_str(), endpoint.port)).map_err(|error| {
            NekoDropError::Network(format!(
                "failed to connect to {}:{}: {error}",
                endpoint.host, endpoint.port
            ))
        })
    }
}

fn unsupported_transport_error(
    requested: TransportKind,
    supported: TransportKind,
) -> NekoDropError {
    NekoDropError::Network(format!(
        "unsupported transport: requested {}, supported {}",
        requested.as_str(),
        supported.as_str()
    ))
}

fn transport_not_available_error(transport: TransportKind) -> NekoDropError {
    NekoDropError::Network(format!(
        "{} transport is not available in this build",
        transport.as_str()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_kind_has_stable_wire_labels() {
        assert_eq!(TransportKind::Tcp.as_str(), "tcp");
        assert_eq!(TransportKind::parse("tcp"), Some(TransportKind::Tcp));
        assert_eq!(TransportKind::Iroh.as_str(), "iroh");
        assert_eq!(TransportKind::parse("iroh"), Some(TransportKind::Iroh));
        assert_eq!(TransportKind::Relay.as_str(), "relay");
        assert_eq!(TransportKind::parse("relay"), Some(TransportKind::Relay));
        assert_eq!(TransportKind::parse("unknown"), None);
    }

    #[test]
    fn tcp_transport_rejects_non_tcp_endpoint() {
        let transport = TcpTransport;
        let endpoint = Endpoint {
            host: "127.0.0.1".to_string(),
            port: 45821,
            transport: TransportKind::Iroh,
        };
        let error = transport.connect(&endpoint).unwrap_err();

        assert!(error.to_string().contains("requested iroh"));
    }

    #[test]
    fn connect_endpoint_routes_unsupported_transport_to_clear_error() {
        let endpoint = Endpoint {
            host: "127.0.0.1".to_string(),
            port: 45821,
            transport: TransportKind::Iroh,
        };
        let error = connect_endpoint(&endpoint).err().unwrap();

        assert!(error
            .to_string()
            .contains("iroh transport is not available"));
    }
}
