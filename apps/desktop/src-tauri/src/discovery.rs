use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent, ServiceInfo};
use nekodrop_core::{Device, DeviceId, DevicePlatform, DeviceTrustState};

use crate::app_state::{ActiveReceiveSession, AppState};
use crate::device_identity::LocalDeviceIdentity;
use crate::network::primary_lan_ip;

const SERVICE_TYPE: &str = "_nekodrop._tcp.local.";
const REGISTER_INTERVAL: Duration = Duration::from_secs(2);

pub fn start_discovery(state: &AppState) {
    let device_identity = state.device_identity.clone();
    let receive_session = state.receive_session.clone();
    let nearby_devices = state.nearby_devices.clone();

    thread::spawn(move || {
        let Ok(daemon) = ServiceDaemon::new() else {
            return;
        };
        let Ok(receiver) = daemon.browse(SERVICE_TYPE) else {
            return;
        };

        spawn_service_advertiser(daemon.clone(), device_identity.clone(), receive_session);

        while let Ok(event) = receiver.recv() {
            match event {
                ServiceEvent::ServiceResolved(info) => {
                    add_or_update_device(&device_identity, &nearby_devices, &info);
                }
                ServiceEvent::ServiceRemoved(_, fullname) => {
                    remove_device_by_fullname(&nearby_devices, &fullname);
                }
                _ => {}
            }
        }
    });
}

fn spawn_service_advertiser(
    daemon: ServiceDaemon,
    device_identity: LocalDeviceIdentity,
    receive_session: Arc<Mutex<Option<ActiveReceiveSession>>>,
) {
    thread::spawn(move || {
        let mut last_port = None;
        let mut last_fullname: Option<String> = None;

        loop {
            let Some(port) = current_receive_port(&receive_session) else {
                thread::sleep(REGISTER_INTERVAL);
                continue;
            };
            if last_port == Some(port) {
                thread::sleep(REGISTER_INTERVAL);
                continue;
            }

            let identity = device_identity.public_identity();
            let Some(host_ip) = primary_lan_ip() else {
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

            last_fullname = Some(info.get_fullname().to_string());
            last_port = Some(port);
            let _ = daemon.register(info);
            thread::sleep(REGISTER_INTERVAL);
        }
    });
}

fn add_or_update_device(
    local_identity: &LocalDeviceIdentity,
    nearby_devices: &Arc<Mutex<Vec<Device>>>,
    info: &ResolvedService,
) {
    let local = local_identity.public_identity();
    let device_id = info
        .get_property_val_str("device_id")
        .map(str::to_string)
        .unwrap_or_else(|| info.get_fullname().to_string());
    if device_id == local.device_id {
        return;
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
        return;
    };

    let Ok(id) = DeviceId::new(device_id.clone()) else {
        return;
    };
    let Ok(mut device) = Device::new(id, name, platform, host.to_string(), info.get_port()) else {
        return;
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
}

fn remove_device_by_fullname(nearby_devices: &Arc<Mutex<Vec<Device>>>, fullname: &str) {
    if let Ok(mut devices) = nearby_devices.lock() {
        devices.retain(|device| device.id.as_str() != fullname);
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
