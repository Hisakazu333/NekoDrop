use nekodrop_core::Device;
use nekolink_protocol::DeviceIdentity;

use super::{DeviceDto, DeviceIdentityDto, DiscoveryStatusDto, TrustedDeviceDto};
use crate::app_state::AppState;
use crate::trusted_devices::{
    pairing_code_for_device, trusted_record_matches, TrustedDeviceRecord,
};

pub(super) fn discovery_status_snapshot(
    state: &AppState,
    device_count: usize,
) -> Result<DiscoveryStatusDto, String> {
    let status = state
        .discovery_status
        .lock()
        .map_err(|error| error.to_string())?;

    Ok(DiscoveryStatusDto {
        phase: status.phase.clone(),
        message: status.message.clone(),
        service_type: status.service_type.clone(),
        advertised: status.advertised,
        lan_ip: status.lan_ip.clone(),
        port: status.port,
        device_count,
        last_seen_seconds_ago: status
            .last_seen_at
            .map(|seen_at| seen_at.elapsed().as_secs()),
        last_error: status.last_error.clone(),
    })
}

pub(super) fn device_to_dto(
    device: &Device,
    local_identity: &DeviceIdentity,
    trusted_devices: &[TrustedDeviceRecord],
) -> DeviceDto {
    let is_trusted = trusted_devices
        .iter()
        .any(|record| trusted_record_matches(device, record));
    DeviceDto {
        id: device.id.as_str().to_string(),
        name: device.name.clone(),
        platform: format!("{:?}", device.platform),
        host: device.host.clone(),
        port: device.port,
        trust_state: if is_trusted {
            "Trusted".to_string()
        } else {
            format!("{:?}", device.trust_state)
        },
        public_key: device.public_key.clone(),
        public_key_fingerprint: device.public_key_fingerprint.clone(),
        pairing_code: pairing_code_for_device(local_identity, device),
    }
}

pub(super) fn trusted_device_to_dto(device: &TrustedDeviceRecord) -> TrustedDeviceDto {
    TrustedDeviceDto {
        device_id: device.device_id.clone(),
        device_name: device.device_name.clone(),
        platform: device.platform.clone(),
        host: device.host.clone(),
        port: device.port,
        public_key: device.public_key.clone(),
        public_key_fingerprint: device.public_key_fingerprint.clone(),
        pairing_code: device.pairing_code.clone(),
        paired_at_ms: device.paired_at_ms,
        last_seen_at_ms: device.last_seen_at_ms,
    }
}

pub(super) fn device_identity_to_dto(identity: &DeviceIdentity) -> DeviceIdentityDto {
    DeviceIdentityDto {
        device_id: identity.device_id.clone(),
        device_name: identity.device_name.clone(),
        device_kind: identity.device_kind.as_str().to_string(),
        platform: identity.platform.as_str().to_string(),
        public_key_fingerprint: identity.public_key_fingerprint.clone(),
        capabilities: identity
            .capabilities
            .iter()
            .map(|capability| capability.as_str().to_string())
            .collect(),
    }
}
