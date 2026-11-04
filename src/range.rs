//! Limited, explicit ICMP target expansion. Never expand unbounded IPv6 networks.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub fn expand(input: &str, max_hosts: usize, v6: Option<bool>) -> Result<Vec<IpAddr>, String> {
    let list = if let Some((ip, prefix)) = input.split_once('/') {
        expand_cidr(ip, prefix, max_hosts)?
    } else if let Some((first, last)) = input.split_once('-') {
        let start: Ipv4Addr = first.parse().map_err(|_| "invalid IPv4 range start")?;
        let end: Ipv4Addr = last.parse().map_err(|_| "invalid IPv4 range end")?;
        let a = u32::from(start);
        let b = u32::from(end);
        if b < a {
            return Err("range end precedes range start".into());
        }
        let count = u64::from(b) - u64::from(a) + 1;
        if count > max_hosts as u64 {
            return Err(format!("range has {count} hosts; --max-hosts={max_hosts}"));
        }
        (a..=b).map(|n| IpAddr::V4(Ipv4Addr::from(n))).collect()
    } else {
        vec![input.parse().map_err(|_| "range needs an IP, CIDR or IPv4 START-END")?]
    };
    if list.len() > max_hosts {
        return Err(format!("range exceeds --max-hosts={max_hosts}"));
    }
    if list.iter().any(|ip| v6.is_some_and(|is_v6| is_v6 != ip.is_ipv6())) {
        return Err("range addresses conflict with -4/-6".into());
    }
    Ok(list)
}

fn expand_cidr(ip: &str, prefix: &str, max: usize) -> Result<Vec<IpAddr>, String> {
    let ip: IpAddr = ip.parse().map_err(|_| "invalid CIDR address")?;
    let prefix: u32 = prefix.parse().map_err(|_| "invalid CIDR prefix")?;
    let (bits, raw) = match ip {
        IpAddr::V4(v) => (32_u32, u128::from(u32::from(v))),
        IpAddr::V6(v) => (128_u32, u128::from(v)),
    };
    if prefix > bits {
        return Err(format!("CIDR prefix must be <= {bits}"));
    }
    let host_bits = bits - prefix;
    if host_bits > 12 {
        return Err("CIDR expands beyond hard limit of 4096 addresses".into());
    }
    let total = 1_usize << host_bits;
    // Traditional IPv4 networks omit network and broadcast for /0../30.
    let skip_edges = bits == 32 && prefix <= 30 && total > 2;
    let usable = if skip_edges { total - 2 } else { total };
    if usable > max {
        return Err(format!("CIDR has {usable} hosts; --max-hosts={max}"));
    }
    let mask = if prefix == 0 { 0 } else { u128::MAX << host_bits };
    let start = raw & mask;
    let mut result = Vec::with_capacity(usable);
    for offset in 0..total {
        if skip_edges && (offset == 0 || offset == total - 1) {
            continue;
        }
        let v = start + offset as u128;
        let addr = if bits == 32 {
            IpAddr::V4(Ipv4Addr::from(v as u32))
        } else {
            IpAddr::V6(Ipv6Addr::from(v))
        };
        result.push(addr);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipv4_cidr_excludes_network_and_broadcast() {
        assert_eq!(expand("192.0.2.0/30", 256, None).unwrap(), vec![
            IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)),
            IpAddr::V4(Ipv4Addr::new(192, 0, 2, 2)),
        ]);
    }

    #[test]
    fn slash_31_and_32_include_endpoints() {
        assert_eq!(expand("192.0.2.0/31", 256, None).unwrap().len(), 2);
        assert_eq!(expand("192.0.2.7/32", 256, None).unwrap().len(), 1);
    }

    #[test]
    fn ipv6_is_bounded() {
        assert_eq!(expand("2001:db8::/126", 16, Some(true)).unwrap().len(), 4);
        assert!(expand("2001:db8::/64", 4096, None).is_err());
    }

    #[test]
    fn endpoint_range_and_limits() {
        assert_eq!(expand("192.0.2.1-192.0.2.3", 3, None).unwrap().len(), 3);
        assert!(expand("192.0.2.1-192.0.2.3", 2, None).is_err());
        assert!(expand("192.0.2.8-192.0.2.3", 3, None).is_err());
        assert!(expand("192.0.2.1", 1, Some(true)).is_err());
    }

    #[test]
    fn ipv6_network_normalization_is_correct() {
        let values = expand("2001:db8::f/126", 4, None).unwrap();
        assert_eq!(values[0].to_string(), "2001:db8::c");
        assert_eq!(values[3].to_string(), "2001:db8::f");
    }
}
