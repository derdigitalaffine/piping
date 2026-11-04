//! Linux IP_RECVERR error-queue adapter for unprivileged ICMP ping sockets.
//! Time Exceeded / Destination Unreachable originate in the kernel error queue;
//! no raw IP socket or root privileges are necessary when ping sockets are allowed.

use crate::linux::{self, PingSocket, Received};
use std::ffi::{c_int, c_void};
use std::io;
use std::mem::size_of;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

const SOL_IP: c_int = 0;
const SOL_IPV6: c_int = 41;
const IP_TTL: c_int = 2;
const IPV6_UNICAST_HOPS: c_int = 16;
const IP_RECVERR: c_int = 11;
const IPV6_RECVERR: c_int = 25;
const MSG_DONTWAIT: c_int = 0x40;
const MSG_ERRQUEUE: c_int = 0x2000;
const MSG_CTRUNC: c_int = 0x08;
const MSG_TRUNC: c_int = 0x20;
const POLLIN: i16 = 1;
const POLLERR: i16 = 8;
const AF_INET: u16 = 2;
const AF_INET6: u16 = 10;

#[repr(C)]
struct IoVec {
    base: *mut c_void,
    len: usize,
}
#[repr(C)]
struct MsgHdr {
    name: *mut c_void,
    name_len: u32,
    iov: *mut IoVec,
    iov_len: usize,
    control: *mut c_void,
    control_len: usize,
    flags: c_int,
}
#[repr(C)]
struct PollFd {
    fd: c_int,
    events: i16,
    revents: i16,
}
#[repr(C, align(8))]
struct Control([u8; 256]);

unsafe extern "C" {
    fn setsockopt(fd: c_int, level: c_int, option: c_int, value: *const c_int, len: u32) -> c_int;
    fn recvmsg(fd: c_int, message: *mut MsgHdr, flags: c_int) -> isize;
    fn poll(fds: *mut PollFd, count: usize, timeout: c_int) -> c_int;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Expired,
    Unreachable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProbeId {
    pub seq: u16,
    pub nonce: Option<u64>,
    pub token: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct ErrorReply {
    pub destination: IpAddr,
    pub router: IpAddr,
    pub kind: ErrorKind,
    pub id: ProbeId,
}

fn ip_from_sockaddr(bytes: &[u8]) -> Option<IpAddr> {
    let family = u16::from_ne_bytes(bytes.get(..2)?.try_into().ok()?);
    match family {
        AF_INET if bytes.len() >= 16 => Some(IpAddr::V4(Ipv4Addr::new(
            bytes[4], bytes[5], bytes[6], bytes[7],
        ))),
        AF_INET6 if bytes.len() >= 28 => {
            Some(IpAddr::V6(Ipv6Addr::from(<[u8; 16]>::try_from(bytes.get(8..24)?).ok()?)))
        }
        _ => None,
    }
}

/// Linux ping error queues usually return the original ICMP Echo; some kernels
/// strip the 8-byte Echo header and return just the payload. Validate both.
/// A minimal RFC 792 quoted Echo header has a unique 16-bit sequence as fallback.
fn request_id(bytes: &[u8], ipv6: bool) -> Option<ProbeId> {
    let kind = if ipv6 { 128 } else { 8 };
    if bytes.len() >= 8 && bytes[0] == kind && bytes[1] == 0 {
        let seq = u16::from_be_bytes([bytes[6], bytes[7]]);
        if bytes.len() >= 24 {
            return Some(ProbeId {
                seq,
                nonce: Some(u64::from_be_bytes(bytes[8..16].try_into().ok()?)),
                token: Some(u64::from_be_bytes(bytes[16..24].try_into().ok()?)),
            });
        }
        return Some(ProbeId { seq, nonce: None, token: None });
    }
    // For a stripped ping payload the first 16 bytes are nonce + token.
    if bytes.len() >= 16 {
        let token = u64::from_be_bytes(bytes[8..16].try_into().ok()?);
        return Some(ProbeId {
            seq: token as u16,
            nonce: Some(u64::from_be_bytes(bytes[0..8].try_into().ok()?)),
            token: Some(token),
        });
    }
    None
}

fn parse_control(
    control: &[u8],
    original: &[u8],
    dest: IpAddr,
    ipv6: bool,
) -> Option<ErrorReply> {
    let header = size_of::<usize>() + 8;
    let mut cursor = 0_usize;
    while cursor + header <= control.len() {
        let len = usize::from_ne_bytes(control.get(cursor..cursor + size_of::<usize>())?.try_into().ok()?);
        let level = i32::from_ne_bytes(control.get(cursor + size_of::<usize>()..cursor + size_of::<usize>() + 4)?.try_into().ok()?);
        let ty = i32::from_ne_bytes(control.get(cursor + size_of::<usize>() + 4..cursor + header)?.try_into().ok()?);
        if len < header || len > control.len() - cursor {
            return None;
        }
        let value = control.get(cursor + header..cursor + len)?;
        if level == (if ipv6 { SOL_IPV6 } else { SOL_IP })
            && ty == (if ipv6 { IPV6_RECVERR } else { IP_RECVERR })
            && value.len() >= 16
            && value[4] == (if ipv6 { 3 } else { 2 })
        {
            let kind = match (ipv6, value[5]) {
                (false, 11) | (true, 3) => ErrorKind::Expired,
                (false, 3) | (true, 1) => ErrorKind::Unreachable,
                _ => return None,
            };
            let router = ip_from_sockaddr(&value[16..])?;
            let id = request_id(original, ipv6)?;
            return Some(ErrorReply { destination: dest, router, kind, id });
        }
        let step = len.checked_add(size_of::<usize>() - 1)? & !(size_of::<usize>() - 1);
        if step == 0 || step > control.len() - cursor { break; }
        cursor += step;
    }
    None
}

pub struct TraceSocket {
    socket: PingSocket,
    ipv6: bool,
}

impl TraceSocket {
    pub fn new(ipv6: bool) -> Result<Self, String> {
        let sock = PingSocket::new(ipv6, None)
            .map_err(|e| format!("cannot open Linux ICMP ping socket for trace: {e}"))?;
        let one: c_int = 1;
        let (level, option) = if ipv6 {
            (SOL_IPV6, IPV6_RECVERR)
        } else {
            (SOL_IP, IP_RECVERR)
        };
        // SAFETY: valid owned socket and initialized integer option.
        if unsafe { setsockopt(sock.fd(), level, option, &one, 4) } < 0 {
            return Err(format!("IP_RECVERR required for tracing: {}", io::Error::last_os_error()));
        }
        Ok(Self { socket: sock, ipv6 })
    }

    pub fn send(&self, dest: SocketAddr, packet: &[u8], hops: u8) -> io::Result<()> {
        let value = c_int::from(hops);
        let (level, option) = if self.ipv6 {
            (SOL_IPV6, IPV6_UNICAST_HOPS)
        } else {
            (SOL_IP, IP_TTL)
        };
        // SAFETY: packet TTL/hop limit only affects this private ping socket.
        if unsafe { setsockopt(self.socket.fd(), level, option, &value, 4) } < 0 {
            return Err(io::Error::last_os_error());
        }
        self.socket.send(dest, packet)
    }

    pub fn recv_echo(&self) -> io::Result<Option<Received>> {
        self.socket.recv()
    }

    pub fn recv_error(&self) -> io::Result<Option<ErrorReply>> {
        let mut original = [0_u8; 2048];
        let mut name = [0_u8; 128];
        let mut control = Control([0_u8; 256]);
        let mut iov = IoVec { base: original.as_mut_ptr().cast(), len: original.len() };
        let mut msg = MsgHdr {
            name: name.as_mut_ptr().cast(), name_len: name.len() as u32,
            iov: &mut iov, iov_len: 1,
            control: control.0.as_mut_ptr().cast(),
            control_len: control.0.len(), flags: 0,
        };
        // SAFETY: each pointer references initialized, writable, live stack storage.
        let got = unsafe { recvmsg(self.socket.fd(), &mut msg, MSG_ERRQUEUE | MSG_DONTWAIT) };
        if got < 0 {
            let err = io::Error::last_os_error();
            if matches!(err.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted) {
                return Ok(None);
            }
            return Err(err);
        }
        if msg.flags & (MSG_CTRUNC | MSG_TRUNC) != 0
            || msg.name_len as usize > name.len()
            || msg.control_len > control.0.len()
            || got as usize > original.len()
        {
            return Ok(None);
        }
        let Some(dest) = ip_from_sockaddr(&name[..msg.name_len as usize]) else {
            return Ok(None);
        };
        Ok(parse_control(
            &control.0[..msg.control_len],
            &original[..got as usize],
            dest, self.ipv6,
        ))
    }

    pub fn wait(&self, timeout: Duration) -> io::Result<()> {
        let mut pfd = PollFd { fd: self.socket.fd(), events: POLLIN | POLLERR, revents: 0 };
        let millis = timeout.as_micros().div_ceil(1000).min(60_000) as c_int;
        // SAFETY: pfd is a valid single-element poll buffer.
        if unsafe { poll(&mut pfd, 1, millis) } < 0 {
            let e = io::Error::last_os_error();
            if e.kind() != io::ErrorKind::Interrupted {
                return Err(e);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cmsg(original: &[u8], family6: bool, kind: u8) -> Option<ErrorReply> {
        let mut ctrl = [0_u8; 80];
        let sockaddr_len = if family6 { 28 } else { 16 };
        let len = 16 + 16 + sockaddr_len;
        ctrl[0..8].copy_from_slice(&(len as u64).to_ne_bytes());
        ctrl[8..12].copy_from_slice(&(if family6 { SOL_IPV6 } else { SOL_IP }).to_ne_bytes());
        ctrl[12..16].copy_from_slice(&(if family6 { IPV6_RECVERR } else { IP_RECVERR }).to_ne_bytes());
        ctrl[20] = if family6 { 3 } else { 2 };
        ctrl[21] = kind;
        ctrl[32..34].copy_from_slice(&(if family6 { AF_INET6 } else { AF_INET }).to_ne_bytes());
        if family6 {
            ctrl[40..56].copy_from_slice(&Ipv6Addr::LOCALHOST.octets());
        } else {
            ctrl[36..40].copy_from_slice(&[10, 0, 0, 1]);
        }
        let dest = if family6 { IpAddr::V6(Ipv6Addr::LOCALHOST) }
            else { IpAddr::V4(Ipv4Addr::new(10,0,0,2)) };
        parse_control(&ctrl[..len], original, dest, family6)
    }

    #[test]
    fn parses_expiry_and_unreachable_error_queue_and_correlation() {
        let mut packet = vec![0x42; 64];
        linux::write_echo_request(&mut packet, false, 77, 123, 77);
        let err = cmsg(&packet, false, 11).unwrap();
        assert_eq!(err.kind, ErrorKind::Expired);
        assert_eq!(err.router.to_string(), "10.0.0.1");
        assert_eq!(err.id.token, Some(77));
        assert_eq!(cmsg(&packet, false, 3).unwrap().kind, ErrorKind::Unreachable);
        let mut v6 = vec![0x42; 64];
        linux::write_echo_request(&mut v6, true, 9, 123, 9);
        assert_eq!(cmsg(&v6, true, 3).unwrap().router, IpAddr::V6(Ipv6Addr::LOCALHOST));
        assert_eq!(cmsg(&v6, true, 1).unwrap().kind, ErrorKind::Unreachable);
    }

    #[test]
    fn accepts_payload_only_and_rejects_malformed_cmsg() {
        let mut packet = vec![0x42; 64];
        linux::write_echo_request(&mut packet, false, 42, 15, 42);
        assert_eq!(cmsg(&packet[8..], false, 11).unwrap().id.token, Some(42));
        assert_eq!(cmsg(&packet[..8], false, 11).unwrap().id.seq, 42);
        assert!(parse_control(&[0; 5], &packet, IpAddr::V4(Ipv4Addr::LOCALHOST), false).is_none());
        assert!(cmsg(&packet[..3], false, 11).is_none());
    }

    #[test]
    fn abi_layout_for_64_bit_linux() {
        assert_eq!(size_of::<MsgHdr>(), if cfg!(target_pointer_width = "64") { 56 } else { 28 });
        assert_eq!(size_of::<IoVec>(), size_of::<usize>() * 2);
    }
}
