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
    /// IO 层错误（连接被拒/重置/超时/DNS 等），message 携带 io::ErrorKind 名与原文。
    Io {
        kind: String,
        message: String,
    },
    /// 对方用户或本机用户明确拒绝，重试无意义。
    TransferDeclined(String),
    /// 用户主动取消。
    TransferCancelled,
    /// 连接码无法解析。
    InvalidConnectionCode(String),
    /// 请求的 transport 在当前构建里不存在。
    UnsupportedTransport(String),
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
            Self::Io { kind, message } => write!(f, "io error ({kind}): {message}"),
            Self::TransferDeclined(reason) => write!(f, "receiver declined transfer: {reason}"),
            Self::TransferCancelled => write!(f, "transfer cancelled"),
            Self::InvalidConnectionCode(reason) => write!(f, "invalid connection code: {reason}"),
            Self::UnsupportedTransport(reason) => write!(f, "{reason}"),
        }
    }
}

impl std::error::Error for NekoDropError {}
