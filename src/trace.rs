//! Bounded MTR-style hop tracing using Linux ICMP ping sockets and IP_RECVERR.
//! One probe is outstanding at a time: conservative network load, no thread pool.

use crate::cli;
use crate::graph;
use crate::linux;
use crate::stats::Stats;
use crate::trace_socket::{ErrorKind, ErrorReply, ProbeId, TraceSocket};
use std::collections::VecDeque;
use std::io::{self, IsTerminal};
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

const HISTORY: usize = 60;

#[derive(Debug, Clone)]
struct Options {
    target: String,
    family: Option<bool>,
    cycles: Option<u32>,
    hops: u8,
    timeout: Duration,
    interval: Duration,
    smoke: bool,
    ascii: bool,
    tui: bool,
    text: bool,
    live_rows: bool,
}

fn help() -> &'static str {
    concat!(
        "pipi trace — per-hop ICMP diagnostics (Linux IP_RECVERR)\n",
        "USAGE: pipi trace [OPTIONS] HOST\n",
        "  -4 / -6             Force IPv4 or IPv6\n",
        "  -c, --count N       Trace cycles (1..1000); default continuous, Ctrl-C exits\n",
        "  --max-hops N        Maximum hop limit (1..32; default 16)\n",
        "  -W, --timeout SEC   Per-hop timeout (0.1..5; default 0.5)\n",
        "  -i, --interval SEC  Minimum interval between sends (0.05..5; default 0.1)\n",
        "  --smoke             Per-hop SmokePing-style RTT/loss history\n",
        "  --ascii             ASCII-only history bars\n",
        "  --tui               Live in-place hop table (requires --features tui)\n",
        "  --text              Force classic per-cycle output, even on a TTY\n",
        "  --help              This help\n\n",
        "Tracer uses Linux unprivileged ICMP datagram sockets and ICMP error queues.\n",
        "Some routers suppress ICMP Time Exceeded; * means unresponsive, not absent.\n",
        "No raw socket, automatic sudo, background scanning or DNS per hop.\n"
    )
}

fn parse(args: &[String]) -> Result<Option<Options>, String> {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        return Ok(None);
    }
    let mut options = Options {
        target: String::new(),
        family: None,
        cycles: None,
        hops: 16,
        timeout: Duration::from_millis(500),
        interval: Duration::from_millis(100),
        smoke: false,
        ascii: false,
        tui: false,
        text: false,
        live_rows: false,
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-4" => {
                if options.family == Some(true) {
                    return Err("-4 conflicts with -6".into());
                }
                options.family = Some(false);
            }
            "-6" => {
                if options.family == Some(false) {
                    return Err("-6 conflicts with -4".into());
                }
                options.family = Some(true);
            }
            "-c" | "--count" => {
                i += 1;
                let n: u32 = args
                    .get(i)
                    .ok_or("--count needs a value")?
                    .parse()
                    .map_err(|_| "invalid --count")?;
                if !(1..=1000).contains(&n) {
                    return Err("--count must be 1..=1000".into());
                }
                options.cycles = Some(n);
            }
            "--max-hops" => {
                i += 1;
                let n: u8 = args
                    .get(i)
                    .ok_or("--max-hops needs a value")?
                    .parse()
                    .map_err(|_| "invalid --max-hops")?;
                if !(1..=32).contains(&n) {
                    return Err("--max-hops must be 1..=32".into());
                }
                options.hops = n;
            }
            "-W" | "--timeout" => {
                options.timeout = seconds(args, &mut i, "--timeout", 0.1, 5.0)?;
            }
            "-i" | "--interval" => {
                options.interval = seconds(args, &mut i, "--interval", 0.05, 5.0)?;
            }
            "--smoke" => options.smoke = true,
            "--text" => options.text = true,
            "--live-rows" => {
                options.live_rows = true;
                options.smoke = true;
            },
            "--ascii" => {
                options.ascii = true;
                options.smoke = true;
            }
            "--tui" => {
                options.tui = true;
                options.smoke = true;
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown trace option: {other}"));
            }
            other if options.target.is_empty() => options.target = other.to_owned(),
            other => return Err(format!("unexpected trace argument: {other}")),
        }
        i += 1;
    }
    if (options.tui && options.text) || (options.tui && options.live_rows) {
        return Err("--tui cannot be combined with --text or --live-rows".into());
    }
    if options.target.trim().is_empty() {
        return Err("trace requires HOST (see 'pipi trace --help')".into());
    }
    Ok(Some(options))
}

fn seconds(
    args: &[String],
    i: &mut usize,
    flag: &str,
    min: f64,
    max: f64,
) -> Result<Duration, String> {
    *i += 1;
    let n: f64 = args
        .get(*i)
        .ok_or_else(|| format!("{flag} needs a value"))?
        .parse()
        .map_err(|_| format!("invalid {flag}"))?;
    if !n.is_finite() || !(min..=max).contains(&n) {
        return Err(format!("{flag} must be {min}..={max} seconds"));
    }
    Ok(Duration::from_secs_f64(n))
}

struct Hop {
    ip: Option<IpAddr>,
    stats: Stats,
    recent: VecDeque<Option<f64>>,
}

impl Default for Hop {
    fn default() -> Self {
        Self {
            ip: None,
            stats: Stats::for_sweep(),
            recent: VecDeque::new(),
        }
    }
}

impl Hop {
    fn sample(&mut self, ms: Option<f64>) {
        if self.recent.len() == HISTORY {
            self.recent.pop_front();
        }
        self.recent.push_back(ms);
        if let Some(ms) = ms {
            self.stats.on_reply(ms);
        }
    }
}

enum Outcome {
    Reached(f64),
    Router(IpAddr, f64),
    Unreachable(IpAddr, f64),
    NoResponse,
}

fn correlated(id: ProbeId, seq: u16, nonce: u64, token: u64) -> bool {
    id.seq == seq && id.nonce.is_none_or(|n| n == nonce) && id.token.is_none_or(|t| t == token)
}

fn match_error(error: &ErrorReply, target: IpAddr, seq: u16, nonce: u64, token: u64) -> bool {
    error.destination == target && correlated(error.id, seq, nonce, token)
}

fn probe(
    sock: &TraceSocket,
    target: SocketAddr,
    packet: &mut [u8],
    hop: u8,
    nonce: u64,
    token: u64,
    timeout: Duration,
) -> Result<Outcome, String> {
    let seq = token as u16;
    linux::write_echo_request(packet, target.is_ipv6(), seq, nonce, token);
    let start = Instant::now();
    sock.send(target, packet, hop)
        .map_err(|e| format!("trace ICMP send (TTL={hop}): {e}"))?;
    let deadline = start + timeout;
    loop {
        if crate::engine::interrupted() {
            return Ok(Outcome::NoResponse);
        }
        // Linux reports IPv4 Time Exceeded and Unreachable via MSG_ERRQUEUE.
        for _ in 0..32 {
            match sock
                .recv_error()
                .map_err(|e| format!("trace error queue: {e}"))?
            {
                Some(error) if match_error(&error, target.ip(), seq, nonce, token) => {
                    let rtt = start.elapsed().as_secs_f64() * 1000.0;
                    return Ok(match error.kind {
                        ErrorKind::Expired => Outcome::Router(error.router, rtt),
                        ErrorKind::Unreachable => Outcome::Unreachable(error.router, rtt),
                    });
                }
                Some(_) => {}
                None => break,
            }
        }
        for _ in 0..32 {
            match sock
                .recv_echo()
                .map_err(|e| format!("trace ICMP receive: {e}"))?
            {
                Some(reply)
                    if reply.source == target.ip()
                        && reply.echo.seq == seq
                        && reply.echo.nonce == nonce
                        && reply.echo.token == token =>
                {
                    return Ok(Outcome::Reached(start.elapsed().as_secs_f64() * 1000.0));
                }
                Some(_) => {}
                None => break,
            }
        }
        let now = Instant::now();
        if now >= deadline {
            return Ok(Outcome::NoResponse);
        }
        sock.wait(
            deadline
                .saturating_duration_since(now)
                .min(Duration::from_millis(250)),
        )
        .map_err(|e| format!("trace poll: {e}"))?;
    }
}

fn render_rows(hops: &[Hop], smoke: bool, ascii: bool, width: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(hops.len() + 1);
    if width < 54 {
        out.push("HOP ADDRESS     LOSS AVG  SMOKE".into());
        for (i, hop) in hops.iter().enumerate() {
            let ip = hop.ip.map_or_else(|| "*".to_owned(), |ip| ip.to_string());
            let addr: String = ip.chars().take(11).collect();
            let avg = if hop.stats.received == 0 {
                "-".to_owned()
            } else {
                format!("{:.1}", hop.stats.mean)
            };
            let prefix = format!(
                "{:>2}  {:<11} {:>3.0}% {:>5}",
                i + 1, addr, hop.stats.loss_percent(), avg
            );
            let available = width.saturating_sub(prefix.chars().count() + 2);
            out.push(format!(
                "{prefix} {}",
                if smoke { graph::sparkline(&hop.recent, available, ascii) }
                else { String::new() }
            ));
        }
        return out;
    }
    if width < 110 {
        out.push("HOP ADDRESS                LOSS  SENT  LAST   AVG  SMOKE".into());
        for (i, hop) in hops.iter().enumerate() {
            let ip = hop.ip.map_or_else(|| "*".to_owned(), |ip| ip.to_string());
            let addr: String = ip.chars().take(20).collect();
            let last = hop.recent.back().copied().flatten()
                .map_or_else(|| "-".to_owned(), |ms| format!("{ms:.1}"));
            let avg = if hop.stats.received == 0 {
                "-".to_owned()
            } else {
                format!("{:.1}", hop.stats.mean)
            };
            let prefix = format!(
                "{:>2}  {:<20} {:>4.0}%  {:>4} {:>6} {:>6}",
                i + 1, addr, hop.stats.loss_percent(), hop.stats.sent, last, avg
            );
            let available = width.saturating_sub(prefix.chars().count() + 2).min(40);
            out.push(format!(
                "{prefix} {}",
                if smoke { graph::sparkline(&hop.recent, available, ascii) }
                else { String::new() }
            ));
        }
        return out;
    }
    out.push(if smoke {
        "HOP ADDRESS                 LOSS%  SENT  LAST    AVG   BEST  WORST  SMOKE".into()
    } else {
        "HOP ADDRESS                 LOSS%  SENT  LAST    AVG   BEST  WORST".into()
    });
    for (i, hop) in hops.iter().enumerate() {
        let ip = hop.ip.map_or_else(|| "*".to_owned(), |ip| ip.to_string());
        let last = hop.recent.back().copied().flatten()
            .map_or_else(|| "-".to_owned(), |ms| format!("{ms:.1}"));
        let fmt = |value: f64| -> String {
            if hop.stats.received == 0 { "-".into() }
            else { format!("{value:.1}") }
        };
        let mut row = format!(
            "{:>2}  {:<23} {:>5.1}  {:>4} {:>6} {:>6} {:>6} {:>6}",
            i + 1, ip, hop.stats.loss_percent(), hop.stats.sent,
            last, fmt(hop.stats.mean), fmt(hop.stats.min), fmt(hop.stats.max)
        );
        if smoke {
            let available = width.saturating_sub(row.chars().count() + 2).min(36);
            row.push(' ');
            row.push_str(&graph::sparkline(&hop.recent, available, ascii));
        }
        out.push(row);
    }
    out
}

#[cfg(feature = "tui")]
fn visible_rows(hops: &[Hop], highest: usize, ascii: bool, width: usize) -> Vec<String> {
    // The visible hop count never shrinks when a new trace cycle starts.
    render_rows(&hops[..highest], true, ascii, width)
}

/// One stable per-hop update for the menu's TUI, instead of printing a whole
/// MTR table per cycle. The index is one-based and the payload is safe text.
fn live_update(hops: &[Hop], hop: usize, cycle: u32, ascii: bool) -> String {
    let rendered = render_rows(&hops[..hop], true, ascii, 74);
    format!("@pipi-hop\t{hop}\t{cycle}\t{}", rendered[hop])
}

fn text_cycle(hops: &[Hop], cycle: u32, dest: IpAddr, smoke: bool, ascii: bool) {
    crate::engine::emit(&format!(
        "pipi trace {dest} | cycle {cycle} | hop stats (ICMP)"
    ));
    for row in render_rows(hops, smoke, ascii, 110) {
        crate::engine::emit(&row);
    }
}

pub fn run(args: &[String]) -> Result<i32, String> {
    let Some(options) = parse(args)? else {
        print!("{}", help());
        return Ok(0);
    };
    if options.tui && !cfg!(feature = "tui") {
        return Err("trace --tui requires 'cargo install --features tui'".into());
    }
    // MTR-like behavior by default on interactive TTYs. Scripts and pipes keep
    // conventional per-cycle output; --text explicitly opts out on a terminal.
    let full_screen = options.tui || (
        !options.text && !options.live_rows
            && cfg!(feature = "tui") && io::stdout().is_terminal()
    );
    crate::colour::configured(!full_screen && !options.live_rows);
    let target = cli::resolve(&options.target, options.family).map_err(|e| e.to_string())?;
    let sock = TraceSocket::new(target.is_ipv6())?;
    crate::engine::install_signals();
    let nonce = crate::engine::entropy_nonce()?;
    let mut packet = [0x42_u8; 64];
    let mut hops: Vec<Hop> = (0..options.hops).map(|_| Hop::default()).collect();
    let mut seq = 1_u64;
    let mut reached = false;
    let mut cycle = 0_u32;
    #[cfg(feature = "tui")]
    let mut highest_hop = 0_usize;
    #[cfg(feature = "tui")]
    let screen = if full_screen {
        Some(crate::tui::Screen::enter()?)
    } else {
        None
    };
    let mut next_send = Instant::now();
    while !crate::engine::interrupted() && options.cycles.is_none_or(|n| cycle < n) {
        cycle += 1;
        for ttl in 1..=options.hops {
            if crate::engine::interrupted() {
                break;
            }
            // Keep Ctrl-C responsive even if a user selects a five-second interval.
            while Instant::now() < next_send && !crate::engine::interrupted() {
                std::thread::sleep(
                    next_send
                        .saturating_duration_since(Instant::now())
                        .min(Duration::from_millis(50)),
                );
            }
            if crate::engine::interrupted() {
                break;
            }
            next_send = Instant::now() + options.interval;
            let index = usize::from(ttl - 1);
            hops[index].stats.on_send();
            let outcome = probe(&sock, target, &mut packet, ttl, nonce, seq, options.timeout)?;
            seq = seq.checked_add(1).ok_or("trace probe ID exhausted")?;
            let terminal = match outcome {
                Outcome::Reached(ms) => {
                    hops[index].ip = Some(target.ip());
                    hops[index].sample(Some(ms));
                    reached = true;
                    true
                }
                Outcome::Router(ip, ms) => {
                    hops[index].ip = Some(ip);
                    hops[index].sample(Some(ms));
                    false
                }
                Outcome::Unreachable(ip, ms) => {
                    hops[index].ip = Some(ip);
                    hops[index].sample(Some(ms));
                    true
                }
                Outcome::NoResponse => {
                    hops[index].sample(None);
                    false
                }
            };
            if options.live_rows {
                crate::engine::emit(&live_update(
                    &hops, usize::from(ttl), cycle, options.ascii
                ));
            }
            #[cfg(feature = "tui")]
            if let Some(screen) = screen.as_ref() {
                highest_hop = highest_hop.max(usize::from(ttl));
                let width = crate::tui::dimensions().0.saturating_sub(5);
                let rows = visible_rows(&hops, highest_hop, options.ascii, width);
                screen.show_lines(
                    &format!(
                        "pipi TRACE {}  | cycle {}",
                        cli::visible_host(&options.target), cycle
                    ),
                    &rows,
                    "MTR live | RTT in ms | X=timeout | Ctrl-C exit",
                );
            }
            if terminal {
                break;
            }
        }
        if !full_screen && !options.live_rows {
            text_cycle(&hops, cycle, target.ip(), options.smoke, options.ascii);
        }
    }
    #[cfg(feature = "tui")]
    drop(screen);
    if full_screen {
        crate::engine::emit(&format!(
            "pipi trace {} | {} cycles | {} hops seen",
            target.ip(), cycle,
            hops.iter().rposition(|hop| hop.stats.sent > 0).map_or(0, |i| i + 1)
        ));
    }
    Ok(if reached { 0 } else { 1 })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mtr_event_contains_one_hop_and_keeps_history_in_place() {
        let mut hops = vec![Hop::default(), Hop::default()];
        hops[0].ip = Some("127.0.0.1".parse().unwrap());
        hops[0].stats.on_send();
        hops[0].sample(Some(2.5));
        let first = live_update(&hops, 1, 1, true);
        assert!(first.starts_with("@pipi-hop\t1\t1\t"));
        assert!(first.contains("127.0.0.1"));
        assert!(!first.contains("HOP ADDRESS"));
        hops[0].stats.on_send();
        hops[0].sample(None);
        let second = live_update(&hops, 1, 2, true);
        assert!(second.starts_with("@pipi-hop\t1\t2\t"));
        assert!(second.contains("50%"));
        assert!(second.ends_with('X'));
    }

    #[cfg(feature = "tui")]
    #[test]
    fn earlier_hops_remain_visible_at_start_of_next_cycle() {
        let mut hops: Vec<Hop> = (0..4).map(|_| Hop::default()).collect();
        for hop in &mut hops {
            hop.stats.on_send();
            hop.sample(Some(1.0));
        }
        let first = visible_rows(&hops, 4, true, 90);
        hops[0].stats.on_send();
        hops[0].sample(Some(2.0));
        let next_cycle = visible_rows(&hops, 4, true, 90);
        assert_eq!(first.len(), next_cycle.len());
        assert_eq!(first[4], next_cycle[4]);
        assert_ne!(first[1], next_cycle[1]);
    }

    #[test]
    fn trace_flags_have_safe_limits() {
        assert!(parse(&["--max-hops".into(), "33".into(), "x".into()]).is_err());
        assert!(parse(&["--count".into(), "0".into(), "x".into()]).is_err());
        assert!(parse(&["--timeout".into(), "nan".into(), "x".into()]).is_err());
        assert!(
            parse(&["--tui".into(), "--smoke".into(), "127.0.0.1".into()])
                .unwrap()
                .unwrap()
                .tui
        );
    }

    #[test]
    fn trace_hop_stats_and_smoke_show_loss_bounded() {
        let mut hops = vec![Hop::default()];
        hops[0].stats.on_send();
        hops[0].sample(Some(2.2));
        hops[0].stats.on_send();
        hops[0].sample(None);
        assert_eq!(hops[0].stats.sent, 2);
        assert_eq!(hops[0].stats.received, 1);
        let lines = render_rows(&hops, true, true, 110);
        assert!(lines[1].contains("50.0"));
        assert!(lines[1].contains("X"));
        for _ in 0..100 {
            hops[0].sample(None);
        }
        assert_eq!(hops[0].recent.len(), HISTORY);
    }

    #[test]
    fn extended_error_requires_target_and_probe_identity() {
        let target: IpAddr = "127.0.0.1".parse().unwrap();
        let error = ErrorReply {
            destination: target,
            router: target,
            kind: ErrorKind::Expired,
            id: ProbeId {
                seq: 4,
                nonce: Some(44),
                token: Some(4),
            },
        };
        assert!(match_error(&error, target, 4, 44, 4));
        assert!(!match_error(&error, target, 5, 44, 4));
        assert!(!match_error(&error, target, 4, 42, 4));
        assert!(!match_error(&error, "127.0.0.2".parse().unwrap(), 4, 44, 4));
    }
}
