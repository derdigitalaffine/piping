//! Layer-2 neighbor discovery: passive kernel ARP/NDP cache, optional bounded
//! Ethernet/ARP probing on an explicit on-link IPv4 interface and CIDR.

use crate::cli::Output;
use crate::neighbor::{self, Neighbor};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{self, Write};
use std::net::IpAddr;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Options {
    pub interface: Option<String>,
    pub family: Option<bool>,
    pub output: Output,
    pub max_results: usize,
    pub active: Option<String>,
    pub max_hosts: usize,
    pub pps: u32,
    pub timeout: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            interface: None, family: None, output: Output::Text,
            max_results: 4096, active: None, max_hosts: 256, pps: 20,
            timeout: Duration::from_secs(2),
        }
    }
}

pub fn help() -> &'static str {
    concat!(
        "piping discover — local Layer-2 neighbor discovery (Linux)\n",
        "USAGE:\n",
        "  piping discover [--interface IFACE] [--family 4|6|all]\n",
        "  piping discover --active CIDR --interface IFACE [--pps N]\n\n",
        "Passive by default: read the kernel's ARP/NDP neighbor cache through\n",
        "RTM_GETNEIGH. No packets or privileges required. Empty caches are normal.\n",
        "Active is opt-in: IPv4 Ethernet ARP probes confined to an on-link\n",
        "interface/CIDR, requires CAP_NET_RAW. Never auto-sudo or change sysctl.\n\n",
        "  -i, --interface IFACE  Local interface, e.g. eth0\n",
        "  --family 4|6|all      Filter IP family (default all)\n",
        "  --output text|jsonl|csv  Output format (default text)\n",
        "  --max-results N      Maximum displayed neighbors (1..4096)\n",
        "  --active CIDR        Actively ARP-probe a directly connected IPv4 CIDR\n",
        "  --max-hosts N        Maximum active target IPs (1..256, default 256)\n",
        "  --pps N              Global ARP rate ceiling (1..100, default 20)\n",
        "  --timeout SEC        Listen after last ARP packet (0.1..5, default 2)\n",
        "  -h, --help           Show this help\n\n",
        "Examples:\n",
        "  piping discover --interface eth0\n",
        "  piping discover --family 6 --output jsonl\n",
        "  piping discover --active 192.168.1.0/24 -i eth0 --pps 20\n",
        "Use on networks you administer or have permission to test.\n"
    )
}

fn arg(args: &[String], pos: &mut usize, flag: &str) -> Result<String, String> {
    *pos += 1;
    args.get(*pos).cloned().ok_or_else(|| format!("{flag} needs a value"))
}

pub fn parse(args: &[String]) -> Result<Option<Options>, String> {
    if args.iter().any(|x| x == "-h" || x == "--help") {
        return Ok(None);
    }
    let mut out = Options::default();
    let mut i = 0;
    let mut set_pps = false;
    let mut set_timeout = false;
    let mut set_max_hosts = false;
    while i < args.len() {
        match args[i].as_str() {
            "-i" | "--interface" => {
                let name = arg(args, &mut i, "--interface")?;
                if !valid_interface(&name) {
                    return Err("invalid interface name (ASCII, at most 15 characters)".into());
                }
                out.interface = Some(name);
            }
            "--family" => {
                out.family = match arg(args, &mut i, "--family")?.as_str() {
                    "4" => Some(false),
                    "6" => Some(true),
                    "all" => None,
                    _ => return Err("--family must be 4, 6 or all".into()),
                };
            }
            "--output" => {
                out.output = match arg(args, &mut i, "--output")?.as_str() {
                    "text" => Output::Text,
                    "jsonl" => Output::Jsonl,
                    "csv" => Output::Csv,
                    _ => return Err("--output must be text, jsonl or csv".into()),
                };
            }
            "--max-results" => {
                let v = arg(args, &mut i, "--max-results")?;
                out.max_results = v.parse::<usize>().map_err(|_| "invalid --max-results")?;
                if !(1..=4096).contains(&out.max_results) {
                    return Err("--max-results must be 1..=4096".into());
                }
            }
            "--active" => {
                let cidr = arg(args, &mut i, "--active")?;
                if !cidr.contains('/') {
                    return Err("--active requires an explicit IPv4 CIDR".into());
                }
                out.active = Some(cidr);
            }
            "--max-hosts" => {
                let value = arg(args, &mut i, "--max-hosts")?;
                out.max_hosts = value.parse().map_err(|_| "invalid --max-hosts")?;
                if !(1..=256).contains(&out.max_hosts) {
                    return Err("--max-hosts must be 1..=256".into());
                }
                set_max_hosts = true;
            }
            "--pps" => {
                let value = arg(args, &mut i, "--pps")?;
                out.pps = value.parse().map_err(|_| "invalid --pps")?;
                if !(1..=100).contains(&out.pps) {
                    return Err("--pps must be 1..=100".into());
                }
                set_pps = true;
            }
            "--timeout" => {
                let value = arg(args, &mut i, "--timeout")?;
                let seconds: f64 = value.parse().map_err(|_| "invalid --timeout")?;
                if !seconds.is_finite() || !(0.1..=5.0).contains(&seconds) {
                    return Err("--timeout must be 0.1..=5 seconds".into());
                }
                out.timeout = Duration::from_secs_f64(seconds);
                set_timeout = true;
            }
            other => return Err(format!("unexpected discover option: {other}")),
        }
        i += 1;
    }
    if out.active.is_some() {
        if out.interface.is_none() {
            return Err("--active requires --interface IFACE".into());
        }
        if out.family == Some(true) {
            return Err("active ARP supports IPv4 only; remove --family 6".into());
        }
    } else if set_pps || set_max_hosts || set_timeout {
        return Err("--pps/--max-hosts/--timeout require --active".into());
    }
    Ok(Some(out))
}

pub fn valid_interface(name: &str) -> bool {
    !name.is_empty() && name.len() <= 15 && name.bytes().all(|b| {
        b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b':')
    })
}

fn state_label(state: u16, active: bool) -> &'static str {
    if active { return "ARP_REPLY"; }
    match state {
        1 => "INCOMPLETE", 2 => "REACHABLE", 4 => "STALE",
        8 => "DELAY", 16 => "PROBE", 32 => "FAILED",
        64 => "NOARP", 128 => "PERMANENT",
        0 => "NONE", _ => "OTHER",
    }
}

fn mac_label(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "-".into();
    }
    let mut out = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i != 0 { out.push(':'); }
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

fn json_escape(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => { let _ = write!(&mut out, "\\u{:04x}", c as u32); }
            c => out.push(c),
        }
    }
    out
}

fn csv_escape(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn emit(text: &str) -> Result<(), String> {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    match writeln!(lock, "{text}") {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(format!("stdout: {e}")),
    }
}

pub fn run(args: &[String]) -> Result<i32, String> {
    let Some(options) = parse(args)? else {
        print!("{}", help());
        return Ok(0);
    };
    let interfaces = neighbor::interfaces()?;
    let filter_ifindex = options.interface.as_ref().map(|name| {
        interfaces.iter().find(|(_, value)| value == &name)
            .map(|(idx, _)| *idx)
            .ok_or_else(|| format!("interface {} not found in /sys/class/net", name))
    }).transpose()?;
    let mut rows = if let Some(ref cidr) = options.active {
        crate::arp::scan(cidr, options.interface.as_deref().unwrap_or_default(),
            filter_ifindex.unwrap_or_default(), options.pps,
            options.timeout, options.max_hosts)?
    } else {
        neighbor::dump()?
    };
    rows.retain(|n| {
        filter_ifindex.is_none_or(|i| n.ifindex == i)
            && options.family.is_none_or(|v6| n.ip.is_ipv6() == v6)
    });
    rows.sort_unstable_by(|a, b| a.ifindex.cmp(&b.ifindex).then(a.ip.cmp(&b.ip)));
    rows.dedup_by(|a, b| a.ifindex == b.ifindex && a.ip == b.ip);
    let total = rows.len();
    let truncated = total > options.max_results;
    rows.truncate(options.max_results);
    display(&rows, &interfaces, options.output, total, truncated, options.active.is_some())?;
    Ok(0)
}

fn display(
    neighbors: &[Neighbor],
    interfaces: &BTreeMap<i32, String>,
    output: Output,
    total: usize,
    truncated: bool,
    active: bool,
) -> Result<(), String> {
    match output {
        Output::Text => emit("IP ADDRESS                              INTERFACE         MAC / LINK ADDRESS              STATE")?,
        Output::Csv => emit("ip,interface,mac,state,source")?,
        Output::Jsonl => {}
    }
    for n in neighbors {
        let name = interfaces.get(&n.ifindex).cloned()
            .unwrap_or_else(|| format!("ifindex:{}", n.ifindex));
        let name = name.chars().filter(|c| !c.is_control()).collect::<String>();
        let mac = mac_label(&n.mac);
        let state = state_label(n.state, n.active);
        let source = if n.active { "active-arp" } else { "kernel-neighbor-cache" };
        match output {
            Output::Text => emit(&format!("{:<39} {:<17} {:<31} {}", n.ip, name, mac, state))?,
            Output::Jsonl => emit(&format!(
                "{{\"schema\":1,\"event\":\"neighbor\",\"ip\":\"{}\",\"interface\":\"{}\",\"mac\":\"{}\",\"state\":\"{state}\",\"source\":\"{source}\"}}",
                n.ip, json_escape(&name), json_escape(&mac)
            ))?,
            Output::Csv => emit(&format!(
                "{},{},{},{},{}", csv_escape(&n.ip.to_string()), csv_escape(&name),
                csv_escape(&mac), csv_escape(state), csv_escape(source)
            ))?,
        }
    }
    match output {
        Output::Text => emit(&format!(
            "{} neighbor(s) shown, {} total{}; {}",
            neighbors.len(), total,
            if truncated { " (truncated by --max-results)" } else { "" },
            if active { "explicit ARP discovery" } else { "passive kernel cache; may be stale" }
        )),
        Output::Jsonl => emit(&format!(
            "{{\"schema\":1,\"event\":\"summary\",\"total\":{total},\"shown\":{},\"truncated\":{truncated}}}",
            neighbors.len()
        )),
        Output::Csv => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn passive_requires_no_target_or_interface() {
        let opt = parse(&[]).unwrap().unwrap();
        assert!(opt.active.is_none());
        assert!(opt.interface.is_none());
        assert_eq!(opt.max_results, 4096);
    }

    #[test]
    fn active_requires_explicit_interface_and_cidr() {
        assert!(parse(&a(&["--active", "192.0.2.0/24"])).is_err());
        assert!(parse(&a(&["--active", "192.0.2.1", "-i", "eth0"])).is_err());
        assert!(parse(&a(&["--active", "192.0.2.0/24", "-i", "eth0", "--family", "6"])).is_err());
        assert!(parse(&a(&["--pps", "40"])).is_err());
        let config = parse(&a(&["--active", "192.168.1.0/24", "-i", "eth0"])).unwrap().unwrap();
        assert_eq!(config.pps, 20);
    }

    #[test]
    fn bounds_and_terminal_input_are_validated() {
        assert!(!valid_interface("../net/eth0"));
        assert!(!valid_interface("x\n"));
        assert!(valid_interface("eth0.2"));
        assert!(parse(&a(&["--max-results", "0"])).is_err());
        assert!(parse(&a(&["--active", "10.0.0.0/24", "-i", "eth0", "--pps", "101"])).is_err());
        assert!(parse(&a(&["--active", "10.0.0.0/24", "-i", "eth0", "--max-hosts", "999"])).is_err());
    }

    #[test]
    fn machine_string_escaping_and_mac_formatting() {
        assert_eq!(mac_label(&[0, 17, 34, 51, 68, 85]), "00:11:22:33:44:55");
        assert_eq!(mac_label(&[]), "-");
        assert_eq!(json_escape("a\"\n"), "a\\\"\\n");
        assert_eq!(csv_escape("a\"b"), "\"a\"\"b\"");
        assert_eq!(state_label(4, false), "STALE");
    }
}
