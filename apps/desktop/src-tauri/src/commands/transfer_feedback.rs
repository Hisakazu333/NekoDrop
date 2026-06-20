pub(super) fn friendly_transfer_error(error: &str) -> String {
    let lower = error.to_lowercase();

    if lower.contains("receiver declined") || lower.contains("transfer declined by receiver") {
        return "对方拒绝了这次传输".to_string();
    }
    if lower.contains("transfer cancelled") {
        return "传输已取消".to_string();
    }
    if lower.contains("insufficient receive space") || lower.contains("disk full") {
        return "接收目录所在磁盘空间不足。请清理空间，或在设置里选择另一个接收目录后重试。"
            .to_string();
    }

    if lower.contains("unsupported connection code")
        || lower.contains("connection code missing")
        || lower.contains("invalid connection code")
        || lower.contains("invalid percent encoding")
        || lower.contains("connection field is not utf-8")
        || lower.contains("connection ticket only supports")
    {
        return "连接码无效，请重新复制对方生成的连接码。".to_string();
    }

    if lower.contains("invalid endpoint label")
        || lower.contains("invalid endpoint port")
        || lower.contains("empty endpoint host")
    {
        return "历史记录里的目标地址无效，请重新从附近设备发送，或重新复制连接码。".to_string();
    }

    if lower.contains("transport is not available")
        || lower.contains("unsupported transport")
        || lower.contains("requested iroh")
        || lower.contains("requested relay")
        || lower.contains("requested quic")
    {
        return "当前版本还没有接入这个传输通道。请先使用局域网自动发现或连接码兜底。".to_string();
    }

    if lower.contains("198.18.") || lower.contains("198.19.") {
        return "连接地址落在 198.18/198.19 测试网段，通常是代理、VPN 或虚拟网卡。请关闭相关网络工具，或改用真实局域网地址/连接码。".to_string();
    }

    if lower.contains("127.0.0.1") || lower.contains("localhost") {
        return "连接地址指向了本机，另一台电脑无法访问。请重新打开接收端，复制新的连接码，或使用附近设备自动发现。".to_string();
    }

    if lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("由于连接方在一段时间后没有正确答复")
        || lower.contains("连接尝试失败")
    {
        return "连接超时。常见原因是 Windows 防火墙拦截、两台设备不在同一网段、路由器隔离了有线/无线，或 VPN/代理影响了局域网连接。".to_string();
    }

    if lower.contains("connection refused")
        || lower.contains("actively refused")
        || lower.contains("connection reset")
        || lower.contains("failed to connect")
        || lower.contains("由于目标计算机积极拒绝")
    {
        return "无法连接对方电脑。请确认对方 NekoDrop 正在运行、收件已开启、防火墙允许访问，且两台设备网络互通。".to_string();
    }

    if lower.contains("network is unreachable")
        || lower.contains("no route to host")
        || lower.contains("host unreachable")
        || lower.contains("无法访问目标主机")
    {
        return "当前网络无法到达对方设备。请确认两台设备在同一局域网，或使用连接码/后续 Relay 方案。".to_string();
    }

    if lower.contains("permission denied")
        || lower.contains("access is denied")
        || lower.contains("operation not permitted")
        || lower.contains("权限")
    {
        return "系统权限阻止了这次操作。请检查接收目录权限、防火墙权限，或重新选择一个可写入的接收目录。".to_string();
    }

    if lower.contains("checksum")
        || lower.contains("sha-256")
        || lower.contains("sha256")
        || lower.contains("does not match accepted offer")
    {
        return "文件校验失败，已拒绝把不一致的内容当作完成文件。请重新发送。".to_string();
    }

    if lower.contains("no such file") || lower.contains("not found") || lower.contains("路径不存在")
    {
        return "文件或目录不存在，请确认源文件没有被移动、删除，或重新选择文件。".to_string();
    }

    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friendly_transfer_error_explains_connection_failures() {
        let refused = friendly_transfer_error(
            "network error: failed to connect to 192.168.1.8:45821: Connection refused",
        );
        assert!(refused.contains("无法连接对方电脑"));

        let timeout = friendly_transfer_error(
            "network error: failed to connect to 192.168.1.8:45821: timed out",
        );
        assert!(timeout.contains("连接超时"));
        assert!(timeout.contains("防火墙"));
    }

    #[test]
    fn friendly_transfer_error_explains_bad_network_addresses() {
        let benchmark =
            friendly_transfer_error("network error: failed to connect to 198.18.0.1:45821");
        assert!(benchmark.contains("198.18/198.19"));

        let loopback = friendly_transfer_error("failed to connect to 127.0.0.1:45821");
        assert!(loopback.contains("指向了本机"));
    }

    #[test]
    fn friendly_transfer_error_explains_unsupported_transport_and_integrity_failures() {
        let transport = friendly_transfer_error("iroh transport is not available in this build");
        assert!(transport.contains("还没有接入这个传输通道"));

        let checksum = friendly_transfer_error("incoming file does not match accepted offer");
        assert!(checksum.contains("文件校验失败"));
    }

    #[test]
    fn friendly_transfer_error_explains_insufficient_receive_space() {
        let message = friendly_transfer_error(
            "storage error: insufficient receive space: need 100 bytes, available 70 bytes",
        );

        assert!(message.contains("接收目录"));
        assert!(message.contains("空间不足"));
    }
}
