//! Local neighborhood visualization: a truthful per-interface star tree built from
//! Linux kernel ARP/NDP cache. Default is passive, read-only, no packets.

use crate::neighbor::{self, Neighbor};
use std::collections::BTreeMap;
#[cfg(test)]
use std::net::IpAddr;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
struct Options {
    interface: Option<String>,
    family: Option<bool>,
    limit: usize,
    ascii: bool,
    tui: bool,
    watch: Option<Duration>,
    count: Option<u32>,
    active: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            interface: None, family: None, limit: 128, ascii: false,
            tui: false, watch: None, count: None, active: None,
        }
    }
}

fn help() -> &'static str {
    concat!(
        "pipi neighborhood — local neighbor graph from ARP and IPv6 NDP (Linux)\n",
        "USAGE: pipi neighborhood [OPTIONS]\n",
        "  -i, --interface IFACE  Display one local interface\n",
        "  --family 4|6|all      Display IPv4 ARP and/or IPv6 NDP\n",
        "  --max-results N       Max cached neighbors (1..512; default 128)\n",
        "  --ascii               ASCII-only graph for small/non-Unicode terminals\n",
        "  --tui                 Compact auto-refresh fullscreen (Cargo feature tui)\n",
        "  --watch SEC           Read-only refresh interval (2..120 seconds)\n",
        "  -c, --count N         Refresh count (1..1000), default 1 for snapshot\n",
        "  --active CIDR         Single explicit on-link IPv4 ARP scan; needs --interface\n",
        "  --scan-local          Opt-in auto-scan of one private /24..../31 on-link CIDR\n",
        "                        CAP_NET_RAW required; max 256 hosts at 20 PPS\n",
        "  -h, --help            Show help\n\n",
        "Default: automatic discovery of local interfaces and their cached neighbors.\n",
        "A local graph is NOT a physical topology or LLDP link map; stale/empty\n",
        "neighbor caches are normal. No packet scan runs unless --active is given.\n",
        "Active scanning cannot be combined with --tui or --watch.\n"
    )
}

fn parse(args: &[String]) -> Result<Option<Options>, String> {
    if args.iter().any(|x| matches!(x.as_str(), "-h" | "--help")) {
        return Ok(None);
    }
    let mut options = Options::default();
    let mut i = 0;
    while i < args.len() {
        let key = args[i].as_str();
        match key {
            "-i" | "--interface" => {
                i += 1;
                let name = args.get(i).ok_or("--interface needs a name")?.to_owned();
                if !crate::discovery::valid_interface(&name) {
                    return Err("invalid Linux interface name".into());
                }
                options.interface = Some(name);
            }
            "--family" => {
                i += 1;
                options.family = match args.get(i).map(String::as_str) {
                    Some("all") => None,
                    Some("4") => Some(false),
                    Some("6") => Some(true),
                    _ => return Err("--family must be 4, 6, or all".into()),
                };
            }
            "--max-results" => {
                i += 1;
                let number: usize = args.get(i).ok_or("--max-results needs a value")?
                    .parse().map_err(|_| "invalid --max-results")?;
                if !(1..=512).contains(&number) {
                    return Err("--max-results must be 1..=512".into());
                }
                options.limit = number;
            }
            "--ascii" => options.ascii = true,
            "--tui" => options.tui = true,
            "--watch" => {
                i += 1;
                let seconds: u64 = args.get(i).ok_or("--watch needs seconds")?
                    .parse().map_err(|_| "invalid --watch")?;
                if !(2..=120).contains(&seconds) {
                    return Err("--watch seconds must be 2..=120".into());
                }
                options.watch = Some(Duration::from_secs(seconds));
            }
            "-c" | "--count" => {
                i += 1;
                let number: u32 = args.get(i).ok_or("--count needs a value")?
                    .parse().map_err(|_| "invalid --count")?;
                if !(1..=1000).contains(&number) {
                    return Err("--count must be 1..=1000".into());
                }
                options.count = Some(number);
            }
            "--active" => {
                i += 1;
                let cidr = args.get(i).ok_or("--active needs an IPv4 CIDR")?.to_owned();
                if !cidr.contains('/') || options.active.is_some() {
                    return Err("--active requires a single explicit IPv4 CIDR".into());
                }
                options.active = Some(cidr);
            }
            "--scan-local" => {
                if options.active.is_some() {
                    return Err("specify only one of --scan-local and --active".into());
                }
                options.active = Some("auto".into());
            }
            other => return Err(format!("unknown neighborhood argument: {other}")),
        }
        i += 1;
    }
    if options.active.is_some()
        && (options.interface.is_none() || options.tui || options.watch.is_some()
            || options.count.is_some_and(|n| n != 1) || options.family == Some(true))
    {
        return Err("--active requires --interface and a one-shot IPv4 view; no --tui/--watch/IPv6".into());
    }
    Ok(Some(options))
}

fn sanitize(s: &str) -> String {
    s.chars().filter(|c| !c.is_control() && *c != '\u{1b}').take(80).collect()
}

fn mac(mac: &[u8]) -> String {
    if mac.is_empty() { return "unknown-mac".to_owned(); }
    mac.iter().map(|byte| format!("{byte:02x}")).collect::<Vec<_>>().join(":")
}

fn state(n: &Neighbor) -> &'static str {
    if n.active { "ARP-REPLY" }
    else {
        match n.state {
            2 => "reachable",
            4 => "stale",
            1 => "incomplete",
            8 => "delay",
            16 => "probe",
            32 => "failed",
            64 => "noarp",
            128 => "permanent",
            _ => "unknown",
        }
    }
}

/// Return a bounded, deterministic graph; indenting means observed local
/// interface membership, never physical connectivity between remote devices.
fn render(
    interfaces: &BTreeMap<i32, String>,
    neighbors: &[Neighbor],
    selected: Option<i32>,
    family: Option<bool>,
    max_results: usize,
    ascii: bool,
) -> (Vec<String>, usize, bool) {
    let mut rows = Vec::new();
    let joint = if ascii { "+--" } else { "├──" };
    let child = if ascii { "|  " } else { "│  " };
    let leaf = if ascii { "+--" } else { "└──" };
    rows.push("LOCAL HOST".to_owned());
    let mut selected_neighbors: Vec<&Neighbor> = neighbors.iter().filter(|n| {
        selected.is_none_or(|i| n.ifindex == i)
            && family.is_none_or(|v6| n.ip.is_ipv6() == v6)
    }).collect();
    selected_neighbors.sort_unstable_by(|a,b| a.ifindex.cmp(&b.ifindex).then(a.ip.cmp(&b.ip)));
    selected_neighbors.dedup_by(|a,b| a.ifindex == b.ifindex && a.ip == b.ip);
    let total = selected_neighbors.len();
    selected_neighbors.truncate(max_results);
    let truncated = total > selected_neighbors.len();
    for (&ifindex, name) in interfaces {
        if selected.is_some_and(|i| i != ifindex) { continue; }
        let group: Vec<_> = selected_neighbors.iter()
            .filter(|n| n.ifindex == ifindex)
            .collect();
        rows.push(format!("{joint} {} (ifindex {})  [{} cached]", sanitize(name), ifindex, group.len()));
        if group.is_empty() {
            rows.push(format!("{child}{leaf} no observed neighbors"));
        }
        for (i, entry) in group.iter().enumerate() {
            let marker = if i + 1 == group.len() { leaf } else { joint };
            rows.push(format!(
                "{child}{marker} {}  [{}]  {}  ({})",
                entry.ip,
                if entry.ip.is_ipv4() { "ARP" } else { "NDP" },
                mac(&entry.mac), state(entry),
            ));
        }
    }
    rows.push(format!(
        "Observed {} / {} neighbors{} — cache-only; not an Ethernet topology",
        selected_neighbors.len(), total,
        if truncated { " (truncated)" } else { "" },
    ));
    (rows, total, truncated)
}

fn snapshot(options: &Options) -> Result<(Vec<String>, usize), String> {
    let interfaces = neighbor::interfaces()?;
    let ifindex = options.interface.as_ref().map(|iface| {
        interfaces.iter().find_map(|(index, name)| (name == iface).then_some(*index))
            .ok_or_else(|| format!("interface {iface} is not present"))
    }).transpose()?;
    let rows = if let Some(ref cidr) = options.active {
        let interface = options.interface.as_deref().unwrap_or_default();
        let net = if cidr == "auto" {
            crate::arp::local_cidr(interface)?
        } else {
            cidr.to_owned()
        };
        crate::arp::scan(
            &net, interface, ifindex.unwrap_or_default(), 20, Duration::from_secs(2), 256,
        )?
    } else {
        neighbor::dump()?
    };
    let (rows, total, _) = render(
        &interfaces, &rows, ifindex, options.family, options.limit, options.ascii,
    );
    Ok((rows, total))
}

pub fn run(args: &[String]) -> Result<i32, String> {
    let Some(options) = parse(args)? else {
        print!("{}", help());
        return Ok(0);
    };
    if options.tui && !cfg!(feature = "tui") {
        return Err("neighborhood --tui requires cargo install --features tui".into());
    }
    crate::engine::install_signals();
    #[cfg(feature = "tui")]
    let screen = if options.tui { Some(crate::tui::Screen::enter()?) } else { None };
    let interval = options.watch.unwrap_or(Duration::from_secs(5));
    let max = options.count.or(if options.tui || options.watch.is_some() { None } else { Some(1) });
    let mut cycle = 0_u32;
    while !crate::engine::interrupted() && max.is_none_or(|count| cycle < count) {
        cycle += 1;
        let start = Instant::now();
        let (rows, total) = snapshot(&options)?;
        #[cfg(feature = "tui")]
        if let Some(screen) = screen.as_ref() {
            screen.show_lines(
                &format!("pipi NEIGHBORHOOD | {} neighbors | refresh {}s",
                    total, interval.as_secs()),
                &rows,
                "ARP/NDP cache | observed local adjacency only | Ctrl-C exit",
            );
        }
        if !options.tui {
            crate::engine::emit(&format!("pipi neighborhood | snapshot {cycle} | local, passive={}", options.active.is_none()));
            for row in &rows { crate::engine::emit(row); }
        }
        if max.is_some_and(|count| cycle >= count) { break; }
        while !crate::engine::interrupted() && start.elapsed() < interval {
            std::thread::sleep(Duration::from_millis(100)
                .min(interval.saturating_sub(start.elapsed())));
        }
    }
    #[cfg(feature = "tui")]
    drop(screen);
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn automatic_passive_default_never_scans() {
        let options = parse(&[]).unwrap().unwrap();
        assert_eq!(options.limit, 128);
        assert!(options.active.is_none());
        assert!(!options.tui);
        assert!(parse(&["--active".into(), "10.0.0.0/24".into()]).is_err());
        assert!(parse(&["--active".into(), "10.0.0.0/24".into(), "-i".into(), "eth0".into(), "--tui".into()]).is_err());
        assert!(parse(&["--watch".into(), "1".into()]).is_err());
        assert!(parse(&["--scan-local".into()]).is_err());
        assert!(parse(&["--scan-local".into(), "-i".into(), "eth0".into(), "--tui".into()]).is_err());
        assert!(parse(&["--scan-local".into(), "-i".into(), "eth0".into()]).unwrap().unwrap().active.is_some());
    }

    #[test]
    fn graph_shows_only_observed_interface_neighbors() {
        let interfaces = BTreeMap::from([(2, "eth0".into()), (3, "wlan0".into())]);
        let neighbors = vec![
            Neighbor { ifindex: 2, ip: IpAddr::V4(Ipv4Addr::new(192,168,1,1)),
                mac: vec![0,17,34,51,68,85], state: 2, active: false },
            Neighbor { ifindex: 3, ip: IpAddr::V6(Ipv6Addr::LOCALHOST),
                mac: vec![6,7,8,9,10,11], state: 4, active: false },
        ];
        let (lines, total, truncated) = render(&interfaces, &neighbors, None, None, 128, true);
        assert_eq!(total, 2);
        assert!(!truncated);
        assert!(lines.join("\n").contains("192.168.1.1"));
        assert!(lines.join("\n").contains("00:11:22:33:44:55"));
        assert!(lines.join("\n").contains("stale"));
        let (_, total, truncated) = render(&interfaces, &neighbors, None, None, 1, false);
        assert_eq!(total, 2);
        assert!(truncated);
        let (lines, _, _) = render(&interfaces, &neighbors, Some(3), Some(false), 128, true);
        assert!(!lines.join("\n").contains("192.168.1.1"));
    }

    #[test]
    fn no_neighbors_and_terminal_input_are_safe() {
        let interfaces = BTreeMap::from([(2, "eth0\x1b[31m".into())]);
        let (rows, total, _) = render(&interfaces, &[], None, None, 100, false);
        assert_eq!(total, 0);
        assert!(!rows.join("\n").contains('\x1b'));
        assert!(rows.join("\n").contains("no observed neighbors"));
    }
}
