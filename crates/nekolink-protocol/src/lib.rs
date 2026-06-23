use std::{
    collections::{BTreeMap, BTreeSet},
    time::{SystemTime, UNIX_EPOCH},
};

use aes_gcm::{Aes256Gcm, Nonce as AesGcmNonce};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chacha20poly1305::{
    aead::{Aead, Payload},
    KeyInit, XChaCha20Poly1305, XNonce,
};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret as X25519StaticSecret};

pub const PROTOCOL_NAME: &str = "nekolink";
pub const PROTOCOL_VERSION: u16 = 1;
pub const SESSION_KEY_AGREEMENT_X25519: &str = "x25519";
pub const SESSION_CIPHER_XCHACHA20POLY1305: &str = "xchacha20poly1305";
pub const SESSION_CIPHER_AES256GCM: &str = "aes256gcm";
pub const SESSION_SHARED_SECRET_LEN: usize = 32;
pub const SESSION_TRAFFIC_KEY_LEN: usize = 32;
pub const SESSION_PUBLIC_KEY_BASE64_LEN: usize = 43;
pub const SESSION_AES256GCM_NONCE_LEN: usize = 12;
pub const SESSION_XCHACHA20POLY1305_NONCE_LEN: usize = 24;
pub const SESSION_REPLAY_WINDOW_SIZE: u64 = 64;
pub const SESSION_FILE_FRAME_MAX_PLAINTEXT_LEN: u64 = 64 * 1024;
pub const DEVICE_IDENTITY_SIGNATURE_ED25519: &str = "ed25519";
pub const DEVICE_IDENTITY_SIGNING_KEY_LEN: usize = 32;
pub const DEVICE_IDENTITY_PUBLIC_KEY_LEN: usize = 32;
pub const DEVICE_IDENTITY_SIGNATURE_LEN: usize = 64;
pub const BUNDLE_SCHEMA_V1: &str = "nekolink.bundle.v1";
pub const BUNDLE_CHECKSUM_SHA256: &str = "sha256";

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
    #[serde(rename = "session.hello")]
    SessionHello,
    #[serde(rename = "session.ready")]
    SessionReady,
    #[serde(rename = "session.identity")]
    SessionIdentity,
    #[serde(rename = "session.control")]
    SessionControl,
    #[serde(rename = "pairing.request")]
    PairingRequest,
    #[serde(rename = "pairing.accept")]
    PairingAccept,
    #[serde(rename = "pairing.reject")]
    PairingReject,
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
            Self::SessionHello => "session.hello",
            Self::SessionReady => "session.ready",
            Self::SessionIdentity => "session.identity",
            Self::SessionControl => "session.control",
            Self::PairingRequest => "pairing.request",
            Self::PairingAccept => "pairing.accept",
            Self::PairingReject => "pairing.reject",
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingRequestPayload {
    pub request_id: String,
    pub device_id: String,
    pub device_name: String,
    pub platform: String,
    pub public_key: String,
    pub public_key_fingerprint: String,
    pub pairing_code: String,
    pub listen_port: u16,
}

impl PairingRequestPayload {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.request_id.trim().is_empty() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "request_id cannot be empty",
            ));
        }
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
        let public_key = DeviceIdentityPublicKey::from_encoded(&self.public_key)?;
        if self.public_key_fingerprint != public_key.fingerprint {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "public_key_fingerprint must match public_key",
            ));
        }
        if self.pairing_code.trim().is_empty() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "pairing_code cannot be empty",
            ));
        }
        if self.listen_port == 0 {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "listen_port cannot be 0",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingDecisionPayload {
    pub accepted: bool,
    pub reason: Option<String>,
}

impl PairingDecisionPayload {
    pub fn accept() -> Self {
        Self {
            accepted: true,
            reason: None,
        }
    }

    pub fn reject(reason: impl Into<String>) -> Self {
        Self {
            accepted: false,
            reason: Some(reason.into()),
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
    BundleTransfer,
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
            Self::BundleTransfer => "bundle_transfer",
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

    pub fn supports(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }

    pub fn require_capability(&self, capability: Capability) -> Result<(), ProtocolError> {
        if self.supports(capability) {
            return Ok(());
        }

        Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            format!(
                "device {} does not support {}",
                self.device_id,
                capability.as_str()
            ),
        ))
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

    pub fn supports(&self, capability: Capability) -> bool {
        self.identity.supports(capability)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionHelloPayload {
    pub session_id: String,
    pub identity: DeviceIdentity,
    pub key_agreement: String,
    pub ephemeral_public_key: String,
    pub supported_ciphers: Vec<String>,
}

impl SessionHelloPayload {
    pub fn new(
        session_id: impl Into<String>,
        identity: DeviceIdentity,
        key_agreement: impl Into<String>,
        ephemeral_public_key: impl Into<String>,
        supported_ciphers: Vec<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            identity,
            key_agreement: key_agreement.into(),
            ephemeral_public_key: ephemeral_public_key.into(),
            supported_ciphers,
        }
    }

    pub fn default_crypto(
        session_id: impl Into<String>,
        identity: DeviceIdentity,
        ephemeral_public_key: impl Into<String>,
    ) -> Self {
        Self::new(
            session_id,
            identity,
            SESSION_KEY_AGREEMENT_X25519,
            ephemeral_public_key,
            default_session_cipher_preference(),
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_session_id(&self.session_id)?;
        self.identity.validate()?;
        self.identity
            .require_capability(Capability::EncryptedSession)?;
        validate_session_key_agreement(&self.key_agreement)?;
        validate_session_crypto_label("ephemeral_public_key", &self.ephemeral_public_key)?;
        if self.supported_ciphers.is_empty() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "supported_ciphers cannot be empty",
            ));
        }
        for cipher in &self.supported_ciphers {
            validate_session_cipher("supported_ciphers", cipher)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionReadyPayload {
    pub session_id: String,
    pub identity: DeviceIdentity,
    pub key_agreement: String,
    pub ephemeral_public_key: String,
    pub cipher: String,
    pub handshake_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedSessionHandshake {
    pub session_id: String,
    pub key_agreement: String,
    pub cipher: String,
    pub handshake_hash: String,
    pub initiator_ephemeral_public_key: String,
    pub responder_ephemeral_public_key: String,
    pub initiator: DeviceIdentity,
    pub responder: DeviceIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionParticipantRole {
    Initiator,
    Responder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionIdentityBinding {
    pub role: SessionParticipantRole,
    pub session_id: String,
    pub device_id: String,
    pub public_key_fingerprint: String,
    pub session_ephemeral_public_key: String,
    pub handshake_hash: String,
}

#[derive(Clone)]
pub struct DeviceIdentitySigningKey {
    signing_key: SigningKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceIdentityPublicKey {
    pub algorithm: String,
    pub public_key: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedSessionIdentityBinding {
    pub binding: SessionIdentityBinding,
    pub algorithm: String,
    pub public_key: String,
    pub public_key_fingerprint: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionKeyDerivationContext {
    pub session_id: String,
    pub key_agreement: String,
    pub cipher: String,
    pub handshake_hash: String,
    pub salt: String,
    pub send_info: String,
    pub receive_info: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionKeyMaterial {
    pub send_key: [u8; SESSION_TRAFFIC_KEY_LEN],
    pub receive_key: [u8; SESSION_TRAFFIC_KEY_LEN],
}

impl SessionKeyMaterial {
    pub fn seal_send_payload(
        &self,
        header: &SessionTrafficFrameHeader,
        associated_data: &[u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, ProtocolError> {
        seal_session_payload(
            &header.cipher,
            &self.send_key,
            &header.nonce,
            associated_data,
            plaintext,
        )
    }

    pub fn open_receive_payload(
        &self,
        header: &SessionTrafficFrameHeader,
        associated_data: &[u8],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, ProtocolError> {
        open_session_payload(
            &header.cipher,
            &self.receive_key,
            &header.nonce,
            associated_data,
            ciphertext,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionFrameKind {
    Control,
    File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionFrameDirection {
    Send,
    Receive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTrafficFrameHeader {
    pub cipher: String,
    pub kind: SessionFrameKind,
    pub direction: SessionFrameDirection,
    pub counter: u64,
    pub nonce: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedSessionPayload {
    pub inner_kind: MessageKind,
    pub header: SessionTrafficFrameHeader,
    pub ciphertext: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedFileFrameHeader {
    pub transfer_id: String,
    pub manifest_path: String,
    pub offset: u64,
    pub plain_size: u64,
    pub traffic: SessionTrafficFrameHeader,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedFileFrame {
    pub header: EncryptedFileFrameHeader,
    pub ciphertext: Vec<u8>,
}

impl EncryptedSessionPayload {
    pub fn seal_control<T: Serialize>(
        session_id: impl Into<String>,
        message_id: impl Into<String>,
        keys: &SessionKeyMaterial,
        header: SessionTrafficFrameHeader,
        inner_kind: MessageKind,
        payload: &T,
    ) -> Result<Envelope<Self>, ProtocolError> {
        if header.kind != SessionFrameKind::Control {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "encrypted control payload requires a control frame header",
            ));
        }
        let session_id = session_id.into();
        let message_id = message_id.into();
        let plaintext = serde_json::to_vec(payload).map_err(|error| {
            ProtocolError::new(
                ErrorCode::InvalidPayload,
                format!("failed to serialize session control payload: {error}"),
            )
        })?;
        let associated_data = session_control_associated_data(&session_id, &message_id, inner_kind);
        let ciphertext = keys.seal_send_payload(&header, &associated_data, &plaintext)?;
        Ok(Envelope::new(
            session_id,
            message_id,
            MessageKind::SessionControl,
            Self {
                inner_kind,
                header,
                ciphertext,
            },
        ))
    }

    pub fn open_control<T: for<'de> Deserialize<'de>>(
        envelope: &Envelope<Self>,
        keys: &SessionKeyMaterial,
    ) -> Result<T, ProtocolError> {
        Self::open_control_inner(envelope, keys)
    }

    pub fn open_control_once<T: for<'de> Deserialize<'de>>(
        envelope: &Envelope<Self>,
        keys: &SessionKeyMaterial,
        replay_window: &mut SessionReplayWindow,
    ) -> Result<T, ProtocolError> {
        let payload = Self::open_control_inner(envelope, keys)?;
        replay_window.accept(&envelope.payload.header)?;
        Ok(payload)
    }

    fn open_control_inner<T: for<'de> Deserialize<'de>>(
        envelope: &Envelope<Self>,
        keys: &SessionKeyMaterial,
    ) -> Result<T, ProtocolError> {
        envelope.validate_kind(MessageKind::SessionControl)?;
        if envelope.payload.header.kind != SessionFrameKind::Control {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "encrypted control payload requires a control frame header",
            ));
        }
        let associated_data = session_control_associated_data(
            &envelope.session_id,
            &envelope.message_id,
            envelope.payload.inner_kind,
        );
        let plaintext = keys.open_receive_payload(
            &envelope.payload.header,
            &associated_data,
            &envelope.payload.ciphertext,
        )?;
        serde_json::from_slice(&plaintext).map_err(|error| {
            ProtocolError::new(
                ErrorCode::InvalidPayload,
                format!("failed to deserialize session control payload: {error}"),
            )
        })
    }
}

impl EncryptedFileFrameHeader {
    pub fn new(
        transfer_id: impl Into<String>,
        manifest_path: impl Into<String>,
        offset: u64,
        plain_size: u64,
        traffic: SessionTrafficFrameHeader,
    ) -> Result<Self, ProtocolError> {
        let header = Self {
            transfer_id: transfer_id.into(),
            manifest_path: manifest_path.into(),
            offset,
            plain_size,
            traffic,
        };
        header.validate()?;
        Ok(header)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_non_empty("transfer_id", &self.transfer_id)?;
        validate_transfer_manifest_path(&self.manifest_path)?;
        if self.plain_size == 0 {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "encrypted file frame plain_size cannot be zero",
            ));
        }
        if self.plain_size > SESSION_FILE_FRAME_MAX_PLAINTEXT_LEN {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                format!(
                    "encrypted file frame plain_size exceeds maximum: {} > {}",
                    self.plain_size, SESSION_FILE_FRAME_MAX_PLAINTEXT_LEN
                ),
            ));
        }
        if self.traffic.kind != SessionFrameKind::File {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "encrypted file frame requires a file traffic header",
            ));
        }
        Ok(())
    }

    pub fn associated_data(&self) -> Result<Vec<u8>, ProtocolError> {
        self.validate()?;
        let mut data = Vec::new();
        append_aad_field(&mut data, "transfer_id", &self.transfer_id);
        append_aad_field(&mut data, "manifest_path", &self.manifest_path);
        append_aad_field(&mut data, "offset", &self.offset.to_string());
        append_aad_field(&mut data, "plain_size", &self.plain_size.to_string());
        append_aad_field(&mut data, "frame_kind", "file");
        append_aad_field(&mut data, "traffic.cipher", &self.traffic.cipher);
        append_aad_field(&mut data, "traffic.kind", "file");
        append_aad_field(
            &mut data,
            "traffic.direction",
            self.traffic.direction.as_str(),
        );
        append_aad_field(
            &mut data,
            "traffic.counter",
            &self.traffic.counter.to_string(),
        );
        append_aad_field(
            &mut data,
            "traffic.nonce",
            &hex::encode(&self.traffic.nonce),
        );
        Ok(data)
    }
}

impl EncryptedFileFrame {
    pub fn seal(
        keys: &SessionKeyMaterial,
        header: EncryptedFileFrameHeader,
        plaintext: &[u8],
    ) -> Result<Self, ProtocolError> {
        header.validate()?;
        if plaintext.len() as u64 != header.plain_size {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                format!(
                    "encrypted file frame plaintext length mismatch: {} != {}",
                    plaintext.len(),
                    header.plain_size
                ),
            ));
        }
        let associated_data = header.associated_data()?;
        let ciphertext = keys.seal_send_payload(&header.traffic, &associated_data, plaintext)?;
        Ok(Self { header, ciphertext })
    }

    pub fn open(&self, keys: &SessionKeyMaterial) -> Result<Vec<u8>, ProtocolError> {
        self.header.validate()?;
        let associated_data = self.header.associated_data()?;
        let plaintext =
            keys.open_receive_payload(&self.header.traffic, &associated_data, &self.ciphertext)?;
        if plaintext.len() as u64 != self.header.plain_size {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                format!(
                    "encrypted file frame plaintext length mismatch: {} != {}",
                    plaintext.len(),
                    self.header.plain_size
                ),
            ));
        }
        Ok(plaintext)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionTrafficCounters {
    send_counter: u64,
    receive_counter: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionReplayWindow {
    window_size: u64,
    highest_counter: Option<u64>,
    seen_counters: BTreeSet<u64>,
}

impl SessionTrafficCounters {
    pub fn new(send_counter: u64, receive_counter: u64) -> Self {
        Self {
            send_counter,
            receive_counter,
        }
    }

    pub fn next_send_header(
        &mut self,
        cipher: &str,
        kind: SessionFrameKind,
    ) -> Result<SessionTrafficFrameHeader, ProtocolError> {
        let counter = next_session_counter(&mut self.send_counter)?;
        SessionTrafficFrameHeader::new(cipher, kind, SessionFrameDirection::Send, counter)
    }

    pub fn next_receive_header(
        &mut self,
        cipher: &str,
        kind: SessionFrameKind,
    ) -> Result<SessionTrafficFrameHeader, ProtocolError> {
        let counter = next_session_counter(&mut self.receive_counter)?;
        SessionTrafficFrameHeader::new(cipher, kind, SessionFrameDirection::Receive, counter)
    }
}

impl Default for SessionReplayWindow {
    fn default() -> Self {
        Self {
            window_size: SESSION_REPLAY_WINDOW_SIZE,
            highest_counter: None,
            seen_counters: BTreeSet::new(),
        }
    }
}

impl SessionReplayWindow {
    pub fn with_window_size(window_size: u64) -> Result<Self, ProtocolError> {
        if window_size == 0 {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "session replay window size cannot be zero",
            ));
        }

        Ok(Self {
            window_size,
            highest_counter: None,
            seen_counters: BTreeSet::new(),
        })
    }

    pub fn accept(&mut self, header: &SessionTrafficFrameHeader) -> Result<(), ProtocolError> {
        if let Some(highest_counter) = self.highest_counter {
            let minimum_counter =
                highest_counter.saturating_sub(self.window_size.saturating_sub(1));
            if header.counter < minimum_counter {
                return Err(ProtocolError::new(
                    ErrorCode::InvalidPayload,
                    "session frame counter is outside replay window",
                ));
            }
        }

        if self.seen_counters.contains(&header.counter) {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "replayed session frame counter",
            ));
        }

        self.seen_counters.insert(header.counter);
        self.highest_counter = Some(
            self.highest_counter
                .map_or(header.counter, |highest| highest.max(header.counter)),
        );
        self.prune();

        Ok(())
    }

    fn prune(&mut self) {
        let Some(highest_counter) = self.highest_counter else {
            return;
        };
        let minimum_counter = highest_counter.saturating_sub(self.window_size.saturating_sub(1));
        self.seen_counters
            .retain(|counter| *counter >= minimum_counter);
    }
}

impl SessionTrafficFrameHeader {
    pub fn new(
        cipher: &str,
        kind: SessionFrameKind,
        direction: SessionFrameDirection,
        counter: u64,
    ) -> Result<Self, ProtocolError> {
        validate_session_cipher("cipher", cipher)?;
        Ok(Self {
            cipher: cipher.to_string(),
            kind,
            direction,
            counter,
            nonce: session_frame_nonce(cipher, counter)?,
        })
    }
}

impl SessionFrameDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Send => "send",
            Self::Receive => "receive",
        }
    }
}

#[derive(Clone)]
pub struct SessionEphemeralKeyPair {
    secret: [u8; SESSION_SHARED_SECRET_LEN],
    pub public_key: String,
}

impl std::fmt::Debug for SessionEphemeralKeyPair {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionEphemeralKeyPair")
            .field("public_key", &self.public_key)
            .finish_non_exhaustive()
    }
}

impl SessionEphemeralKeyPair {
    pub fn generate() -> Result<Self, ProtocolError> {
        let mut secret = [0_u8; SESSION_SHARED_SECRET_LEN];
        getrandom::fill(&mut secret).map_err(|_| {
            ProtocolError::new(ErrorCode::InternalError, "failed to generate session key")
        })?;
        Self::from_secret(secret)
    }

    pub fn from_secret(secret: [u8; SESSION_SHARED_SECRET_LEN]) -> Result<Self, ProtocolError> {
        if secret == [0_u8; SESSION_SHARED_SECRET_LEN] {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "session secret cannot be all zero",
            ));
        }
        let public_key = session_public_key_from_secret(secret);
        Ok(Self { secret, public_key })
    }

    pub fn shared_secret_from_peer_public_key(
        &self,
        peer_public_key: &str,
    ) -> Result<[u8; SESSION_SHARED_SECRET_LEN], ProtocolError> {
        let peer_public_key = decode_session_public_key(peer_public_key)?;
        let secret = X25519StaticSecret::from(self.secret);
        let shared_secret = secret.diffie_hellman(&X25519PublicKey::from(peer_public_key));
        let shared_secret = shared_secret.to_bytes();
        if shared_secret == [0_u8; SESSION_SHARED_SECRET_LEN] {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "x25519 shared secret cannot be all zero",
            ));
        }
        Ok(shared_secret)
    }
}

impl SessionKeyDerivationContext {
    pub fn derive_key_material(
        &self,
        shared_secret: &[u8; SESSION_SHARED_SECRET_LEN],
    ) -> Result<SessionKeyMaterial, ProtocolError> {
        let salt = session_hash_bytes("salt", &self.salt)?;
        let hkdf = Hkdf::<Sha256>::new(Some(&salt), shared_secret);
        let mut send_key = [0_u8; SESSION_TRAFFIC_KEY_LEN];
        let mut receive_key = [0_u8; SESSION_TRAFFIC_KEY_LEN];
        hkdf.expand(self.send_info.as_bytes(), &mut send_key)
            .map_err(|_| session_key_derivation_error("send"))?;
        hkdf.expand(self.receive_info.as_bytes(), &mut receive_key)
            .map_err(|_| session_key_derivation_error("receive"))?;
        Ok(SessionKeyMaterial {
            send_key,
            receive_key,
        })
    }
}

impl VerifiedSessionHandshake {
    pub fn from_ready(
        hello: &SessionHelloPayload,
        ready: &SessionReadyPayload,
    ) -> Result<Self, ProtocolError> {
        ready.verify_for_hello(hello)?;
        Ok(Self {
            session_id: hello.session_id.clone(),
            key_agreement: hello.key_agreement.clone(),
            cipher: ready.cipher.clone(),
            handshake_hash: ready.handshake_hash.clone(),
            initiator_ephemeral_public_key: hello.ephemeral_public_key.clone(),
            responder_ephemeral_public_key: ready.ephemeral_public_key.clone(),
            initiator: hello.identity.clone(),
            responder: ready.identity.clone(),
        })
    }

    pub fn key_derivation_context(&self) -> SessionKeyDerivationContext {
        self.build_key_derivation_context(&self.initiator.device_id, &self.responder.device_id)
    }

    pub fn key_derivation_context_for_local_device(
        &self,
        local_device_id: &str,
    ) -> Result<SessionKeyDerivationContext, ProtocolError> {
        if local_device_id == self.initiator.device_id {
            return Ok(self.build_key_derivation_context(
                &self.initiator.device_id,
                &self.responder.device_id,
            ));
        }
        if local_device_id == self.responder.device_id {
            return Ok(self.build_key_derivation_context(
                &self.responder.device_id,
                &self.initiator.device_id,
            ));
        }

        Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            format!("device {local_device_id} is not part of verified session"),
        ))
    }

    fn build_key_derivation_context(
        &self,
        local_device_id: &str,
        peer_device_id: &str,
    ) -> SessionKeyDerivationContext {
        SessionKeyDerivationContext {
            session_id: self.session_id.clone(),
            key_agreement: self.key_agreement.clone(),
            cipher: self.cipher.clone(),
            handshake_hash: self.handshake_hash.clone(),
            salt: self.handshake_hash.clone(),
            send_info: self.key_derivation_info(local_device_id, peer_device_id),
            receive_info: self.key_derivation_info(peer_device_id, local_device_id),
        }
    }

    fn key_derivation_info(&self, from_device_id: &str, to_device_id: &str) -> String {
        format!(
            "{}/{}/{}/{}/{}->{}",
            PROTOCOL_NAME,
            self.session_id,
            self.key_agreement,
            self.cipher,
            from_device_id,
            to_device_id
        )
    }
}

impl SessionIdentityBinding {
    pub fn for_initiator(handshake: &VerifiedSessionHandshake) -> Result<Self, ProtocolError> {
        Self::new(
            SessionParticipantRole::Initiator,
            &handshake.session_id,
            &handshake.initiator,
            &handshake.initiator_ephemeral_public_key,
            &handshake.handshake_hash,
        )
    }

    pub fn for_responder(handshake: &VerifiedSessionHandshake) -> Result<Self, ProtocolError> {
        Self::new(
            SessionParticipantRole::Responder,
            &handshake.session_id,
            &handshake.responder,
            &handshake.responder_ephemeral_public_key,
            &handshake.handshake_hash,
        )
    }

    pub fn new(
        role: SessionParticipantRole,
        session_id: impl Into<String>,
        identity: &DeviceIdentity,
        session_ephemeral_public_key: impl Into<String>,
        handshake_hash: impl Into<String>,
    ) -> Result<Self, ProtocolError> {
        identity.validate()?;
        identity.require_capability(Capability::EncryptedSession)?;
        let binding = Self {
            role,
            session_id: session_id.into(),
            device_id: identity.device_id.clone(),
            public_key_fingerprint: identity.public_key_fingerprint.clone(),
            session_ephemeral_public_key: session_ephemeral_public_key.into(),
            handshake_hash: handshake_hash.into(),
        };
        binding.validate()?;
        Ok(binding)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_session_id(&self.session_id)?;
        validate_non_empty("device_id", &self.device_id)?;
        validate_non_empty("public_key_fingerprint", &self.public_key_fingerprint)?;
        validate_session_crypto_label(
            "session_ephemeral_public_key",
            &self.session_ephemeral_public_key,
        )?;
        validate_sha256_digest_label("handshake_hash", &self.handshake_hash)?;
        Ok(())
    }

    pub fn verify_identity(&self, identity: &DeviceIdentity) -> Result<(), ProtocolError> {
        self.validate()?;
        identity.validate()?;
        if self.device_id != identity.device_id
            || self.public_key_fingerprint != identity.public_key_fingerprint
        {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "session identity binding does not match device identity",
            ));
        }
        Ok(())
    }

    pub fn canonical_payload_hash(&self) -> Result<String, ProtocolError> {
        self.validate()?;
        let mut hasher = Sha256::new();
        hash_field(&mut hasher, "protocol", PROTOCOL_NAME);
        hash_field(&mut hasher, "version", &PROTOCOL_VERSION.to_string());
        hash_field(&mut hasher, "kind", "session.identity_binding");
        hash_field(&mut hasher, "role", self.role.as_str());
        hash_field(&mut hasher, "session_id", &self.session_id);
        hash_field(&mut hasher, "device_id", &self.device_id);
        hash_field(
            &mut hasher,
            "public_key_fingerprint",
            &self.public_key_fingerprint,
        );
        hash_field(
            &mut hasher,
            "session_ephemeral_public_key",
            &self.session_ephemeral_public_key,
        );
        hash_field(&mut hasher, "handshake_hash", &self.handshake_hash);
        Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
    }
}

impl DeviceIdentitySigningKey {
    pub fn generate() -> Result<Self, ProtocolError> {
        let mut seed = [0_u8; DEVICE_IDENTITY_SIGNING_KEY_LEN];
        getrandom::fill(&mut seed).map_err(|error| {
            ProtocolError::new(
                ErrorCode::InvalidPayload,
                format!("failed to generate device identity signing key: {error}"),
            )
        })?;
        Ok(Self::from_seed(seed))
    }

    pub fn from_seed(seed: [u8; DEVICE_IDENTITY_SIGNING_KEY_LEN]) -> Self {
        Self {
            signing_key: SigningKey::from_bytes(&seed),
        }
    }

    pub fn seed_bytes(&self) -> [u8; DEVICE_IDENTITY_SIGNING_KEY_LEN] {
        self.signing_key.to_bytes()
    }

    pub fn public_key(&self) -> DeviceIdentityPublicKey {
        let public_key = self.signing_key.verifying_key();
        DeviceIdentityPublicKey::from_verifying_key(public_key)
    }

    pub fn public_key_fingerprint(&self) -> String {
        self.public_key().fingerprint
    }

    fn sign_binding(&self, binding: &SessionIdentityBinding) -> Result<String, ProtocolError> {
        let payload_hash = binding.canonical_payload_hash()?;
        let signature = self.signing_key.sign(payload_hash.as_bytes());
        Ok(URL_SAFE_NO_PAD.encode(signature.to_bytes()))
    }
}

impl std::fmt::Debug for DeviceIdentitySigningKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DeviceIdentitySigningKey")
            .field("public_key", &self.public_key().public_key)
            .field("fingerprint", &self.public_key_fingerprint())
            .finish_non_exhaustive()
    }
}

impl DeviceIdentityPublicKey {
    pub fn from_encoded(public_key: impl Into<String>) -> Result<Self, ProtocolError> {
        let public_key = public_key.into();
        let verifying_key = decode_device_identity_public_key(&public_key)?;
        Ok(Self::from_verifying_key(verifying_key))
    }

    fn from_verifying_key(verifying_key: VerifyingKey) -> Self {
        let bytes = verifying_key.to_bytes();
        Self {
            algorithm: DEVICE_IDENTITY_SIGNATURE_ED25519.to_string(),
            public_key: URL_SAFE_NO_PAD.encode(bytes),
            fingerprint: device_identity_public_key_fingerprint(&bytes),
        }
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_device_identity_signature_algorithm(&self.algorithm)?;
        let verifying_key = decode_device_identity_public_key(&self.public_key)?;
        let expected = device_identity_public_key_fingerprint(&verifying_key.to_bytes());
        if self.fingerprint != expected {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "device identity public key fingerprint mismatch",
            ));
        }
        Ok(())
    }
}

impl SignedSessionIdentityBinding {
    pub fn sign(
        binding: SessionIdentityBinding,
        signing_key: &DeviceIdentitySigningKey,
    ) -> Result<Self, ProtocolError> {
        binding.validate()?;
        let public_key = signing_key.public_key();
        Ok(Self {
            binding: binding.clone(),
            algorithm: DEVICE_IDENTITY_SIGNATURE_ED25519.to_string(),
            public_key: public_key.public_key,
            public_key_fingerprint: public_key.fingerprint,
            signature: signing_key.sign_binding(&binding)?,
        })
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.binding.validate()?;
        validate_device_identity_signature_algorithm(&self.algorithm)?;
        let public_key = DeviceIdentityPublicKey {
            algorithm: self.algorithm.clone(),
            public_key: self.public_key.clone(),
            fingerprint: self.public_key_fingerprint.clone(),
        };
        public_key.validate()?;
        decode_device_identity_signature(&self.signature)?;
        Ok(())
    }

    pub fn verify(&self, expected_binding: &SessionIdentityBinding) -> Result<(), ProtocolError> {
        let public_key = DeviceIdentityPublicKey {
            algorithm: self.algorithm.clone(),
            public_key: self.public_key.clone(),
            fingerprint: self.public_key_fingerprint.clone(),
        };
        self.verify_with_public_key(expected_binding, &public_key)
    }

    pub fn verify_with_public_key(
        &self,
        expected_binding: &SessionIdentityBinding,
        public_key: &DeviceIdentityPublicKey,
    ) -> Result<(), ProtocolError> {
        self.validate()?;
        expected_binding.validate()?;
        public_key.validate()?;
        if self.public_key != public_key.public_key
            || self.public_key_fingerprint != public_key.fingerprint
        {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "session identity signature public key mismatch",
            ));
        }
        if &self.binding != expected_binding {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "session identity signature binding mismatch",
            ));
        }

        let verifying_key = decode_device_identity_public_key(&self.public_key)?;
        let signature = decode_device_identity_signature(&self.signature)?;
        let payload_hash = expected_binding.canonical_payload_hash()?;
        verifying_key
            .verify(payload_hash.as_bytes(), &signature)
            .map_err(|_| {
                ProtocolError::new(
                    ErrorCode::InvalidPayload,
                    "session identity signature verification failed",
                )
            })
    }
}

impl SessionParticipantRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Initiator => "initiator",
            Self::Responder => "responder",
        }
    }
}

impl SessionReadyPayload {
    pub fn new(
        session_id: impl Into<String>,
        identity: DeviceIdentity,
        key_agreement: impl Into<String>,
        ephemeral_public_key: impl Into<String>,
        cipher: impl Into<String>,
        handshake_hash: impl Into<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            identity,
            key_agreement: key_agreement.into(),
            ephemeral_public_key: ephemeral_public_key.into(),
            cipher: cipher.into(),
            handshake_hash: handshake_hash.into(),
        }
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_session_id(&self.session_id)?;
        self.identity.validate()?;
        self.identity
            .require_capability(Capability::EncryptedSession)?;
        validate_session_key_agreement(&self.key_agreement)?;
        validate_session_crypto_label("ephemeral_public_key", &self.ephemeral_public_key)?;
        validate_session_cipher("cipher", &self.cipher)?;
        validate_session_crypto_label("handshake_hash", &self.handshake_hash)?;
        Ok(())
    }

    pub fn for_hello(
        hello: &SessionHelloPayload,
        identity: DeviceIdentity,
        ephemeral_public_key: impl Into<String>,
        cipher: impl Into<String>,
    ) -> Result<Self, ProtocolError> {
        let mut ready = Self::new(
            hello.session_id.clone(),
            identity,
            hello.key_agreement.clone(),
            ephemeral_public_key,
            cipher,
            "sha256:pending",
        );
        ready.handshake_hash = session_handshake_hash(hello, &ready)?;
        Ok(ready)
    }

    pub fn for_hello_with_cipher_preference(
        hello: &SessionHelloPayload,
        identity: DeviceIdentity,
        ephemeral_public_key: impl Into<String>,
        local_cipher_preference: &[String],
    ) -> Result<Self, ProtocolError> {
        let cipher = select_session_cipher(local_cipher_preference, &hello.supported_ciphers)?;
        Self::for_hello(hello, identity, ephemeral_public_key, cipher)
    }

    pub fn verify_for_hello(&self, hello: &SessionHelloPayload) -> Result<(), ProtocolError> {
        validate_sha256_digest_label("handshake_hash", &self.handshake_hash)?;
        let expected_hash = session_handshake_hash(hello, self)?;
        if self.handshake_hash != expected_hash {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "session ready handshake_hash mismatch",
            ));
        }
        Ok(())
    }
}

pub fn default_session_cipher_preference() -> Vec<String> {
    vec![
        SESSION_CIPHER_XCHACHA20POLY1305.to_string(),
        SESSION_CIPHER_AES256GCM.to_string(),
    ]
}

pub fn is_supported_session_key_agreement(value: &str) -> bool {
    value == SESSION_KEY_AGREEMENT_X25519
}

pub fn is_supported_session_cipher(value: &str) -> bool {
    matches!(
        value,
        SESSION_CIPHER_XCHACHA20POLY1305 | SESSION_CIPHER_AES256GCM
    )
}

pub fn shared_capabilities(left: &[Capability], right: &[Capability]) -> Vec<Capability> {
    left.iter()
        .copied()
        .filter(|capability| right.contains(capability))
        .collect()
}

pub fn select_session_cipher(
    local_preference: &[String],
    peer_supported: &[String],
) -> Result<String, ProtocolError> {
    if local_preference.is_empty() {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "local session cipher list cannot be empty",
        ));
    }
    if peer_supported.is_empty() {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "peer session cipher list cannot be empty",
        ));
    }
    for cipher in local_preference {
        validate_session_cipher("local session cipher", cipher)?;
    }
    for cipher in peer_supported {
        validate_session_cipher("peer session cipher", cipher)?;
    }

    local_preference
        .iter()
        .find(|cipher| peer_supported.contains(*cipher))
        .cloned()
        .ok_or_else(|| {
            ProtocolError::new(
                ErrorCode::InvalidPayload,
                "no mutually supported session cipher",
            )
        })
}

pub fn session_handshake_hash(
    hello: &SessionHelloPayload,
    ready: &SessionReadyPayload,
) -> Result<String, ProtocolError> {
    hello.validate()?;
    ready.validate()?;
    if hello.session_id != ready.session_id {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "session handshake hash requires matching session_id",
        ));
    }
    if hello.key_agreement != ready.key_agreement {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "session handshake hash requires matching key_agreement",
        ));
    }
    if !hello.supported_ciphers.contains(&ready.cipher) {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "session ready cipher must be offered by session hello",
        ));
    }

    let mut hasher = Sha256::new();
    hash_field(&mut hasher, "protocol", PROTOCOL_NAME);
    hash_field(&mut hasher, "version", &PROTOCOL_VERSION.to_string());
    hash_field(
        &mut hasher,
        "kind.hello",
        MessageKind::SessionHello.as_str(),
    );
    hash_field(
        &mut hasher,
        "kind.ready",
        MessageKind::SessionReady.as_str(),
    );
    hash_field(&mut hasher, "session_id", &hello.session_id);
    hash_identity(&mut hasher, "hello.identity", &hello.identity);
    hash_identity(&mut hasher, "ready.identity", &ready.identity);
    hash_field(&mut hasher, "key_agreement", &hello.key_agreement);
    hash_field(
        &mut hasher,
        "hello.ephemeral_public_key",
        &hello.ephemeral_public_key,
    );
    hash_field(
        &mut hasher,
        "ready.ephemeral_public_key",
        &ready.ephemeral_public_key,
    );
    for cipher in &hello.supported_ciphers {
        hash_field(&mut hasher, "hello.supported_cipher", cipher);
    }
    hash_field(&mut hasher, "ready.cipher", &ready.cipher);

    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleType {
    Skill,
    Session,
    Workspace,
    AgentProfile,
    ConfigSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleSender {
    pub device_id: String,
    pub device_name: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleCompatibility {
    pub min_nekolink_version: u16,
    pub required_capabilities: Vec<Capability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleSummary {
    pub file_count: usize,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleManifest {
    pub schema: String,
    pub bundle_id: String,
    pub bundle_type: BundleType,
    pub display_name: String,
    pub source_app: String,
    pub created_at: String,
    pub sender: BundleSender,
    pub compatibility: BundleCompatibility,
    pub summary: BundleSummary,
    pub files: Vec<BundleFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleChecksum {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleChecksums {
    pub algorithm: String,
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BundlePermissionScope {
    #[serde(rename = "skill.install")]
    SkillInstall,
    #[serde(rename = "session.import")]
    SessionImport,
    #[serde(rename = "workspace.import")]
    WorkspaceImport,
    #[serde(rename = "agent_profile.import")]
    AgentProfileImport,
    #[serde(rename = "config.import")]
    ConfigImport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleWriteMode {
    CreateOnly,
    ManualImport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleWritePermission {
    pub target: String,
    pub mode: BundleWriteMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleSecretsPolicy {
    pub contains_secrets: bool,
    pub redacted_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundlePermissions {
    pub requested_scopes: Vec<BundlePermissionScope>,
    pub writes: Vec<BundleWritePermission>,
    pub secrets: BundleSecretsPolicy,
}

impl BundleManifest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema != BUNDLE_SCHEMA_V1 {
            return Err(ProtocolError::new(
                ErrorCode::UnsupportedVersion,
                format!("unsupported bundle schema: {}", self.schema),
            ));
        }
        validate_non_empty("bundle_id", &self.bundle_id)?;
        validate_non_empty("display_name", &self.display_name)?;
        validate_non_empty("source_app", &self.source_app)?;
        validate_non_empty("created_at", &self.created_at)?;
        self.sender.validate()?;
        self.compatibility.validate()?;
        if self.summary.file_count != self.files.len() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                format!(
                    "bundle file count mismatch: {} != {}",
                    self.summary.file_count,
                    self.files.len()
                ),
            ));
        }
        let total_bytes = self.files.iter().map(|file| file.size).sum::<u64>();
        if self.summary.total_bytes != total_bytes {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                format!(
                    "bundle total bytes mismatch: {} != {}",
                    self.summary.total_bytes, total_bytes
                ),
            ));
        }
        for file in &self.files {
            file.validate()?;
        }
        Ok(())
    }
}

impl BundleSender {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_non_empty("sender device_id", &self.device_id)?;
        validate_non_empty("sender device_name", &self.device_name)?;
        validate_non_empty("sender fingerprint", &self.fingerprint)
    }
}

impl BundleCompatibility {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.min_nekolink_version == 0 {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "min_nekolink_version cannot be 0",
            ));
        }
        if self.required_capabilities.is_empty() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "required_capabilities cannot be empty",
            ));
        }
        Ok(())
    }
}

impl BundleFile {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_bundle_path(&self.path)?;
        validate_sha256_hex("bundle file sha256", &self.sha256)?;
        validate_non_empty("bundle file role", &self.role)
    }
}

impl BundleChecksums {
    pub fn validate_against(&self, manifest: &BundleManifest) -> Result<(), ProtocolError> {
        if self.algorithm != BUNDLE_CHECKSUM_SHA256 {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                format!("unsupported bundle checksum algorithm: {}", self.algorithm),
            ));
        }
        if self.files.len() != manifest.files.len() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "bundle checksum file count mismatch",
            ));
        }
        for (path, sha256) in &self.files {
            validate_bundle_path(path)?;
            validate_sha256_hex("bundle checksum sha256", sha256)?;
        }
        for manifest_file in &manifest.files {
            let checksum = self.files.get(&manifest_file.path).ok_or_else(|| {
                ProtocolError::new(
                    ErrorCode::InvalidPayload,
                    format!("missing checksum for {}", manifest_file.path),
                )
            })?;
            if checksum != &manifest_file.sha256 {
                return Err(ProtocolError::new(
                    ErrorCode::InvalidPayload,
                    format!("checksum mismatch for {}", manifest_file.path),
                ));
            }
        }
        Ok(())
    }
}

impl BundlePermissions {
    pub fn can_import(&self) -> Result<bool, ProtocolError> {
        self.validate()?;
        Ok(!self.secrets.contains_secrets)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.requested_scopes.is_empty() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "requested_scopes cannot be empty",
            ));
        }
        for write in &self.writes {
            write.validate()?;
        }
        for field in &self.secrets.redacted_fields {
            validate_non_empty("redacted field", field)?;
        }
        Ok(())
    }
}

impl BundleWritePermission {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_logical_target(&self.target)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload")]
pub enum LocalBridgeRequest {
    #[serde(rename = "devices.list")]
    ListDevices(LocalBridgeListDevicesRequest),
    #[serde(rename = "bundle.send")]
    SendBundle(LocalBridgeSendBundleRequest),
    #[serde(rename = "bundle.detail")]
    BundleDetail(LocalBridgeBundleDetailRequest),
    #[serde(rename = "bundle.import")]
    ImportBundle(LocalBridgeImportBundleRequest),
    #[serde(rename = "bundle.rollback")]
    RollbackBundleImport(LocalBridgeRollbackBundleImportRequest),
    #[serde(rename = "authorization.request")]
    AuthorizationRequest(LocalBridgeAuthorizationRequest),
    #[serde(rename = "transfer.status")]
    TransferStatus(LocalBridgeTransferStatusRequest),
    #[serde(rename = "events.poll")]
    PollEvents(LocalBridgePollEventsRequest),
    #[serde(rename = "actions.results")]
    ActionResults(LocalBridgeActionResultsRequest),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalBridgeClientIdentity {
    pub client_id: String,
    pub display_name: String,
    pub app_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalBridgeListDevicesRequest {
    pub request_id: String,
    pub client: Option<LocalBridgeClientIdentity>,
    pub trusted_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalBridgeSendBundleRequest {
    pub request_id: String,
    pub client: Option<LocalBridgeClientIdentity>,
    pub target_device_id: Option<String>,
    pub bundle_root: String,
    pub bundle_type: BundleType,
    pub require_trusted_device: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalBridgeBundleDetailRequest {
    pub request_id: String,
    pub client: Option<LocalBridgeClientIdentity>,
    pub staged_bundle_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalBridgeImportBundleRequest {
    pub request_id: String,
    pub client: Option<LocalBridgeClientIdentity>,
    pub staged_bundle_id: String,
    pub expected_bundle_type: Option<BundleType>,
    #[serde(default)]
    pub conflict_strategy: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalBridgeRollbackBundleImportRequest {
    pub request_id: String,
    pub client: Option<LocalBridgeClientIdentity>,
    pub bundle_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalBridgeTransferStatusRequest {
    pub request_id: String,
    pub client: Option<LocalBridgeClientIdentity>,
    pub transfer_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalBridgePollEventsRequest {
    pub request_id: String,
    #[serde(default)]
    pub client: Option<LocalBridgeClientIdentity>,
    #[serde(default)]
    pub after_event_id: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalBridgeActionResultsRequest {
    pub request_id: String,
    pub client: Option<LocalBridgeClientIdentity>,
    pub after_claimed_at_ms: Option<u128>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalBridgeAuthorizationRequest {
    pub request_id: String,
    pub client: LocalBridgeClientIdentity,
    pub requested_scopes: Vec<LocalBridgePermissionScope>,
    pub reason: String,
    pub ttl_seconds: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocalBridgePermissionScope {
    #[serde(rename = "device.read")]
    DeviceRead,
    #[serde(rename = "transfer.status.read")]
    TransferStatusRead,
    #[serde(rename = "bundle.read")]
    BundleRead,
    #[serde(rename = "bundle.send")]
    BundleSend,
    #[serde(rename = "bundle.import.request")]
    BundleImportRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload")]
pub enum LocalBridgeEvent {
    #[serde(rename = "bundle.received")]
    BundleReceived(LocalBridgeBundleReceivedEvent),
    #[serde(rename = "bundle.send.preflight")]
    BundleSendPreflight(LocalBridgeBundleSendPreflightEvent),
    #[serde(rename = "action.updated")]
    ActionUpdated(LocalBridgeActionUpdatedEvent),
    #[serde(rename = "transfer.updated")]
    TransferUpdated(LocalBridgeTransferUpdatedEvent),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalBridgeBundleReceivedEvent {
    pub event_id: String,
    pub transfer_id: String,
    pub bundle_id: String,
    pub bundle_type: BundleType,
    pub display_name: String,
    pub source_app: String,
    pub file_count: usize,
    pub total_bytes: u64,
    pub import_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalBridgeBundleSendPreflightEvent {
    pub event_id: String,
    pub request_id: String,
    pub client_id: String,
    pub status: LocalBridgeBundleSendPreflightStatus,
    pub reason: Option<String>,
    pub bundle_id: Option<String>,
    pub bundle_type: Option<BundleType>,
    pub target_device_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalBridgeBundleSendPreflightStatus {
    Ready,
    FailedPreflight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalBridgeActionLifecycleStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Conflict,
    Cancelled,
}

impl LocalBridgeActionLifecycleStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Conflict => "conflict",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalBridgeActionUpdatedEvent {
    pub event_id: String,
    pub request_id: String,
    pub action_kind: String,
    pub client_id: String,
    pub status: LocalBridgeActionLifecycleStatus,
    pub reason: Option<String>,
    pub message: String,
    pub bundle_id: Option<String>,
    pub bundle_type: Option<BundleType>,
    pub target_device_id: Option<String>,
    pub updated_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalBridgeTransferUpdatedEvent {
    pub event_id: String,
    pub transfer_id: String,
    pub phase: LocalBridgeTransferPhase,
    pub bytes_transferred: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalBridgeTransferPhase {
    Queued,
    Sending,
    Receiving,
    Completed,
    Failed,
    Cancelled,
}

impl LocalBridgeRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Self::ListDevices(request) => request.validate(),
            Self::SendBundle(request) => request.validate(),
            Self::BundleDetail(request) => request.validate(),
            Self::ImportBundle(request) => request.validate(),
            Self::RollbackBundleImport(request) => request.validate(),
            Self::AuthorizationRequest(request) => request.validate(),
            Self::TransferStatus(request) => request.validate(),
            Self::PollEvents(request) => request.validate(),
            Self::ActionResults(request) => request.validate(),
        }
    }
}

impl LocalBridgeListDevicesRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_non_empty("request_id", &self.request_id)?;
        validate_optional_bridge_client(self.client.as_ref())
    }
}

impl LocalBridgeSendBundleRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_non_empty("request_id", &self.request_id)?;
        validate_optional_bridge_client(self.client.as_ref())?;
        validate_optional_non_empty("target_device_id", self.target_device_id.as_deref())?;
        validate_bridge_bundle_root(&self.bundle_root)
    }
}

impl LocalBridgeBundleDetailRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_non_empty("request_id", &self.request_id)?;
        validate_optional_bridge_client(self.client.as_ref())?;
        validate_staged_bundle_id(&self.staged_bundle_id)
    }
}

impl LocalBridgeImportBundleRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_non_empty("request_id", &self.request_id)?;
        validate_optional_bridge_client(self.client.as_ref())?;
        validate_staged_bundle_id(&self.staged_bundle_id)?;
        if let Some(strategy) = self.conflict_strategy.as_deref() {
            if !matches!(strategy, "reject" | "rename" | "skip_conflicts") {
                return Err(ProtocolError::new(
                    ErrorCode::InvalidPayload,
                    "bundle import conflict_strategy must be reject, rename, or skip_conflicts",
                ));
            }
        }
        Ok(())
    }
}

impl LocalBridgeRollbackBundleImportRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_non_empty("request_id", &self.request_id)?;
        validate_optional_bridge_client(self.client.as_ref())?;
        validate_staged_bundle_id(&self.bundle_id)
    }
}

impl LocalBridgeTransferStatusRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_non_empty("request_id", &self.request_id)?;
        validate_optional_bridge_client(self.client.as_ref())?;
        validate_optional_non_empty("transfer_id", self.transfer_id.as_deref())
    }
}

impl LocalBridgePollEventsRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_non_empty("request_id", &self.request_id)?;
        validate_optional_bridge_client(self.client.as_ref())?;
        validate_optional_non_empty("after_event_id", self.after_event_id.as_deref())?;
        if let Some(limit) = self.limit {
            if limit == 0 || limit > 100 {
                return Err(ProtocolError::new(
                    ErrorCode::InvalidPayload,
                    "event poll limit must be between 1 and 100",
                ));
            }
        }
        if let Some(timeout_ms) = self.timeout_ms {
            if timeout_ms > 30_000 {
                return Err(ProtocolError::new(
                    ErrorCode::InvalidPayload,
                    "event poll timeout_ms must be between 0 and 30000",
                ));
            }
        }
        Ok(())
    }
}

impl LocalBridgeActionResultsRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_non_empty("request_id", &self.request_id)?;
        validate_optional_bridge_client(self.client.as_ref())?;
        if let Some(limit) = self.limit {
            if limit == 0 || limit > 100 {
                return Err(ProtocolError::new(
                    ErrorCode::InvalidPayload,
                    "action results limit must be between 1 and 100",
                ));
            }
        }
        Ok(())
    }
}

impl LocalBridgeAuthorizationRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_non_empty("request_id", &self.request_id)?;
        self.client.validate()?;
        if self.requested_scopes.is_empty() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "requested_scopes cannot be empty",
            ));
        }
        validate_non_empty("reason", &self.reason)?;
        if let Some(ttl_seconds) = self.ttl_seconds {
            if ttl_seconds == 0 || ttl_seconds > 604_800 {
                return Err(ProtocolError::new(
                    ErrorCode::InvalidPayload,
                    "ttl_seconds must be between 1 and 604800",
                ));
            }
        }
        Ok(())
    }
}

impl LocalBridgeClientIdentity {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_bridge_client_id(&self.client_id)?;
        validate_non_empty("client display_name", &self.display_name)?;
        validate_optional_non_empty("client app_kind", self.app_kind.as_deref())
    }
}

impl LocalBridgeEvent {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Self::BundleReceived(event) => event.validate(),
            Self::BundleSendPreflight(event) => event.validate(),
            Self::ActionUpdated(event) => event.validate(),
            Self::TransferUpdated(event) => event.validate(),
        }
    }
}

impl LocalBridgeBundleReceivedEvent {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_non_empty("event_id", &self.event_id)?;
        validate_non_empty("transfer_id", &self.transfer_id)?;
        validate_staged_bundle_id(&self.bundle_id)?;
        validate_non_empty("display_name", &self.display_name)?;
        validate_non_empty("source_app", &self.source_app)?;
        if self.file_count == 0 {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "bundle received file_count must be greater than 0",
            ));
        }
        Ok(())
    }
}

impl LocalBridgeBundleSendPreflightEvent {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_non_empty("event_id", &self.event_id)?;
        validate_non_empty("request_id", &self.request_id)?;
        validate_bridge_client_id(&self.client_id)?;
        validate_optional_non_empty("reason", self.reason.as_deref())?;
        if let Some(bundle_id) = self.bundle_id.as_deref() {
            validate_staged_bundle_id(bundle_id)?;
        }
        validate_optional_non_empty("target_device_id", self.target_device_id.as_deref())
    }
}

impl LocalBridgeActionUpdatedEvent {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_non_empty("event_id", &self.event_id)?;
        validate_non_empty("request_id", &self.request_id)?;
        validate_non_empty("action_kind", &self.action_kind)?;
        validate_bridge_client_id(&self.client_id)?;
        validate_optional_non_empty("reason", self.reason.as_deref())?;
        validate_non_empty("message", &self.message)?;
        if let Some(bundle_id) = self.bundle_id.as_deref() {
            validate_staged_bundle_id(bundle_id)?;
        }
        validate_optional_non_empty("target_device_id", self.target_device_id.as_deref())
    }
}

impl LocalBridgeTransferUpdatedEvent {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_non_empty("event_id", &self.event_id)?;
        validate_non_empty("transfer_id", &self.transfer_id)?;
        if self.bytes_transferred > self.total_bytes {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "bytes_transferred cannot exceed total_bytes",
            ));
        }
        Ok(())
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_device_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_device_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_public_key_fingerprint: Option<String>,
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
            sender_device_id: None,
            sender_device_name: None,
            sender_public_key_fingerprint: None,
            files,
        }
    }

    pub fn with_sender_identity(mut self, identity: &DeviceIdentity) -> Self {
        self.sender_device_id = Some(identity.device_id.clone());
        self.sender_device_name = Some(identity.device_name.clone());
        self.sender_public_key_fingerprint = Some(identity.public_key_fingerprint.clone());
        self
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.transfer_id.trim().is_empty() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "transfer_id cannot be empty",
            ));
        }
        if self.root_name.trim().is_empty() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "root_name cannot be empty",
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
        if self
            .sender_device_id
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "sender_device_id cannot be empty",
            ));
        }
        if self
            .sender_device_name
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "sender_device_name cannot be empty",
            ));
        }
        if self
            .sender_public_key_fingerprint
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "sender_public_key_fingerprint cannot be empty",
            ));
        }
        if self.sender_device_id.is_some() != self.sender_public_key_fingerprint.is_some() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "sender identity requires both device_id and public_key_fingerprint",
            ));
        }
        for file in &self.files {
            validate_transfer_manifest_path(&file.manifest_path)?;
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

fn validate_transfer_manifest_path(path: &str) -> Result<(), ProtocolError> {
    let original = path;
    let trimmed = original.trim();
    if trimmed.is_empty() {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "file manifest_path cannot be empty",
        ));
    }
    if original != trimmed {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "file manifest_path contains leading or trailing whitespace",
        ));
    }
    let path = trimmed;
    if path.starts_with('/') || path.starts_with('\\') || path.contains('\\') {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "file manifest_path must be a relative slash-separated path",
        ));
    }
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "file manifest_path contains an unsafe path segment",
            ));
        }
        if is_windows_unsafe_path_segment(segment) {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "file manifest_path contains a Windows-unsafe path segment",
            ));
        }
    }
    Ok(())
}

fn validate_non_empty(field: &str, value: &str) -> Result<(), ProtocolError> {
    if value.trim().is_empty() {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            format!("{field} cannot be empty"),
        ));
    }
    Ok(())
}

fn validate_optional_non_empty(field: &str, value: Option<&str>) -> Result<(), ProtocolError> {
    if let Some(value) = value {
        validate_non_empty(field, value)?;
    }
    Ok(())
}

fn validate_sha256_hex(field: &str, value: &str) -> Result<(), ProtocolError> {
    if value.len() != 64 || !value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            format!("{field} must be a 64-character hex SHA-256"),
        ));
    }
    Ok(())
}

fn validate_bundle_path(path: &str) -> Result<(), ProtocolError> {
    validate_transfer_manifest_path(path)?;
    if matches!(path, "bundle.json" | "checksums.json" | "permissions.json") {
        return Ok(());
    }
    if !path.starts_with("files/") {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "bundle payload path must be under files/",
        ));
    }
    Ok(())
}

fn validate_bridge_bundle_root(path: &str) -> Result<(), ProtocolError> {
    validate_non_empty("bundle_root", path)?;
    if path.starts_with('/')
        || path.starts_with('\\')
        || path.contains('\\')
        || path.contains("..")
        || path.contains(':')
        || path.contains('\0')
    {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "bundle_root must be a safe relative bundle path",
        ));
    }
    validate_transfer_manifest_path(path).map_err(|error| {
        ProtocolError::new(
            error.code,
            format!(
                "bundle_root must be a safe relative bundle path: {}",
                error.message
            ),
        )
    })
}

fn validate_optional_bridge_client(
    client: Option<&LocalBridgeClientIdentity>,
) -> Result<(), ProtocolError> {
    if let Some(client) = client {
        client.validate()?;
    }
    Ok(())
}

fn validate_bridge_client_id(client_id: &str) -> Result<(), ProtocolError> {
    let trimmed = client_id.trim();
    if trimmed.is_empty()
        || trimmed != client_id
        || client_id.len() > 80
        || !client_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "client_id must use only ASCII letters, numbers, underscore, hyphen, or dot",
        ));
    }
    Ok(())
}

fn validate_staged_bundle_id(bundle_id: &str) -> Result<(), ProtocolError> {
    let trimmed = bundle_id.trim();
    if trimmed.is_empty()
        || trimmed != bundle_id
        || bundle_id.contains('/')
        || bundle_id.contains('\\')
        || bundle_id.contains("..")
        || bundle_id.contains(':')
        || bundle_id.contains('\0')
    {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "staged_bundle_id must be a safe staging id",
        ));
    }
    Ok(())
}

fn validate_logical_target(target: &str) -> Result<(), ProtocolError> {
    validate_non_empty("write target", target)?;
    if target.starts_with('/')
        || target.starts_with('\\')
        || target.contains('\\')
        || target.contains("..")
        || target.contains(':')
    {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "write target must be a logical target, not a filesystem path",
        ));
    }
    Ok(())
}

fn is_windows_unsafe_path_segment(segment: &str) -> bool {
    segment.ends_with(' ')
        || segment.ends_with('.')
        || segment
            .chars()
            .any(|ch| matches!(ch, '<' | '>' | ':' | '"' | '|' | '?' | '*') || ch.is_control())
        || is_windows_reserved_name(segment)
}

fn is_windows_reserved_name(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or(segment);
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferDecision {
    pub accepted: bool,
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resume_files: Vec<TransferResumeFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferResumeFile {
    pub manifest_path: String,
    pub received_bytes: u64,
}

impl TransferDecision {
    pub fn accept() -> Self {
        Self {
            accepted: true,
            reason: None,
            resume_files: Vec::new(),
        }
    }

    pub fn accept_with_resume(resume_files: Vec<TransferResumeFile>) -> Self {
        Self {
            accepted: true,
            reason: None,
            resume_files,
        }
    }

    pub fn decline(reason: impl Into<String>) -> Self {
        Self {
            accepted: false,
            reason: Some(reason.into()),
            resume_files: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if !self.accepted && !self.resume_files.is_empty() {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "declined transfer decisions cannot include resume files",
            ));
        }
        for file in &self.resume_files {
            validate_transfer_manifest_path(&file.manifest_path)?;
            if file.received_bytes == 0 {
                return Err(ProtocolError::new(
                    ErrorCode::InvalidPayload,
                    "resume file received_bytes must be greater than 0",
                ));
            }
        }
        Ok(())
    }
}

impl TransferResumeFile {
    pub fn new(
        manifest_path: impl Into<String>,
        received_bytes: u64,
    ) -> Result<Self, ProtocolError> {
        let file = Self {
            manifest_path: manifest_path.into(),
            received_bytes,
        };
        validate_transfer_manifest_path(&file.manifest_path)?;
        if file.received_bytes == 0 {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "resume file received_bytes must be greater than 0",
            ));
        }
        Ok(file)
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn validate_session_id(session_id: &str) -> Result<(), ProtocolError> {
    let trimmed = session_id.trim();
    if trimmed.is_empty() {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "session_id cannot be empty",
        ));
    }
    if trimmed.len() > 128 {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "session_id cannot exceed 128 bytes",
        ));
    }
    Ok(())
}

fn validate_session_crypto_label(name: &str, value: &str) -> Result<(), ProtocolError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            format!("{name} cannot be empty"),
        ));
    }
    if trimmed.len() > 512 {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            format!("{name} cannot exceed 512 bytes"),
        ));
    }
    Ok(())
}

fn validate_sha256_digest_label(name: &str, value: &str) -> Result<(), ProtocolError> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            format!("{name} must be sha256:<64 hex chars>"),
        ));
    };
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            format!("{name} must be sha256:<64 hex chars>"),
        ));
    }
    Ok(())
}

fn validate_device_identity_signature_algorithm(value: &str) -> Result<(), ProtocolError> {
    if value != DEVICE_IDENTITY_SIGNATURE_ED25519 {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            format!("unsupported device identity signature algorithm: {value}"),
        ));
    }
    Ok(())
}

fn session_hash_bytes(name: &str, value: &str) -> Result<[u8; 32], ProtocolError> {
    validate_sha256_digest_label(name, value)?;
    let hex = value.strip_prefix("sha256:").unwrap_or_default();
    let mut bytes = [0_u8; 32];
    hex::decode_to_slice(hex, &mut bytes).map_err(|_| {
        ProtocolError::new(
            ErrorCode::InvalidPayload,
            format!("{name} must be sha256:<64 hex chars>"),
        )
    })?;
    Ok(bytes)
}

fn next_session_counter(counter: &mut u64) -> Result<u64, ProtocolError> {
    if *counter == u64::MAX {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "session traffic counter exhausted",
        ));
    }
    let current = *counter;
    *counter += 1;
    Ok(current)
}

fn session_frame_nonce(cipher: &str, counter: u64) -> Result<Vec<u8>, ProtocolError> {
    validate_session_cipher("cipher", cipher)?;
    let nonce_len = match cipher {
        SESSION_CIPHER_XCHACHA20POLY1305 => SESSION_XCHACHA20POLY1305_NONCE_LEN,
        SESSION_CIPHER_AES256GCM => SESSION_AES256GCM_NONCE_LEN,
        _ => unreachable!("validate_session_cipher rejects unsupported ciphers"),
    };
    let mut nonce = vec![0_u8; nonce_len];
    let counter_offset = nonce_len - std::mem::size_of::<u64>();
    nonce[counter_offset..].copy_from_slice(&counter.to_be_bytes());
    Ok(nonce)
}

fn session_control_associated_data(
    session_id: &str,
    message_id: &str,
    inner_kind: MessageKind,
) -> Vec<u8> {
    let mut data = Vec::new();
    append_aad_field(&mut data, "protocol", PROTOCOL_NAME);
    append_aad_field(&mut data, "version", &PROTOCOL_VERSION.to_string());
    append_aad_field(&mut data, "session_id", session_id);
    append_aad_field(&mut data, "message_id", message_id);
    append_aad_field(
        &mut data,
        "kind.outer",
        MessageKind::SessionControl.as_str(),
    );
    append_aad_field(&mut data, "kind.inner", inner_kind.as_str());
    data
}

fn append_aad_field(data: &mut Vec<u8>, name: &str, value: &str) {
    data.extend_from_slice(name.as_bytes());
    data.push(0);
    data.extend_from_slice(value.as_bytes());
    data.push(0xff);
}

fn seal_session_payload(
    cipher: &str,
    key: &[u8; SESSION_TRAFFIC_KEY_LEN],
    nonce: &[u8],
    associated_data: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, ProtocolError> {
    validate_session_cipher("cipher", cipher)?;
    match cipher {
        SESSION_CIPHER_XCHACHA20POLY1305 => {
            let nonce = XNonce::from_slice(validate_session_nonce(
                cipher,
                nonce,
                SESSION_XCHACHA20POLY1305_NONCE_LEN,
            )?);
            XChaCha20Poly1305::new(key.into())
                .encrypt(
                    nonce,
                    Payload {
                        msg: plaintext,
                        aad: associated_data,
                    },
                )
                .map_err(|_| session_payload_seal_error())
        }
        SESSION_CIPHER_AES256GCM => {
            let nonce = AesGcmNonce::from_slice(validate_session_nonce(
                cipher,
                nonce,
                SESSION_AES256GCM_NONCE_LEN,
            )?);
            Aes256Gcm::new(key.into())
                .encrypt(
                    nonce,
                    Payload {
                        msg: plaintext,
                        aad: associated_data,
                    },
                )
                .map_err(|_| session_payload_seal_error())
        }
        _ => unreachable!("validate_session_cipher rejects unsupported ciphers"),
    }
}

fn open_session_payload(
    cipher: &str,
    key: &[u8; SESSION_TRAFFIC_KEY_LEN],
    nonce: &[u8],
    associated_data: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, ProtocolError> {
    validate_session_cipher("cipher", cipher)?;
    match cipher {
        SESSION_CIPHER_XCHACHA20POLY1305 => {
            let nonce = XNonce::from_slice(validate_session_nonce(
                cipher,
                nonce,
                SESSION_XCHACHA20POLY1305_NONCE_LEN,
            )?);
            XChaCha20Poly1305::new(key.into())
                .decrypt(
                    nonce,
                    Payload {
                        msg: ciphertext,
                        aad: associated_data,
                    },
                )
                .map_err(|_| session_payload_open_error())
        }
        SESSION_CIPHER_AES256GCM => {
            let nonce = AesGcmNonce::from_slice(validate_session_nonce(
                cipher,
                nonce,
                SESSION_AES256GCM_NONCE_LEN,
            )?);
            Aes256Gcm::new(key.into())
                .decrypt(
                    nonce,
                    Payload {
                        msg: ciphertext,
                        aad: associated_data,
                    },
                )
                .map_err(|_| session_payload_open_error())
        }
        _ => unreachable!("validate_session_cipher rejects unsupported ciphers"),
    }
}

fn validate_session_nonce<'a>(
    cipher: &str,
    nonce: &'a [u8],
    expected_len: usize,
) -> Result<&'a [u8], ProtocolError> {
    if nonce.len() != expected_len {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            format!("{cipher} nonce must be {expected_len} bytes"),
        ));
    }
    Ok(nonce)
}

fn session_payload_seal_error() -> ProtocolError {
    ProtocolError::new(ErrorCode::InvalidPayload, "failed to seal session payload")
}

fn session_payload_open_error() -> ProtocolError {
    ProtocolError::new(ErrorCode::InvalidPayload, "failed to open session payload")
}

fn session_public_key_from_secret(secret: [u8; SESSION_SHARED_SECRET_LEN]) -> String {
    let secret = X25519StaticSecret::from(secret);
    let public_key = X25519PublicKey::from(&secret);
    URL_SAFE_NO_PAD.encode(public_key.as_bytes())
}

fn decode_session_public_key(
    value: &str,
) -> Result<[u8; SESSION_SHARED_SECRET_LEN], ProtocolError> {
    validate_session_crypto_label("ephemeral_public_key", value)?;
    let decoded = URL_SAFE_NO_PAD.decode(value).map_err(|_| {
        ProtocolError::new(
            ErrorCode::InvalidPayload,
            "ephemeral_public_key must be base64url without padding",
        )
    })?;
    if decoded.len() != SESSION_SHARED_SECRET_LEN {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            format!("ephemeral_public_key must decode to {SESSION_SHARED_SECRET_LEN} bytes"),
        ));
    }
    let mut public_key = [0_u8; SESSION_SHARED_SECRET_LEN];
    public_key.copy_from_slice(&decoded);
    Ok(public_key)
}

fn decode_device_identity_public_key(value: &str) -> Result<VerifyingKey, ProtocolError> {
    validate_session_crypto_label("device identity public_key", value)?;
    let decoded = URL_SAFE_NO_PAD.decode(value).map_err(|_| {
        ProtocolError::new(
            ErrorCode::InvalidPayload,
            "device identity public_key must be base64url without padding",
        )
    })?;
    let bytes: [u8; DEVICE_IDENTITY_PUBLIC_KEY_LEN] = decoded.try_into().map_err(|_| {
        ProtocolError::new(
            ErrorCode::InvalidPayload,
            format!(
                "device identity public_key must decode to {DEVICE_IDENTITY_PUBLIC_KEY_LEN} bytes"
            ),
        )
    })?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| {
        ProtocolError::new(
            ErrorCode::InvalidPayload,
            "device identity public_key is not a valid ed25519 key",
        )
    })
}

fn decode_device_identity_signature(value: &str) -> Result<Signature, ProtocolError> {
    validate_session_crypto_label("device identity signature", value)?;
    let decoded = URL_SAFE_NO_PAD.decode(value).map_err(|_| {
        ProtocolError::new(
            ErrorCode::InvalidPayload,
            "device identity signature must be base64url without padding",
        )
    })?;
    let bytes: [u8; DEVICE_IDENTITY_SIGNATURE_LEN] = decoded.try_into().map_err(|_| {
        ProtocolError::new(
            ErrorCode::InvalidPayload,
            format!(
                "device identity signature must decode to {DEVICE_IDENTITY_SIGNATURE_LEN} bytes"
            ),
        )
    })?;
    Ok(Signature::from_bytes(&bytes))
}

fn device_identity_public_key_fingerprint(
    public_key: &[u8; DEVICE_IDENTITY_PUBLIC_KEY_LEN],
) -> String {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, "kind", "device.identity.ed25519");
    hasher.update(public_key);
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn session_key_derivation_error(direction: &str) -> ProtocolError {
    ProtocolError::new(
        ErrorCode::InvalidPayload,
        format!("failed to derive {direction} session key"),
    )
}

fn validate_session_key_agreement(value: &str) -> Result<(), ProtocolError> {
    validate_session_crypto_label("key_agreement", value)?;
    if !is_supported_session_key_agreement(value) {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            format!("unsupported session key_agreement: {value}"),
        ));
    }
    Ok(())
}

fn validate_session_cipher(name: &str, value: &str) -> Result<(), ProtocolError> {
    validate_session_crypto_label(name, value)?;
    if !is_supported_session_cipher(value) {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            format!("unsupported session cipher: {value}"),
        ));
    }
    Ok(())
}

fn hash_identity(hasher: &mut Sha256, prefix: &str, identity: &DeviceIdentity) {
    hash_field(hasher, &format!("{prefix}.device_id"), &identity.device_id);
    hash_field(
        hasher,
        &format!("{prefix}.device_name"),
        &identity.device_name,
    );
    hash_field(
        hasher,
        &format!("{prefix}.device_kind"),
        identity.device_kind.as_str(),
    );
    hash_field(
        hasher,
        &format!("{prefix}.platform"),
        identity.platform.as_str(),
    );
    hash_field(
        hasher,
        &format!("{prefix}.public_key_fingerprint"),
        &identity.public_key_fingerprint,
    );
    for capability in &identity.capabilities {
        hash_field(hasher, &format!("{prefix}.capability"), capability.as_str());
    }
}

fn hash_field(hasher: &mut Sha256, name: &str, value: &str) {
    hasher.update(name.as_bytes());
    hasher.update([0]);
    hasher.update(value.as_bytes());
    hasher.update([0xff]);
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
    fn transfer_offer_rejects_empty_root_name() {
        let offer = TransferOffer::new("transfer-1", " ", Vec::new());

        let error = offer.validate().unwrap_err();

        assert!(error.message.contains("root_name"));
    }

    #[test]
    fn transfer_offer_rejects_unsafe_manifest_paths() {
        for manifest_path in [
            "/tmp/file.txt",
            "drop/../secret.txt",
            "drop//file.txt",
            r"drop\file.txt",
            "drop/CON.txt",
            "drop/file.txt:stream",
            "drop/trailing.",
            "drop/trailing ",
        ] {
            let offer = TransferOffer::new(
                "transfer-1",
                "drop",
                vec![TransferOfferFile {
                    manifest_path: manifest_path.to_string(),
                    size: 1,
                    sha256: "abc".to_string(),
                }],
            );

            assert!(
                offer.validate().is_err(),
                "expected invalid manifest path: {manifest_path}"
            );
        }
    }

    #[test]
    fn transfer_offer_can_include_sender_identity() {
        let identity = DeviceIdentity::new(
            "neko-device-abc123",
            "Hisakazu Mac",
            DeviceKind::Desktop,
            PlatformKind::Macos,
            "sha256:abc123",
            vec![Capability::FileSend],
        );
        let offer =
            TransferOffer::new("transfer-1", "drop", Vec::new()).with_sender_identity(&identity);

        assert_eq!(
            offer.sender_device_id.as_deref(),
            Some("neko-device-abc123")
        );
        assert_eq!(offer.sender_device_name.as_deref(), Some("Hisakazu Mac"));
        assert_eq!(
            offer.sender_public_key_fingerprint.as_deref(),
            Some("sha256:abc123")
        );
        offer.validate().unwrap();
    }

    #[test]
    fn transfer_decision_can_include_resume_files() {
        let decision = TransferDecision::accept_with_resume(vec![TransferResumeFile::new(
            "drop/sample.txt",
            128,
        )
        .unwrap()]);

        decision.validate().unwrap();
        assert_eq!(decision.resume_files[0].manifest_path, "drop/sample.txt");
        assert_eq!(decision.resume_files[0].received_bytes, 128);
    }

    #[test]
    fn transfer_decision_rejects_invalid_resume_files() {
        assert!(TransferResumeFile::new("drop/../secret.txt", 128).is_err());
        assert!(TransferResumeFile::new("drop/sample.txt", 0).is_err());

        let mut decision = TransferDecision::decline("no");
        decision.resume_files = vec![TransferResumeFile::new("drop/sample.txt", 1).unwrap()];

        let error = decision.validate().unwrap_err();
        assert!(error.message.contains("declined transfer decisions"));
    }

    #[test]
    fn transfer_offer_rejects_partial_sender_identity() {
        let mut offer = TransferOffer::new("transfer-1", "drop", Vec::new());
        offer.sender_device_id = Some("neko-device-abc123".to_string());

        let error = offer.validate().unwrap_err();

        assert!(error
            .message
            .contains("requires both device_id and public_key_fingerprint"));
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
    fn pairing_request_requires_public_key_matching_fingerprint() {
        let signing_key = DeviceIdentitySigningKey::from_seed([17_u8; 32]);
        let public_key = signing_key.public_key();
        let request = PairingRequestPayload {
            request_id: "pairing-1".to_string(),
            device_id: "neko-device-local".to_string(),
            device_name: "Local Mac".to_string(),
            platform: "macos".to_string(),
            public_key: public_key.public_key.clone(),
            public_key_fingerprint: public_key.fingerprint.clone(),
            pairing_code: "ABC-123".to_string(),
            listen_port: 45821,
        };

        request.validate().unwrap();

        let mut tampered = request;
        tampered.public_key_fingerprint = format!("sha256:{}", "0".repeat(64));
        let error = tampered.validate().unwrap_err();

        assert!(error
            .message
            .contains("public_key_fingerprint must match public_key"));
    }

    #[test]
    fn checks_and_intersects_capabilities() {
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
        let hello = DeviceHello::new(identity, "NekoDrop", "0.1.0");

        assert!(hello.supports(Capability::FileTransfer));
        assert!(hello
            .identity
            .require_capability(Capability::DevicePairing)
            .is_ok());
        assert!(hello
            .identity
            .require_capability(Capability::AgentCommand)
            .is_err());
        assert_eq!(
            shared_capabilities(
                &hello.identity.capabilities,
                &[Capability::FileTransfer, Capability::AgentCommand]
            ),
            vec![Capability::FileTransfer]
        );
    }

    #[test]
    fn validates_session_handshake_payloads() {
        let identity = DeviceIdentity::new(
            "neko-device-abc123",
            "Hisakazu Mac",
            DeviceKind::Desktop,
            PlatformKind::Macos,
            "sha256:abc123",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let hello = SessionHelloPayload::new(
            "session-1",
            identity.clone(),
            "x25519",
            "base64-x25519-public-key",
            vec!["xchacha20poly1305".to_string()],
        );
        let ready = SessionReadyPayload::new(
            "session-1",
            identity,
            "x25519",
            "base64-peer-public-key",
            "xchacha20poly1305",
            "sha256:handshake-transcript",
        );

        hello.validate().unwrap();
        ready.validate().unwrap();
        assert!(hello
            .identity
            .require_capability(Capability::EncryptedSession)
            .is_ok());
        assert_eq!(hello.session_id, ready.session_id);
        assert_eq!(ready.cipher, "xchacha20poly1305");
    }

    #[test]
    fn rejects_session_handshake_without_encrypted_session_capability() {
        let identity = DeviceIdentity::new(
            "neko-device-abc123",
            "Hisakazu Mac",
            DeviceKind::Desktop,
            PlatformKind::Macos,
            "sha256:abc123",
            [Capability::FileTransfer],
        );
        let hello = SessionHelloPayload::new(
            "session-1",
            identity,
            "x25519",
            "base64-x25519-public-key",
            vec!["xchacha20poly1305".to_string()],
        );

        let error = hello.validate().unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("encrypted_session"));
    }

    #[test]
    fn builds_session_hello_with_default_crypto_labels() {
        let identity = DeviceIdentity::new(
            "neko-device-abc123",
            "Hisakazu Mac",
            DeviceKind::Desktop,
            PlatformKind::Macos,
            "sha256:abc123",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );

        let hello = SessionHelloPayload::default_crypto("session-1", identity, "base64-local-key");

        hello.validate().unwrap();
        assert_eq!(hello.key_agreement, SESSION_KEY_AGREEMENT_X25519);
        assert_eq!(hello.supported_ciphers, default_session_cipher_preference());
    }

    #[test]
    fn selects_first_mutual_session_cipher_by_local_preference() {
        let selected = select_session_cipher(
            &["xchacha20poly1305".to_string(), "aes256gcm".to_string()],
            &["aes256gcm".to_string(), "xchacha20poly1305".to_string()],
        )
        .unwrap();

        assert_eq!(selected, "xchacha20poly1305");
    }

    #[test]
    fn rejects_session_cipher_negotiation_without_overlap() {
        let error = select_session_cipher(
            &["xchacha20poly1305".to_string()],
            &["aes256gcm".to_string()],
        )
        .unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error
            .message
            .contains("no mutually supported session cipher"));
    }

    #[test]
    fn rejects_session_cipher_negotiation_with_unknown_ciphers() {
        let error =
            select_session_cipher(&["rot13".to_string()], &["rot13".to_string()]).unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("unsupported session cipher"));
    }

    #[test]
    fn rejects_unknown_session_key_agreement() {
        let identity = DeviceIdentity::new(
            "neko-device-abc123",
            "Hisakazu Mac",
            DeviceKind::Desktop,
            PlatformKind::Macos,
            "sha256:abc123",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let hello = SessionHelloPayload::new(
            "session-1",
            identity,
            "p256",
            "base64-local-key",
            default_session_cipher_preference(),
        );

        let error = hello.validate().unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("unsupported session key_agreement"));
    }

    #[test]
    fn rejects_unknown_session_ciphers() {
        let identity = DeviceIdentity::new(
            "neko-device-abc123",
            "Hisakazu Mac",
            DeviceKind::Desktop,
            PlatformKind::Macos,
            "sha256:abc123",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let hello = SessionHelloPayload::new(
            "session-1",
            identity.clone(),
            SESSION_KEY_AGREEMENT_X25519,
            "base64-local-key",
            vec!["rot13".to_string()],
        );
        let ready = SessionReadyPayload::new(
            "session-1",
            identity,
            SESSION_KEY_AGREEMENT_X25519,
            "base64-peer-key",
            "rot13",
            "sha256:placeholder",
        );

        let hello_error = hello.validate().unwrap_err();
        let ready_error = ready.validate().unwrap_err();

        assert_eq!(hello_error.code, ErrorCode::InvalidPayload);
        assert!(hello_error.message.contains("unsupported session cipher"));
        assert_eq!(ready_error.code, ErrorCode::InvalidPayload);
        assert!(ready_error.message.contains("unsupported session cipher"));
    }

    #[test]
    fn derives_stable_session_handshake_hash_from_transcript_fields() {
        let identity = DeviceIdentity::new(
            "neko-device-abc123",
            "Hisakazu Mac",
            DeviceKind::Desktop,
            PlatformKind::Macos,
            "sha256:abc123",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let peer = DeviceIdentity::new(
            "neko-device-peer",
            "Peer Windows",
            DeviceKind::Desktop,
            PlatformKind::Windows,
            "sha256:peer",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let hello = SessionHelloPayload::new(
            "session-1",
            identity,
            "x25519",
            "base64-local-key",
            vec!["xchacha20poly1305".to_string()],
        );
        let ready = SessionReadyPayload::new(
            "session-1",
            peer,
            "x25519",
            "base64-peer-key",
            "xchacha20poly1305",
            "sha256:placeholder",
        );

        let first = session_handshake_hash(&hello, &ready).unwrap();
        let second = session_handshake_hash(&hello, &ready).unwrap();
        let mut changed_ready = ready.clone();
        changed_ready.ephemeral_public_key = "base64-other-peer-key".to_string();
        let changed = session_handshake_hash(&hello, &changed_ready).unwrap();

        assert!(first.starts_with("sha256:"));
        assert_eq!(first, second);
        assert_ne!(first, changed);
    }

    #[test]
    fn builds_session_ready_with_transcript_hash() {
        let identity = DeviceIdentity::new(
            "neko-device-abc123",
            "Hisakazu Mac",
            DeviceKind::Desktop,
            PlatformKind::Macos,
            "sha256:abc123",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let peer = DeviceIdentity::new(
            "neko-device-peer",
            "Peer Windows",
            DeviceKind::Desktop,
            PlatformKind::Windows,
            "sha256:peer",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let hello = SessionHelloPayload::new(
            "session-1",
            identity,
            "x25519",
            "base64-local-key",
            vec!["xchacha20poly1305".to_string(), "aes256gcm".to_string()],
        );

        let ready =
            SessionReadyPayload::for_hello(&hello, peer, "base64-peer-key", "xchacha20poly1305")
                .unwrap();
        let expected_hash = session_handshake_hash(&hello, &ready).unwrap();

        assert_eq!(ready.session_id, "session-1");
        assert_eq!(ready.key_agreement, "x25519");
        assert_eq!(ready.cipher, "xchacha20poly1305");
        assert_eq!(ready.handshake_hash, expected_hash);
    }

    #[test]
    fn builds_session_ready_with_local_cipher_preference() {
        let identity = DeviceIdentity::new(
            "neko-device-abc123",
            "Hisakazu Mac",
            DeviceKind::Desktop,
            PlatformKind::Macos,
            "sha256:abc123",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let peer = DeviceIdentity::new(
            "neko-device-peer",
            "Peer Windows",
            DeviceKind::Desktop,
            PlatformKind::Windows,
            "sha256:peer",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let hello = SessionHelloPayload::new(
            "session-1",
            identity,
            SESSION_KEY_AGREEMENT_X25519,
            "base64-local-key",
            vec![SESSION_CIPHER_AES256GCM.to_string()],
        );

        let ready = SessionReadyPayload::for_hello_with_cipher_preference(
            &hello,
            peer,
            "base64-peer-key",
            &default_session_cipher_preference(),
        )
        .unwrap();

        assert_eq!(ready.key_agreement, SESSION_KEY_AGREEMENT_X25519);
        assert_eq!(ready.cipher, SESSION_CIPHER_AES256GCM);
        ready.verify_for_hello(&hello).unwrap();
    }

    #[test]
    fn rejects_session_ready_builder_with_unoffered_cipher() {
        let identity = DeviceIdentity::new(
            "neko-device-abc123",
            "Hisakazu Mac",
            DeviceKind::Desktop,
            PlatformKind::Macos,
            "sha256:abc123",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let peer = DeviceIdentity::new(
            "neko-device-peer",
            "Peer Windows",
            DeviceKind::Desktop,
            PlatformKind::Windows,
            "sha256:peer",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let hello = SessionHelloPayload::new(
            "session-1",
            identity,
            "x25519",
            "base64-local-key",
            vec!["xchacha20poly1305".to_string()],
        );

        let error = SessionReadyPayload::for_hello(&hello, peer, "base64-peer-key", "aes256gcm")
            .unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("must be offered"));
    }

    #[test]
    fn verifies_session_ready_against_hello_transcript_hash() {
        let identity = DeviceIdentity::new(
            "neko-device-abc123",
            "Hisakazu Mac",
            DeviceKind::Desktop,
            PlatformKind::Macos,
            "sha256:abc123",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let peer = DeviceIdentity::new(
            "neko-device-peer",
            "Peer Windows",
            DeviceKind::Desktop,
            PlatformKind::Windows,
            "sha256:peer",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let hello = SessionHelloPayload::new(
            "session-1",
            identity,
            "x25519",
            "base64-local-key",
            vec!["xchacha20poly1305".to_string()],
        );
        let ready =
            SessionReadyPayload::for_hello(&hello, peer, "base64-peer-key", "xchacha20poly1305")
                .unwrap();

        ready.verify_for_hello(&hello).unwrap();

        let mut tampered = ready.clone();
        tampered.handshake_hash = format!("sha256:{}", "0".repeat(64));
        let error = tampered.verify_for_hello(&hello).unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("handshake_hash mismatch"));
    }

    #[test]
    fn rejects_malformed_session_ready_handshake_hash_before_verifying_transcript() {
        let identity = DeviceIdentity::new(
            "neko-device-abc123",
            "Hisakazu Mac",
            DeviceKind::Desktop,
            PlatformKind::Macos,
            "sha256:abc123",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let peer = DeviceIdentity::new(
            "neko-device-peer",
            "Peer Windows",
            DeviceKind::Desktop,
            PlatformKind::Windows,
            "sha256:peer",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let hello = SessionHelloPayload::new(
            "session-1",
            identity,
            SESSION_KEY_AGREEMENT_X25519,
            "base64-local-key",
            vec![SESSION_CIPHER_XCHACHA20POLY1305.to_string()],
        );
        let mut ready = SessionReadyPayload::for_hello(
            &hello,
            peer,
            "base64-peer-key",
            SESSION_CIPHER_XCHACHA20POLY1305,
        )
        .unwrap();
        ready.handshake_hash = "sha256:not-hex".to_string();

        let error = ready.verify_for_hello(&hello).unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("handshake_hash must be sha256"));
    }

    #[test]
    fn builds_verified_session_handshake_from_ready() {
        let identity = DeviceIdentity::new(
            "neko-device-abc123",
            "Hisakazu Mac",
            DeviceKind::Desktop,
            PlatformKind::Macos,
            "sha256:abc123",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let peer = DeviceIdentity::new(
            "neko-device-peer",
            "Peer Windows",
            DeviceKind::Desktop,
            PlatformKind::Windows,
            "sha256:peer",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let hello = SessionHelloPayload::default_crypto("session-1", identity, "base64-local-key");
        let ready = SessionReadyPayload::for_hello_with_cipher_preference(
            &hello,
            peer,
            "base64-peer-key",
            &default_session_cipher_preference(),
        )
        .unwrap();

        let handshake = VerifiedSessionHandshake::from_ready(&hello, &ready).unwrap();

        assert_eq!(handshake.session_id, "session-1");
        assert_eq!(handshake.key_agreement, SESSION_KEY_AGREEMENT_X25519);
        assert_eq!(handshake.cipher, SESSION_CIPHER_XCHACHA20POLY1305);
        assert_eq!(handshake.handshake_hash, ready.handshake_hash);
        assert_eq!(
            handshake.initiator_ephemeral_public_key,
            hello.ephemeral_public_key
        );
        assert_eq!(
            handshake.responder_ephemeral_public_key,
            ready.ephemeral_public_key
        );
        assert_eq!(handshake.initiator.device_id, hello.identity.device_id);
        assert_eq!(handshake.responder.device_id, ready.identity.device_id);
    }

    #[test]
    fn builds_session_identity_bindings_from_verified_handshake() {
        let identity = DeviceIdentity::new(
            "neko-device-abc123",
            "Hisakazu Mac",
            DeviceKind::Desktop,
            PlatformKind::Macos,
            "sha256:abc123",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let peer = DeviceIdentity::new(
            "neko-device-peer",
            "Peer Windows",
            DeviceKind::Desktop,
            PlatformKind::Windows,
            "sha256:peer",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let hello = SessionHelloPayload::default_crypto("session-1", identity, "base64-local-key");
        let ready = SessionReadyPayload::for_hello(
            &hello,
            peer,
            "base64-peer-key",
            SESSION_CIPHER_XCHACHA20POLY1305,
        )
        .unwrap();
        let handshake = VerifiedSessionHandshake::from_ready(&hello, &ready).unwrap();

        let initiator = SessionIdentityBinding::for_initiator(&handshake).unwrap();
        let responder = SessionIdentityBinding::for_responder(&handshake).unwrap();

        assert_eq!(initiator.role, SessionParticipantRole::Initiator);
        assert_eq!(initiator.session_id, "session-1");
        assert_eq!(initiator.device_id, "neko-device-abc123");
        assert_eq!(initiator.public_key_fingerprint, "sha256:abc123");
        assert_eq!(initiator.session_ephemeral_public_key, "base64-local-key");
        assert_eq!(initiator.handshake_hash, ready.handshake_hash);
        assert_ne!(
            initiator.canonical_payload_hash().unwrap(),
            responder.canonical_payload_hash().unwrap()
        );

        assert_eq!(responder.role, SessionParticipantRole::Responder);
        assert_eq!(responder.device_id, "neko-device-peer");
        assert_eq!(responder.public_key_fingerprint, "sha256:peer");
        assert_eq!(responder.session_ephemeral_public_key, "base64-peer-key");
        assert!(initiator.verify_identity(&handshake.initiator).is_ok());
        assert!(responder.verify_identity(&handshake.responder).is_ok());

        let error = initiator.verify_identity(&handshake.responder).unwrap_err();
        assert!(error
            .message
            .contains("session identity binding does not match device identity"));
    }

    #[test]
    fn session_identity_binding_hash_changes_with_session_material() {
        let identity = DeviceIdentity::new(
            "neko-device-abc123",
            "Hisakazu Mac",
            DeviceKind::Desktop,
            PlatformKind::Macos,
            "sha256:abc123",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let binding = SessionIdentityBinding::new(
            SessionParticipantRole::Initiator,
            "session-1",
            &identity,
            "base64-local-key",
            format!("sha256:{}", "1".repeat(64)),
        )
        .unwrap();

        let mut changed_key = binding.clone();
        changed_key.session_ephemeral_public_key = "base64-other-key".to_string();
        let mut changed_hash = binding.clone();
        changed_hash.handshake_hash = format!("sha256:{}", "2".repeat(64));

        assert_ne!(
            binding.canonical_payload_hash().unwrap(),
            changed_key.canonical_payload_hash().unwrap()
        );
        assert_ne!(
            binding.canonical_payload_hash().unwrap(),
            changed_hash.canonical_payload_hash().unwrap()
        );
    }

    #[test]
    fn signed_session_identity_binding_verifies_and_rejects_tampering() {
        let identity = DeviceIdentity::new(
            "neko-device-abc123",
            "Hisakazu Mac",
            DeviceKind::Desktop,
            PlatformKind::Macos,
            "sha256:abc123",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let binding = SessionIdentityBinding::new(
            SessionParticipantRole::Initiator,
            "session-1",
            &identity,
            "base64-local-key",
            format!("sha256:{}", "1".repeat(64)),
        )
        .unwrap();
        let signing_key = DeviceIdentitySigningKey::from_seed([7_u8; 32]);

        let signed = SignedSessionIdentityBinding::sign(binding.clone(), &signing_key).unwrap();

        assert_eq!(signed.algorithm, DEVICE_IDENTITY_SIGNATURE_ED25519);
        assert_eq!(
            signed.public_key_fingerprint,
            signing_key.public_key_fingerprint()
        );
        signed.verify(&binding).unwrap();

        let mut tampered = signed.clone();
        tampered.binding.handshake_hash = format!("sha256:{}", "2".repeat(64));
        let error = tampered.verify(&tampered.binding).unwrap_err();
        assert!(error
            .message
            .contains("session identity signature verification failed"));

        let other_key = DeviceIdentitySigningKey::from_seed([8_u8; 32]);
        let error = signed
            .verify_with_public_key(&binding, &other_key.public_key())
            .unwrap_err();
        assert!(error
            .message
            .contains("session identity signature public key mismatch"));
    }

    #[test]
    fn builds_session_key_derivation_context_from_verified_handshake() {
        let identity = DeviceIdentity::new(
            "neko-device-abc123",
            "Hisakazu Mac",
            DeviceKind::Desktop,
            PlatformKind::Macos,
            "sha256:abc123",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let peer = DeviceIdentity::new(
            "neko-device-peer",
            "Peer Windows",
            DeviceKind::Desktop,
            PlatformKind::Windows,
            "sha256:peer",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let hello = SessionHelloPayload::default_crypto("session-1", identity, "base64-local-key");
        let ready = SessionReadyPayload::for_hello(
            &hello,
            peer,
            "base64-peer-key",
            SESSION_CIPHER_XCHACHA20POLY1305,
        )
        .unwrap();
        let handshake = VerifiedSessionHandshake::from_ready(&hello, &ready).unwrap();

        let context = handshake.key_derivation_context();

        assert_eq!(context.session_id, "session-1");
        assert_eq!(context.key_agreement, SESSION_KEY_AGREEMENT_X25519);
        assert_eq!(context.cipher, SESSION_CIPHER_XCHACHA20POLY1305);
        assert_eq!(context.handshake_hash, ready.handshake_hash);
        assert_eq!(context.salt, ready.handshake_hash);
        assert_eq!(
            context.send_info,
            "nekolink/session-1/x25519/xchacha20poly1305/neko-device-abc123->neko-device-peer"
        );
        assert_eq!(
            context.receive_info,
            "nekolink/session-1/x25519/xchacha20poly1305/neko-device-peer->neko-device-abc123"
        );
    }

    #[test]
    fn builds_session_key_derivation_context_for_local_device_direction() {
        let identity = DeviceIdentity::new(
            "neko-device-abc123",
            "Hisakazu Mac",
            DeviceKind::Desktop,
            PlatformKind::Macos,
            "sha256:abc123",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let peer = DeviceIdentity::new(
            "neko-device-peer",
            "Peer Windows",
            DeviceKind::Desktop,
            PlatformKind::Windows,
            "sha256:peer",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let hello = SessionHelloPayload::default_crypto("session-1", identity, "base64-local-key");
        let ready = SessionReadyPayload::for_hello(
            &hello,
            peer,
            "base64-peer-key",
            SESSION_CIPHER_XCHACHA20POLY1305,
        )
        .unwrap();
        let handshake = VerifiedSessionHandshake::from_ready(&hello, &ready).unwrap();

        let initiator_context = handshake
            .key_derivation_context_for_local_device("neko-device-abc123")
            .unwrap();
        let responder_context = handshake
            .key_derivation_context_for_local_device("neko-device-peer")
            .unwrap();

        assert_eq!(initiator_context.send_info, responder_context.receive_info);
        assert_eq!(initiator_context.receive_info, responder_context.send_info);

        let error = handshake
            .key_derivation_context_for_local_device("unknown-device")
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("not part of verified session"));
    }

    #[test]
    fn derives_directional_session_key_material_from_shared_secret() {
        let identity = DeviceIdentity::new(
            "neko-device-abc123",
            "Hisakazu Mac",
            DeviceKind::Desktop,
            PlatformKind::Macos,
            "sha256:abc123",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let peer = DeviceIdentity::new(
            "neko-device-peer",
            "Peer Windows",
            DeviceKind::Desktop,
            PlatformKind::Windows,
            "sha256:peer",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let hello = SessionHelloPayload::default_crypto("session-1", identity, "base64-local-key");
        let ready = SessionReadyPayload::for_hello(
            &hello,
            peer,
            "base64-peer-key",
            SESSION_CIPHER_XCHACHA20POLY1305,
        )
        .unwrap();
        let handshake = VerifiedSessionHandshake::from_ready(&hello, &ready).unwrap();
        let initiator_context = handshake
            .key_derivation_context_for_local_device("neko-device-abc123")
            .unwrap();
        let responder_context = handshake
            .key_derivation_context_for_local_device("neko-device-peer")
            .unwrap();
        let shared_secret = [7_u8; SESSION_SHARED_SECRET_LEN];

        let initiator_keys = initiator_context
            .derive_key_material(&shared_secret)
            .unwrap();
        let responder_keys = responder_context
            .derive_key_material(&shared_secret)
            .unwrap();
        let repeated_keys = initiator_context
            .derive_key_material(&shared_secret)
            .unwrap();

        assert_eq!(initiator_keys, repeated_keys);
        assert_eq!(initiator_keys.send_key, responder_keys.receive_key);
        assert_eq!(initiator_keys.receive_key, responder_keys.send_key);
        assert_ne!(initiator_keys.send_key, initiator_keys.receive_key);
        assert_ne!(initiator_keys.send_key, [0_u8; SESSION_TRAFFIC_KEY_LEN]);
    }

    #[test]
    fn rejects_session_key_derivation_with_malformed_salt() {
        let identity = DeviceIdentity::new(
            "neko-device-abc123",
            "Hisakazu Mac",
            DeviceKind::Desktop,
            PlatformKind::Macos,
            "sha256:abc123",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let peer = DeviceIdentity::new(
            "neko-device-peer",
            "Peer Windows",
            DeviceKind::Desktop,
            PlatformKind::Windows,
            "sha256:peer",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let hello = SessionHelloPayload::default_crypto("session-1", identity, "base64-local-key");
        let ready = SessionReadyPayload::for_hello(
            &hello,
            peer,
            "base64-peer-key",
            SESSION_CIPHER_XCHACHA20POLY1305,
        )
        .unwrap();
        let handshake = VerifiedSessionHandshake::from_ready(&hello, &ready).unwrap();
        let mut context = handshake.key_derivation_context();
        context.salt = "not-a-sha256-label".to_string();

        let error = context
            .derive_key_material(&[7_u8; SESSION_SHARED_SECRET_LEN])
            .unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("salt must be sha256"));
    }

    #[test]
    fn derives_same_x25519_shared_secret_from_peer_public_keys() {
        let initiator = SessionEphemeralKeyPair::generate().unwrap();
        let responder = SessionEphemeralKeyPair::generate().unwrap();

        let initiator_secret = initiator
            .shared_secret_from_peer_public_key(&responder.public_key)
            .unwrap();
        let responder_secret = responder
            .shared_secret_from_peer_public_key(&initiator.public_key)
            .unwrap();

        assert_eq!(initiator.public_key.len(), SESSION_PUBLIC_KEY_BASE64_LEN);
        assert_eq!(responder.public_key.len(), SESSION_PUBLIC_KEY_BASE64_LEN);
        assert_eq!(initiator_secret, responder_secret);
        assert_ne!(initiator_secret, [0_u8; SESSION_SHARED_SECRET_LEN]);
    }

    #[test]
    fn rejects_malformed_x25519_peer_public_key() {
        let keypair = SessionEphemeralKeyPair::generate().unwrap();

        let error = keypair
            .shared_secret_from_peer_public_key("not-valid-base64")
            .unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("ephemeral_public_key"));
    }

    #[test]
    fn builds_stable_x25519_public_key_from_secret() {
        let first =
            SessionEphemeralKeyPair::from_secret([3_u8; SESSION_SHARED_SECRET_LEN]).unwrap();
        let second =
            SessionEphemeralKeyPair::from_secret([3_u8; SESSION_SHARED_SECRET_LEN]).unwrap();

        assert_eq!(first.public_key, second.public_key);
        assert_eq!(first.public_key.len(), SESSION_PUBLIC_KEY_BASE64_LEN);

        let error =
            SessionEphemeralKeyPair::from_secret([0_u8; SESSION_SHARED_SECRET_LEN]).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("all zero"));
    }

    #[test]
    fn session_ephemeral_keypair_debug_omits_secret_material() {
        let keypair =
            SessionEphemeralKeyPair::from_secret([3_u8; SESSION_SHARED_SECRET_LEN]).unwrap();

        let debug = format!("{keypair:?}");

        assert!(debug.contains("public_key"));
        assert!(!debug.contains("secret"));
        assert!(!debug.contains("[3, 3, 3"));
    }

    #[test]
    fn allocates_directional_session_traffic_frame_headers() {
        let mut counters = SessionTrafficCounters::default();

        let first = counters
            .next_send_header(SESSION_CIPHER_XCHACHA20POLY1305, SessionFrameKind::Control)
            .unwrap();
        let second = counters
            .next_send_header(SESSION_CIPHER_XCHACHA20POLY1305, SessionFrameKind::File)
            .unwrap();
        let receive = counters
            .next_receive_header(SESSION_CIPHER_XCHACHA20POLY1305, SessionFrameKind::Control)
            .unwrap();

        assert_eq!(first.direction, SessionFrameDirection::Send);
        assert_eq!(first.kind, SessionFrameKind::Control);
        assert_eq!(first.counter, 0);
        assert_eq!(first.nonce.len(), SESSION_XCHACHA20POLY1305_NONCE_LEN);
        assert_eq!(second.direction, SessionFrameDirection::Send);
        assert_eq!(second.kind, SessionFrameKind::File);
        assert_eq!(second.counter, 1);
        assert_eq!(receive.direction, SessionFrameDirection::Receive);
        assert_eq!(receive.counter, 0);
        assert_eq!(first.cipher, SESSION_CIPHER_XCHACHA20POLY1305);
        assert_ne!(first.nonce, second.nonce);
        assert_eq!(first.nonce, receive.nonce);
    }

    #[test]
    fn rejects_session_traffic_counter_overflow() {
        let mut counters = SessionTrafficCounters::new(u64::MAX, 0);

        let error = counters
            .next_send_header(SESSION_CIPHER_XCHACHA20POLY1305, SessionFrameKind::Control)
            .unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("counter exhausted"));
    }

    #[test]
    fn session_replay_window_rejects_duplicate_receive_counters() {
        let mut window = SessionReplayWindow::default();
        let header = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_XCHACHA20POLY1305,
            SessionFrameKind::Control,
            SessionFrameDirection::Receive,
            7,
        )
        .unwrap();

        window.accept(&header).unwrap();
        let error = window.accept(&header).unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("replayed session frame"));
    }

    #[test]
    fn session_replay_window_rejects_counters_older_than_window() {
        let mut window = SessionReplayWindow::with_window_size(4).unwrap();
        for counter in 0..=4 {
            let header = SessionTrafficFrameHeader::new(
                SESSION_CIPHER_XCHACHA20POLY1305,
                SessionFrameKind::Control,
                SessionFrameDirection::Receive,
                counter,
            )
            .unwrap();
            window.accept(&header).unwrap();
        }

        let old_header = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_XCHACHA20POLY1305,
            SessionFrameKind::Control,
            SessionFrameDirection::Receive,
            0,
        )
        .unwrap();
        let error = window.accept(&old_header).unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("outside replay window"));
    }

    #[test]
    fn builds_session_traffic_nonce_for_negotiated_cipher() {
        let xchacha = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_XCHACHA20POLY1305,
            SessionFrameKind::Control,
            SessionFrameDirection::Send,
            7,
        )
        .unwrap();
        let aes = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_AES256GCM,
            SessionFrameKind::Control,
            SessionFrameDirection::Send,
            7,
        )
        .unwrap();

        assert_eq!(xchacha.nonce.len(), SESSION_XCHACHA20POLY1305_NONCE_LEN);
        assert_eq!(aes.nonce.len(), SESSION_AES256GCM_NONCE_LEN);
        assert_ne!(xchacha.nonce, aes.nonce);

        let error = SessionTrafficFrameHeader::new(
            "rot13",
            SessionFrameKind::Control,
            SessionFrameDirection::Send,
            7,
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("unsupported session cipher"));
    }

    #[test]
    fn seals_and_opens_xchacha20poly1305_session_payloads() {
        let keys = SessionKeyMaterial {
            send_key: [11_u8; SESSION_TRAFFIC_KEY_LEN],
            receive_key: [11_u8; SESSION_TRAFFIC_KEY_LEN],
        };
        let header = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_XCHACHA20POLY1305,
            SessionFrameKind::Control,
            SessionFrameDirection::Send,
            3,
        )
        .unwrap();
        let plaintext = br#"{"kind":"file.offer"}"#;
        let aad = b"session-1";

        let sealed = keys.seal_send_payload(&header, aad, plaintext).unwrap();
        let opened = keys.open_receive_payload(&header, aad, &sealed).unwrap();

        assert_ne!(sealed, plaintext);
        assert_eq!(opened, plaintext);
    }

    #[test]
    fn rejects_tampered_session_payloads() {
        let keys = SessionKeyMaterial {
            send_key: [11_u8; SESSION_TRAFFIC_KEY_LEN],
            receive_key: [11_u8; SESSION_TRAFFIC_KEY_LEN],
        };
        let header = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_XCHACHA20POLY1305,
            SessionFrameKind::Control,
            SessionFrameDirection::Send,
            3,
        )
        .unwrap();
        let aad = b"session-1";
        let mut sealed = keys
            .seal_send_payload(&header, aad, b"control payload")
            .unwrap();
        sealed[0] ^= 0x80;

        let error = keys
            .open_receive_payload(&header, aad, &sealed)
            .unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("failed to open session payload"));
    }

    #[test]
    fn seals_and_opens_aes256gcm_session_payloads() {
        let keys = SessionKeyMaterial {
            send_key: [19_u8; SESSION_TRAFFIC_KEY_LEN],
            receive_key: [19_u8; SESSION_TRAFFIC_KEY_LEN],
        };
        let header = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_AES256GCM,
            SessionFrameKind::Control,
            SessionFrameDirection::Send,
            5,
        )
        .unwrap();

        let sealed = keys
            .seal_send_payload(&header, b"session-1", b"aes payload")
            .unwrap();
        let opened = keys
            .open_receive_payload(&header, b"session-1", &sealed)
            .unwrap();

        assert_ne!(sealed, b"aes payload");
        assert_eq!(opened, b"aes payload");
    }

    #[test]
    fn builds_encrypted_session_control_envelope() {
        let keys = SessionKeyMaterial {
            send_key: [11_u8; SESSION_TRAFFIC_KEY_LEN],
            receive_key: [11_u8; SESSION_TRAFFIC_KEY_LEN],
        };
        let header = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_XCHACHA20POLY1305,
            SessionFrameKind::Control,
            SessionFrameDirection::Send,
            9,
        )
        .unwrap();
        let payload = TransferDecision::accept();

        let envelope = EncryptedSessionPayload::seal_control(
            "session-1",
            "message-1",
            &keys,
            header,
            MessageKind::FileAccept,
            &payload,
        )
        .unwrap();
        let opened: TransferDecision =
            EncryptedSessionPayload::open_control(&envelope, &keys).unwrap();

        assert_eq!(envelope.kind, MessageKind::SessionControl);
        assert_eq!(envelope.payload.inner_kind, MessageKind::FileAccept);
        assert_eq!(envelope.payload.header.kind, SessionFrameKind::Control);
        assert!(!envelope.payload.ciphertext.is_empty());
        assert!(!String::from_utf8_lossy(&envelope.payload.ciphertext).contains("accepted"));
        assert_eq!(opened.accepted, payload.accepted);
    }

    #[test]
    fn opens_encrypted_session_control_envelope_once_with_replay_window() {
        let keys = SessionKeyMaterial {
            send_key: [11_u8; SESSION_TRAFFIC_KEY_LEN],
            receive_key: [11_u8; SESSION_TRAFFIC_KEY_LEN],
        };
        let header = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_XCHACHA20POLY1305,
            SessionFrameKind::Control,
            SessionFrameDirection::Send,
            9,
        )
        .unwrap();
        let envelope = EncryptedSessionPayload::seal_control(
            "session-1",
            "message-1",
            &keys,
            header,
            MessageKind::FileAccept,
            &TransferDecision::accept(),
        )
        .unwrap();
        let mut replay_window = SessionReplayWindow::default();

        let opened: TransferDecision =
            EncryptedSessionPayload::open_control_once(&envelope, &keys, &mut replay_window)
                .unwrap();
        let error = EncryptedSessionPayload::open_control_once::<TransferDecision>(
            &envelope,
            &keys,
            &mut replay_window,
        )
        .unwrap_err();

        assert!(opened.accepted);
        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("replayed session frame"));
    }

    #[test]
    fn tampered_encrypted_session_control_does_not_advance_replay_window() {
        let keys = SessionKeyMaterial {
            send_key: [11_u8; SESSION_TRAFFIC_KEY_LEN],
            receive_key: [11_u8; SESSION_TRAFFIC_KEY_LEN],
        };
        let header = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_XCHACHA20POLY1305,
            SessionFrameKind::Control,
            SessionFrameDirection::Send,
            9,
        )
        .unwrap();
        let envelope = EncryptedSessionPayload::seal_control(
            "session-1",
            "message-1",
            &keys,
            header,
            MessageKind::FileAccept,
            &TransferDecision::accept(),
        )
        .unwrap();
        let mut tampered = envelope.clone();
        tampered.message_id = "message-2".to_string();
        let mut replay_window = SessionReplayWindow::default();

        let tampered_error = EncryptedSessionPayload::open_control_once::<TransferDecision>(
            &tampered,
            &keys,
            &mut replay_window,
        )
        .unwrap_err();
        let opened: TransferDecision =
            EncryptedSessionPayload::open_control_once(&envelope, &keys, &mut replay_window)
                .unwrap();

        assert_eq!(tampered_error.code, ErrorCode::InvalidPayload);
        assert!(tampered_error
            .message
            .contains("failed to open session payload"));
        assert!(opened.accepted);
    }

    #[test]
    fn rejects_encrypted_session_control_envelope_with_tampered_aad() {
        let keys = SessionKeyMaterial {
            send_key: [11_u8; SESSION_TRAFFIC_KEY_LEN],
            receive_key: [11_u8; SESSION_TRAFFIC_KEY_LEN],
        };
        let header = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_XCHACHA20POLY1305,
            SessionFrameKind::Control,
            SessionFrameDirection::Send,
            9,
        )
        .unwrap();
        let mut envelope = EncryptedSessionPayload::seal_control(
            "session-1",
            "message-1",
            &keys,
            header,
            MessageKind::FileAccept,
            &TransferDecision::accept(),
        )
        .unwrap();
        envelope.message_id = "message-2".to_string();

        let error = EncryptedSessionPayload::open_control::<TransferDecision>(&envelope, &keys)
            .unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("failed to open session payload"));
    }

    #[test]
    fn encrypted_file_frame_binds_transfer_path_offset_and_size_to_aad() {
        let keys = SessionKeyMaterial {
            send_key: [31_u8; SESSION_TRAFFIC_KEY_LEN],
            receive_key: [31_u8; SESSION_TRAFFIC_KEY_LEN],
        };
        let traffic_header = SessionTrafficFrameHeader::new(
            SESSION_CIPHER_XCHACHA20POLY1305,
            SessionFrameKind::File,
            SessionFrameDirection::Send,
            12,
        )
        .unwrap();
        let frame_header =
            EncryptedFileFrameHeader::new("transfer-1", "drop/sample.txt", 6, 5, traffic_header)
                .unwrap();
        let sealed = EncryptedFileFrame::seal(&keys, frame_header.clone(), b"world").unwrap();

        assert_eq!(sealed.open(&keys).unwrap(), b"world");
        assert_ne!(sealed.ciphertext, b"world");

        let mut tampered_transfer = sealed.clone();
        tampered_transfer.header.transfer_id = "transfer-2".to_string();
        assert!(tampered_transfer.open(&keys).is_err());

        let mut tampered_path = sealed.clone();
        tampered_path.header.manifest_path = "drop/other.txt".to_string();
        assert!(tampered_path.open(&keys).is_err());

        let mut tampered_offset = sealed.clone();
        tampered_offset.header.offset = 7;
        assert!(tampered_offset.open(&keys).is_err());

        let mut tampered_size = sealed.clone();
        tampered_size.header.plain_size = 6;
        assert!(tampered_size.open(&keys).is_err());

        let mut tampered_traffic = sealed.clone();
        tampered_traffic.header.traffic.direction = SessionFrameDirection::Receive;
        assert!(tampered_traffic.open(&keys).is_err());
    }

    #[test]
    fn rejects_session_handshake_hash_for_mismatched_transcript() {
        let identity = DeviceIdentity::new(
            "neko-device-abc123",
            "Hisakazu Mac",
            DeviceKind::Desktop,
            PlatformKind::Macos,
            "sha256:abc123",
            [
                Capability::FileTransfer,
                Capability::DevicePairing,
                Capability::EncryptedSession,
            ],
        );
        let hello = SessionHelloPayload::new(
            "session-1",
            identity.clone(),
            "x25519",
            "base64-local-key",
            vec!["xchacha20poly1305".to_string()],
        );
        let ready = SessionReadyPayload::new(
            "session-2",
            identity,
            "x25519",
            "base64-peer-key",
            "xchacha20poly1305",
            "sha256:placeholder",
        );

        let error = session_handshake_hash(&hello, &ready).unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("matching session_id"));
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

    #[test]
    fn validates_skill_bundle_manifest() {
        let manifest = valid_bundle_manifest();
        let checksums = valid_bundle_checksums();
        let permissions = valid_bundle_permissions(false);

        manifest.validate().unwrap();
        checksums.validate_against(&manifest).unwrap();
        assert!(permissions.can_import().unwrap());
    }

    #[test]
    fn bundle_checksums_match_documented_path_map_json() {
        let checksums = valid_bundle_checksums();

        let json = serde_json::to_value(&checksums).unwrap();
        let files = json["files"].as_object().unwrap();

        assert_eq!(
            files["files/manifest.json"],
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        );
        assert_eq!(
            serde_json::from_value::<BundleChecksums>(json).unwrap(),
            checksums
        );
    }

    #[test]
    fn bundle_permissions_use_documented_scope_labels() {
        let permissions = valid_bundle_permissions(false);

        let json = serde_json::to_value(&permissions).unwrap();

        assert_eq!(json["requested_scopes"][0], "skill.install");
        assert_eq!(json["requested_scopes"][1], "workspace.import");
        assert_eq!(
            serde_json::from_value::<BundlePermissions>(json).unwrap(),
            permissions
        );
    }

    #[test]
    fn rejects_bundle_manifest_with_mismatched_summary() {
        let mut manifest = valid_bundle_manifest();
        manifest.summary.file_count = 99;

        let error = manifest.validate().unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("file count mismatch"));
    }

    #[test]
    fn rejects_bundle_manifest_with_unsafe_paths() {
        let mut manifest = valid_bundle_manifest();
        manifest.files[0].path = "../secret.txt".to_string();

        let error = manifest.validate().unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("unsafe path segment"));
    }

    #[test]
    fn rejects_bundle_manifest_with_bad_sha256() {
        let mut manifest = valid_bundle_manifest();
        manifest.files[0].sha256 = "not-a-hash".to_string();

        let error = manifest.validate().unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("64-character hex"));
    }

    #[test]
    fn rejects_bundle_manifest_with_unsupported_schema() {
        let mut manifest = valid_bundle_manifest();
        manifest.schema = "nekolink.bundle.v2".to_string();

        let error = manifest.validate().unwrap_err();

        assert_eq!(error.code, ErrorCode::UnsupportedVersion);
        assert!(error.message.contains("unsupported bundle schema"));
    }

    #[test]
    fn rejects_bundle_payload_files_outside_files_directory() {
        let mut manifest = valid_bundle_manifest();
        manifest.files[0].path = "manifest.json".to_string();

        let error = manifest.validate().unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("under files/"));
    }

    #[test]
    fn rejects_bundle_checksums_that_do_not_match_manifest() {
        let manifest = valid_bundle_manifest();
        let mut checksums = valid_bundle_checksums();
        *checksums.files.get_mut("files/manifest.json").unwrap() =
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".to_string();

        let error = checksums.validate_against(&manifest).unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("checksum mismatch"));
    }

    #[test]
    fn rejects_bundle_checksums_missing_manifest_files() {
        let manifest = valid_bundle_manifest();
        let mut checksums = valid_bundle_checksums();
        checksums.files.remove("files/content.bin");

        let error = checksums.validate_against(&manifest).unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("file count mismatch"));
    }

    #[test]
    fn marks_bundle_permissions_with_secrets_as_not_importable() {
        let permissions = valid_bundle_permissions(true);

        assert!(!permissions.can_import().unwrap());
    }

    #[test]
    fn rejects_bundle_write_permissions_with_filesystem_targets() {
        let mut permissions = valid_bundle_permissions(false);
        permissions.writes[0].target = "/Users/example/.ssh".to_string();

        let error = permissions.can_import().unwrap_err();

        assert_eq!(error.code, ErrorCode::InvalidPayload);
        assert!(error.message.contains("logical target"));
    }

    #[test]
    fn local_bridge_send_bundle_request_uses_stable_json_shape() {
        let request = LocalBridgeRequest::SendBundle(LocalBridgeSendBundleRequest {
            request_id: "bridge-request-1".to_string(),
            client: None,
            target_device_id: Some("device-b".to_string()),
            bundle_root: "bundle".to_string(),
            bundle_type: BundleType::Skill,
            require_trusted_device: true,
        });

        request.validate().unwrap();

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["kind"], "bundle.send");
        assert_eq!(json["payload"]["request_id"], "bridge-request-1");
        assert_eq!(json["payload"]["target_device_id"], "device-b");
        assert_eq!(json["payload"]["bundle_root"], "bundle");
        assert_eq!(json["payload"]["bundle_type"], "skill");
        assert_eq!(json["payload"]["require_trusted_device"], true);
        assert_eq!(
            serde_json::from_value::<LocalBridgeRequest>(json).unwrap(),
            request
        );
    }

    #[test]
    fn local_bridge_request_accepts_optional_client_identity() {
        let request = LocalBridgeRequest::ListDevices(LocalBridgeListDevicesRequest {
            request_id: "bridge-request-1".to_string(),
            client: Some(LocalBridgeClientIdentity {
                client_id: "local-agent-app".to_string(),
                display_name: "Local Agent App".to_string(),
                app_kind: Some("agent".to_string()),
            }),
            trusted_only: true,
        });

        request.validate().unwrap();

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["kind"], "devices.list");
        assert_eq!(json["payload"]["client"]["client_id"], "local-agent-app");
        assert_eq!(json["payload"]["client"]["display_name"], "Local Agent App");
        assert_eq!(json["payload"]["client"]["app_kind"], "agent");
        assert_eq!(
            serde_json::from_value::<LocalBridgeRequest>(json).unwrap(),
            request
        );
    }

    #[test]
    fn local_bridge_request_rejects_unsafe_client_identity() {
        let request = LocalBridgeRequest::ListDevices(LocalBridgeListDevicesRequest {
            request_id: "bridge-request-1".to_string(),
            client: Some(LocalBridgeClientIdentity {
                client_id: "../local-agent".to_string(),
                display_name: "Local Agent App".to_string(),
                app_kind: Some("agent".to_string()),
            }),
            trusted_only: true,
        });

        let error = request.validate().unwrap_err();

        assert!(error.message.contains("client_id"));
    }

    #[test]
    fn local_bridge_authorization_request_uses_stable_json_shape() {
        let request = LocalBridgeRequest::AuthorizationRequest(LocalBridgeAuthorizationRequest {
            request_id: "bridge-auth-1".to_string(),
            client: LocalBridgeClientIdentity {
                client_id: "local-agent-app".to_string(),
                display_name: "Local Agent App".to_string(),
                app_kind: Some("agent".to_string()),
            },
            requested_scopes: vec![
                LocalBridgePermissionScope::DeviceRead,
                LocalBridgePermissionScope::BundleSend,
            ],
            reason: "Send a skill bundle to a trusted desktop device".to_string(),
            ttl_seconds: Some(900),
        });

        request.validate().unwrap();

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["kind"], "authorization.request");
        assert_eq!(json["payload"]["request_id"], "bridge-auth-1");
        assert_eq!(json["payload"]["client"]["client_id"], "local-agent-app");
        assert_eq!(json["payload"]["requested_scopes"][0], "device.read");
        assert_eq!(json["payload"]["requested_scopes"][1], "bundle.send");
        assert_eq!(
            json["payload"]["reason"],
            "Send a skill bundle to a trusted desktop device"
        );
        assert_eq!(json["payload"]["ttl_seconds"], 900);
        assert_eq!(
            serde_json::from_value::<LocalBridgeRequest>(json).unwrap(),
            request
        );
    }

    #[test]
    fn local_bridge_authorization_request_requires_scopes_and_reason() {
        let request = LocalBridgeRequest::AuthorizationRequest(LocalBridgeAuthorizationRequest {
            request_id: "bridge-auth-1".to_string(),
            client: LocalBridgeClientIdentity {
                client_id: "local-agent-app".to_string(),
                display_name: "Local Agent App".to_string(),
                app_kind: Some("agent".to_string()),
            },
            requested_scopes: Vec::new(),
            reason: " ".to_string(),
            ttl_seconds: Some(900),
        });

        let error = request.validate().unwrap_err();

        assert!(error.message.contains("requested_scopes"));
    }

    #[test]
    fn local_bridge_authorization_request_rejects_invalid_ttl() {
        let request = LocalBridgeRequest::AuthorizationRequest(LocalBridgeAuthorizationRequest {
            request_id: "bridge-auth-1".to_string(),
            client: LocalBridgeClientIdentity {
                client_id: "local-agent-app".to_string(),
                display_name: "Local Agent App".to_string(),
                app_kind: Some("agent".to_string()),
            },
            requested_scopes: vec![LocalBridgePermissionScope::BundleRead],
            reason: "Read staged bundle metadata".to_string(),
            ttl_seconds: Some(604_801),
        });

        let error = request.validate().unwrap_err();

        assert!(error.message.contains("ttl_seconds"));
    }

    #[test]
    fn local_bridge_bundle_detail_request_uses_stable_json_shape() {
        let request = LocalBridgeRequest::BundleDetail(LocalBridgeBundleDetailRequest {
            request_id: "bridge-request-detail".to_string(),
            client: None,
            staged_bundle_id: "bundle_1234567890".to_string(),
        });

        request.validate().unwrap();

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["kind"], "bundle.detail");
        assert_eq!(json["payload"]["request_id"], "bridge-request-detail");
        assert_eq!(json["payload"]["staged_bundle_id"], "bundle_1234567890");
        assert_eq!(
            serde_json::from_value::<LocalBridgeRequest>(json).unwrap(),
            request
        );
    }

    #[test]
    fn local_bridge_poll_events_request_uses_stable_json_shape() {
        let request = LocalBridgeRequest::PollEvents(LocalBridgePollEventsRequest {
            request_id: "bridge-events-1".to_string(),
            client: Some(LocalBridgeClientIdentity {
                client_id: "local-agent-app".to_string(),
                display_name: "Local Agent App".to_string(),
                app_kind: Some("agent".to_string()),
            }),
            after_event_id: Some("bridge-event-1".to_string()),
            limit: Some(10),
            timeout_ms: Some(30_000),
        });

        request.validate().unwrap();

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["kind"], "events.poll");
        assert_eq!(json["payload"]["request_id"], "bridge-events-1");
        assert_eq!(json["payload"]["after_event_id"], "bridge-event-1");
        assert_eq!(json["payload"]["limit"], 10);
        assert_eq!(json["payload"]["timeout_ms"], 30_000);
        assert_eq!(
            serde_json::from_value::<LocalBridgeRequest>(json).unwrap(),
            request
        );
    }

    #[test]
    fn local_bridge_action_results_request_uses_stable_json_shape() {
        let request = LocalBridgeRequest::ActionResults(LocalBridgeActionResultsRequest {
            request_id: "bridge-results-1".to_string(),
            client: Some(LocalBridgeClientIdentity {
                client_id: "local-agent-app".to_string(),
                display_name: "Local Agent App".to_string(),
                app_kind: Some("agent".to_string()),
            }),
            after_claimed_at_ms: Some(1_000),
            limit: Some(10),
        });

        request.validate().unwrap();

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["kind"], "actions.results");
        assert_eq!(json["payload"]["request_id"], "bridge-results-1");
        assert_eq!(json["payload"]["after_claimed_at_ms"], 1_000);
        assert_eq!(json["payload"]["limit"], 10);
        assert_eq!(
            serde_json::from_value::<LocalBridgeRequest>(json).unwrap(),
            request
        );
    }

    #[test]
    fn local_bridge_rejects_unsafe_bundle_roots() {
        let request = LocalBridgeRequest::SendBundle(LocalBridgeSendBundleRequest {
            request_id: "bridge-request-1".to_string(),
            client: None,
            target_device_id: None,
            bundle_root: "../bundle".to_string(),
            bundle_type: BundleType::Skill,
            require_trusted_device: true,
        });

        let error = request.validate().unwrap_err();

        assert!(error.message.contains("bundle_root"));
    }

    #[test]
    fn local_bridge_bundle_received_event_uses_stable_json_shape() {
        let event = LocalBridgeEvent::BundleReceived(LocalBridgeBundleReceivedEvent {
            event_id: "bridge-event-1".to_string(),
            transfer_id: "transfer-1".to_string(),
            bundle_id: "bundle_1234567890".to_string(),
            bundle_type: BundleType::Skill,
            display_name: "voice_transcribe".to_string(),
            source_app: "Generic Agent App".to_string(),
            file_count: 2,
            total_bytes: 28,
            import_allowed: true,
        });

        event.validate().unwrap();

        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"], "bundle.received");
        assert_eq!(json["payload"]["event_id"], "bridge-event-1");
        assert_eq!(json["payload"]["bundle_type"], "skill");
        assert_eq!(
            serde_json::from_value::<LocalBridgeEvent>(json).unwrap(),
            event
        );
    }

    #[test]
    fn local_bridge_bundle_send_preflight_event_uses_stable_json_shape() {
        let event = LocalBridgeEvent::BundleSendPreflight(LocalBridgeBundleSendPreflightEvent {
            event_id: "bridge-event-send-1".to_string(),
            request_id: "bridge-send-1".to_string(),
            client_id: "local-agent-app".to_string(),
            status: LocalBridgeBundleSendPreflightStatus::FailedPreflight,
            reason: Some("bundle_root_missing".to_string()),
            bundle_id: None,
            bundle_type: Some(BundleType::Skill),
            target_device_id: Some("device-a".to_string()),
        });

        event.validate().unwrap();

        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"], "bundle.send.preflight");
        assert_eq!(json["payload"]["event_id"], "bridge-event-send-1");
        assert_eq!(json["payload"]["request_id"], "bridge-send-1");
        assert_eq!(json["payload"]["client_id"], "local-agent-app");
        assert_eq!(json["payload"]["status"], "failed_preflight");
        assert_eq!(json["payload"]["reason"], "bundle_root_missing");
        assert_eq!(json["payload"]["bundle_type"], "skill");
        assert!(json["payload"].get("bundle_root").is_none());
        assert_eq!(
            serde_json::from_value::<LocalBridgeEvent>(json).unwrap(),
            event
        );
    }

    #[test]
    fn local_bridge_action_updated_event_uses_stable_json_shape() {
        let event = LocalBridgeEvent::ActionUpdated(LocalBridgeActionUpdatedEvent {
            event_id: "bridge-action-bridge-send-1-running-2000".to_string(),
            request_id: "bridge-send-1".to_string(),
            action_kind: "bundle.send".to_string(),
            client_id: "local-agent-app".to_string(),
            status: LocalBridgeActionLifecycleStatus::Running,
            reason: None,
            message: "local bridge bundle send is running".to_string(),
            bundle_id: Some("bundle_1234567890".to_string()),
            bundle_type: Some(BundleType::Skill),
            target_device_id: Some("device-a".to_string()),
            updated_at_ms: 2_000,
        });

        event.validate().unwrap();

        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"], "action.updated");
        assert_eq!(
            json["payload"]["event_id"],
            "bridge-action-bridge-send-1-running-2000"
        );
        assert_eq!(json["payload"]["request_id"], "bridge-send-1");
        assert_eq!(json["payload"]["action_kind"], "bundle.send");
        assert_eq!(json["payload"]["client_id"], "local-agent-app");
        assert_eq!(json["payload"]["status"], "running");
        assert_eq!(json["payload"]["bundle_type"], "skill");
        assert_eq!(json["payload"]["target_device_id"], "device-a");
        assert_eq!(json["payload"]["updated_at_ms"], 2_000);
        assert!(json["payload"].get("bundle_root").is_none());
        assert_eq!(
            serde_json::from_value::<LocalBridgeEvent>(json).unwrap(),
            event
        );
    }

    #[test]
    fn documented_bundle_samples_validate_against_protocol_types() {
        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("workspace root");
        let samples_root = repo_root.join("docs").join("bundle-samples");
        for sample in [
            "skill-basic",
            "session-summary",
            "workspace-fragment",
            "agent-profile",
            "config-snapshot",
        ] {
            let sample_root = samples_root.join(sample);
            let manifest = std::fs::read_to_string(sample_root.join("bundle.json")).unwrap();
            let checksums = std::fs::read_to_string(sample_root.join("checksums.json")).unwrap();
            let permissions =
                std::fs::read_to_string(sample_root.join("permissions.json")).unwrap();

            let manifest = serde_json::from_str::<BundleManifest>(&manifest).unwrap();
            let checksums = serde_json::from_str::<BundleChecksums>(&checksums).unwrap();
            let permissions = serde_json::from_str::<BundlePermissions>(&permissions).unwrap();

            manifest.validate().unwrap();
            checksums.validate_against(&manifest).unwrap();
            assert!(permissions.can_import().unwrap());
        }
    }

    fn valid_bundle_manifest() -> BundleManifest {
        BundleManifest {
            schema: BUNDLE_SCHEMA_V1.to_string(),
            bundle_id: "bundle_1234567890".to_string(),
            bundle_type: BundleType::Skill,
            display_name: "voice_transcribe".to_string(),
            source_app: "Generic Agent App".to_string(),
            created_at: "2026-06-14T10:30:00Z".to_string(),
            sender: BundleSender {
                device_id: "neko-device-1234567890".to_string(),
                device_name: "MacBook".to_string(),
                fingerprint: "sha256:0123456789abcdef".to_string(),
            },
            compatibility: BundleCompatibility {
                min_nekolink_version: PROTOCOL_VERSION,
                required_capabilities: vec![Capability::BundleTransfer],
            },
            summary: BundleSummary {
                file_count: 2,
                total_bytes: 4096,
            },
            files: vec![
                BundleFile {
                    path: "files/manifest.json".to_string(),
                    size: 1024,
                    sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                        .to_string(),
                    role: "manifest".to_string(),
                },
                BundleFile {
                    path: "files/content.bin".to_string(),
                    size: 3072,
                    sha256: "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
                        .to_string(),
                    role: "payload".to_string(),
                },
            ],
        }
    }

    fn valid_bundle_checksums() -> BundleChecksums {
        let mut files = BTreeMap::new();
        files.insert(
            "files/manifest.json".to_string(),
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        );
        files.insert(
            "files/content.bin".to_string(),
            "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".to_string(),
        );
        BundleChecksums {
            algorithm: BUNDLE_CHECKSUM_SHA256.to_string(),
            files,
        }
    }

    fn valid_bundle_permissions(contains_secrets: bool) -> BundlePermissions {
        BundlePermissions {
            requested_scopes: vec![
                BundlePermissionScope::SkillInstall,
                BundlePermissionScope::WorkspaceImport,
            ],
            writes: vec![BundleWritePermission {
                target: "agent.skills".to_string(),
                mode: BundleWriteMode::CreateOnly,
            }],
            secrets: BundleSecretsPolicy {
                contains_secrets,
                redacted_fields: Vec::new(),
            },
        }
    }
}
