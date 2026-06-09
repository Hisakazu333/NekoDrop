use crate::device::DeviceId;
use crate::errors::{NekoDropError, NekoDropResult};
use crate::manifest::FileManifest;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TransferId(String);

impl TransferId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferDirection {
    Send,
    Receive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferStatus {
    Draft,
    Offered,
    AwaitingApproval,
    Transferring,
    Paused,
    Verifying,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferJob {
    pub id: TransferId,
    pub direction: TransferDirection,
    pub peer_device_id: DeviceId,
    pub manifest: FileManifest,
    pub status: TransferStatus,
    pub transferred_bytes: u64,
}

impl TransferJob {
    pub fn new(
        id: TransferId,
        direction: TransferDirection,
        peer_device_id: DeviceId,
        manifest: FileManifest,
    ) -> Self {
        Self {
            id,
            direction,
            peer_device_id,
            manifest,
            status: TransferStatus::Draft,
            transferred_bytes: 0,
        }
    }

    pub fn progress(&self) -> f32 {
        let total = self.manifest.total_bytes();
        if total == 0 {
            return 0.0;
        }

        (self.transferred_bytes as f32 / total as f32).clamp(0.0, 1.0)
    }

    pub fn mark_transferring(&mut self) -> NekoDropResult<()> {
        if matches!(
            self.status,
            TransferStatus::Completed | TransferStatus::Failed | TransferStatus::Cancelled
        ) {
            return Err(NekoDropError::TransferAlreadyFinished);
        }

        self.status = TransferStatus::Transferring;
        Ok(())
    }
}
