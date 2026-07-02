use std::net::{IpAddr, SocketAddr};

use super::{ActiveReceiveSession, ReceivePortDiagnosticsDto, ReceiveSessionDto};

pub(super) fn receive_session_to_dto(session: &ActiveReceiveSession) -> ReceiveSessionDto {
    ReceiveSessionDto {
        bind_addr: session.bind_addr.clone(),
        receive_dir: session.receive_dir.clone(),
        connection_code: session.connection_code.clone(),
    }
}

pub(super) fn receive_port_diagnostics_from_session(
    session: Option<&ActiveReceiveSession>,
    lan_ips: Vec<IpAddr>,
) -> ReceivePortDiagnosticsDto {
    let lan_ip_labels = lan_ips.iter().map(ToString::to_string).collect::<Vec<_>>();
    let Some(session) = session else {
        return ReceivePortDiagnosticsDto {
            phase: "closed".to_string(),
            listening: false,
            bind_addr: None,
            advertised_host: None,
            port: None,
            lan_ips: lan_ip_labels,
            message: "收件未开启，当前没有监听端口".to_string(),
            checks: vec!["打开收件后才会生成连接码和监听端口".to_string()],
        };
    };

    let Some((bind_ip, port)) = parse_receive_bind_addr(&session.bind_addr) else {
        return ReceivePortDiagnosticsDto {
            phase: "invalid_bind_addr".to_string(),
            listening: true,
            bind_addr: Some(session.bind_addr.clone()),
            advertised_host: None,
            port: None,
            lan_ips: lan_ip_labels,
            message: "收件监听地址异常，请关闭收件后重新开启".to_string(),
            checks: receive_port_diagnostic_checks(),
        };
    };

    let advertised_host = if bind_ip.is_unspecified() {
        lan_ips.first().map(ToString::to_string)
    } else {
        Some(bind_ip.to_string())
    };
    let phase = if advertised_host.is_some() {
        "listening"
    } else {
        "no_lan_ip"
    };
    let message = if let Some(host) = advertised_host.as_deref() {
        format!("收件监听中，其他设备应连接 {host}:{port}")
    } else {
        "收件监听已开启，但没有可用于其他设备连接的局域网地址".to_string()
    };

    ReceivePortDiagnosticsDto {
        phase: phase.to_string(),
        listening: true,
        bind_addr: Some(session.bind_addr.clone()),
        advertised_host,
        port: Some(port),
        lan_ips: lan_ip_labels,
        message,
        checks: receive_port_diagnostic_checks(),
    }
}

fn parse_receive_bind_addr(bind_addr: &str) -> Option<(IpAddr, u16)> {
    bind_addr
        .parse::<SocketAddr>()
        .ok()
        .map(|addr| (addr.ip(), addr.port()))
}

fn receive_port_diagnostic_checks() -> Vec<String> {
    vec![
        "确认两台设备在同一局域网，且没有被路由器 AP 隔离".to_string(),
        "Windows 防火墙需要允许 NekoDrop 访问专用网络".to_string(),
        "VPN、代理或虚拟网卡可能让连接码拿到错误地址".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use std::sync::{atomic::AtomicBool, Arc};

    use super::*;

    #[test]
    fn receive_port_diagnostics_reports_closed_receiver() {
        let diagnostics = receive_port_diagnostics_from_session(None, vec![]);

        assert_eq!(diagnostics.phase, "closed");
        assert!(!diagnostics.listening);
        assert_eq!(diagnostics.bind_addr, None);
        assert_eq!(diagnostics.port, None);
        assert!(diagnostics.message.contains("收件未开启"));
    }

    #[test]
    fn receive_port_diagnostics_uses_lan_ip_for_unspecified_bind() {
        let session = ActiveReceiveSession {
            bind_addr: "0.0.0.0:45821".to_string(),
            receive_dir: "/tmp/nekodrop".to_string(),
            connection_code: "ticket".to_string(),
            cancel: Arc::new(AtomicBool::new(false)),
        };

        let diagnostics = receive_port_diagnostics_from_session(
            Some(&session),
            vec![IpAddr::from([192, 168, 1, 20]), IpAddr::from([10, 0, 0, 8])],
        );

        assert_eq!(diagnostics.phase, "listening");
        assert!(diagnostics.listening);
        assert_eq!(diagnostics.bind_addr.as_deref(), Some("0.0.0.0:45821"));
        assert_eq!(diagnostics.advertised_host.as_deref(), Some("192.168.1.20"));
        assert_eq!(diagnostics.port, Some(45821));
        assert!(diagnostics
            .checks
            .iter()
            .any(|check| check.contains("防火墙")));
    }

    #[test]
    fn receive_port_diagnostics_warns_when_no_lan_ip_is_available() {
        let session = ActiveReceiveSession {
            bind_addr: "0.0.0.0:45821".to_string(),
            receive_dir: "/tmp/nekodrop".to_string(),
            connection_code: "ticket".to_string(),
            cancel: Arc::new(AtomicBool::new(false)),
        };

        let diagnostics = receive_port_diagnostics_from_session(Some(&session), vec![]);

        assert_eq!(diagnostics.phase, "no_lan_ip");
        assert!(diagnostics.listening);
        assert_eq!(diagnostics.advertised_host, None);
        assert_eq!(diagnostics.port, Some(45821));
        assert!(diagnostics.message.contains("局域网地址"));
    }
}
