use nekodrop_core::{FileManifest, PairingRequest, TransferId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolVersion(pub u16);

impl Default for ProtocolVersion {
    fn default() -> Self {
        Self(1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolMessage {
    PairRequest(PairingRequest),
    PairAccepted {
        short_code: String,
    },
    PairRejected {
        reason: String,
    },
    SendOffer {
        transfer_id: TransferId,
        manifest: FileManifest,
    },
    SendAccepted {
        transfer_id: TransferId,
        resume_token: Option<String>,
    },
    SendDeclined {
        transfer_id: TransferId,
        reason: String,
    },
    FileComplete {
        transfer_id: TransferId,
        path: String,
        sha256: String,
    },
    TransferComplete {
        transfer_id: TransferId,
        verified: bool,
    },
    Cancel {
        transfer_id: TransferId,
        reason: String,
    },
}

impl ProtocolMessage {
    pub fn message_type(&self) -> &'static str {
        match self {
            Self::PairRequest(_) => "PAIR_REQ",
            Self::PairAccepted { .. } => "PAIR_ACK",
            Self::PairRejected { .. } => "PAIR_REJECT",
            Self::SendOffer { .. } => "SEND_OFFER",
            Self::SendAccepted { .. } => "SEND_ACCEPT",
            Self::SendDeclined { .. } => "SEND_DECLINE",
            Self::FileComplete { .. } => "FILE_COMPLETE",
            Self::TransferComplete { .. } => "TRANSFER_COMPLETE",
            Self::Cancel { .. } => "CANCEL",
        }
    }
}
