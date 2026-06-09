use crate::transport::{Endpoint, TransportKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiveServerConfig {
    pub bind_host: String,
    pub port: u16,
    pub transport: TransportKind,
}

impl Default for ReceiveServerConfig {
    fn default() -> Self {
        Self {
            bind_host: "0.0.0.0".to_string(),
            port: 45821,
            transport: TransportKind::Tcp,
        }
    }
}

impl ReceiveServerConfig {
    pub fn endpoint(&self) -> Endpoint {
        Endpoint {
            host: self.bind_host.clone(),
            port: self.port,
            transport: self.transport,
        }
    }
}
