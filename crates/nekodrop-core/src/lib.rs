pub mod config;
pub mod device;
pub mod errors;
pub mod manifest;
pub mod pairing;
pub mod transfer;

pub use config::{AppConfig, ReceivePolicy};
pub use device::{Device, DeviceId, DevicePlatform, DeviceTrustState, TrustedDevice};
pub use errors::{NekoDropError, NekoDropResult};
pub use manifest::{FileManifest, ManifestItem, ManifestItemKind};
pub use pairing::{PairingRequest, PairingState};
pub use transfer::{TransferDirection, TransferId, TransferJob, TransferStatus};

/// Unix 纪元以来的毫秒数；时钟异常时返回 0。
pub fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}
