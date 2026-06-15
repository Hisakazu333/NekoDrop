use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use nekodrop_core::{Device, DevicePlatform};
use nekolink_protocol::{DeviceIdentity, DeviceIdentityPublicKey};
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
    pub public_key: String,
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
    let mut records = records;
    retain_usable_trusted_devices(&mut records);
    normalize_trusted_devices(&mut records);
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
    let Some(public_key) = device.public_key.clone() else {
        return Err("这个设备缺少长期公钥，当前不能加入可信设备。".to_string());
    };
    validate_public_key_pair(&public_key, &public_key_fingerprint)?;
    let now = now_ms();
    Ok(TrustedDeviceRecord {
        schema_version: TRUSTED_DEVICES_SCHEMA_VERSION,
        device_id: device.id.as_str().to_string(),
        device_name: device.name.clone(),
        platform: platform_to_string(device.platform).to_string(),
        host: device.host.clone(),
        port: device.port,
        public_key,
        pairing_code: pairing_code_for_values(
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
    Some(pairing_code_for_values(
        &local_identity.device_id,
        &local_identity.public_key_fingerprint,
        device.id.as_str(),
        fingerprint,
    ))
}

pub fn trusted_device_record_from_remote(
    local_identity: &DeviceIdentity,
    device_id: String,
    device_name: String,
    platform: String,
    host: String,
    port: u16,
    public_key: String,
    public_key_fingerprint: String,
) -> Result<TrustedDeviceRecord, String> {
    validate_public_key_pair(&public_key, &public_key_fingerprint)?;
    let now = now_ms();
    Ok(TrustedDeviceRecord {
        schema_version: TRUSTED_DEVICES_SCHEMA_VERSION,
        pairing_code: pairing_code_for_values(
            &local_identity.device_id,
            &local_identity.public_key_fingerprint,
            &device_id,
            &public_key_fingerprint,
        ),
        device_id,
        device_name,
        platform,
        host,
        port,
        public_key,
        public_key_fingerprint,
        paired_at_ms: now,
        last_seen_at_ms: now,
    })
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
    normalize_trusted_devices(records);
}

pub fn refresh_trusted_device_contact(
    records: &mut Vec<TrustedDeviceRecord>,
    device_id: &str,
    public_key: &str,
    public_key_fingerprint: &str,
    device_name: Option<&str>,
    seen_at_ms: u128,
) -> bool {
    let Some(record) = records.iter_mut().find(|record| {
        trusted_record_matches_identity(device_id, public_key, public_key_fingerprint, record)
    }) else {
        return false;
    };

    let mut changed = false;
    if record.last_seen_at_ms < seen_at_ms {
        record.last_seen_at_ms = seen_at_ms;
        changed = true;
    }

    if let Some(device_name) = device_name.map(str::trim).filter(|value| !value.is_empty()) {
        if record.device_name != device_name {
            record.device_name = device_name.to_string();
            changed = true;
        }
    }

    if changed {
        normalize_trusted_devices(records);
    }
    changed
}

fn normalize_trusted_devices(records: &mut Vec<TrustedDeviceRecord>) {
    sort_trusted_devices(records);
    let mut seen_device_ids = HashSet::new();
    records.retain(|record| seen_device_ids.insert(record.device_id.clone()));
}

fn retain_usable_trusted_devices(records: &mut Vec<TrustedDeviceRecord>) {
    records.retain(|record| validate_trusted_device(record).is_ok());
}

fn sort_trusted_devices(records: &mut [TrustedDeviceRecord]) {
    records.sort_by(|left, right| {
        right
            .last_seen_at_ms
            .cmp(&left.last_seen_at_ms)
            .then_with(|| right.paired_at_ms.cmp(&left.paired_at_ms))
            .then_with(|| left.device_name.cmp(&right.device_name))
            .then_with(|| left.device_id.cmp(&right.device_id))
    });
}

pub fn trusted_record_matches(device: &Device, record: &TrustedDeviceRecord) -> bool {
    device
        .public_key
        .as_ref()
        .zip(device.public_key_fingerprint.as_ref())
        .is_some_and(|(public_key, fingerprint)| {
            trusted_record_matches_identity(device.id.as_str(), public_key, fingerprint, record)
        })
}

pub fn trusted_record_matches_identity(
    device_id: &str,
    public_key: &str,
    public_key_fingerprint: &str,
    record: &TrustedDeviceRecord,
) -> bool {
    record.device_id == device_id
        && record.public_key == public_key
        && record.public_key_fingerprint == public_key_fingerprint
}

fn validate_trusted_device(record: &TrustedDeviceRecord) -> Result<(), String> {
    if record.schema_version != TRUSTED_DEVICES_SCHEMA_VERSION {
        return Err(format!("不支持的可信设备版本: {}", record.schema_version));
    }
    if record.device_id.trim().is_empty() {
        return Err("可信设备缺少 device_id".to_string());
    }
    if record.public_key.trim().is_empty() {
        return Err("可信设备缺少 public_key".to_string());
    }
    validate_public_key_pair(&record.public_key, &record.public_key_fingerprint)?;
    if record.pairing_code.trim().is_empty() {
        return Err("可信设备缺少 pairing_code".to_string());
    }
    Ok(())
}

fn validate_public_key_pair(public_key: &str, fingerprint: &str) -> Result<(), String> {
    let public_key = DeviceIdentityPublicKey::from_encoded(public_key)
        .map_err(|error| format!("{:?}: {}", error.code, error.message))?;
    if public_key.fingerprint != fingerprint {
        return Err("可信设备 public_key_fingerprint 与 public_key 不匹配".to_string());
    }
    Ok(())
}

fn trusted_devices_file_path() -> Result<PathBuf, String> {
    Ok(app_config_dir()?.join("trusted_devices.json"))
}

pub fn pairing_code_for_values(
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
        let first = pairing_code_for_values("a", "sha256:111", "b", "sha256:222");
        let second = pairing_code_for_values("b", "sha256:222", "a", "sha256:111");

        assert_eq!(first, second);
        assert_eq!(first.len(), 7);
    }

    #[test]
    fn sorts_trusted_devices_by_recent_activity() {
        let mut records = vec![
            trusted_record("old", "Old Mac", 100),
            trusted_record("new", "New PC", 300),
            trusted_record("middle", "Middle Laptop", 200),
        ];

        sort_trusted_devices(&mut records);

        assert_eq!(records[0].device_id, "new");
        assert_eq!(records[1].device_id, "middle");
        assert_eq!(records[2].device_id, "old");
    }

    #[test]
    fn normalizes_duplicate_device_ids_to_latest_record() {
        let mut records = vec![
            trusted_record("device-a", "Old Mac", 100),
            trusted_record("device-a", "New Mac", 300),
            trusted_record("device-b", "Windows", 200),
        ];

        normalize_trusted_devices(&mut records);

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].device_id, "device-a");
        assert_eq!(records[0].device_name, "New Mac");
        assert_eq!(
            records[0].public_key_fingerprint,
            test_public_key("device-a").fingerprint
        );
        assert_eq!(records[1].device_id, "device-b");
    }

    #[test]
    fn refreshes_contact_only_for_matching_identity() {
        let mut records = vec![
            trusted_record("device-a", "Old Name", 100),
            trusted_record("device-b", "Other", 200),
        ];

        let changed = refresh_trusted_device_contact(
            &mut records,
            "device-a",
            &test_public_key("device-a").public_key,
            &test_public_key("device-a").fingerprint,
            Some("New Name"),
            300,
        );

        assert!(changed);
        assert_eq!(records[0].device_id, "device-a");
        assert_eq!(records[0].device_name, "New Name");
        assert_eq!(records[0].last_seen_at_ms, 300);

        let rejected = refresh_trusted_device_contact(
            &mut records,
            "device-a",
            &test_public_key("device-a").public_key,
            &test_public_key("device-b").fingerprint,
            Some("Wrong"),
            400,
        );

        assert!(!rejected);
        assert_eq!(records[0].device_name, "New Name");
        assert_eq!(records[0].last_seen_at_ms, 300);
    }

    #[test]
    fn trusted_records_require_public_key_and_match_it() {
        let mut device = Device::new(
            nekodrop_core::DeviceId::new("device-a").unwrap(),
            "MacBook",
            DevicePlatform::MacOS,
            "192.168.1.20",
            45821,
        )
        .unwrap();
        let device_key = test_public_key("device-a");
        let other_key = test_public_key("device-b");
        device.public_key_fingerprint = Some(device_key.fingerprint.clone());
        device.public_key = Some(device_key.public_key.clone());
        let record = trusted_record("device-a", "MacBook", 100);

        assert!(trusted_record_matches(&device, &record));

        device.public_key = Some(other_key.public_key);

        assert!(!trusted_record_matches(&device, &record));

        let mut legacy = record;
        legacy.public_key.clear();
        assert!(validate_trusted_device(&legacy).is_err());
    }

    #[test]
    fn drops_unusable_legacy_records_before_normalizing() {
        let mut records = vec![
            trusted_record("device-a", "MacBook", 100),
            trusted_record("device-b", "Old Windows", 200),
        ];
        records[1].public_key.clear();

        retain_usable_trusted_devices(&mut records);

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].device_id, "device-a");
    }

    fn trusted_record(
        device_id: impl Into<String>,
        device_name: impl Into<String>,
        last_seen_at_ms: u128,
    ) -> TrustedDeviceRecord {
        let device_id = device_id.into();
        let public_key = test_public_key(&device_id);
        TrustedDeviceRecord {
            schema_version: TRUSTED_DEVICES_SCHEMA_VERSION,
            device_name: device_name.into(),
            platform: "macos".to_string(),
            host: "192.168.1.20".to_string(),
            port: 45821,
            public_key: public_key.public_key,
            public_key_fingerprint: public_key.fingerprint,
            pairing_code: "ABC-123".to_string(),
            paired_at_ms: 1,
            last_seen_at_ms,
            device_id,
        }
    }

    fn test_public_key(label: &str) -> DeviceIdentityPublicKey {
        let mut seed = [0_u8; nekolink_protocol::DEVICE_IDENTITY_SIGNING_KEY_LEN];
        for (index, byte) in label.as_bytes().iter().enumerate() {
            seed[index % seed.len()] ^= *byte;
        }
        nekolink_protocol::DeviceIdentitySigningKey::from_seed(seed).public_key()
    }
}
