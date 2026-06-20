use std::net::IpAddr;

use nekodrop_core::Device;
use nekodrop_network::{ConnectionTicket, Endpoint};
use nekolink_protocol::{DeviceIdentity, SignedSessionIdentityBinding};

use super::{friendly_transfer_error, ReceiveTrustContext};
use crate::app_state::AppState;
use crate::network::local_lan_ips;
use crate::transfer_history::TransferHistoryRecord;
use crate::trusted_devices::{trusted_record_matches, TrustedDeviceRecord};

#[derive(Debug, Clone)]
pub(super) struct TransferPeer {
    pub(super) device_id: Option<String>,
    pub(super) name: Option<String>,
    pub(super) fingerprint: Option<String>,
    pub(super) trusted_public_key: Option<String>,
    pub(super) trusted_public_key_fingerprint: Option<String>,
    pub(super) target_host: Option<String>,
}

pub(super) fn endpoint_and_peer_for_device_id(
    state: &AppState,
    device_id: &str,
) -> Result<(Endpoint, TransferPeer), String> {
    if let Some((endpoint, peer)) = endpoint_and_peer_from_nearby_device(state, device_id)? {
        return Ok((endpoint, peer));
    }
    if let Some((endpoint, peer)) = endpoint_and_peer_from_trusted_device(state, device_id)? {
        return Ok((endpoint, peer));
    }
    Err("设备不在线或尚未被自动扫描到，请确认对方收件开启后重试。".to_string())
}

fn endpoint_and_peer_from_nearby_device(
    state: &AppState,
    device_id: &str,
) -> Result<Option<(Endpoint, TransferPeer)>, String> {
    let device = {
        let devices = state
            .nearby_devices
            .lock()
            .map_err(|error| error.to_string())?;
        devices
            .iter()
            .find(|item| item.id.as_str() == device_id)
            .cloned()
    };
    let Some(device) = device else {
        return Ok(None);
    };

    let trusted_devices = state
        .trusted_devices
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(Some(trusted_peer_from_nearby_device(
        &device,
        &trusted_devices,
    )?))
}

pub(super) fn trusted_peer_from_nearby_device(
    device: &Device,
    trusted_devices: &[TrustedDeviceRecord],
) -> Result<(Endpoint, TransferPeer), String> {
    let trusted_record = trusted_devices
        .iter()
        .find(|record| trusted_record_matches(device, record));
    let Some(trusted_record) = trusted_record else {
        return Err("这台设备还没有可信配对，请先完成配对再发送文件。".to_string());
    };

    let endpoint = Endpoint::tcp(device.host.clone(), device.port);
    let peer = TransferPeer {
        device_id: Some(device.id.as_str().to_string()),
        name: Some(device.name.clone()),
        fingerprint: device.public_key_fingerprint.clone(),
        trusted_public_key: Some(trusted_record.public_key.clone()),
        trusted_public_key_fingerprint: Some(trusted_record.public_key_fingerprint.clone()),
        target_host: Some(endpoint_label(&endpoint)),
    };
    Ok((endpoint, peer))
}

pub(super) fn reject_self_peer(
    local_identity: &DeviceIdentity,
    peer: &TransferPeer,
) -> Result<(), String> {
    if peer
        .device_id
        .as_deref()
        .is_some_and(|device_id| device_id == local_identity.device_id)
    {
        return Err("不能把文件发送给本机，请选择另一台设备。".to_string());
    }
    Ok(())
}

fn verify_signed_session_against_trusted_pin(
    identity: &DeviceIdentity,
    signed_binding: &SignedSessionIdentityBinding,
    expected_device_id: Option<&str>,
    expected_public_key: Option<&str>,
    expected_public_key_fingerprint: Option<&str>,
) -> Result<(), String> {
    signed_binding
        .binding
        .verify_identity(identity)
        .map_err(|error| format!("可信设备身份校验失败：binding 不匹配: {}", error.message))?;
    if let Some(expected_device_id) = expected_device_id {
        if identity.device_id != expected_device_id {
            return Err("可信设备身份校验失败：device_id 不匹配".to_string());
        }
    }
    if let Some(expected_fingerprint) = expected_public_key_fingerprint {
        if identity.public_key_fingerprint != expected_fingerprint {
            return Err("可信设备身份校验失败：session 指纹不匹配".to_string());
        }
        if signed_binding.public_key_fingerprint != expected_fingerprint {
            return Err("可信设备身份校验失败：签名指纹不匹配".to_string());
        }
    }
    let Some(expected_public_key) = expected_public_key else {
        return Ok(());
    };
    if expected_public_key_fingerprint.is_none() {
        return Err("可信设备身份校验失败：缺少可信指纹".to_string());
    }
    if signed_binding.public_key != expected_public_key {
        return Err("可信设备身份校验失败：长期公钥不匹配".to_string());
    }
    Ok(())
}

pub(super) fn verify_peer_matches_transfer_peer(
    peer: &TransferPeer,
    identity: &DeviceIdentity,
    signed_binding: &SignedSessionIdentityBinding,
) -> Result<(), String> {
    verify_signed_session_against_trusted_pin(
        identity,
        signed_binding,
        peer.device_id.as_deref(),
        peer.trusted_public_key.as_deref(),
        peer.trusted_public_key_fingerprint
            .as_deref()
            .or(peer.fingerprint.as_deref()),
    )
}

pub(super) fn verify_incoming_peer_against_trusted_devices(
    trusted_devices: &[TrustedDeviceRecord],
    identity: &DeviceIdentity,
    signed_binding: &SignedSessionIdentityBinding,
) -> Result<ReceiveTrustContext, String> {
    let Some(record) = trusted_devices
        .iter()
        .find(|record| record.device_id == identity.device_id)
    else {
        return Ok(ReceiveTrustContext::Untrusted);
    };

    verify_signed_session_against_trusted_pin(
        identity,
        signed_binding,
        Some(record.device_id.as_str()),
        Some(record.public_key.as_str()),
        Some(record.public_key_fingerprint.as_str()),
    )?;
    Ok(ReceiveTrustContext::AuthenticatedTrusted)
}

fn endpoint_and_peer_from_trusted_device(
    state: &AppState,
    device_id: &str,
) -> Result<Option<(Endpoint, TransferPeer)>, String> {
    let trusted_devices = state
        .trusted_devices
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(trusted_devices
        .iter()
        .find(|item| item.device_id == device_id)
        .map(|device| {
            let endpoint = Endpoint::tcp(device.host.clone(), device.port);
            let peer = TransferPeer {
                device_id: Some(device.device_id.clone()),
                name: Some(device.device_name.clone()),
                fingerprint: Some(device.public_key_fingerprint.clone()),
                trusted_public_key: Some(device.public_key.clone()),
                trusted_public_key_fingerprint: Some(device.public_key_fingerprint.clone()),
                target_host: Some(endpoint_label(&endpoint)),
            };
            (endpoint, peer)
        }))
}

pub(super) fn endpoint_and_peer_for_history_record(
    state: &AppState,
    record: &TransferHistoryRecord,
) -> Result<(Endpoint, TransferPeer), String> {
    if let Some(device_id) = record.peer_device_id.as_deref() {
        return endpoint_and_peer_for_device_id(state, device_id)
            .map_err(|error| format!("这条历史记录绑定的设备当前不能重发：{error}"));
    }

    let target_host = record
        .target_host
        .as_deref()
        .ok_or_else(|| "这条历史没有可重连的目标地址".to_string())?;
    let endpoint = endpoint_from_label(target_host)?;
    let peer = TransferPeer {
        device_id: record.peer_device_id.clone(),
        name: record.peer_name.clone(),
        fingerprint: None,
        trusted_public_key: None,
        trusted_public_key_fingerprint: None,
        target_host: Some(endpoint_label(&endpoint)),
    };
    Ok((endpoint, peer))
}

pub(super) fn endpoint_and_peer_from_connection_input(
    value: &str,
) -> Result<(Endpoint, TransferPeer), String> {
    match ConnectionTicket::parse(value) {
        Ok(ticket) => {
            let endpoint = ticket.endpoint.clone();
            let peer = TransferPeer {
                device_id: ticket.device_id.clone(),
                name: ticket.device_name.clone(),
                fingerprint: ticket.fingerprint.clone(),
                trusted_public_key: None,
                trusted_public_key_fingerprint: None,
                target_host: Some(endpoint_label(&endpoint)),
            };
            Ok((endpoint, peer))
        }
        Err(error) => {
            if looks_like_endpoint_label(value) {
                let endpoint = endpoint_from_label(value)?;
                let peer = TransferPeer {
                    device_id: None,
                    name: None,
                    fingerprint: None,
                    trusted_public_key: None,
                    trusted_public_key_fingerprint: None,
                    target_host: Some(endpoint_label(&endpoint)),
                };
                return Ok((endpoint, peer));
            }
            Err(friendly_transfer_error(&error.to_string()))
        }
    }
}

fn looks_like_endpoint_label(value: &str) -> bool {
    let value = value.trim();
    !value.starts_with("nekodrop-v1") && value.rsplit_once(':').is_some()
}

pub(super) fn endpoint_label(endpoint: &Endpoint) -> String {
    format!("{}:{}", endpoint.host, endpoint.port)
}

fn endpoint_from_label(value: &str) -> Result<Endpoint, String> {
    let (host, port) = value
        .rsplit_once(':')
        .ok_or_else(|| friendly_transfer_error(&format!("invalid endpoint label: {value}")))?;
    let port = port
        .parse::<u16>()
        .map_err(|error| friendly_transfer_error(&format!("invalid endpoint port: {error}")))?;
    if host.trim().is_empty() {
        return Err(friendly_transfer_error("empty endpoint host"));
    }
    Ok(Endpoint::tcp(host.to_string(), port))
}

pub(super) fn validate_endpoint_for_desktop_send(endpoint: &Endpoint) -> Result<(), String> {
    if endpoint.transport.as_str() != "tcp" {
        return Err(friendly_transfer_error(&format!(
            "unsupported transport: requested {}",
            endpoint.transport.as_str()
        )));
    }
    if endpoint.port == 0 {
        return Err("目标端口无效，请重新从附近设备发送，或重新复制连接码。".to_string());
    }

    let host = endpoint.host.trim();
    if host.is_empty() {
        return Err("目标地址缺少主机，请重新从附近设备发送，或重新复制连接码。".to_string());
    }

    let lower = host.to_lowercase();
    if lower == "localhost" {
        return Err(friendly_transfer_error("failed to connect to localhost"));
    }

    if let Ok(ip) = host.parse::<IpAddr>() {
        if ip.is_loopback() {
            return Err(friendly_transfer_error(&format!(
                "failed to connect to {host}:{}",
                endpoint.port
            )));
        }
        if is_current_lan_ip(ip, &local_lan_ips()) {
            return Err(
                "目标地址是本机局域网地址，不能把文件发送给自己。请选择另一台设备或复制对方连接码。"
                    .to_string(),
            );
        }
        if ip.is_unspecified() {
            return Err(
                "目标地址是 0.0.0.0 或 ::，这只是监听地址，不能被另一台设备连接。请重新复制接收端连接码。"
                    .to_string(),
            );
        }
        if let IpAddr::V4(ipv4) = ip {
            let octets = ipv4.octets();
            if octets[0] == 198 && (octets[1] == 18 || octets[1] == 19) {
                return Err(friendly_transfer_error(&format!(
                    "failed to connect to {host}:{}",
                    endpoint.port
                )));
            }
            if octets[0] == 169 && octets[1] == 254 {
                return Err(
                    "目标地址是 169.254.x.x，这通常表示没有拿到可用局域网地址。请确认两台设备在同一网络，或重新打开接收端生成连接码。"
                        .to_string(),
                );
            }
        }
    }

    Ok(())
}

pub(super) fn is_current_lan_ip(target: IpAddr, current_lan_ips: &[IpAddr]) -> bool {
    current_lan_ips.contains(&target)
}
