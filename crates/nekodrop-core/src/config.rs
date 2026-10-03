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
    pub receive_port: u16,
    pub launch_at_login: bool,
    pub tray_enabled: bool,
    pub discovery_enabled: bool,
    pub receive_policy: ReceivePolicy,
    /// 发送限速（KB/s），0 = 不限
    pub send_limit_kbps: u32,
    /// 接收后按发送设备名归档到子目录
    pub organize_receive_by_device: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            device_name: "这台电脑".to_string(),
            receive_dir: "~/Downloads/NekoDrop".to_string(),
            receive_port: 45821,
            launch_at_login: false,
            tray_enabled: false,
            discovery_enabled: true,
            receive_policy: ReceivePolicy::AlwaysAsk,
            send_limit_kbps: 0,
            organize_receive_by_device: false,
        }
    }
}
