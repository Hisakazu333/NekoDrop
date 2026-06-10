use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent, ServiceInfo};
use nekodrop_core::{Device, DeviceId, DevicePlatform, DeviceTrustState};

use crate::app_state::{ActiveReceiveSession, AppState, DiscoveryStatusState};
use crate::device_identity::LocalDeviceIdentity;
use crate::network::primary_lan_ip;

const SERVICE_TYPE: &str = "_nekodrop._tcp.local.";
const REGISTER_INTERVAL: Duration = Duration::from_secs(2);
const DEVICE_STALE_AFTER: Duration = Duration::from_secs(12);

pub fn start_discovery(state: &AppState) {
    let device_identity = state.device_identity.clone();
    let receive_session = state.receive_session.clone();
    let nearby_devices = state.nearby_devices.clone();
    let nearby_seen_at = state.nearby_devices_seen_at.clone();
    let discovery_status = state.discovery_status.clone();

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
                    status.phase = "unavailable".to_string();
                    status.message = "mDNS 服务启动失败，自动发现不可用".to_string();
                    status.last_error = Some(error.to_string());
                });
                return;
            }
        };
        let receiver = match daemon.browse(SERVICE_TYPE) {
            Ok(receiver) => receiver,
            Err(error) => {
                update_discovery_status(&discovery_status, |status| {
                    status.phase = "unavailable".to_string();
                    status.message = "无法浏览 NekoDrop 设备服务".to_string();
                    status.last_error = Some(error.to_string());
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
