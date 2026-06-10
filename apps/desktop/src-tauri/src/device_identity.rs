use std::fs;
use std::path::PathBuf;
#[cfg(any(
    target_os = "macos",
    all(not(target_os = "macos"), not(target_os = "windows"))
))]
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use nekolink_protocol::{Capability, DeviceIdentity, DeviceKind, PlatformKind};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const IDENTITY_SCHEMA_VERSION: u16 = 1;
const SECRET_SEED_BYTES: usize = 32;

#[derive(Debug, Clone)]
pub struct LocalDeviceIdentity {
    persisted: PersistedDeviceIdentity,
}

impl LocalDeviceIdentity {
    pub fn public_identity(&self) -> DeviceIdentity {
        DeviceIdentity::new(
            self.persisted.device_id.clone(),
            self.persisted.device_name.clone(),
            self.persisted.device_kind,
            self.persisted.platform,
            self.persisted.public_key_fingerprint.clone(),
            desktop_capabilities(),
        )
    }

    pub fn device_name(&self) -> &str {
        &self.persisted.device_name
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedDeviceIdentity {
    schema_version: u16,
    device_id: String,
    device_name: String,
    device_kind: DeviceKind,
    platform: PlatformKind,
    secret_seed_hex: String,
    public_key_fingerprint: String,
    created_at_ms: u128,
}

pub fn load_or_create_device_identity() -> Result<LocalDeviceIdentity, String> {
    let path = identity_file_path()?;
    if path.exists() {
        return read_device_identity(path);
    }

    let identity = new_device_identity()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("无法创建设备身份目录 {}: {error}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(&identity)
        .map_err(|error| format!("无法序列化设备身份: {error}"))?;
    fs::write(&path, json)
        .map_err(|error| format!("无法写入设备身份文件 {}: {error}", path.display()))?;

    Ok(LocalDeviceIdentity {
        persisted: identity,
    })
}

fn read_device_identity(path: PathBuf) -> Result<LocalDeviceIdentity, String> {
    let content = fs::read_to_string(&path)
        .map_err(|error| format!("无法读取设备身份文件 {}: {error}", path.display()))?;
    let identity = serde_json::from_str::<PersistedDeviceIdentity>(&content)
        .map_err(|error| format!("设备身份文件格式无效 {}: {error}", path.display()))?;
    validate_persisted_identity(&identity)?;
    Ok(LocalDeviceIdentity {
        persisted: identity,
    })
}

fn new_device_identity() -> Result<PersistedDeviceIdentity, String> {
    let mut secret_seed = [0_u8; SECRET_SEED_BYTES];
    getrandom::fill(&mut secret_seed).map_err(|error| format!("无法生成设备密钥种子: {error}"))?;

    let secret_seed_hex = hex::encode(secret_seed);
    let public_key_fingerprint = fingerprint_for_seed(&secret_seed);
    let id_suffix = public_key_fingerprint
        .strip_prefix("sha256:")
        .unwrap_or(&public_key_fingerprint)
        .chars()
        .take(16)
        .collect::<String>();

    Ok(PersistedDeviceIdentity {
        schema_version: IDENTITY_SCHEMA_VERSION,
        device_id: format!("neko-device-{id_suffix}"),
        device_name: default_device_name(),
        device_kind: DeviceKind::Desktop,
        platform: current_platform(),
        secret_seed_hex,
        public_key_fingerprint,
        created_at_ms: now_ms(),
    })
}

fn validate_persisted_identity(identity: &PersistedDeviceIdentity) -> Result<(), String> {
    if identity.schema_version != IDENTITY_SCHEMA_VERSION {
        return Err(format!("不支持的设备身份版本: {}", identity.schema_version));
    }
    if identity.device_id.trim().is_empty() {
        return Err("设备身份缺少 device_id".to_string());
    }
    if identity.device_name.trim().is_empty() {
        return Err("设备身份缺少 device_name".to_string());
    }
    if identity.secret_seed_hex.len() != SECRET_SEED_BYTES * 2 {
        return Err("设备身份密钥种子长度无效".to_string());
    }
    if identity.public_key_fingerprint.trim().is_empty() {
        return Err("设备身份缺少 public_key_fingerprint".to_string());
    }
    Ok(())
}

fn identity_file_path() -> Result<PathBuf, String> {
    let base = config_base_dir()?;
    Ok(base.join("NekoDrop").join("device_identity.json"))
}

#[cfg(target_os = "macos")]
fn config_base_dir() -> Result<PathBuf, String> {
    home_dir()
        .map(|home| home.join("Library").join("Application Support"))
        .ok_or_else(|| "无法定位用户目录，不能保存设备身份".to_string())
}

#[cfg(target_os = "windows")]
fn config_base_dir() -> Result<PathBuf, String> {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "无法定位 APPDATA，不能保存设备身份".to_string())
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn config_base_dir() -> Result<PathBuf, String> {
    if let Some(value) = std::env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(value));
    }
    home_dir()
        .map(|home| home.join(".config"))
        .ok_or_else(|| "无法定位用户配置目录，不能保存设备身份".to_string())
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn default_device_name() -> String {
    system_device_name().unwrap_or_else(|| match current_platform() {
        PlatformKind::Macos => "Mac".to_string(),
        PlatformKind::Windows => "Windows PC".to_string(),
        PlatformKind::Linux => "Linux PC".to_string(),
        _ => "这台电脑".to_string(),
    })
}

#[cfg(target_os = "macos")]
fn system_device_name() -> Option<String> {
    command_output("/usr/sbin/scutil", &["--get", "ComputerName"])
        .or_else(|| command_output("/bin/hostname", &[]))
}

#[cfg(target_os = "windows")]
fn system_device_name() -> Option<String> {
    std::env::var("COMPUTERNAME")
        .ok()
        .and_then(non_empty_string)
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn system_device_name() -> Option<String> {
    std::env::var("HOSTNAME")
        .ok()
        .and_then(non_empty_string)
        .or_else(|| command_output("/bin/hostname", &[]))
}

#[cfg(any(
    target_os = "macos",
    all(not(target_os = "macos"), not(target_os = "windows"))
))]
fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .and_then(non_empty_string)
}

fn non_empty_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn current_platform() -> PlatformKind {
    match std::env::consts::OS {
        "macos" => PlatformKind::Macos,
        "windows" => PlatformKind::Windows,
        "linux" => PlatformKind::Linux,
        _ => PlatformKind::Unknown,
    }
}

fn desktop_capabilities() -> Vec<Capability> {
    vec![
        Capability::FileTransfer,
        Capability::FileSend,
        Capability::FileReceive,
        Capability::FileSha256,
        Capability::DevicePairing,
        Capability::EncryptedSession,
        Capability::DesktopAgentHost,
    ]
}

fn fingerprint_for_seed(secret_seed: &[u8; SECRET_SEED_BYTES]) -> String {
    let digest = Sha256::digest(secret_seed);
    format!("sha256:{}", hex::encode(digest))
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
    fn creates_stable_public_identity_shape() {
        let identity = new_device_identity().unwrap();
        let public = LocalDeviceIdentity {
            persisted: identity,
        }
        .public_identity();

        assert!(public.device_id.starts_with("neko-device-"));
        assert_eq!(public.device_kind, DeviceKind::Desktop);
        assert!(public.public_key_fingerprint.starts_with("sha256:"));
        assert!(public.capabilities.contains(&Capability::FileTransfer));
        assert!(public.capabilities.contains(&Capability::DevicePairing));
    }
}
