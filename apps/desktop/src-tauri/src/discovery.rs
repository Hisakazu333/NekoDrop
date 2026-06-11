use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent, ServiceInfo};
use nekodrop_core::{Device, DeviceId, DevicePlatform, DeviceTrustState};
use nekodrop_network::{
    decode_discovery_beacon, encode_discovery_beacon, DiscoveryAdvertisement,
    MAX_DISCOVERY_BEACON_BYTES, UDP_DISCOVERY_PORT,
};
use nekolink_protocol::{DeviceIdentity, PlatformKind};

use crate::app_state::{ActiveReceiveSession, AppState, DiscoveryStatusState};
use crate::device_identity::LocalDeviceIdentity;
use crate::network::primary_lan_ip;
use crate::trusted_devices::{save_trusted_devices, trusted_record_matches, TrustedDeviceRecord};

const SERVICE_TYPE: &str = "_nekodrop._tcp.local.";
const REGISTER_INTERVAL: Duration = Duration::from_secs(2);
const UDP_DISCOVERY_INTERVAL: Duration = Duration::from_secs(2);
const UDP_DISCOVERY_SOCKET_TIMEOUT: Duration = Duration::from_millis(800);
const DEVICE_STALE_AFTER: Duration = Duration::from_secs(90);
const TRUSTED_DEVICE_REFRESH_AFTER_MS: u128 = 30_000;

pub fn start_discovery(state: &AppState) {
    let device_identity = state.device_identity.clone();
    let receive_session = state.receive_session.clone();
    let nearby_devices = state.nearby_devices.clone();
    let nearby_seen_at = state.nearby_devices_seen_at.clone();
    let discovery_status = state.discovery_status.clone();
    let trusted_devices = state.trusted_devices.clone();

    spawn_udp_discovery_fallback(
        device_identity.clone(),
        receive_session.clone(),
        nearby_devices.clone(),
        nearby_seen_at.clone(),
        trusted_devices.clone(),
        discovery_status.clone(),
    );

    thread::spawn(move || {
        update_discovery_status(&discovery_status, |status| {
            status.phase = "starting".to_string();
            status.message = "正在启动自动发现".to_string();
            status.last_error = None;
        });

        let daemon = match ServiceDaemon::new() {
            Ok(daemon) => daemon,
            Err(error) => {
                update_discovery_status(&discovery_status, |status| {
                    status.phase = "active".to_string();
                    status.message = format!("mDNS 服务启动失败，已启用 UDP 兜底扫描: {error}");
                    status.last_error = None;
                });
                return;
            }
        };
        let receiver = match daemon.browse(SERVICE_TYPE) {
            Ok(receiver) => receiver,
            Err(error) => {
                update_discovery_status(&discovery_status, |status| {
                    status.phase = "active".to_string();
                    status.message = format!("mDNS 浏览失败，已启用 UDP 兜底扫描: {error}");
                    status.last_error = None;
                });
                return;
            }
        };

        update_discovery_status(&discovery_status, |status| {
            status.phase = "active".to_string();
            status.message = "正在扫描附近设备".to_string();
            status.last_error = None;
        });

        spawn_service_advertiser(
            daemon.clone(),
            device_identity.clone(),
            receive_session,
            discovery_status.clone(),
        );

        loop {
            while let Ok(event) = receiver.try_recv() {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        let added = add_or_update_device(
                            &device_identity,
                            &nearby_devices,
                            &nearby_seen_at,
                            &trusted_devices,
                            &info,
                        );
                        if added {
                            update_discovery_status(&discovery_status, |status| {
                                status.phase = "active".to_string();
                                status.message = "已发现附近设备".to_string();
                                status.last_seen_at = Some(Instant::now());
                                status.last_error = None;
                            });
                        }
                    }
                    ServiceEvent::ServiceRemoved(_, fullname) => {
                        remove_device_by_fullname(&nearby_devices, &nearby_seen_at, &fullname);
                    }
                    _ => {}
                }
            }
            purge_stale_devices(&nearby_devices, &nearby_seen_at);
            thread::sleep(Duration::from_millis(800));
        }
    });
}

fn spawn_udp_discovery_fallback(
    device_identity: LocalDeviceIdentity,
    receive_session: Arc<Mutex<Option<ActiveReceiveSession>>>,
    nearby_devices: Arc<Mutex<Vec<Device>>>,
    nearby_seen_at: Arc<Mutex<HashMap<String, Instant>>>,
    trusted_devices: Arc<Mutex<Vec<TrustedDeviceRecord>>>,
    discovery_status: Arc<Mutex<DiscoveryStatusState>>,
) {
    spawn_udp_discovery_listener(
        device_identity.clone(),
        nearby_devices,
        nearby_seen_at,
        trusted_devices,
        discovery_status.clone(),
    );
    spawn_udp_discovery_advertiser(device_identity, receive_session, discovery_status);
}

fn spawn_udp_discovery_listener(
    device_identity: LocalDeviceIdentity,
    nearby_devices: Arc<Mutex<Vec<Device>>>,
    nearby_seen_at: Arc<Mutex<HashMap<String, Instant>>>,
    trusted_devices: Arc<Mutex<Vec<TrustedDeviceRecord>>>,
    discovery_status: Arc<Mutex<DiscoveryStatusState>>,
) {
    thread::spawn(move || {
        let socket = match UdpSocket::bind(("0.0.0.0", UDP_DISCOVERY_PORT)) {
            Ok(socket) => socket,
            Err(error) => {
                update_discovery_status(&discovery_status, |status| {
                    status.message = format!("UDP 兜底监听失败: {error}");
                });
                return;
            }
        };
        let _ = socket.set_read_timeout(Some(UDP_DISCOVERY_SOCKET_TIMEOUT));
        let local_device_id = device_identity.public_identity().device_id;
        let mut buffer = vec![0_u8; MAX_DISCOVERY_BEACON_BYTES + 1];

        loop {
            match socket.recv_from(&mut buffer) {
                Ok((length, source)) => {
                    if let Ok(mut advertisement) = decode_discovery_beacon(&buffer[..length]) {
                        advertisement.host = source.ip().to_string();
                        let added = add_or_update_advertised_device(
                            &local_device_id,
                            &nearby_devices,
                            &nearby_seen_at,
                            &trusted_devices,
                            advertisement,
                        );
                        if added {
                            update_discovery_status(&discovery_status, |status| {
                                status.phase = "active".to_string();
                                status.message = "已发现附近设备".to_string();
                                status.last_seen_at = Some(Instant::now());
                                status.last_error = None;
                            });
                        }
                    }
                }
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(error) => {
                    update_discovery_status(&discovery_status, |status| {
                        status.message = format!("UDP 兜底扫描异常: {error}");
                    });
                    thread::sleep(UDP_DISCOVERY_INTERVAL);
                }
            }
            purge_stale_devices(&nearby_devices, &nearby_seen_at);
        }
    });
}

fn spawn_udp_discovery_advertiser(
    device_identity: LocalDeviceIdentity,
    receive_session: Arc<Mutex<Option<ActiveReceiveSession>>>,
    discovery_status: Arc<Mutex<DiscoveryStatusState>>,
) {
    thread::spawn(move || {
        let socket = match UdpSocket::bind(("0.0.0.0", 0)) {
            Ok(socket) => socket,
            Err(error) => {
                update_discovery_status(&discovery_status, |status| {
                    status.message = format!("UDP 兜底广播启动失败: {error}");
                });
                return;
            }
        };
        if let Err(error) = socket.set_broadcast(true) {
            update_discovery_status(&discovery_status, |status| {
                status.message = format!("UDP 兜底广播不可用: {error}");
            });
            return;
        }
        let target = SocketAddr::from(([255, 255, 255, 255], UDP_DISCOVERY_PORT));

        loop {
            let Some(port) = current_receive_port(&receive_session) else {
                thread::sleep(UDP_DISCOVERY_INTERVAL);
                continue;
            };
            let Some(host_ip) = primary_lan_ip() else {
                update_discovery_status(&discovery_status, |status| {
                    status.advertised = false;
                    status.lan_ip = None;
                    status.port = None;
                    status.message = "无法找到可广播的局域网地址".to_string();
                    status.last_error = Some(
                        "请确认已连接 Wi-Fi/有线局域网，并关闭会抢占路由的代理或虚拟网卡。"
                            .to_string(),
                    );
                });
                thread::sleep(UDP_DISCOVERY_INTERVAL);
                continue;
            };

            let identity = device_identity.public_identity();
            let advertisement = discovery_advertisement_for_identity(&identity, host_ip, port);
            let send_result = encode_discovery_beacon(&advertisement)
                .map_err(|error| error.to_string())
                .and_then(|payload| {
                    socket
                        .send_to(&payload, target)
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                });

            match send_result {
                Ok(()) => {
                    update_discovery_status(&discovery_status, |status| {
                        status.phase = "active".to_string();
                        status.advertised = true;
                        status.lan_ip = Some(host_ip.to_string());
                        status.port = Some(port);
                        status.message = "本机已广播，正在扫描附近设备".to_string();
                        status.last_error = None;
                    });
                }
                Err(error) => {
                    update_discovery_status(&discovery_status, |status| {
                        status.advertised = false;
                        status.message = "UDP 兜底广播失败".to_string();
                        status.last_error = Some(error.to_string());
                    });
                }
            }

            thread::sleep(UDP_DISCOVERY_INTERVAL);
        }
    });
}

fn spawn_service_advertiser(
    daemon: ServiceDaemon,
    device_identity: LocalDeviceIdentity,
    receive_session: Arc<Mutex<Option<ActiveReceiveSession>>>,
    discovery_status: Arc<Mutex<DiscoveryStatusState>>,
) {
    thread::spawn(move || {
        let mut last_port: Option<u16> = None;
        let mut last_fullname: Option<String> = None;

        loop {
            let Some(port) = current_receive_port(&receive_session) else {
                if let Some(fullname) = last_fullname.take() {
                    let _ = daemon.unregister(&fullname);
                }
                if last_port.take().is_some() {
                    update_discovery_status(&discovery_status, |status| {
                        status.advertised = false;
                        status.port = None;
                        status.message = "后台收件已关闭，本机未广播".to_string();
                    });
                }
                thread::sleep(REGISTER_INTERVAL);
                continue;
            };
            if last_port == Some(port) {
                thread::sleep(REGISTER_INTERVAL);
                continue;
            }

            let identity = device_identity.public_identity();
            let Some(host_ip) = primary_lan_ip() else {
                if let Some(fullname) = last_fullname.take() {
                    let _ = daemon.unregister(&fullname);
                }
                last_port = None;
                update_discovery_status(&discovery_status, |status| {
                    status.advertised = false;
                    status.lan_ip = None;
                    status.port = None;
                    status.message = "无法找到可广播的局域网地址".to_string();
                    status.last_error = Some(
                        "请确认已连接 Wi-Fi/有线局域网，并关闭会抢占路由的代理或虚拟网卡。"
                            .to_string(),
                    );
                });
                thread::sleep(REGISTER_INTERVAL);
                continue;
            };

            if let Some(fullname) = last_fullname.take() {
                let _ = daemon.unregister(&fullname);
            }

            let instance_name = service_instance_name(&identity.device_name, &identity.device_id);
            let host_name = format!("{}.local.", instance_name);
            let properties = [
                ("device_id", identity.device_id.as_str()),
                ("device_name", identity.device_name.as_str()),
                ("platform", identity.platform.as_str()),
                ("fingerprint", identity.public_key_fingerprint.as_str()),
            ];
            let Ok(info) = ServiceInfo::new(
                SERVICE_TYPE,
                &instance_name,
                &host_name,
                host_ip,
                port,
                &properties[..],
            ) else {
                thread::sleep(REGISTER_INTERVAL);
                continue;
            };

            let fullname = info.get_fullname().to_string();
            match daemon.register(info) {
                Ok(_) => {
                    last_fullname = Some(fullname);
                    last_port = Some(port);
                    update_discovery_status(&discovery_status, |status| {
                        status.phase = "active".to_string();
                        status.advertised = true;
                        status.lan_ip = Some(host_ip.to_string());
                        status.port = Some(port);
                        status.message = "本机已广播，正在扫描附近设备".to_string();
                        status.last_error = None;
                    });
                }
                Err(error) => {
                    last_port = None;
                    update_discovery_status(&discovery_status, |status| {
                        status.advertised = false;
                        status.port = None;
                        status.message = "本机广播失败".to_string();
                        status.last_error = Some(error.to_string());
                    });
                }
            }
            thread::sleep(REGISTER_INTERVAL);
        }
    });
}

fn add_or_update_device(
    local_identity: &LocalDeviceIdentity,
    nearby_devices: &Arc<Mutex<Vec<Device>>>,
    nearby_seen_at: &Arc<Mutex<HashMap<String, Instant>>>,
    trusted_devices: &Arc<Mutex<Vec<TrustedDeviceRecord>>>,
    info: &ResolvedService,
) -> bool {
    let local = local_identity.public_identity();
    let device_id = info
        .get_property_val_str("device_id")
        .map(str::to_string)
        .unwrap_or_else(|| info.get_fullname().to_string());
    if device_id == local.device_id {
        return false;
    }

    let name = info
        .get_property_val_str("device_name")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("NekoDrop");
    let platform = info
        .get_property_val_str("platform")
        .map(platform_from_str)
        .unwrap_or(DevicePlatform::Unknown);
    let Some(host) = first_ip_addr(info) else {
        return false;
    };

    let Ok(id) = DeviceId::new(device_id.clone()) else {
        return false;
    };
    let Ok(mut device) = Device::new(id, name, platform, host.to_string(), info.get_port()) else {
        return false;
    };
    device.public_key_fingerprint = info.get_property_val_str("fingerprint").map(str::to_string);
    device.trust_state = DeviceTrustState::Untrusted;

    refresh_trusted_device_endpoint(trusted_devices, &device);

    if let Ok(mut devices) = nearby_devices.lock() {
        if let Some(existing) = devices
            .iter_mut()
            .find(|item| item.id.as_str() == device_id || item.host == device.host)
        {
            *existing = device;
        } else {
            devices.push(device);
        }
    }
    if let Ok(mut seen_at) = nearby_seen_at.lock() {
        seen_at.insert(device_id, Instant::now());
    }
    true
}

fn add_or_update_advertised_device(
    local_device_id: &str,
    nearby_devices: &Arc<Mutex<Vec<Device>>>,
    nearby_seen_at: &Arc<Mutex<HashMap<String, Instant>>>,
    trusted_devices: &Arc<Mutex<Vec<TrustedDeviceRecord>>>,
    advertisement: DiscoveryAdvertisement,
) -> bool {
    let Ok(Some(device)) = device_from_discovery_advertisement(local_device_id, advertisement)
    else {
        return false;
    };

    let device_id = device.id.as_str().to_string();
    refresh_trusted_device_endpoint(trusted_devices, &device);

    if let Ok(mut devices) = nearby_devices.lock() {
        if let Some(existing) = devices
            .iter_mut()
            .find(|item| item.id.as_str() == device_id || item.host == device.host)
        {
            *existing = device;
        } else {
            devices.push(device);
        }
    }
    if let Ok(mut seen_at) = nearby_seen_at.lock() {
        seen_at.insert(device_id, Instant::now());
    }
    true
}

fn refresh_trusted_device_endpoint(
    trusted_devices: &Arc<Mutex<Vec<TrustedDeviceRecord>>>,
    device: &Device,
) {
    let Ok(mut records) = trusted_devices.lock() else {
        return;
    };
    let Some(record) = records
        .iter_mut()
        .find(|record| trusted_record_matches(device, record))
    else {
        return;
    };

    let now = now_ms();
    let platform = platform_wire_label(device.platform);
    let should_persist = record.host != device.host
        || record.port != device.port
        || record.device_name != device.name
        || record.platform != platform
        || now.saturating_sub(record.last_seen_at_ms) > TRUSTED_DEVICE_REFRESH_AFTER_MS;

    if !should_persist {
        return;
    }

    record.device_name = device.name.clone();
    record.platform = platform.to_string();
    record.host = device.host.clone();
    record.port = device.port;
    record.last_seen_at_ms = now;

    let next_records = records.clone();
    drop(records);
    let _ = save_trusted_devices(&next_records);
}

fn remove_device_by_fullname(
    nearby_devices: &Arc<Mutex<Vec<Device>>>,
    nearby_seen_at: &Arc<Mutex<HashMap<String, Instant>>>,
    fullname: &str,
) {
    if let Ok(mut devices) = nearby_devices.lock() {
        devices.retain(|device| device.id.as_str() != fullname);
    }
    if let Ok(mut seen_at) = nearby_seen_at.lock() {
        seen_at.remove(fullname);
    }
}

fn purge_stale_devices(
    nearby_devices: &Arc<Mutex<Vec<Device>>>,
    nearby_seen_at: &Arc<Mutex<HashMap<String, Instant>>>,
) {
    let stale_ids = if let Ok(seen_at) = nearby_seen_at.lock() {
        seen_at
            .iter()
            .filter_map(|(device_id, seen_at)| {
                if seen_at.elapsed() > DEVICE_STALE_AFTER {
                    Some(device_id.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    if stale_ids.is_empty() {
        return;
    }

    if let Ok(mut devices) = nearby_devices.lock() {
        devices.retain(|device| !stale_ids.iter().any(|id| id == device.id.as_str()));
    }
    if let Ok(mut seen_at) = nearby_seen_at.lock() {
        for device_id in stale_ids {
            seen_at.remove(&device_id);
        }
    }
}

fn current_receive_port(receive_session: &Arc<Mutex<Option<ActiveReceiveSession>>>) -> Option<u16> {
    let session = receive_session.lock().ok()?.clone()?;
    session.bind_addr.rsplit_once(':')?.1.parse().ok()
}

fn first_ip_addr(info: &ResolvedService) -> Option<IpAddr> {
    info.get_addresses_v4().into_iter().next().map(IpAddr::V4)
}

fn platform_from_str(value: &str) -> DevicePlatform {
    match value {
        "macos" => DevicePlatform::MacOS,
        "windows" => DevicePlatform::Windows,
        "linux" => DevicePlatform::Linux,
        _ => DevicePlatform::Unknown,
    }
}

fn platform_wire_label(platform: DevicePlatform) -> &'static str {
    match platform {
        DevicePlatform::MacOS => "macos",
        DevicePlatform::Windows => "windows",
        DevicePlatform::Linux => "linux",
        DevicePlatform::Unknown => "unknown",
    }
}

fn discovery_advertisement_for_identity(
    identity: &DeviceIdentity,
    host_ip: IpAddr,
    port: u16,
) -> DiscoveryAdvertisement {
    DiscoveryAdvertisement {
        device_id: DeviceId::new(identity.device_id.clone())
            .expect("local device identity must have a device id"),
        device_name: identity.device_name.clone(),
        platform: platform_from_identity(identity.platform),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        host: host_ip.to_string(),
        port,
        public_key_fingerprint: identity.public_key_fingerprint.clone(),
    }
}

fn platform_from_identity(platform: PlatformKind) -> DevicePlatform {
    match platform {
        PlatformKind::Macos => DevicePlatform::MacOS,
        PlatformKind::Windows => DevicePlatform::Windows,
        PlatformKind::Linux => DevicePlatform::Linux,
        _ => DevicePlatform::Unknown,
    }
}

fn device_from_discovery_advertisement(
    local_device_id: &str,
    advertisement: DiscoveryAdvertisement,
) -> Result<Option<Device>, String> {
    if advertisement.device_id.as_str() == local_device_id {
        return Ok(None);
    }

    let mut device = Device::new(
        advertisement.device_id,
        advertisement.device_name,
        advertisement.platform,
        advertisement.host,
        advertisement.port,
    )
    .map_err(|error| error.to_string())?;
    device.public_key_fingerprint = Some(advertisement.public_key_fingerprint);
    device.trust_state = DeviceTrustState::Untrusted;

    Ok(Some(device))
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn service_instance_name(device_name: &str, device_id: &str) -> String {
    let suffix = device_id
        .chars()
        .rev()
        .take(6)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    let name = device_name
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == '-')
        .take(24)
        .collect::<String>();
    if name.is_empty() {
        format!("NekoDrop-{suffix}")
    } else {
        format!("{name}-{suffix}")
    }
}

fn update_discovery_status(
    discovery_status: &Arc<Mutex<DiscoveryStatusState>>,
    update: impl FnOnce(&mut DiscoveryStatusState),
) {
    if let Ok(mut status) = discovery_status.lock() {
        update(&mut status);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nekolink_protocol::{DeviceIdentity, DeviceKind, PlatformKind};

    #[test]
    fn udp_advertisement_builds_untrusted_nearby_device() {
        let advertisement = DiscoveryAdvertisement {
            device_id: DeviceId::new("neko-device-remote").unwrap(),
            device_name: "Windows Desk".to_string(),
            platform: DevicePlatform::Windows,
            app_version: "0.1.0".to_string(),
            host: "192.168.1.42".to_string(),
            port: 45821,
            public_key_fingerprint: "sha256:remote".to_string(),
        };

        let device = device_from_discovery_advertisement("neko-device-local", advertisement)
            .unwrap()
            .unwrap();

        assert_eq!(device.id.as_str(), "neko-device-remote");
        assert_eq!(device.name, "Windows Desk");
        assert_eq!(device.platform, DevicePlatform::Windows);
        assert_eq!(device.host, "192.168.1.42");
        assert_eq!(device.port, 45821);
        assert_eq!(
            device.public_key_fingerprint.as_deref(),
            Some("sha256:remote")
        );
        assert_eq!(device.trust_state, DeviceTrustState::Untrusted);
    }

    #[test]
    fn udp_advertisement_ignores_local_device() {
        let advertisement = DiscoveryAdvertisement {
            device_id: DeviceId::new("neko-device-local").unwrap(),
            device_name: "This Mac".to_string(),
            platform: DevicePlatform::MacOS,
            app_version: "0.1.0".to_string(),
            host: "192.168.1.20".to_string(),
            port: 45821,
            public_key_fingerprint: "sha256:local".to_string(),
        };

        let device =
            device_from_discovery_advertisement("neko-device-local", advertisement).unwrap();

        assert!(device.is_none());
    }

    #[test]
    fn udp_advertisement_updates_nearby_device_index() {
        let nearby_devices = Arc::new(Mutex::new(Vec::new()));
        let nearby_seen_at = Arc::new(Mutex::new(HashMap::new()));
        let trusted_devices = Arc::new(Mutex::new(Vec::new()));

        let added = add_or_update_advertised_device(
            "neko-device-local",
            &nearby_devices,
            &nearby_seen_at,
            &trusted_devices,
            DiscoveryAdvertisement {
                device_id: DeviceId::new("neko-device-remote").unwrap(),
                device_name: "Windows Desk".to_string(),
                platform: DevicePlatform::Windows,
                app_version: "0.1.0".to_string(),
                host: "192.168.1.42".to_string(),
                port: 45821,
                public_key_fingerprint: "sha256:remote".to_string(),
            },
        );

        assert!(added);
        let devices = nearby_devices.lock().unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].host, "192.168.1.42");
        drop(devices);
        assert!(nearby_seen_at
            .lock()
            .unwrap()
            .contains_key("neko-device-remote"));
    }

    #[test]
    fn udp_advertisement_uses_local_identity_and_receive_endpoint() {
        let identity = DeviceIdentity::new(
            "neko-device-local",
            "This PC",
            DeviceKind::Desktop,
            PlatformKind::Windows,
            "sha256:local",
            Vec::new(),
        );

        let advertisement =
            discovery_advertisement_for_identity(&identity, IpAddr::from([192, 168, 1, 20]), 45821);

        assert_eq!(advertisement.device_id.as_str(), "neko-device-local");
        assert_eq!(advertisement.device_name, "This PC");
        assert_eq!(advertisement.platform, DevicePlatform::Windows);
        assert_eq!(advertisement.host, "192.168.1.20");
        assert_eq!(advertisement.port, 45821);
        assert_eq!(advertisement.public_key_fingerprint, "sha256:local");
    }
}
