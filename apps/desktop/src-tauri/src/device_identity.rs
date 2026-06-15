use std::fs;
use std::path::PathBuf;
#[cfg(any(
    target_os = "macos",
    all(not(target_os = "macos"), not(target_os = "windows"))
))]
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use nekolink_protocol::{
    Capability, DeviceIdentity, DeviceIdentitySigningKey, DeviceKind, PlatformKind,
    SessionIdentityBinding, SignedSessionIdentityBinding, DEVICE_IDENTITY_SIGNING_KEY_LEN,
};
use serde::{Deserialize, Serialize};

const IDENTITY_SCHEMA_VERSION: u16 = 2;
const LEGACY_IDENTITY_SCHEMA_VERSION: u16 = 1;
const SECRET_SEED_BYTES: usize = 32;

#[derive(Debug, Clone)]
pub struct LocalDeviceIdentity {
    persisted: Arc<Mutex<PersistedDeviceIdentity>>,
}

impl LocalDeviceIdentity {
    pub fn public_identity(&self) -> DeviceIdentity {
        let persisted = self
            .persisted
            .lock()
            .expect("device identity lock poisoned");
        DeviceIdentity::new(
            persisted.device_id.clone(),
            persisted.device_name.clone(),
            persisted.device_kind,
            persisted.platform,
            persisted.public_key_fingerprint.clone(),
            desktop_capabilities(),
        )
    }

    pub fn device_name(&self) -> String {
        self.persisted
            .lock()
            .expect("device identity lock poisoned")
            .device_name
            .clone()
    }

    pub fn public_key(&self) -> Result<String, String> {
        let signing_key = {
            let persisted = self.persisted.lock().map_err(|error| error.to_string())?;
            signing_key_from_seed_hex(&persisted.signing_seed_hex)?
        };
        Ok(signing_key.public_key().public_key)
    }

    pub fn set_device_name(&self, device_name: &str) -> Result<String, String> {
        let device_name = normalize_device_name(device_name)?;
        let mut persisted = self.persisted.lock().map_err(|error| error.to_string())?;
        persisted.device_name = device_name.clone();
        Ok(device_name)
    }

    pub fn save_device_name(&self, device_name: &str) -> Result<String, String> {
        let device_name = normalize_device_name(device_name)?;
        let next_identity = {
            let persisted = self.persisted.lock().map_err(|error| error.to_string())?;
            if persisted.device_name == device_name {
                return Ok(device_name);
            }
            let mut next_identity = persisted.clone();
            next_identity.device_name = device_name.clone();
            validate_persisted_identity(&next_identity)?;
            next_identity
        };

        save_persisted_identity(&next_identity)?;
        let mut persisted = self.persisted.lock().map_err(|error| error.to_string())?;
        *persisted = next_identity;
        Ok(device_name)
    }

    pub fn sign_session_identity_binding(
        &self,
        binding: SessionIdentityBinding,
    ) -> Result<SignedSessionIdentityBinding, String> {
        let signing_key = {
            let persisted = self.persisted.lock().map_err(|error| error.to_string())?;
            signing_key_from_seed_hex(&persisted.signing_seed_hex)?
        };
        SignedSessionIdentityBinding::sign(binding, &signing_key).map_err(|error| error.message)
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
    #[serde(default)]
    signing_seed_hex: String,
    public_key_fingerprint: String,
    created_at_ms: u128,
}

pub fn load_or_create_device_identity() -> Result<LocalDeviceIdentity, String> {
    let path = identity_file_path()?;
    if path.exists() {
        return read_device_identity(path);
    }

    let identity = new_device_identity()?;
    write_persisted_identity(&path, &identity)?;

    Ok(LocalDeviceIdentity {
        persisted: Arc::new(Mutex::new(identity)),
    })
}

fn read_device_identity(path: PathBuf) -> Result<LocalDeviceIdentity, String> {
    let content = fs::read_to_string(&path)
        .map_err(|error| format!("无法读取设备身份文件 {}: {error}", path.display()))?;
    let identity = serde_json::from_str::<PersistedDeviceIdentity>(&content)
        .map_err(|error| format!("设备身份文件格式无效 {}: {error}", path.display()))?;
    let original_schema_version = identity.schema_version;
    let identity = migrate_persisted_identity(identity)?;
    validate_persisted_identity(&identity)?;
    if original_schema_version != IDENTITY_SCHEMA_VERSION {
        write_persisted_identity(&path, &identity)?;
    }
    Ok(LocalDeviceIdentity {
        persisted: Arc::new(Mutex::new(identity)),
    })
}

fn new_device_identity() -> Result<PersistedDeviceIdentity, String> {
    let mut secret_seed = [0_u8; SECRET_SEED_BYTES];
    getrandom::fill(&mut secret_seed).map_err(|error| format!("无法生成设备密钥种子: {error}"))?;
    let mut signing_seed = [0_u8; DEVICE_IDENTITY_SIGNING_KEY_LEN];
    getrandom::fill(&mut signing_seed)
        .map_err(|error| format!("无法生成设备身份签名密钥: {error}"))?;

    let secret_seed_hex = hex::encode(secret_seed);
    let signing_seed_hex = hex::encode(signing_seed);
    let signing_key = DeviceIdentitySigningKey::from_seed(signing_seed);
    let public_key_fingerprint = signing_key.public_key_fingerprint();
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
        signing_seed_hex,
        public_key_fingerprint,
        created_at_ms: now_ms(),
    })
}

fn migrate_persisted_identity(
    mut identity: PersistedDeviceIdentity,
) -> Result<PersistedDeviceIdentity, String> {
    if identity.schema_version == IDENTITY_SCHEMA_VERSION {
        return Ok(identity);
    }
    if identity.schema_version != LEGACY_IDENTITY_SCHEMA_VERSION {
        return Err(format!("不支持的设备身份版本: {}", identity.schema_version));
    }

    let mut signing_seed = [0_u8; DEVICE_IDENTITY_SIGNING_KEY_LEN];
    getrandom::fill(&mut signing_seed)
        .map_err(|error| format!("无法迁移设备身份签名密钥: {error}"))?;
    let signing_key = DeviceIdentitySigningKey::from_seed(signing_seed);
    identity.schema_version = IDENTITY_SCHEMA_VERSION;
    identity.signing_seed_hex = hex::encode(signing_seed);
    identity.public_key_fingerprint = signing_key.public_key_fingerprint();
    Ok(identity)
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
    if identity.signing_seed_hex.len() != DEVICE_IDENTITY_SIGNING_KEY_LEN * 2 {
        return Err("设备身份签名密钥长度无效".to_string());
    }
    if identity.public_key_fingerprint.trim().is_empty() {
        return Err("设备身份缺少 public_key_fingerprint".to_string());
    }
    let signing_key = signing_key_from_seed_hex(&identity.signing_seed_hex)?;
    if identity.public_key_fingerprint != signing_key.public_key_fingerprint() {
        return Err("设备身份签名密钥与 fingerprint 不匹配".to_string());
    }
    Ok(())
}

fn normalize_device_name(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("设备名不能为空".to_string());
    }
    Ok(value.to_string())
}

fn identity_file_path() -> Result<PathBuf, String> {
    Ok(app_config_dir()?.join("device_identity.json"))
}

fn save_persisted_identity(identity: &PersistedDeviceIdentity) -> Result<(), String> {
    write_persisted_identity(&identity_file_path()?, identity)
}

fn write_persisted_identity(
    path: &PathBuf,
    identity: &PersistedDeviceIdentity,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("无法创建设备身份目录 {}: {error}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(identity)
        .map_err(|error| format!("无法序列化设备身份: {error}"))?;
    fs::write(path, json)
        .map_err(|error| format!("无法写入设备身份文件 {}: {error}", path.display()))
}

pub fn app_config_dir() -> Result<PathBuf, String> {
    Ok(config_base_dir()?.join("NekoDrop"))
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
    ]
}

fn signing_key_from_seed_hex(value: &str) -> Result<DeviceIdentitySigningKey, String> {
    let decoded =
        hex::decode(value).map_err(|error| format!("设备身份签名密钥不是 hex: {error}"))?;
    let seed: [u8; DEVICE_IDENTITY_SIGNING_KEY_LEN] = decoded
        .try_into()
        .map_err(|_| "设备身份签名密钥长度无效".to_string())?;
    Ok(DeviceIdentitySigningKey::from_seed(seed))
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
            persisted: Arc::new(Mutex::new(identity)),
        }
        .public_identity();

        assert!(public.device_id.starts_with("neko-device-"));
        assert_eq!(public.device_kind, DeviceKind::Desktop);
        assert!(public.public_key_fingerprint.starts_with("sha256:"));
        assert!(public.capabilities.contains(&Capability::FileTransfer));
        assert!(public.capabilities.contains(&Capability::DevicePairing));
    }

    #[test]
    fn local_identity_can_sign_session_identity_binding() {
        let identity = LocalDeviceIdentity {
            persisted: Arc::new(Mutex::new(new_device_identity().unwrap())),
        };
        let public = identity.public_identity();
        let binding = nekolink_protocol::SessionIdentityBinding::new(
            nekolink_protocol::SessionParticipantRole::Initiator,
            "session-1",
            &public,
            "base64-local-key",
            format!("sha256:{}", "1".repeat(64)),
        )
        .unwrap();

        let signed = identity
            .sign_session_identity_binding(binding.clone())
            .unwrap();

        assert_eq!(signed.public_key_fingerprint, public.public_key_fingerprint);
        signed.verify(&binding).unwrap();
    }

    #[test]
    fn migrates_legacy_identity_to_signing_key_schema() {
        let mut legacy = new_device_identity().unwrap();
        legacy.schema_version = LEGACY_IDENTITY_SCHEMA_VERSION;
        legacy.signing_seed_hex = String::new();

        let migrated = migrate_persisted_identity(legacy).unwrap();

        assert_eq!(migrated.schema_version, IDENTITY_SCHEMA_VERSION);
        assert_eq!(
            migrated.signing_seed_hex.len(),
            DEVICE_IDENTITY_SIGNING_KEY_LEN * 2
        );
        validate_persisted_identity(&migrated).unwrap();
    }

    #[test]
    fn desktop_identity_advertises_implemented_desktop_capabilities() {
        let capabilities = desktop_capabilities();

        assert!(capabilities.contains(&Capability::EncryptedSession));
        assert!(!capabilities.contains(&Capability::DesktopAgentHost));
    }

    #[test]
    fn updates_device_name_for_future_public_identity() {
        let identity = LocalDeviceIdentity {
            persisted: Arc::new(Mutex::new(new_device_identity().unwrap())),
        };
        let original_id = identity.public_identity().device_id;

        let saved_name = identity.set_device_name("  Work Mac  ").unwrap();
        let public = identity.public_identity();

        assert_eq!(saved_name, "Work Mac");
        assert_eq!(public.device_name, "Work Mac");
        assert_eq!(public.device_id, original_id);
    }

    #[test]
    fn rejects_empty_device_name_updates() {
        let identity = LocalDeviceIdentity {
            persisted: Arc::new(Mutex::new(new_device_identity().unwrap())),
        };

        assert!(identity.set_device_name("  \n  ").is_err());
    }
}
