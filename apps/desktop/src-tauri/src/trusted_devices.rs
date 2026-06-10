use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use nekodrop_core::{Device, DevicePlatform};
use nekolink_protocol::DeviceIdentity;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::device_identity::app_config_dir;

const TRUSTED_DEVICES_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedDeviceRecord {
    pub schema_version: u16,
    pub device_id: String,
    pub device_name: String,
    pub platform: String,
    pub host: String,
    pub port: u16,
    pub public_key_fingerprint: String,
    pub pairing_code: String,
    pub paired_at_ms: u128,
    pub last_seen_at_ms: u128,
}

pub fn load_trusted_devices() -> Result<Vec<TrustedDeviceRecord>, String> {
    let path = trusted_devices_file_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("无法读取可信设备文件 {}: {error}", path.display()))?;
    let records = serde_json::from_str::<Vec<TrustedDeviceRecord>>(&content)
        .map_err(|error| format!("可信设备文件格式无效 {}: {error}", path.display()))?;
    for record in &records {
        validate_trusted_device(record)?;
    }
    Ok(records)
}

pub fn save_trusted_devices(records: &[TrustedDeviceRecord]) -> Result<(), String> {
    let path = trusted_devices_file_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("无法创建可信设备目录 {}: {error}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(records)
        .map_err(|error| format!("无法序列化可信设备: {error}"))?;
    fs::write(&path, json)
        .map_err(|error| format!("无法写入可信设备文件 {}: {error}", path.display()))
}

pub fn trust_device_record(
    local_identity: &DeviceIdentity,
    device: &Device,
) -> Result<TrustedDeviceRecord, String> {
    let Some(public_key_fingerprint) = device.public_key_fingerprint.clone() else {
        return Err("这个设备缺少公开指纹，当前不能加入可信设备。".to_string());
    };
    let now = now_ms();
    Ok(TrustedDeviceRecord {
        schema_version: TRUSTED_DEVICES_SCHEMA_VERSION,
        device_id: device.id.as_str().to_string(),
        device_name: device.name.clone(),
        platform: platform_to_string(device.platform).to_string(),
        host: device.host.clone(),
        port: device.port,
        pairing_code: pairing_code_for(
            &local_identity.device_id,
            &local_identity.public_key_fingerprint,
            device.id.as_str(),
            &public_key_fingerprint,
        ),
        public_key_fingerprint,
        paired_at_ms: now,
        last_seen_at_ms: now,
    })
}

pub fn pairing_code_for_device(local_identity: &DeviceIdentity, device: &Device) -> Option<String> {
    let fingerprint = device.public_key_fingerprint.as_ref()?;
    Some(pairing_code_for(
        &local_identity.device_id,
        &local_identity.public_key_fingerprint,
        device.id.as_str(),
        fingerprint,
    ))
}

pub fn upsert_trusted_device(
    records: &mut Vec<TrustedDeviceRecord>,
    next_record: TrustedDeviceRecord,
) {
    if let Some(record) = records
        .iter_mut()
        .find(|record| record.device_id == next_record.device_id)
    {
        *record = next_record;
    } else {
        records.push(next_record);
    }
}

pub fn trusted_record_matches(device: &Device, record: &TrustedDeviceRecord) -> bool {
    record.device_id == device.id.as_str()
        && device
            .public_key_fingerprint
            .as_ref()
            .is_some_and(|fingerprint| fingerprint == &record.public_key_fingerprint)
}

fn validate_trusted_device(record: &TrustedDeviceRecord) -> Result<(), String> {
    if record.schema_version != TRUSTED_DEVICES_SCHEMA_VERSION {
        return Err(format!("不支持的可信设备版本: {}", record.schema_version));
    }
    if record.device_id.trim().is_empty() {
        return Err("可信设备缺少 device_id".to_string());
    }
    if record.public_key_fingerprint.trim().is_empty() {
        return Err("可信设备缺少 public_key_fingerprint".to_string());
    }
    if record.pairing_code.trim().is_empty() {
        return Err("可信设备缺少 pairing_code".to_string());
    }
    Ok(())
}

fn trusted_devices_file_path() -> Result<PathBuf, String> {
    Ok(app_config_dir()?.join("trusted_devices.json"))
}

fn pairing_code_for(
    local_device_id: &str,
    local_fingerprint: &str,
    remote_device_id: &str,
    remote_fingerprint: &str,
) -> String {
    let mut parts = [
        format!("{local_device_id}|{local_fingerprint}"),
        format!("{remote_device_id}|{remote_fingerprint}"),
    ];
    parts.sort();
    let digest = Sha256::digest(parts.join("\n").as_bytes());
    let hex = hex::encode(digest);
    format!(
        "{}-{}",
        hex[..3].to_ascii_uppercase(),
        hex[3..6].to_ascii_uppercase()
    )
}

fn platform_to_string(platform: DevicePlatform) -> &'static str {
    match platform {
        DevicePlatform::MacOS => "macos",
        DevicePlatform::Windows => "windows",
        DevicePlatform::Linux => "linux",
        DevicePlatform::Unknown => "unknown",
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
    fn pairing_code_is_symmetric() {
        let first = pairing_code_for("a", "sha256:111", "b", "sha256:222");
        let second = pairing_code_for("b", "sha256:222", "a", "sha256:111");

        assert_eq!(first, second);
        assert_eq!(first.len(), 7);
    }
}
