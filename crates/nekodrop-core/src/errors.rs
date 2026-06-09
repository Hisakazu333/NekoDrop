use std::fmt;

pub type NekoDropResult<T> = Result<T, NekoDropError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NekoDropError {
    InvalidDeviceName,
    InvalidManifestPath(String),
    DeviceNotTrusted,
    PairingRequired,
    TransferAlreadyFinished,
    UnsupportedProtocol,
    Storage(String),
    Network(String),
}

impl fmt::Display for NekoDropError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDeviceName => write!(f, "device name cannot be empty"),
            Self::InvalidManifestPath(path) => write!(f, "invalid manifest path: {path}"),
            Self::DeviceNotTrusted => write!(f, "device is not trusted"),
            Self::PairingRequired => write!(f, "pairing is required"),
            Self::TransferAlreadyFinished => write!(f, "transfer is already finished"),
            Self::UnsupportedProtocol => write!(f, "unsupported protocol version"),
            Self::Storage(message) => write!(f, "storage error: {message}"),
            Self::Network(message) => write!(f, "network error: {message}"),
        }
    }
}

impl std::error::Error for NekoDropError {}
