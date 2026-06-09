use nekodrop_core::{NekoDropError, NekoDropResult};

use crate::transport::Endpoint;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferClient {
    pub endpoint: Endpoint,
}

impl TransferClient {
    pub fn new(endpoint: Endpoint) -> Self {
        Self { endpoint }
    }

    pub fn ensure_supported(&self) -> NekoDropResult<()> {
        if self.endpoint.port == 0 {
            return Err(NekoDropError::Network("endpoint port cannot be 0".into()));
        }

        Ok(())
    }
}
