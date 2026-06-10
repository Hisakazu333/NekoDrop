use std::net::{IpAddr, Ipv4Addr, UdpSocket};
use std::process::Command;

pub fn primary_lan_ip() -> Option<IpAddr> {
    primary_lan_ipv4().map(IpAddr::V4)
}

pub fn local_lan_ips() -> Vec<IpAddr> {
    local_lan_ipv4s().into_iter().map(IpAddr::V4).collect()
}

fn primary_lan_ipv4() -> Option<Ipv4Addr> {
    local_lan_ipv4s().into_iter().next()
}

fn local_lan_ipv4s() -> Vec<Ipv4Addr> {
    let mut ips = Vec::new();
    if let Some(ip) = probed_default_ipv4().filter(|ip| is_usable_lan_ipv4(*ip)) {
        push_unique_ipv4(&mut ips, ip);
    }
    for ip in candidate_ipv4s_from_os()
        .into_iter()
        .filter(|ip| is_usable_lan_ipv4(*ip))
    {
        push_unique_ipv4(&mut ips, ip);
    }
    ips
}

fn push_unique_ipv4(ips: &mut Vec<Ipv4Addr>, ip: Ipv4Addr) {
    if !ips.contains(&ip) {
        ips.push(ip);
    }
}

fn probed_default_ipv4() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("1.1.1.1:80").ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) => Some(ip),
        IpAddr::V6(_) => None,
    }
}

fn candidate_ipv4s_from_os() -> Vec<Ipv4Addr> {
    #[cfg(target_os = "windows")]
    {
        return command_ipv4s(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "Get-NetIPAddress -AddressFamily IPv4 | Where-Object { $_.AddressState -eq 'Preferred' } | Sort-Object InterfaceMetric | Select-Object -ExpandProperty IPAddress",
            ],
        );
    }

    #[cfg(target_os = "macos")]
    {
        return command_ipv4s("/sbin/ifconfig", &[]);
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let ips = command_ipv4s("/bin/hostname", &["-I"]);
        if ips.is_empty() {
            command_ipv4s("/sbin/ip", &["-4", "-o", "addr"])
        } else {
            ips
        }
    }
}

fn command_ipv4s(program: &str, args: &[&str]) -> Vec<Ipv4Addr> {
    let Ok(output) = Command::new(program).args(args).output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let text = String::from_utf8_lossy(&output.stdout);
    text.split(|character: char| character.is_whitespace() || character == '/')
        .filter_map(|token| token.parse::<Ipv4Addr>().ok())
        .collect()
}

fn is_usable_lan_ipv4(ip: Ipv4Addr) -> bool {
    is_private_ipv4(ip) && !is_blocked_candidate(ip)
}

fn is_private_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 10
        || (octets[0] == 172 && (16..=31).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 168)
}

fn is_blocked_candidate(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_link_local()
        || (octets[0] == 198 && (octets[1] == 18 || octets[1] == 19))
        || octets[0] >= 224
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_private_lan_ranges() {
        assert!(is_usable_lan_ipv4(Ipv4Addr::new(192, 168, 1, 10)));
        assert!(is_usable_lan_ipv4(Ipv4Addr::new(10, 0, 0, 8)));
        assert!(is_usable_lan_ipv4(Ipv4Addr::new(172, 16, 4, 20)));
        assert!(is_usable_lan_ipv4(Ipv4Addr::new(172, 31, 4, 20)));
    }

    #[test]
    fn rejects_loopback_link_local_and_benchmark_ranges() {
        assert!(!is_usable_lan_ipv4(Ipv4Addr::new(127, 0, 0, 1)));
        assert!(!is_usable_lan_ipv4(Ipv4Addr::new(169, 254, 1, 5)));
        assert!(!is_usable_lan_ipv4(Ipv4Addr::new(198, 18, 0, 1)));
        assert!(!is_usable_lan_ipv4(Ipv4Addr::new(198, 19, 0, 1)));
    }

    #[test]
    fn rejects_public_and_reserved_ranges() {
        assert!(!is_usable_lan_ipv4(Ipv4Addr::new(8, 8, 8, 8)));
        assert!(!is_usable_lan_ipv4(Ipv4Addr::new(100, 64, 0, 1)));
        assert!(!is_usable_lan_ipv4(Ipv4Addr::new(224, 0, 0, 1)));
    }

    #[test]
    fn pushes_unique_ipv4s_only_once() {
        let mut ips = Vec::new();
        push_unique_ipv4(&mut ips, Ipv4Addr::new(192, 168, 1, 20));
        push_unique_ipv4(&mut ips, Ipv4Addr::new(192, 168, 1, 20));
        push_unique_ipv4(&mut ips, Ipv4Addr::new(10, 0, 0, 8));

        assert_eq!(
            ips,
            vec![Ipv4Addr::new(192, 168, 1, 20), Ipv4Addr::new(10, 0, 0, 8)]
        );
    }
}
