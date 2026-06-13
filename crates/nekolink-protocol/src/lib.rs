use std::time::{SystemTime, UNIX_EPOCH};

use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const PROTOCOL_NAME: &str = "nekolink";
pub const PROTOCOL_VERSION: u16 = 1;
pub const SESSION_KEY_AGREEMENT_X25519: &str = "x25519";
pub const SESSION_CIPHER_XCHACHA20POLY1305: &str = "xchacha20poly1305";
pub const SESSION_CIPHER_AES256GCM: &str = "aes256gcm";
pub const SESSION_SHARED_SECRET_LEN: usize = 32;
pub const SESSION_TRAFFIC_KEY_LEN: usize = 32;

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
}
