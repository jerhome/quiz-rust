use std::net::{Ipv4Addr, SocketAddr};

/// An IPv4 address of this machine, for display (e.g. "tell friends to join 192.168.1.20").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAddr {
    pub ip: Ipv4Addr,
    pub interface_name: String,
    pub is_likely_virtual: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct IfaceInfo {
    pub name: String,
    pub ip: Ipv4Addr,
    pub netmask: Ipv4Addr,
    pub is_loopback: bool,
}

/// Addresses of this machine worth showing to other players, most likely real LAN first.
/// Loopback and link-local (169.254/16) addresses are excluded.
pub fn local_ipv4_addrs() -> Vec<LocalAddr> {
    local_addrs_from(&system_ifaces())
}

pub(crate) fn system_ifaces() -> Vec<IfaceInfo> {
    let Ok(all) = if_addrs::get_if_addrs() else {
        return Vec::new();
    };
    all.into_iter()
        .filter(|i| i.is_oper_up())
        .filter_map(|i| match &i.addr {
            if_addrs::IfAddr::V4(v4) => Some(IfaceInfo {
                is_loopback: i.is_loopback(),
                name: i.name.clone(),
                ip: v4.ip,
                netmask: v4.netmask,
            }),
            if_addrs::IfAddr::V6(_) => None,
        })
        .collect()
}

const VIRTUAL_CONTAINS: [&str; 11] = [
    "vethernet",
    "wsl",
    "hyper-v",
    "docker",
    "vpn",
    "virtualbox",
    "vmware",
    "vmnet",
    "virbr",
    "veth",
    "tailscale",
];
const VIRTUAL_PREFIX: [&str; 7] = ["tun", "tap", "wg", "utun", "br-", "vbox", "zt"];
const REAL_PREFIX: [&str; 6] = ["eth", "en", "wlan", "wl", "wi-fi", "wifi"];

fn is_172_16_12(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    o[0] == 172 && (16..32).contains(&o[1])
}

fn is_likely_virtual(name: &str, ip: Ipv4Addr) -> bool {
    let n = name.to_lowercase();
    if VIRTUAL_CONTAINS.iter().any(|k| n.contains(k))
        || VIRTUAL_PREFIX.iter().any(|p| n.starts_with(p))
    {
        return true;
    }
    // 172.16/12 is what Hyper-V/WSL/docker pick; trust it only if the name says "real NIC".
    let known_real = REAL_PREFIX.iter().any(|p| n.starts_with(p)) || n.contains("local area");
    is_172_16_12(ip) && !known_real
}

fn range_rank(ip: Ipv4Addr) -> u8 {
    let o = ip.octets();
    match (o[0], o[1]) {
        (192, 168) => 0,
        (10, _) => 1,
        _ if is_172_16_12(ip) => 2,
        _ => 3,
    }
}

/// Usable, non-loopback, non-link-local interfaces; real LAN first (stable order inside a rank).
fn ranked(ifaces: &[IfaceInfo]) -> Vec<(&IfaceInfo, bool)> {
    let mut v: Vec<(&IfaceInfo, bool)> = ifaces
        .iter()
        .filter(|i| {
            !i.is_loopback && !i.ip.is_loopback() && !i.ip.is_link_local() && !i.ip.is_unspecified()
        })
        .map(|i| (i, is_likely_virtual(&i.name, i.ip)))
        .collect();
    v.sort_by_key(|(i, virt)| (*virt, range_rank(i.ip)));
    v
}

pub(crate) fn local_addrs_from(ifaces: &[IfaceInfo]) -> Vec<LocalAddr> {
    ranked(ifaces)
        .into_iter()
        .map(|(i, virt)| LocalAddr {
            ip: i.ip,
            interface_name: i.name.clone(),
            is_likely_virtual: virt,
        })
        .collect()
}

pub(crate) fn directed_broadcast(ip: Ipv4Addr, mask: Ipv4Addr) -> Ipv4Addr {
    Ipv4Addr::from(u32::from(ip) | !u32::from(mask))
}

/// Where to send the announcement: the directed broadcast of every usable interface
/// (the limited 255.255.255.255 only leaves through one interface, often a VPN or
/// vEthernet one), real LAN first, then the limited broadcast as a catch-all.
pub(crate) fn announce_targets(ifaces: &[IfaceInfo], port: u16) -> Vec<SocketAddr> {
    let mut out: Vec<SocketAddr> = Vec::new();
    let global = SocketAddr::from((Ipv4Addr::BROADCAST, port));
    for (i, _) in ranked(ifaces) {
        let b = directed_broadcast(i.ip, i.netmask);
        let t = SocketAddr::from((b, port));
        if b != i.ip && t != global && !out.contains(&t) {
            out.push(t);
        }
    }
    out.push(global);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iface(name: &str, ip: [u8; 4], mask: [u8; 4], lo: bool) -> IfaceInfo {
        IfaceInfo {
            name: name.into(),
            ip: ip.into(),
            netmask: mask.into(),
            is_loopback: lo,
        }
    }

    /// A typical office laptop: VPN, Hyper-V/WSL switches, docker and the real Wi-Fi.
    fn sim() -> Vec<IfaceInfo> {
        vec![
            iface("lo", [127, 0, 0, 1], [255, 0, 0, 0], true),
            iface(
                "vEthernet (Default Switch)",
                [172, 28, 0, 1],
                [255, 255, 240, 0],
                false,
            ),
            iface("tun0", [10, 8, 0, 2], [255, 255, 255, 0], false),
            iface("Wi-Fi", [192, 168, 1, 20], [255, 255, 255, 0], false),
            iface(
                "vEthernet (WSL)",
                [172, 20, 0, 1],
                [255, 255, 240, 0],
                false,
            ),
            iface("docker0", [172, 17, 0, 1], [255, 255, 0, 0], false),
            iface("Ethernet 2", [169, 254, 3, 4], [255, 255, 0, 0], false),
        ]
    }

    #[test]
    fn directed_broadcast_from_ip_and_mask() {
        assert_eq!(
            directed_broadcast([192, 168, 1, 20].into(), [255, 255, 255, 0].into()),
            Ipv4Addr::new(192, 168, 1, 255)
        );
        assert_eq!(
            directed_broadcast([10, 1, 2, 3].into(), [255, 255, 0, 0].into()),
            Ipv4Addr::new(10, 1, 255, 255)
        );
        assert_eq!(
            directed_broadcast([172, 28, 0, 1].into(), [255, 255, 240, 0].into()),
            Ipv4Addr::new(172, 28, 15, 255)
        );
    }

    #[test]
    fn real_lan_is_listed_first_and_noise_is_excluded() {
        let v = local_addrs_from(&sim());
        assert_eq!(v[0].ip, Ipv4Addr::new(192, 168, 1, 20));
        assert!(!v[0].is_likely_virtual);
        assert!(v[1..].iter().all(|a| a.is_likely_virtual), "{v:?}");
        assert!(v
            .iter()
            .all(|a| !a.ip.is_loopback() && !a.ip.is_link_local()));
        assert_eq!(v.len(), 5);
    }

    #[test]
    fn ambiguous_172_16_names_are_virtual_but_known_real_names_are_not() {
        let a = local_addrs_from(&[iface(
            "Foo Adapter",
            [172, 16, 5, 5],
            [255, 255, 0, 0],
            false,
        )]);
        assert!(a[0].is_likely_virtual);
        let b = local_addrs_from(&[iface("eth0", [172, 16, 5, 5], [255, 255, 0, 0], false)]);
        assert!(!b[0].is_likely_virtual);
        let c = local_addrs_from(&[iface("Ethernet", [10, 0, 0, 5], [255, 0, 0, 0], false)]);
        assert!(!c[0].is_likely_virtual);
    }

    #[test]
    fn announcement_targets_start_with_the_real_lan() {
        let t = announce_targets(&sim(), 7879);
        assert_eq!(t[0], "192.168.1.255:7879".parse::<SocketAddr>().unwrap());
        assert!(t.contains(&"255.255.255.255:7879".parse().unwrap()));
        assert!(
            t.contains(&"172.28.15.255:7879".parse().unwrap()),
            "every interface"
        );
        assert!(t.iter().all(|a| !a.ip().is_loopback() && a.port() == 7879));
        let mut dedup = t.clone();
        dedup.sort();
        dedup.dedup();
        assert_eq!(dedup.len(), t.len());
    }

    #[test]
    fn host_routes_without_broadcast_are_skipped() {
        let t = announce_targets(
            &[iface("eth0", [10, 0, 0, 9], [255, 255, 255, 255], false)],
            1,
        );
        assert_eq!(t, vec!["255.255.255.255:1".parse::<SocketAddr>().unwrap()]);
    }
}
