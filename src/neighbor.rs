//! Read-only rtnetlink neighbor discovery (ARP/NDP cache), no root and no probes.
//! All kernel-owned data are parsed with checked slice access, not transmuted.

use std::collections::BTreeMap;
use std::ffi::{c_int, c_void};
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::os::fd::RawFd;
use std::time::{Duration, Instant};

const AF_NETLINK: c_int = 16;
const SOCK_RAW: c_int = 3;
const SOCK_CLOEXEC: c_int = 0o2000000;
const NETLINK_ROUTE: c_int = 0;
const AF_INET: u8 = 2;
const AF_INET6: u8 = 10;
const RTM_NEWNEIGH: u16 = 28;
const RTM_GETNEIGH: u16 = 30;
const NLM_F_REQUEST: u16 = 1;
const NLM_F_DUMP: u16 = 0x300;
const NLMSG_ERROR: u16 = 2;
const NLMSG_DONE: u16 = 3;
const NDA_DST: u16 = 1;
const NDA_LLADDR: u16 = 2;
const POLLIN: i16 = 1;
const MAX_NEIGHBORS: usize = 8192;
const SEQ: u32 = 0x5049_5045;

#[repr(C)]
struct SockAddrNl {
    family: u16,
    pad: u16,
    pid: u32,
    groups: u32,
}

#[repr(C)]
struct PollFd {
    fd: c_int,
    events: i16,
    revents: i16,
}

unsafe extern "C" {
    fn socket(family: c_int, ty: c_int, protocol: c_int) -> c_int;
    fn close(fd: c_int) -> c_int;
    fn bind(fd: c_int, addr: *const SockAddrNl, len: u32) -> c_int;
    fn sendto(
        fd: c_int,
        data: *const c_void,
        len: usize,
        flags: c_int,
        addr: *const SockAddrNl,
        addr_len: u32,
    ) -> isize;
    fn recvfrom(
        fd: c_int,
        data: *mut c_void,
        len: usize,
        flags: c_int,
        addr: *mut SockAddrNl,
        addr_len: *mut u32,
    ) -> isize;
    fn poll(fds: *mut PollFd, n: usize, timeout_ms: c_int) -> c_int;
}

struct Socket(RawFd);
impl Drop for Socket {
    fn drop(&mut self) {
        // SAFETY: Socket uniquely owns its Linux file descriptor.
        unsafe {
            close(self.0);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Neighbor {
    pub ifindex: i32,
    pub ip: IpAddr,
    pub mac: Vec<u8>,
    pub state: u16,
    pub active: bool,
}

fn align4(n: usize) -> usize {
    (n + 3) & !3
}

fn u16ne(data: &[u8]) -> Option<u16> {
    Some(u16::from_ne_bytes(data.get(..2)?.try_into().ok()?))
}

fn u32ne(data: &[u8]) -> Option<u32> {
    Some(u32::from_ne_bytes(data.get(..4)?.try_into().ok()?))
}

fn neighbor_message(data: &[u8]) -> Result<Option<Neighbor>, String> {
    if data.len() < 12 {
        return Err("short netlink neighbor message".into());
    }
    let family = data[0];
    let ifindex = i32::from_ne_bytes(data[4..8].try_into().map_err(|_| "bad ifindex")?);
    if ifindex <= 0 || !matches!(family, AF_INET | AF_INET6) {
        return Ok(None);
    }
    let state = u16ne(&data[8..10]).ok_or("bad neighbor state")?;
    let mut ip = None;
    let mut mac = Vec::new();
    let mut offset = 12_usize;
    while offset + 4 <= data.len() {
        let len = u16ne(&data[offset..]).ok_or("bad rtattr len")? as usize;
        let kind = u16ne(&data[offset + 2..]).ok_or("bad rtattr kind")? & 0x3fff;
        if len < 4 || offset + len > data.len() {
            return Err("truncated netlink neighbor attribute".into());
        }
        let value = &data[offset + 4..offset + len];
        match (kind, family) {
            (NDA_DST, AF_INET) if value.len() == 4 => {
                ip = Some(IpAddr::V4(Ipv4Addr::new(
                    value[0], value[1], value[2], value[3],
                )));
            }
            (NDA_DST, AF_INET6) if value.len() == 16 => {
                let bytes: [u8; 16] = value.try_into().map_err(|_| "bad IPv6 neighbor")?;
                ip = Some(IpAddr::V6(Ipv6Addr::from(bytes)));
            }
            (NDA_LLADDR, _) if (1..=32).contains(&value.len()) => {
                mac = value.to_vec();
            }
            _ => {}
        }
        offset = offset
            .checked_add(align4(len))
            .ok_or("netlink attribute overflow")?;
    }
    if offset > data.len() + 3 || data[offset.min(data.len())..].iter().any(|b| *b != 0) {
        return Err("invalid neighbor attribute padding".into());
    }
    Ok(ip.map(|ip| Neighbor {
        ifindex,
        ip,
        mac,
        state,
        active: false,
    }))
}

/// Parse one rtnetlink multipart datagram. Returns true on NLMSG_DONE.
pub fn parse_batch(frame: &[u8], neighbors: &mut Vec<Neighbor>) -> Result<bool, String> {
    let mut offset = 0_usize;
    let mut finished = false;
    while offset < frame.len() {
        if frame.len() - offset < 16 {
            return Err("truncated netlink header".into());
        }
        let header = &frame[offset..];
        let len = u32ne(header).ok_or("bad nlmsg length")? as usize;
        let ty = u16ne(&header[4..]).ok_or("bad nlmsg type")?;
        let seq = u32ne(&header[8..]).ok_or("bad nlmsg sequence")?;
        if len < 16 || len > frame.len() - offset {
            return Err("truncated netlink datagram".into());
        }
        if seq == SEQ {
            match ty {
                NLMSG_DONE => finished = true,
                NLMSG_ERROR => {
                    let errno = header
                        .get(16..20)
                        .and_then(|b| <[u8; 4]>::try_from(b).ok())
                        .map(i32::from_ne_bytes)
                        .ok_or("malformed netlink error")?;
                    if errno < 0 {
                        return Err(format!(
                            "RTM_GETNEIGH failed: {}",
                            io::Error::from_raw_os_error(errno.checked_neg().unwrap_or(i32::MAX))
                        ));
                    }
                }
                RTM_NEWNEIGH => {
                    if let Some(neighbor) = neighbor_message(&header[16..len])? {
                        if neighbors.len() >= MAX_NEIGHBORS {
                            return Err("neighbor table exceeds hard limit of 8192 entries".into());
                        }
                        neighbors.push(neighbor);
                    }
                }
                _ => {}
            }
        }
        let step = align4(len);
        if step > frame.len() - offset {
            if len == frame.len() - offset {
                break;
            }
            return Err("bad netlink alignment".into());
        }
        offset += step;
    }
    Ok(finished)
}

pub fn interfaces() -> Result<BTreeMap<i32, String>, String> {
    let mut result = BTreeMap::new();
    for item in std::fs::read_dir("/sys/class/net")
        .map_err(|e| format!("cannot enumerate local Linux interfaces: {e}"))?
    {
        let item = item.map_err(|e| format!("cannot read network interface: {e}"))?;
        let name = item.file_name().to_string_lossy().to_string();
        let raw = std::fs::read_to_string(item.path().join("ifindex"))
            .map_err(|e| format!("cannot read index for interface {name}: {e}"))?;
        let index = raw
            .trim()
            .parse::<i32>()
            .map_err(|_| format!("invalid ifindex for interface {name}"))?;
        result.insert(index, name);
    }
    Ok(result)
}

/// Return the kernel's current neighbor cache; it may contain stale entries and
/// does not discover hosts that were never reached by the system.
pub fn dump() -> Result<Vec<Neighbor>, String> {
    // SAFETY: Linux sockaddr_nl ABI, well-defined constants and checked syscall result.
    let fd = unsafe { socket(AF_NETLINK, SOCK_RAW | SOCK_CLOEXEC, NETLINK_ROUTE) };
    if fd < 0 {
        return Err(format!(
            "cannot open read-only route netlink socket: {}",
            io::Error::last_os_error()
        ));
    }
    let sock = Socket(fd);
    let local = SockAddrNl {
        family: AF_NETLINK as u16,
        pad: 0,
        pid: 0,
        groups: 0,
    };
    // SAFETY: initialized sockaddr_nl, size matches Linux layout.
    if unsafe { bind(sock.0, &local, 12) } < 0 {
        return Err(format!(
            "cannot bind route netlink socket: {}",
            io::Error::last_os_error()
        ));
    }
    let mut req = [0_u8; 28]; // nlmsghdr (16) + ndmsg (12)
    req[0..4].copy_from_slice(&28_u32.to_ne_bytes());
    req[4..6].copy_from_slice(&RTM_GETNEIGH.to_ne_bytes());
    req[6..8].copy_from_slice(&(NLM_F_REQUEST | NLM_F_DUMP).to_ne_bytes());
    req[8..12].copy_from_slice(&SEQ.to_ne_bytes());
    let kernel = SockAddrNl {
        family: AF_NETLINK as u16,
        pad: 0,
        pid: 0,
        groups: 0,
    };
    // SAFETY: req and kernel are live, correctly sized buffers.
    let sent = unsafe { sendto(sock.0, req.as_ptr().cast(), req.len(), 0, &kernel, 12) };
    if sent != req.len() as isize {
        return Err(format!(
            "RTM_GETNEIGH request failed: {}",
            io::Error::last_os_error()
        ));
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut buffer = vec![0_u8; 65_536]; // One reusable buffer; no per-message allocation.
    let mut neighbors = Vec::new();
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err("rtnetlink neighbor dump exceeded 3s deadline".into());
        }
        let ms = deadline
            .saturating_duration_since(now)
            .as_millis()
            .min(3000) as c_int;
        let mut pollfd = PollFd {
            fd: sock.0,
            events: POLLIN,
            revents: 0,
        };
        // SAFETY: initialized writable pollfd.
        let polled = unsafe { poll(&mut pollfd, 1, ms) };
        if polled == 0 {
            return Err("rtnetlink neighbor dump timed out".into());
        }
        if polled < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(format!("rtnetlink poll: {error}"));
        }
        let mut src = SockAddrNl {
            family: 0,
            pad: 0,
            pid: 0,
            groups: 0,
        };
        let mut src_len = 12_u32;
        // SAFETY: buffer and sockaddr_nl have valid writable storage.
        let received = unsafe {
            recvfrom(
                sock.0,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                0,
                &mut src,
                &mut src_len,
            )
        };
        if received < 0 {
            return Err(format!("rtnetlink recv: {}", io::Error::last_os_error()));
        }
        if src_len != 12 || src.family != AF_NETLINK as u16 || src.pid != 0 {
            return Err("received neighbor data from non-kernel netlink sender".into());
        }
        if parse_batch(&buffer[..received as usize], &mut neighbors)? {
            return Ok(neighbors);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rtattr(kind: u16, value: &[u8]) -> Vec<u8> {
        let len = 4 + value.len();
        let mut attr = vec![0_u8; align4(len)];
        attr[0..2].copy_from_slice(&(len as u16).to_ne_bytes());
        attr[2..4].copy_from_slice(&kind.to_ne_bytes());
        attr[4..4 + value.len()].copy_from_slice(value);
        attr
    }

    fn batch(family: u8, ip: &[u8], mac: &[u8]) -> Vec<u8> {
        let mut body = vec![0_u8; 12];
        body[0] = family;
        body[4..8].copy_from_slice(&7_i32.to_ne_bytes());
        body[8..10].copy_from_slice(&2_u16.to_ne_bytes());
        body.extend(rtattr(NDA_DST, ip));
        body.extend(rtattr(NDA_LLADDR, mac));
        let len = body.len() + 16;
        let mut packet = vec![0_u8; len];
        packet[..4].copy_from_slice(&(len as u32).to_ne_bytes());
        packet[4..6].copy_from_slice(&RTM_NEWNEIGH.to_ne_bytes());
        packet[8..12].copy_from_slice(&SEQ.to_ne_bytes());
        packet[16..].copy_from_slice(&body);
        packet
    }

    #[test]
    fn parses_ipv4_and_ipv6_neighbors() {
        let mut rows = Vec::new();
        let v4 = batch(AF_INET, &[192, 168, 2, 3], &[0, 17, 34, 51, 68, 85]);
        assert!(!parse_batch(&v4, &mut rows).unwrap());
        assert_eq!(rows[0].ip.to_string(), "192.168.2.3");
        assert_eq!(rows[0].mac, [0, 17, 34, 51, 68, 85]);
        assert_eq!(rows[0].ifindex, 7);
        let v6 = batch(AF_INET6, &Ipv6Addr::LOCALHOST.octets(), &[0xaa; 6]);
        parse_batch(&v6, &mut rows).unwrap();
        assert_eq!(rows[1].ip, IpAddr::V6(Ipv6Addr::LOCALHOST));
    }

    #[test]
    fn rejects_invalid_lengths_and_kernel_errors() {
        let mut rows = Vec::new();
        let mut b = batch(AF_INET, &[10, 0, 0, 1], &[0; 6]);
        b[0..4].copy_from_slice(&0_u32.to_ne_bytes());
        assert!(parse_batch(&b, &mut rows).is_err());
        let mut b = batch(AF_INET, &[10, 0, 0, 1], &[0; 6]);
        let i = 16 + 12;
        b[i..i + 2].copy_from_slice(&0_u16.to_ne_bytes());
        assert!(parse_batch(&b, &mut rows).is_err());
        let mut error = vec![0_u8; 20];
        error[0..4].copy_from_slice(&20_u32.to_ne_bytes());
        error[4..6].copy_from_slice(&NLMSG_ERROR.to_ne_bytes());
        error[8..12].copy_from_slice(&SEQ.to_ne_bytes());
        error[16..20].copy_from_slice(&(-1_i32).to_ne_bytes());
        assert!(parse_batch(&error, &mut rows).is_err());
    }

    #[test]
    fn source_sequence_is_ignored() {
        let mut rows = Vec::new();
        let mut b = batch(AF_INET, &[10, 0, 0, 1], &[0; 6]);
        b[8..12].copy_from_slice(&0_u32.to_ne_bytes());
        parse_batch(&b, &mut rows).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn netlink_abi_sizes_are_stable() {
        assert_eq!(std::mem::size_of::<SockAddrNl>(), 12);
        assert_eq!(std::mem::size_of::<PollFd>(), 8);
    }
}
