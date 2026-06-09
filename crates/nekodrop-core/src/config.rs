#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceivePolicy {
    AlwaysAsk,
    AutoAcceptTrusted,
    BlockAll,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub device_name: String,
    pub receive_dir: String,
    pub launch_at_login: bool,
    pub tray_enabled: bool,
    pub discovery_enabled: bool,
    pub receive_policy: ReceivePolicy,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            device_name: "这台电脑".to_string(),
            receive_dir: "~/Downloads/NekoDrop".to_string(),
            launch_at_login: false,
            tray_enabled: false,
            discovery_enabled: false,
            receive_policy: ReceivePolicy::AlwaysAsk,
        }
    }
}
