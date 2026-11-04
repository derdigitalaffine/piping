//! Explicit, privilege-gated IPv4 Ethernet ARP discovery on a directly connected
//! local interface. Never called in passive discovery mode. No auto-sudo.

use crate::neighbor::Neighbor;
use crate::range;
use std::collections::HashSet;
use std::ffi::{c_int, c_ulong, c_void};
use std::io;
use std::net::{IpAddr, Ipv4Addr};
use std::os::fd::RawFd;
use std::time::{Duration, Instant};

const AF_INET: c_int = 2;
const AF_PACKET: c_int = 17;
const SOCK_DGRAM: c_int = 2;
const SOCK_RAW: c_int = 3;
const SOCK_CLOEXEC: c_int = 0o2000000;
const ETH_P_ARP: u16 = 0x0806;
const SIOCGIFADDR: c_ulong = 0x8915;
const MSG_DONTWAIT: c_int = 0x40;
const POLLIN: i16 = 1;

#[repr(C)]
struct SockAddrLl {
    family: u16,
    protocol: u16,
    ifindex: i32,
    hatype: u16,
    pkttype: u8,
    halen: u8,
    addr: [u8; 8],
}

#[repr(C, align(8))]
struct IfReq {
    name: [u8; 16],
    data: [u8; 24],
}

#[repr(C)]
struct PollFd {
    fd: c_int,
    events: i16,
    revents: i16,
}

unsafe extern "C" {
    fn socket(domain: c_int, ty: c_int, protocol: c_int) -> c_int;
    fn close(fd: c_int) -> c_int;
    fn ioctl(fd: c_int, request: c_ulong, arg: *mut c_void) -> c_int;
    fn bind(fd: c_int, addr: *const c_void, len: u32) -> c_int;
    fn sendto(
        fd: c_int,
        data: *const c_void,
        len: usize,
        flags: c_int,
        addr: *const c_void,
        addr_len: u32,
    ) -> isize;
    fn recvfrom(
        fd: c_int,
        data: *mut c_void,
        len: usize,
        flags: c_int,
        addr: *mut c_void,
        addr_len: *mut u32,
    ) -> isize;
    fn poll(fds: *mut PollFd, n: usize, timeout_ms: c_int) -> c_int;
}

struct Socket(RawFd);
impl Drop for Socket {
    fn drop(&mut self) {
        // SAFETY: Socket exclusively owns the Linux socket descriptor.
        unsafe {
            close(self.0);
        }
    }
}

fn interface_ipv4(interface: &str) -> Result<Ipv4Addr, String> {
    // SAFETY: Linux IPv4 datagram socket signature and constants are correct.
    let fd = unsafe { socket(AF_INET, SOCK_DGRAM | SOCK_CLOEXEC, 0) };
    if fd < 0 {
        return Err(format!(
            "cannot inspect local IPv4 address: {}",
            io::Error::last_os_error()
        ));
    }
    let sock = Socket(fd);
    let mut ifreq = IfReq {
        name: [0; 16],
        data: [0; 24],
    };
    ifreq.name[..interface.len()].copy_from_slice(interface.as_bytes());
    // SAFETY: aligned 40-byte ifreq, valid NUL-terminated interface name.
    if unsafe { ioctl(sock.0, SIOCGIFADDR, (&mut ifreq as *mut IfReq).cast()) } < 0 {
        return Err(format!(
            "cannot get IPv4 address for {interface}: {}",
            io::Error::last_os_error()
        ));
    }
    if u16::from_ne_bytes([ifreq.data[0], ifreq.data[1]]) != AF_INET as u16 {
        return Err("interface does not have an IPv4 address".into());
    }
    Ok(Ipv4Addr::new(
        ifreq.data[4],
        ifreq.data[5],
        ifreq.data[6],
        ifreq.data[7],
    ))
}

fn interface_mac(interface: &str) -> Result<[u8; 6], String> {
    let text = std::fs::read_to_string(format!("/sys/class/net/{interface}/address"))
        .map_err(|e| format!("cannot read interface MAC: {e}"))?;
    let pieces = text.trim().split(':').collect::<Vec<_>>();
    if pieces.len() != 6 {
        return Err("active ARP needs a six-byte Ethernet MAC".into());
    }
    let mut mac = [0_u8; 6];
    for (dest, piece) in mac.iter_mut().zip(pieces) {
        *dest = u8::from_str_radix(piece, 16).map_err(|_| "invalid interface MAC")?;
    }
    if mac[0] & 1 != 0 || mac == [0; 6] {
        return Err("active ARP requires a unicast Ethernet MAC".into());
    }
    Ok(mac)
}

/// Linux /proc/net/route stores IPv4 octets in little-endian hex.
fn parse_route_ip(raw: &str) -> Option<u32> {
    let encoded = u32::from_str_radix(raw, 16).ok()?;
    Some(u32::from(Ipv4Addr::from(encoded.to_le_bytes())))
}

/// Parse each route once (no per-target allocations), only routes that are up,
/// directly connected and do not use a gateway.
fn connected_routes(routes: &str, iface: &str) -> Vec<(u32, u32)> {
    let mut connected = Vec::new();
    for line in routes.lines().skip(1) {
        let mut parts = line.split_whitespace();
        let (
            Some(name),
            Some(destination),
            Some(gateway),
            Some(flags),
            Some(_refs),
            Some(_use),
            Some(_metric),
            Some(mask),
        ) = (
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
        )
        else {
            continue;
        };
        if name != iface {
            continue;
        }
        let (Some(dest), Some(gw), Some(mask), Ok(flags)) = (
            parse_route_ip(destination),
            parse_route_ip(gateway),
            parse_route_ip(mask),
            u32::from_str_radix(flags, 16),
        ) else {
            continue;
        };
        if flags & 1 != 0 && flags & 2 == 0 && gw == 0 && mask != 0 {
            connected.push((dest & mask, mask));
        }
    }
    connected
}

/// Infer a safe, single local IPv4 network only on explicit --scan-local.
/// Refuses public/non-RFC1918 addresses and subnets larger than /24 to avoid
/// surprising probes or overly broad packet bursts.
fn infer_local_cidr(routes: &str, interface: &str) -> Result<String, String> {
    let mut candidates = Vec::new();
    for (network, mask) in connected_routes(routes, interface) {
        let prefix = mask.leading_ones();
        if !(24..=31).contains(&prefix)
            || mask != u32::MAX << (32 - prefix)
            || !Ipv4Addr::from(network).is_private()
        {
            continue;
        }
        candidates.push(format!("{}/{prefix}", Ipv4Addr::from(network)));
    }
    candidates.sort();
    candidates.dedup();
    if candidates.len() == 1 {
        Ok(candidates.remove(0))
    } else {
        Err("cannot safely auto-select one private on-link /24..../31 IPv4 network; use '--active CIDR --interface IFACE' explicitly".into())
    }
}

pub fn local_cidr(interface: &str) -> Result<String, String> {
    if !crate::discovery::valid_interface(interface) {
        return Err("invalid interface for --scan-local".into());
    }
    let routes = std::fs::read_to_string("/proc/net/route")
        .map_err(|e| format!("cannot inspect local IPv4 routes: {e}"))?;
    infer_local_cidr(&routes, interface)
}

fn is_onlink(routes: &[(u32, u32)], target: Ipv4Addr) -> bool {
    let ip = u32::from(target);
    routes.iter().any(|&(network, mask)| ip & mask == network)
}

fn request(src_mac: [u8; 6], src_ip: Ipv4Addr, target: Ipv4Addr) -> [u8; 42] {
    let mut frame = [0_u8; 42];
    frame[0..6].fill(0xff); // Ethernet broadcast.
    frame[6..12].copy_from_slice(&src_mac);
    frame[12..14].copy_from_slice(&ETH_P_ARP.to_be_bytes());
    frame[14..16].copy_from_slice(&1_u16.to_be_bytes()); // Ethernet.
    frame[16..18].copy_from_slice(&0x0800_u16.to_be_bytes()); // IPv4.
    frame[18] = 6;
    frame[19] = 4;
    frame[20..22].copy_from_slice(&1_u16.to_be_bytes()); // ARP request.
    frame[22..28].copy_from_slice(&src_mac);
    frame[28..32].copy_from_slice(&src_ip.octets());
    frame[38..42].copy_from_slice(&target.octets());
    frame
}

fn parse_reply(frame: &[u8], own_mac: &[u8; 6], own_ip: Ipv4Addr) -> Option<(Ipv4Addr, [u8; 6])> {
    if frame.len() < 42
        || frame[12..14] != ETH_P_ARP.to_be_bytes()
        || frame[14..16] != 1_u16.to_be_bytes()
        || frame[16..18] != 0x0800_u16.to_be_bytes()
        || frame[18] != 6
        || frame[19] != 4
        || frame[20..22] != 2_u16.to_be_bytes()
        || frame[0..6] != *own_mac
        || frame[32..38] != *own_mac
        || frame[38..42] != own_ip.octets()
        || frame[6..12] != frame[22..28]
        || (frame[22] & 1) != 0
        || frame[22..28] == [0; 6]
    {
        return None;
    }
    let mut mac = [0_u8; 6];
    mac.copy_from_slice(&frame[22..28]);
    let ip = Ipv4Addr::new(frame[28], frame[29], frame[30], frame[31]);
    Some((ip, mac))
}

fn arp_error(context: &str, error: io::Error) -> String {
    if matches!(error.raw_os_error(), Some(1 | 13)) {
        format!(
            "{context}: CAP_NET_RAW required for explicit active ARP ({error}); passive 'pipi discover' needs no privileges"
        )
    } else {
        format!("{context}: {error}")
    }
}

pub fn scan(
    cidr: &str,
    interface: &str,
    ifindex: i32,
    pps: u32,
    timeout: Duration,
    max_hosts: usize,
) -> Result<Vec<Neighbor>, String> {
    if ifindex <= 0 || !crate::discovery::valid_interface(interface) {
        return Err("active ARP needs a valid Linux interface".into());
    }
    let parsed = range::expand(cidr, max_hosts, Some(false))?;
    let mut targets = Vec::with_capacity(parsed.len());
    for ip in parsed {
        let IpAddr::V4(ip) = ip else {
            return Err("active discovery supports IPv4 only".into());
        };
        targets.push(ip);
    }
    if targets.is_empty() {
        return Err("CIDR has no usable IPv4 hosts".into());
    }
    let own_ip = interface_ipv4(interface)?;
    if own_ip.is_loopback() || interface == "lo" {
        return Err("active Ethernet ARP is not supported on loopback".into());
    }
    let own_mac = interface_mac(interface)?;
    let routes = std::fs::read_to_string("/proc/net/route")
        .map_err(|e| format!("cannot inspect IPv4 on-link routes: {e}"))?;
    let connected = connected_routes(&routes, interface);
    if !targets.iter().all(|ip| is_onlink(&connected, *ip)) {
        return Err("active CIDR must be entirely on a directly connected IPv4 route of the chosen interface (no gateways or off-link targets)".into());
    }
    targets.retain(|ip| *ip != own_ip);
    if targets.is_empty() {
        return Err("active CIDR contains only the interface's own address".into());
    }
    let wanted: HashSet<Ipv4Addr> = targets.iter().copied().collect();
    // SAFETY: Linux Ethernet packet socket, raw ARP ethertype in network order.
    let fd = unsafe {
        socket(
            AF_PACKET,
            SOCK_RAW | SOCK_CLOEXEC,
            c_int::from(ETH_P_ARP.to_be()),
        )
    };
    if fd < 0 {
        return Err(arp_error(
            "cannot open Ethernet ARP socket",
            io::Error::last_os_error(),
        ));
    }
    let sock = Socket(fd);
    let local = SockAddrLl {
        family: AF_PACKET as u16,
        protocol: ETH_P_ARP.to_be(),
        ifindex,
        hatype: 0,
        pkttype: 0,
        halen: 0,
        addr: [0; 8],
    };
    // SAFETY: sockaddr_ll has the Linux fixed 20-byte ABI.
    if unsafe { bind(sock.0, (&local as *const SockAddrLl).cast(), 20) } < 0 {
        return Err(arp_error(
            "cannot bind Ethernet ARP socket",
            io::Error::last_os_error(),
        ));
    }
    let dest = SockAddrLl {
        family: AF_PACKET as u16,
        protocol: ETH_P_ARP.to_be(),
        ifindex,
        hatype: 0,
        pkttype: 0,
        halen: 6,
        addr: [255, 255, 255, 255, 255, 255, 0, 0],
    };
    let interval = Duration::from_secs_f64(1.0 / f64::from(pps));
    let mut next_send = Instant::now();
    let mut finish_at = None;
    let mut sent = 0_usize;
    let mut seen = HashSet::with_capacity(targets.len());
    let mut found = Vec::with_capacity(targets.len());
    let mut buffer = [0_u8; 2048];

    loop {
        let now = Instant::now();
        if finish_at.is_some_and(|deadline| now >= deadline) {
            break;
        }
        if sent < targets.len() && now >= next_send {
            let frame = request(own_mac, own_ip, targets[sent]);
            // SAFETY: frame and destination sockaddr_ll are initialized and live.
            let wrote = unsafe { sendto(sock.0, frame.as_ptr().cast(), frame.len(), 0, (&dest as *const SockAddrLl).cast(), 20) };
            if wrote < 0 {
                return Err(arp_error("send ARP request", io::Error::last_os_error()));
            }
            if wrote as usize != frame.len() {
                return Err("short Ethernet ARP frame sent".into());
            }
            sent += 1;
            next_send = Instant::now() + interval;
            if sent == targets.len() {
                finish_at = Some(Instant::now() + timeout);
            }
        }
        let now = Instant::now();
        let deadline = finish_at.unwrap_or(next_send);
        let wait = deadline.saturating_duration_since(now);
        let ms = wait.as_micros().div_ceil(1000).min(1000) as c_int;
        let mut pfd = PollFd {
            fd: sock.0,
            events: POLLIN,
            revents: 0,
        };
        // SAFETY: initialized pollfd, no pointers retained.
        let status = unsafe { poll(&mut pfd, 1, ms) };
        if status < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(arp_error("poll Ethernet ARP", err));
        }
        if status == 0 {
            continue;
        }
        for _ in 0..64 {
            let mut source = SockAddrLl {
                family: 0,
                protocol: 0,
                ifindex: 0,
                hatype: 0,
                pkttype: 0,
                halen: 0,
                addr: [0; 8],
            };
            let mut src_len = 20_u32;
            // SAFETY: bounded initialized frame and sockaddr buffers.
            let len = unsafe {
                recvfrom(
                    sock.0,
                    buffer.as_mut_ptr().cast(),
                    buffer.len(),
                    MSG_DONTWAIT,
                    (&mut source as *mut SockAddrLl).cast(),
                    &mut src_len,
                )
            };
            if len < 0 {
                let err = io::Error::last_os_error();
                if matches!(
                    err.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                ) {
                    break;
                }
                return Err(arp_error("recv Ethernet ARP", err));
            }
            if src_len < 12 || source.ifindex != ifindex || source.pkttype == 4 {
                continue; // Ignore packets from other interfaces or locally outgoing frames.
            }
            if let Some((ip, mac)) = parse_reply(&buffer[..len as usize], &own_mac, own_ip) {
                if wanted.contains(&ip) && seen.insert(ip) {
                    found.push(Neighbor {
                        ifindex,
                        ip: IpAddr::V4(ip),
                        mac: mac.to_vec(),
                        state: 2,
                        active: true,
                    });
                }
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROUTES: &str = "Iface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\tMTU\tWindow\tIRTT\n\
eth0\t0002A8C0\t00000000\t0001\t0\t0\t0\t00FFFFFF\t0\t0\t0\n\
eth0\t00000000\t0102A8C0\t0003\t0\t0\t0\t00000000\t0\t0\t0\n\
eth1\t0000A8C0\t00000000\t0001\t0\t0\t0\t0000FFFF\t0\t0\t0\n";

    #[test]
    fn direct_route_allows_only_the_correct_interface_and_subnet() {
        assert!(is_onlink(
            &connected_routes(ROUTES, "eth0"),
            Ipv4Addr::new(192, 168, 2, 42)
        ));
        assert!(!is_onlink(
            &connected_routes(ROUTES, "eth0"),
            Ipv4Addr::new(192, 168, 3, 42)
        ));
        assert!(is_onlink(
            &connected_routes(ROUTES, "eth1"),
            Ipv4Addr::new(192, 168, 2, 42)
        ));
        assert!(!is_onlink(
            &connected_routes(ROUTES, "lo"),
            Ipv4Addr::new(192, 168, 2, 42)
        ));
    }

    #[test]
    fn automatic_local_selection_requires_single_safe_private_link() {
        assert_eq!(infer_local_cidr(ROUTES, "eth0").unwrap(), "192.168.2.0/24");
        assert!(infer_local_cidr(ROUTES, "eth1").is_err()); // /16 too broad.
        assert!(infer_local_cidr(ROUTES, "unknown").is_err());
        let mut public_routes = ROUTES.to_owned();
        public_routes.push_str("pub0\t000071CB\t00000000\t0001\t0\t0\t0\t00FFFFFF\t0\t0\t0\n");
        assert!(infer_local_cidr(&public_routes, "pub0").is_err());
    }

    #[test]
    fn arp_request_has_correct_ethernet_and_protocol_fields() {
        let mac = [0, 17, 34, 51, 68, 85];
        let req = request(mac, Ipv4Addr::new(10, 0, 0, 1), Ipv4Addr::new(10, 0, 0, 2));
        assert_eq!(&req[0..6], &[255; 6]);
        assert_eq!(&req[6..12], &mac);
        assert_eq!(&req[12..14], &[0x08, 0x06]);
        assert_eq!(&req[20..22], &[0, 1]);
        assert_eq!(&req[38..42], &[10, 0, 0, 2]);
    }

    #[test]
    fn accepts_matching_reply_and_rejects_injection() {
        let mac = [0, 17, 34, 51, 68, 85];
        let peer = [0x02, 0xaa, 0xbb, 0xcc, 0xdd, 0xee];
        let mut frame = request(mac, Ipv4Addr::new(10, 0, 0, 1), Ipv4Addr::new(10, 0, 0, 2));
        frame[..6].copy_from_slice(&mac);
        frame[6..12].copy_from_slice(&peer);
        frame[20..22].copy_from_slice(&2_u16.to_be_bytes());
        frame[22..28].copy_from_slice(&peer);
        frame[28..32].copy_from_slice(&[10, 0, 0, 2]);
        frame[32..38].copy_from_slice(&mac);
        frame[38..42].copy_from_slice(&[10, 0, 0, 1]);
        assert_eq!(
            parse_reply(&frame, &mac, Ipv4Addr::new(10, 0, 0, 1)),
            Some((Ipv4Addr::new(10, 0, 0, 2), peer))
        );
        assert!(parse_reply(&frame[..41], &mac, Ipv4Addr::new(10, 0, 0, 1)).is_none());
        let mut corrupt = frame;
        corrupt[6] ^= 1;
        assert!(parse_reply(&corrupt, &mac, Ipv4Addr::new(10, 0, 0, 1)).is_none());
        corrupt = frame;
        corrupt[38] ^= 1;
        assert!(parse_reply(&corrupt, &mac, Ipv4Addr::new(10, 0, 0, 1)).is_none());
    }

    #[test]
    fn linux_abi_layout_is_stable() {
        assert_eq!(std::mem::size_of::<SockAddrLl>(), 20);
        assert_eq!(std::mem::size_of::<IfReq>(), 40);
        assert_eq!(std::mem::size_of::<PollFd>(), 8);
    }
}
