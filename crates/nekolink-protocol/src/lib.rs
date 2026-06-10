use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_NAME: &str = "nekolink";
pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope<T = Value> {
    pub protocol: String,
    pub version: u16,
    pub session_id: String,
    pub message_id: String,
    pub kind: MessageKind,
    pub sent_at_ms: u128,
    pub capabilities: Vec<Capability>,
    pub payload: T,
}

impl<T> Envelope<T> {
    pub fn new(
        session_id: impl Into<String>,
        message_id: impl Into<String>,
        kind: MessageKind,
        payload: T,
    ) -> Self {
        Self {
            protocol: PROTOCOL_NAME.to_string(),
            version: PROTOCOL_VERSION,
            session_id: session_id.into(),
            message_id: message_id.into(),
            kind,
            sent_at_ms: now_ms(),
            capabilities: Vec::new(),
            payload,
        }
    }

    pub fn with_capabilities(mut self, capabilities: impl Into<Vec<Capability>>) -> Self {
        self.capabilities = capabilities.into();
        self
    }

    pub fn validate_kind(&self, expected: MessageKind) -> Result<(), ProtocolError> {
        self.validate()?;
        if self.kind != expected {
            return Err(ProtocolError::new(
                ErrorCode::UnexpectedMessageKind,
                format!(
                    "unexpected message kind: expected {}, got {}",
                    expected.as_str(),
                    self.kind.as_str()
                ),
            ));
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.protocol != PROTOCOL_NAME {
            return Err(ProtocolError::new(
                ErrorCode::UnsupportedProtocol,
                format!("unsupported protocol: {}", self.protocol),
            ));
        }
        if self.version != PROTOCOL_VERSION {
            return Err(ProtocolError::new(
                ErrorCode::UnsupportedVersion,
                format!("unsupported protocol version: {}", self.version),
            ));
        }
        if self.session_id.trim().is_empty() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidEnvelope,
                "session_id cannot be empty",
            ));
        }
        if self.message_id.trim().is_empty() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidEnvelope,
                "message_id cannot be empty",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageKind {
    #[serde(rename = "device.hello")]
    DeviceHello,
    #[serde(rename = "device.heartbeat")]
    DeviceHeartbeat,
    #[serde(rename = "file.offer")]
    FileOffer,
    #[serde(rename = "file.accept")]
    FileAccept,
    #[serde(rename = "file.decline")]
    FileDecline,
    #[serde(rename = "file.header")]
    FileHeader,
    #[serde(rename = "file.complete")]
    FileComplete,
    #[serde(rename = "transfer.complete")]
    TransferComplete,
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "agent.command")]
    AgentCommand,
    #[serde(rename = "agent.result")]
    AgentResult,
    #[serde(rename = "companion.state")]
    CompanionState,
    #[serde(rename = "state.sync")]
    StateSync,
}

impl MessageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DeviceHello => "device.hello",
            Self::DeviceHeartbeat => "device.heartbeat",
            Self::FileOffer => "file.offer",
            Self::FileAccept => "file.accept",
            Self::FileDecline => "file.decline",
            Self::FileHeader => "file.header",
            Self::FileComplete => "file.complete",
            Self::TransferComplete => "transfer.complete",
            Self::Error => "error",
            Self::AgentCommand => "agent.command",
            Self::AgentResult => "agent.result",
            Self::CompanionState => "companion.state",
            Self::StateSync => "state.sync",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    FileTransfer,
    FileSend,
    FileReceive,
    FileSha256,
    FileResume,
    DevicePairing,
    EncryptedSession,
    AgentCommand,
    DesktopAgentHost,
    MobileCompanion,
    CompanionState,
    StateSync,
}

impl Capability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FileTransfer => "file_transfer",
            Self::FileSend => "file_send",
            Self::FileReceive => "file_receive",
            Self::FileSha256 => "file_sha256",
            Self::FileResume => "file_resume",
            Self::DevicePairing => "device_pairing",
            Self::EncryptedSession => "encrypted_session",
            Self::AgentCommand => "agent_command",
            Self::DesktopAgentHost => "desktop_agent_host",
            Self::MobileCompanion => "mobile_companion",
            Self::CompanionState => "companion_state",
            Self::StateSync => "state_sync",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    Desktop,
    Phone,
    Tablet,
    #[serde(rename = "openharmony")]
    OpenHarmony,
    Web,
    Nas,
    AgentNode,
    Unknown,
}

impl DeviceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::Phone => "phone",
            Self::Tablet => "tablet",
            Self::OpenHarmony => "openharmony",
            Self::Web => "web",
            Self::Nas => "nas",
            Self::AgentNode => "agent_node",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "desktop" => Self::Desktop,
            "phone" => Self::Phone,
            "tablet" => Self::Tablet,
            "openharmony" | "open_harmony" => Self::OpenHarmony,
            "web" => Self::Web,
            "nas" => Self::Nas,
            "agent_node" => Self::AgentNode,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlatformKind {
    Macos,
    Windows,
    Linux,
    Ios,
    Android,
    #[serde(rename = "openharmony")]
    OpenHarmony,
    Web,
    Unknown,
}

impl PlatformKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Macos => "macos",
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::Ios => "ios",
            Self::Android => "android",
            Self::OpenHarmony => "openharmony",
            Self::Web => "web",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "macos" => Self::Macos,
            "windows" => Self::Windows,
            "linux" => Self::Linux,
            "ios" => Self::Ios,
            "android" => Self::Android,
            "openharmony" | "open_harmony" => Self::OpenHarmony,
            "web" => Self::Web,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceIdentity {
    pub device_id: String,
    pub device_name: String,
    pub device_kind: DeviceKind,
    pub platform: PlatformKind,
    pub public_key_fingerprint: String,
    pub capabilities: Vec<Capability>,
}

impl DeviceIdentity {
    pub fn new(
        device_id: impl Into<String>,
        device_name: impl Into<String>,
        device_kind: DeviceKind,
        platform: PlatformKind,
        public_key_fingerprint: impl Into<String>,
        capabilities: impl Into<Vec<Capability>>,
    ) -> Self {
        Self {
            device_id: device_id.into(),
            device_name: device_name.into(),
            device_kind,
            platform,
            public_key_fingerprint: public_key_fingerprint.into(),
            capabilities: capabilities.into(),
        }
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.device_id.trim().is_empty() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "device_id cannot be empty",
            ));
        }
        if self.device_name.trim().is_empty() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "device_name cannot be empty",
            ));
        }
        if self.public_key_fingerprint.trim().is_empty() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "public_key_fingerprint cannot be empty",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceHello {
    pub identity: DeviceIdentity,
    pub app_name: String,
    pub app_version: String,
}

impl DeviceHello {
    pub fn new(
        identity: DeviceIdentity,
        app_name: impl Into<String>,
        app_version: impl Into<String>,
    ) -> Self {
        Self {
            identity,
            app_name: app_name.into(),
            app_version: app_version.into(),
        }
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.identity.validate()?;
        if self.app_name.trim().is_empty() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "app_name cannot be empty",
            ));
        }
        if self.app_version.trim().is_empty() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "app_version cannot be empty",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    UnsupportedProtocol,
    UnsupportedVersion,
    InvalidEnvelope,
    UnexpectedMessageKind,
    InvalidPayload,
    DeviceNotTrusted,
    PairingRequired,
    UserDeclined,
    Timeout,
    DiskFull,
    PermissionDenied,
    FileChanged,
    ChecksumFailed,
    NetworkInterrupted,
    TransferCancelled,
    InternalError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolError {
    pub code: ErrorCode,
    pub message: String,
}

impl ProtocolError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferOfferFile {
    pub manifest_path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferOffer {
    pub transfer_id: String,
    pub root_name: String,
    pub file_count: usize,
    pub total_bytes: u64,
    pub files: Vec<TransferOfferFile>,
}

impl TransferOffer {
    pub fn new(
        transfer_id: impl Into<String>,
        root_name: impl Into<String>,
        files: Vec<TransferOfferFile>,
    ) -> Self {
        let file_count = files.len();
        let total_bytes = files.iter().map(|file| file.size).sum();
        Self {
            transfer_id: transfer_id.into(),
            root_name: root_name.into(),
            file_count,
            total_bytes,
            files,
        }
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.transfer_id.trim().is_empty() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "transfer_id cannot be empty",
            ));
        }
        if self.file_count != self.files.len() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                format!(
                    "transfer offer file count mismatch: {} != {}",
                    self.file_count,
                    self.files.len()
                ),
            ));
        }
        let total_bytes = self.files.iter().map(|file| file.size).sum::<u64>();
        if self.total_bytes != total_bytes {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                format!(
                    "transfer offer size mismatch: {} != {}",
                    self.total_bytes, total_bytes
                ),
            ));
        }
        for file in &self.files {
            if file.manifest_path.trim().is_empty() {
                return Err(ProtocolError::new(
                    ErrorCode::InvalidPayload,
                    "file manifest_path cannot be empty",
                ));
            }
            if file.sha256.trim().is_empty() {
                return Err(ProtocolError::new(
                    ErrorCode::InvalidPayload,
                    "file sha256 cannot be empty",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferDecision {
    pub accepted: bool,
    pub reason: Option<String>,
}

impl TransferDecision {
    pub fn accept() -> Self {
        Self {
            accepted: true,
            reason: None,
        }
    }

    pub fn decline(reason: impl Into<String>) -> Self {
        Self {
            accepted: false,
            reason: Some(reason.into()),
        }
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_envelope_protocol_and_kind() {
        let envelope = Envelope::new(
            "session-1",
            "message-1",
            MessageKind::FileOffer,
            TransferOffer::new(
                "transfer-1",
                "drop",
                vec![TransferOfferFile {
                    manifest_path: "drop/file.txt".to_string(),
                    size: 10,
                    sha256: "abc".to_string(),
                }],
            ),
        );

        envelope.validate_kind(MessageKind::FileOffer).unwrap();
        assert!(envelope.validate_kind(MessageKind::FileAccept).is_err());
    }

    #[test]
    fn validates_transfer_offer_totals() {
        let offer = TransferOffer::new(
            "transfer-1",
            "drop",
            vec![TransferOfferFile {
                manifest_path: "drop/file.txt".to_string(),
                size: 10,
                sha256: "abc".to_string(),
            }],
        );

        assert_eq!(offer.file_count, 1);
        assert_eq!(offer.total_bytes, 10);
        offer.validate().unwrap();
    }

    #[test]
    fn validates_device_identity() {
        let identity = DeviceIdentity::new(
            "neko-device-abc123",
            "Hisakazu Mac",
            DeviceKind::Desktop,
            PlatformKind::Macos,
            "sha256:abc123",
            [
                Capability::FileTransfer,
                Capability::FileReceive,
                Capability::DevicePairing,
            ],
        );
        identity.validate().unwrap();

        let hello = DeviceHello::new(identity, "NekoDrop", "0.1.0");
        hello.validate().unwrap();
    }

    #[test]
    fn parses_unknown_device_kinds_conservatively() {
        assert_eq!(DeviceKind::parse("phone"), DeviceKind::Phone);
        assert_eq!(DeviceKind::parse("watch"), DeviceKind::Unknown);
        assert_eq!(
            PlatformKind::parse("openharmony"),
            PlatformKind::OpenHarmony
        );
        assert_eq!(PlatformKind::parse("visionos"), PlatformKind::Unknown);
    }
}
