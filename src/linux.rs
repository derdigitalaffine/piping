//! Linux ICMP datagram (ping) sockets. No raw sockets, sudo, setuid or sysctl edits.
#![allow(clippy::cast_possible_truncation)]

use std::ffi::{c_int, c_void};
use std::io;
use std::mem::{size_of, zeroed};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::os::fd::RawFd;
use std::time::Duration;

const AF_INET: c_int = 2;
const AF_INET6: c_int = 10;
const SOCK_DGRAM: c_int = 2;
const SOCK_CLOEXEC: c_int = 0o2000000;
const IPPROTO_ICMP: c_int = 1;
const IPPROTO_ICMPV6: c_int = 58;
const IP_TTL: c_int = 2;
const IPV6_UNICAST_HOPS: c_int = 16;
const SOL_IP: c_int = 0;
const SOL_IPV6: c_int = 41;
const MSG_DONTWAIT: c_int = 0x40;
const POLLIN: i16 = 1;

#[repr(C)]
struct SockAddr {
    family: u16,
    data: [u8; 14],
}

#[repr(C)]
struct SockAddrIn {
    family: u16,
    port: u16,
    addr: u32,
    zero: [u8; 8],
}

#[repr(C)]
struct SockAddrIn6 {
    family: u16,
    port: u16,
    flowinfo: u32,
    addr: [u8; 16],
    scope_id: u32,
}

#[repr(C, align(8))]
struct SockAddrStorage {
    bytes: [u8; 128],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PollFd {
    fd: c_int,
    events: i16,
    revents: i16,
}

unsafe extern "C" {
    fn socket(domain: c_int, socket_type: c_int, protocol: c_int) -> c_int;
    fn close(fd: c_int) -> c_int;
    fn setsockopt(fd: c_int, level: c_int, opt: c_int, val: *const c_void, size: u32) -> c_int;
    fn sendto(
        fd: c_int,
        data: *const c_void,
        size: usize,
        flags: c_int,
        addr: *const SockAddr,
        len: u32,
    ) -> isize;
    fn recvfrom(
        fd: c_int,
        data: *mut c_void,
        size: usize,
        flags: c_int,
        addr: *mut SockAddr,
        len: *mut u32,
    ) -> isize;
    fn poll(fds: *mut PollFd, count: usize, timeout_ms: c_int) -> c_int;
}

#[derive(Debug)]
pub struct PingSocket {
    fd: RawFd,
    ipv6: bool,
}

impl Drop for PingSocket {
    fn drop(&mut self) {
        // SAFETY: The descriptor was created by socket() and is owned exclusively.
        unsafe {
            close(self.fd);
        }
    }
}

impl PingSocket {
    pub fn new(ipv6: bool, ttl: Option<u8>) -> io::Result<Self> {
        let (family, protocol) = if ipv6 {
            (AF_INET6, IPPROTO_ICMPV6)
        } else {
            (AF_INET, IPPROTO_ICMP)
        };
        // SAFETY: Linux constants and a valid socket() signature.
        let fd = unsafe { socket(family, SOCK_DGRAM | SOCK_CLOEXEC, protocol) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let sock = Self { fd, ipv6 };
        if let Some(ttl) = ttl {
            let value = c_int::from(ttl);
            let (level, option) = if ipv6 {
                (SOL_IPV6, IPV6_UNICAST_HOPS)
            } else {
                (SOL_IP, IP_TTL)
            };
            // SAFETY: value points to an initialized c_int for the duration of the call.
            if unsafe {
                setsockopt(
                    sock.fd,
                    level,
                    option,
                    (&value as *const c_int).cast(),
                    size_of::<c_int>() as u32,
                )
            } < 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(sock)
    }

    pub fn send(&self, target: SocketAddr, packet: &[u8]) -> io::Result<()> {
        if target.is_ipv6() != self.ipv6 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "IP family mismatch",
            ));
        }
        let sent = match target {
            SocketAddr::V4(v4) => {
                let sockaddr = SockAddrIn {
                    family: AF_INET as u16,
                    port: 0,
                    addr: u32::from_ne_bytes(v4.ip().octets()),
                    zero: [0; 8],
                };
                // SAFETY: sockaddr is initialized and valid for sendto().
                unsafe {
                    sendto(
                        self.fd,
                        packet.as_ptr().cast(),
                        packet.len(),
                        0,
                        (&sockaddr as *const SockAddrIn).cast(),
                        size_of::<SockAddrIn>() as u32,
                    )
                }
            }
            SocketAddr::V6(v6) => {
                let sockaddr = SockAddrIn6 {
                    family: AF_INET6 as u16,
                    port: 0,
                    flowinfo: v6.flowinfo(),
                    addr: v6.ip().octets(),
                    scope_id: v6.scope_id(),
                };
                // SAFETY: sockaddr is initialized and valid for sendto().
                unsafe {
                    sendto(
                        self.fd,
                        packet.as_ptr().cast(),
                        packet.len(),
                        0,
                        (&sockaddr as *const SockAddrIn6).cast(),
                        size_of::<SockAddrIn6>() as u32,
                    )
                }
            }
        };
        if sent < 0 {
            return Err(io::Error::last_os_error());
        }
        if sent as usize != packet.len() {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "short ICMP datagram",
            ));
        }
        Ok(())
    }

    pub fn recv(&self) -> io::Result<Option<Received>> {
        let mut buffer = [0_u8; 2048];
        // SAFETY: All-zero storage is a valid buffer for recvfrom to fill.
        let mut source: SockAddrStorage = unsafe { zeroed() };
        let mut source_len: u32 = size_of::<SockAddrStorage>() as u32;
        // SAFETY: Both byte buffers are initialized with adequate capacity.
        let length = unsafe {
            recvfrom(
                self.fd,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                MSG_DONTWAIT,
                (&mut source as *mut SockAddrStorage).cast(),
                &mut source_len,
            )
        };
        if length < 0 {
            let e = io::Error::last_os_error();
            return if matches!(
                e.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) || matches!(e.raw_os_error(), Some(101 | 111 | 113))
            {
                Ok(None)
            } else {
                Err(e)
            };
        }
        if source_len < 2 {
            return Ok(None);
        }
        // SAFETY: storage is 8-byte aligned, enough for sockaddr_in6, with length checked.
        let family = u16::from_ne_bytes([source.bytes[0], source.bytes[1]]);
        let ip = match c_int::from(family) {
            AF_INET if source_len as usize >= size_of::<SockAddrIn>() => {
                // SAFETY: recvfrom reported a sockaddr_in of sufficient size and alignment.
                let v4 = unsafe { &*((&source as *const SockAddrStorage).cast::<SockAddrIn>()) };
                IpAddr::V4(Ipv4Addr::from(v4.addr.to_ne_bytes()))
            }
            AF_INET6 if source_len as usize >= size_of::<SockAddrIn6>() => {
                // SAFETY: recvfrom reported a sockaddr_in6 of sufficient size and alignment.
                let v6 = unsafe { &*((&source as *const SockAddrStorage).cast::<SockAddrIn6>()) };
                IpAddr::V6(Ipv6Addr::from(v6.addr))
            }
            _ => return Ok(None),
        };
        let length = length as usize;
        Ok(parse_reply(&buffer[..length], self.ipv6).map(|echo| Received { source: ip, echo }))
    }

    pub fn fd(&self) -> RawFd {
        self.fd
    }
}

#[derive(Debug)]
pub struct Echo {
    pub seq: u16,
    pub nonce: u64,
    pub token: u64,
    pub bytes: usize,
}

#[derive(Debug)]
pub struct Received {
    pub source: IpAddr,
    pub echo: Echo,
}

/// Write only the 24 correlation/header bytes into a reusable, prefilled packet.
/// ICMP payload bytes beyond offset 24 are initialized once by the caller.
pub fn write_echo_request(packet: &mut [u8], ipv6: bool, seq: u16, nonce: u64, token: u64) {
    debug_assert!((24..=1408).contains(&packet.len()));
    packet[..8].copy_from_slice(&[
        if ipv6 { 128 } else { 8 },
        0,
        0,
        0, // Kernel computes checksum.
        0,
        0, // Linux ping sockets manage the Echo identifier.
        (seq >> 8) as u8,
        seq as u8,
    ]);
    packet[8..16].copy_from_slice(&nonce.to_be_bytes());
    packet[16..24].copy_from_slice(&token.to_be_bytes());
}

#[cfg(test)]
pub fn echo_request(ipv6: bool, seq: u16, nonce: u64, token: u64, size: usize) -> Vec<u8> {
    assert!((16..=1400).contains(&size));
    let mut packet = vec![0x42; size + 8];
    write_echo_request(&mut packet, ipv6, seq, nonce, token);
    packet
}

/// Validate structural echo fields and recover the 128-bit correlation marker.
pub fn parse_reply(packet: &[u8], ipv6: bool) -> Option<Echo> {
    let expected_type = if ipv6 { 129 } else { 0 };
    if packet.len() < 24 || packet[0] != expected_type || packet[1] != 0 {
        return None;
    }
    let seq = u16::from_be_bytes([packet[6], packet[7]]);
    let nonce = u64::from_be_bytes(packet[8..16].try_into().ok()?);
    let token = u64::from_be_bytes(packet[16..24].try_into().ok()?);
    Some(Echo {
        seq,
        nonce,
        token,
        bytes: packet.len(),
    })
}

pub struct SocketSet {
    pub v4: Option<PingSocket>,
    pub v6: Option<PingSocket>,
}

impl SocketSet {
    pub fn for_targets(targets: &[SocketAddr], ttl: Option<u8>) -> io::Result<Self> {
        let v4 = if targets.iter().any(SocketAddr::is_ipv4) {
            Some(PingSocket::new(false, ttl)?)
        } else {
            None
        };
        let v6 = if targets.iter().any(SocketAddr::is_ipv6) {
            Some(PingSocket::new(true, ttl)?)
        } else {
            None
        };
        Ok(Self { v4, v6 })
    }

    pub fn for_address(&self, addr: SocketAddr) -> &PingSocket {
        if addr.is_ipv4() {
            self.v4.as_ref().expect("socket for IPv4 target")
        } else {
            self.v6.as_ref().expect("socket for IPv6 target")
        }
    }

    /// Handle socket responses immediately: no heap Vec allocation per wakeup.
    /// Bounded to 256 frames per socket so high packet rates cannot starve timers.
    pub fn drain<F>(&self, mut handle: F) -> io::Result<()>
    where
        F: FnMut(Received),
    {
        for socket in self.v4.iter().chain(self.v6.iter()) {
            for _ in 0..256 {
                match socket.recv()? {
                    Some(reply) => handle(reply),
                    None => break,
                }
            }
        }
        Ok(())
    }

    pub fn wait(&self, timeout: Duration) -> io::Result<()> {
        // Fixed stack storage: at most one ping socket for each IP family.
        let empty = PollFd {
            fd: -1,
            events: 0,
            revents: 0,
        };
        let mut fds = [empty; 2];
        let mut n = 0;
        for sock in self.v4.iter().chain(self.v6.iter()) {
            fds[n] = PollFd {
                fd: sock.fd(),
                events: POLLIN,
                revents: 0,
            };
            n += 1;
        }
        // poll is interrupted by SIGINT/SIGTERM; do not wake every 200 ms.
        // Round sub-millisecond waits up rather than spinning with poll(0).
        let millis = timeout.as_micros().div_ceil(1000).min(60_000) as c_int;
        // SAFETY: fds[..n] is initialized and writable until the synchronous call returns.
        let result = unsafe { poll(fds.as_mut_ptr(), n, millis) };
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packets_are_big_endian_and_correlated() {
        let packet = echo_request(false, 515, 0x1122334455667788, 12, 56);
        assert_eq!(packet.len(), 64);
        assert_eq!(&packet[6..8], &[2, 3]);
        assert_eq!(&packet[8..16], &0x1122334455667788_u64.to_be_bytes());
        let mut reply = packet.clone();
        reply[0] = 0;
        let parsed = parse_reply(&reply, false).unwrap();
        assert_eq!(parsed.seq, 515);
        assert_eq!(parsed.token, 12);
        assert_eq!(parsed.bytes, 64);
    }

    #[test]
    fn reject_corrupted_and_wrong_family_echo() {
        let mut reply = echo_request(true, 5, 6, 7, 16);
        reply[0] = 129;
        assert!(parse_reply(&reply, false).is_none());
        assert!(parse_reply(&reply[..23], true).is_none());
        reply[1] = 1;
        assert!(parse_reply(&reply, true).is_none());
    }

    #[test]
    fn reusable_packet_preserves_payload_and_replaces_correlation() {
        let mut packet = vec![0x42; 64];
        write_echo_request(&mut packet, false, 5, 6, 7);
        assert_eq!(packet[0], 8);
        assert_eq!(packet[6..8], [0, 5]);
        assert!(packet[24..].iter().all(|byte| *byte == 0x42));
        write_echo_request(&mut packet, true, 42, 0x1122334455667788, 99);
        assert_eq!(packet[0], 128);
        assert_eq!(&packet[8..16], &0x1122334455667788_u64.to_be_bytes());
        assert_eq!(&packet[16..24], &99_u64.to_be_bytes());
        assert!(packet[24..].iter().all(|byte| *byte == 0x42));
    }

    #[test]
    fn socket_struct_layout_matches_linux() {
        assert_eq!(size_of::<SockAddrIn>(), 16);
        assert_eq!(size_of::<SockAddrIn6>(), 28);
        assert_eq!(size_of::<SockAddrStorage>(), 128);
    }
}
