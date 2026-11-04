//! Single-threaded, bounded scheduler for classic, fast, range and monitor modes.
//! One Linux ping socket per address family, with a 128-bit per-probe correlation marker.

use crate::cli::{self, Config, Mode, Output};
use crate::graph::History;
use crate::linux::{self, Received, SocketSet};
use crate::range;
use crate::stats::Stats;
use std::collections::{HashMap, VecDeque};
use std::io::{self, IsTerminal, Read, Write};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

unsafe extern "C" {
    fn signal(number: i32, handler: extern "C" fn(i32)) -> usize;
}

extern "C" fn stop_signal(_: i32) {
    INTERRUPTED.store(true, Ordering::Relaxed);
}

pub(crate) fn install_signals() {
    INTERRUPTED.store(false, Ordering::Relaxed);
    // SAFETY: signal handlers only write a lock-free atomic and do not allocate.
    unsafe {
        signal(2, stop_signal); // SIGINT
        signal(15, stop_signal); // SIGTERM
    }
}

pub(crate) fn entropy_nonce() -> Result<u64, String> {
    let mut bytes = [0_u8; 8];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|e| format!("cannot obtain Linux entropy for probe IDs: {e}"))?;
    Ok(u64::from_be_bytes(bytes))
}

pub(crate) fn emit(line: &str) {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    let result = writeln!(lock, "{line}").and_then(|_| lock.flush());
    if let Err(e) = result {
        if e.kind() == io::ErrorKind::BrokenPipe {
            // Following common CLI behavior, pipelines ending early are not fatal.
            std::process::exit(0);
        }
        eprintln!("pipi: stdout: {e}");
    }
}

/// Epoch milliseconds use UTC for machine output; probe RTTs still use monotonic Instant.
fn timestamp_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn json_escape(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < '\u{20}' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn csv_escape(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn init_output(config: &Config, targets: &[Target]) {
    match config.output {
        Output::Text => {
            if config.mode == Mode::Range {
                emit(&format!(
                    "PIPING RANGE {}: {} ICMP targets, {} probes each, {:.1} PPS cap, {} jobs",
                    cli::visible_host(&config.target),
                    targets.len(),
                    config.count.unwrap_or(1),
                    1.0 / config.interval.as_secs_f64(),
                    config.jobs,
                ));
            } else if config.mode != Mode::Tui {
                let first = &targets[0];
                let name = if config.numeric {
                    first.addr.ip().to_string()
                } else {
                    first.label.clone()
                };
                emit(&format!(
                    "PIPING {name} ({}): {} data bytes, interval {:.3}s, timeout {:.3}s",
                    first.addr.ip(),
                    config.size,
                    config.interval.as_secs_f64(),
                    config.timeout.as_secs_f64()
                ));
            }
        }
        Output::Jsonl => {}
        Output::Csv => emit("event,target,ip,seq,rtt_ms,sent,received,loss_pct,timestamp_unix_ms"),
    }
}

#[derive(Debug)]
struct Target {
    label: String,
    addr: SocketAddr,
    stats: Stats,
    sent: u32,
}

impl Target {
    fn new(label: String, addr: SocketAddr) -> Self {
        Self {
            label,
            addr,
            stats: Stats::default(),
            sent: 0,
        }
    }

    fn new_sweep(label: String, addr: SocketAddr) -> Self {
        let mut target = Self::new(label, addr);
        target.stats = Stats::for_sweep();
        target
    }
}

#[derive(Debug)]
struct Pending {
    target: usize,
    sent_at: Instant,
    deadline: Instant,
    seq: u16,
    second: u64,
}

/// O(1) fair selection. A range target re-enters the queue only after
/// its previous probe has completed (one outstanding probe per host).
fn select_target(
    targets: &[Target],
    ready: &mut VecDeque<usize>,
    count: Option<u32>,
    is_range: bool,
) -> Option<usize> {
    if is_range {
        ready.pop_front()
    } else if count.is_none_or(|limit| targets[0].sent < limit) {
        Some(0)
    } else {
        None
    }
}

struct SendContext<'a> {
    config: &'a Config,
    sockets: &'a SocketSet,
    start: Instant,
    nonce: u64,
}

fn send_one(
    context: &SendContext<'_>,
    targets: &mut [Target],
    pending: &mut HashMap<u64, Pending>,
    history: &mut History,
    packet: &mut [u8],
    token: u64,
    idx: usize,
) -> Result<(), String> {
    let config = context.config;
    let sockets = context.sockets;
    let target = &mut targets[idx];
    let seq = token as u16;
    linux::write_echo_request(packet, target.addr.is_ipv6(), seq, context.nonce, token);
    sockets
        .for_address(target.addr)
        .send(target.addr, packet)
        .map_err(|e| format!("send ICMP echo to {}: {e}", target.addr.ip()))?;
    let now = Instant::now();
    let second = now.duration_since(context.start).as_secs();
    pending.insert(
        token,
        Pending {
            target: idx,
            sent_at: now,
            deadline: now + config.timeout,
            seq,
            second,
        },
    );
    target.sent += 1;
    target.stats.on_send();
    if matches!(config.mode, Mode::Monitor | Mode::Tui) {
        history.on_send(second);
    }
    Ok(())
}

fn event_reply(config: &Config, target: &Target, seq: u16, ms: f64, bytes: usize) {
    if config.quiet
        || (config.output == Output::Text
            && matches!(config.mode, Mode::Range | Mode::Monitor | Mode::Tui))
    {
        return;
    }
    match config.output {
        Output::Text => emit(&format!(
            "{bytes} bytes from {}: icmp_seq={seq} time={ms:.3} ms",
            target.addr.ip()
        )),
        Output::Jsonl => emit(&format!(
            "{{\"schema\":1,\"event\":\"reply\",\"target\":\"{}\",\"ip\":\"{}\",\"seq\":{seq},\"rtt_ms\":{ms:.3},\"timestamp_unix_ms\":{}}}",
            json_escape(&target.label),
            target.addr.ip(),
            timestamp_unix_ms()
        )),
        Output::Csv => emit(&format!(
            "reply,{},{},{seq},{ms:.3},,,,{}",
            csv_escape(&target.label),
            csv_escape(&target.addr.ip().to_string()),
            timestamp_unix_ms()
        )),
    }
}

fn event_timeout(config: &Config, target: &Target, seq: u16) {
    if config.quiet {
        return;
    }
    match config.output {
        Output::Text => {}
        Output::Jsonl => emit(&format!(
            "{{\"schema\":1,\"event\":\"timeout\",\"target\":\"{}\",\"ip\":\"{}\",\"seq\":{seq},\"timestamp_unix_ms\":{}}}",
            json_escape(&target.label),
            target.addr.ip(),
            timestamp_unix_ms()
        )),
        Output::Csv => emit(&format!(
            "timeout,{},{},{seq},,,,,{}",
            csv_escape(&target.label),
            csv_escape(&target.addr.ip().to_string()),
            timestamp_unix_ms()
        )),
    }
}

fn on_reply(
    config: &Config,
    incoming: Received,
    nonce: u64,
    pending: &mut HashMap<u64, Pending>,
    targets: &mut [Target],
    history: &mut History,
    recent: &mut VecDeque<(u64, usize, bool)>,
) -> Option<usize> {
    if incoming.echo.nonce != nonce {
        return None;
    }
    let token = incoming.echo.token;
    let now = Instant::now();
    if let Some(probe) = pending.get(&token) {
        if probe.seq != incoming.echo.seq
            || targets[probe.target].addr.ip() != incoming.source
            || now > probe.deadline
        {
            return None;
        }
        let Some(probe) = pending.remove(&token) else {
            return None;
        };
        let ms = now.duration_since(probe.sent_at).as_secs_f64() * 1000.0;
        let target = &mut targets[probe.target];
        target.stats.on_reply(ms);
        if matches!(config.mode, Mode::Monitor | Mode::Tui) {
            history.on_reply(probe.second, ms);
        }
        event_reply(config, target, probe.seq, ms, incoming.echo.bytes);
        note_recent(recent, token, probe.target, true);
        return Some(probe.target);
    } else if let Some((_, idx, completed)) = recent.iter().find(|(id, _, _)| *id == token) {
        let target = &mut targets[*idx];
        if target.addr.ip() != incoming.source {
            return None;
        }
        if *completed {
            target.stats.duplicates += 1;
        }
    }
    None
}

fn note_recent(recent: &mut VecDeque<(u64, usize, bool)>, id: u64, idx: usize, replied: bool) {
    if recent.len() == 2048 {
        recent.pop_front();
    }
    recent.push_back((id, idx, replied));
}

fn print_summary(config: &Config, targets: &[Target], elapsed: Duration) -> i32 {
    let sent: u64 = targets.iter().map(|t| t.stats.sent).sum();
    let received: u64 = targets.iter().map(|t| t.stats.received).sum();
    let loss = if sent == 0 {
        100.0
    } else {
        100.0 * sent.saturating_sub(received) as f64 / sent as f64
    };
    let alive = targets.iter().filter(|t| t.stats.received > 0).count();
    let all_alive = alive == targets.len();
    match config.output {
        Output::Text => {
            if config.mode == Mode::Range {
                if !config.quiet {
                    for t in targets {
                        if t.stats.received > 0 {
                            emit(&format!(
                                "UP   {:<39} received {}/{}  avg {:.3} ms",
                                t.addr.ip(),
                                t.stats.received,
                                t.stats.sent,
                                t.stats.mean
                            ));
                        } else if !config.only_alive {
                            emit(&format!(
                                "DOWN {:<39} received 0/{}",
                                t.addr.ip(),
                                t.stats.sent
                            ));
                        }
                    }
                }
                emit(&format!(
                    "--- range summary: {} scanned, {alive} alive, {} down ---",
                    targets.len(),
                    targets.len() - alive
                ));
            } else {
                emit(&format!(
                    "\n--- {} pipi statistics ---",
                    cli::visible_host(&targets[0].label)
                ));
            }
            emit(&format!(
                "{sent} packets transmitted, {received} received, {loss:.1}% packet loss, time {}ms",
                elapsed.as_millis()
            ));
            if config.mode != Mode::Range && received > 0 {
                let s = &targets[0].stats;
                emit(&format!(
                    "rtt min/avg/max/mdev = {:.3}/{:.3}/{:.3}/{:.3} ms  p95={:.3} jitter={:.3} ms",
                    s.min,
                    s.mean,
                    s.max,
                    s.mdev(),
                    s.percentile(0.95),
                    s.jitter()
                ));
                if s.duplicates > 0 {
                    emit(&format!("{} duplicate replies ignored", s.duplicates));
                }
            }
        }
        Output::Jsonl => {
            for t in targets {
                emit(&format!(
                    "{{\"schema\":1,\"event\":\"summary\",\"target\":\"{}\",\"ip\":\"{}\",\"sent\":{},\"received\":{},\"loss_pct\":{:.3},\"min_ms\":{:.3},\"avg_ms\":{:.3},\"max_ms\":{:.3},\"mdev_ms\":{:.3},\"p95_ms\":{:.3},\"jitter_ms\":{:.3},\"duplicates\":{},\"timestamp_unix_ms\":{}}}",
                    json_escape(&t.label),
                    t.addr.ip(),
                    t.stats.sent,
                    t.stats.received,
                    t.stats.loss_percent(),
                    t.stats.min,
                    t.stats.mean,
                    t.stats.max,
                    t.stats.mdev(),
                    t.stats.percentile(0.95),
                    t.stats.jitter(),
                    t.stats.duplicates,
                    timestamp_unix_ms()
                ));
            }
        }
        Output::Csv => {
            for t in targets {
                emit(&format!(
                    "summary,{},{},,,{},{},{:.3},{}",
                    csv_escape(&t.label),
                    csv_escape(&t.addr.ip().to_string()),
                    t.stats.sent,
                    t.stats.received,
                    t.stats.loss_percent(),
                    timestamp_unix_ms()
                ));
            }
        }
    }
    if config.mode == Mode::Range && config.require_all {
        if all_alive { 0 } else { 1 }
    } else if received > 0 {
        0
    } else {
        1
    }
}

fn socket_error(e: io::Error) -> String {
    match e.raw_os_error() {
        Some(1 | 13) => format!(
            "ICMP ping socket not permitted ({e}). Check /proc/sys/net/ipv4/ping_group_range and your group ID; pipi never invokes sudo or changes sysctl"
        ),
        Some(97) => format!("requested IP family is unavailable ({e}); check IPv6 support"),
        _ => format!("cannot open Linux ICMP echo socket: {e}"),
    }
}

pub(crate) fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::Relaxed)
}

/// Executes the complete bounded probe loop; 0 reply / 1 none / 2 reserved for caller errors.
pub fn run(config: Config) -> Result<i32, String> {
    if config.mode == Mode::Tui && !cfg!(feature = "tui") {
        return Err(
            "TUI was not built; use cargo build --features tui or 'pipi monitor HOST'".into(),
        );
    }
    let mut targets = if config.mode == Mode::Range {
        range::expand(&config.target, config.max_hosts, config.family)?
            .into_iter()
            .map(|ip| Target::new_sweep(ip.to_string(), SocketAddr::new(ip, 0)))
            .collect::<Vec<_>>()
    } else {
        let addr = cli::resolve(&config.target, config.family).map_err(|e| e.to_string())?;
        vec![Target::new(cli::visible_host(&config.target), addr)]
    };
    // Use otherwise informational config options without altering classic text output.
    let _colour_enabled = !config.no_color && std::env::var_os("NO_COLOR").is_none();
    let nonce = entropy_nonce()?;
    let addrs: Vec<_> = targets.iter().map(|target| target.addr).collect();
    let sockets = SocketSet::for_targets(&addrs, config.ttl).map_err(socket_error)?;
    install_signals();
    let is_tty = io::stdout().is_terminal();
    let ascii = config.ascii || !is_tty;
    let mut history = History::new(config.window);
    let mut pending: HashMap<u64, Pending> = HashMap::new();
    let mut recent: VecDeque<(u64, usize, bool)> = VecDeque::new();
    let mut ready: VecDeque<usize> = if config.mode == Mode::Range {
        (0..targets.len()).collect()
    } else {
        VecDeque::new()
    };
    // All probe payload memory is allocated once and reused.
    let mut packet = vec![0x42; config.size + 8];
    let mut token = 1_u64;
    let mut second_to_render = 0_u64;
    let start = Instant::now();
    let mut next_send = start;

    #[cfg(feature = "tui")]
    let screen = if config.mode == Mode::Tui {
        Some(crate::tui::Screen::enter()?)
    } else {
        None
    };
    init_output(&config, &targets);

    loop {
        if INTERRUPTED.load(Ordering::Relaxed) {
            break;
        }
        let now = Instant::now();
        pending.retain(|id, probe| {
            if probe.deadline > now {
                return true;
            }
            if matches!(config.mode, Mode::Monitor | Mode::Tui) {
                history.on_timeout(probe.second);
            }
            event_timeout(&config, &targets[probe.target], probe.seq);
            note_recent(&mut recent, *id, probe.target, false);
            if config.mode == Mode::Range
                && config.count.is_some_and(|limit| targets[probe.target].sent < limit)
            {
                ready.push_back(probe.target);
            }
            false
        });
        let elapsed = now.duration_since(start);
        if matches!(config.mode, Mode::Monitor | Mode::Tui) {
            #[cfg(feature = "tui")]
            let redraw = second_to_render < elapsed.as_secs();
            while second_to_render < elapsed.as_secs() {
                if config.mode == Mode::Monitor && config.output == Output::Text && !config.quiet {
                    emit(&history.line(second_to_render, ascii));
                }
                second_to_render += 1;
            }
            #[cfg(feature = "tui")]
            if redraw {
                if let Some(screen) = screen.as_ref() {
                    screen.show(
                        &cli::visible_host(&config.target),
                        &history.recent(22, config.ascii),
                        &targets[0].stats,
                    );
                }
            }
        }
        let finished = config
            .count
            .is_some_and(|limit| targets.iter().all(|t| t.sent >= limit));
        if finished && pending.is_empty() {
            break;
        }
        let candidate = if now >= next_send && pending.len() < config.jobs {
            select_target(&targets, &mut ready, config.count, config.mode == Mode::Range)
        } else {
            None
        };
        if let Some(idx) = candidate {
            send_one(
                &SendContext {
                    config: &config,
                    sockets: &sockets,
                    start,
                    nonce,
                },
                &mut targets,
                &mut pending,
                &mut history,
                &mut packet,
                token,
                idx,
            )?;
            token = token.checked_add(1).ok_or("probe sequence exhausted")?;
            next_send = Instant::now() + config.interval;
        }
        let mut delay = Duration::from_secs(60);
        let now = Instant::now();
        if next_send > now {
            delay = delay.min(next_send.duration_since(now));
        }
        if let Some(deadline) = pending.values().map(|p| p.deadline).min() {
            delay = delay.min(deadline.saturating_duration_since(now));
        }
        if matches!(config.mode, Mode::Monitor | Mode::Tui) {
            let next_graph = start + Duration::from_secs(second_to_render + 1);
            delay = delay.min(next_graph.saturating_duration_since(now));
        }
        sockets
            .wait(delay)
            .map_err(|e| format!("poll ICMP sockets: {e}"))?;
        sockets
            .drain(|reply| {
                let completed = on_reply(
                    &config,
                    reply,
                    nonce,
                    &mut pending,
                    &mut targets,
                    &mut history,
                    &mut recent,
                );
                if let Some(idx) = completed
                    && config.mode == Mode::Range
                    && config.count.is_some_and(|limit| targets[idx].sent < limit)
                {
                    ready.push_back(idx);
                }
            })
            .map_err(|e| format!("receive ICMP: {e}"))?;
    }

    #[cfg(feature = "tui")]
    drop(screen); // Restore terminal before printing conventional summary.
    Ok(print_summary(&config, &targets, start.elapsed()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn range_ready_queue_is_fair_and_one_probe_per_host() {
        let ip = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let targets = vec![
            Target::new_sweep("a".into(), ip),
            Target::new_sweep("b".into(), ip),
            Target::new_sweep("c".into(), ip),
        ];
        let mut ready: VecDeque<_> = (0..targets.len()).collect();
        assert_eq!(select_target(&targets, &mut ready, Some(2), true), Some(0));
        assert_eq!(select_target(&targets, &mut ready, Some(2), true), Some(1));
        assert_eq!(select_target(&targets, &mut ready, Some(2), true), Some(2));
        assert_eq!(select_target(&targets, &mut ready, Some(2), true), None);
        ready.push_back(0);
        ready.push_back(2);
        assert_eq!(select_target(&targets, &mut ready, Some(2), true), Some(0));
        assert_eq!(select_target(&targets, &mut ready, Some(2), true), Some(2));
    }

    #[test]
    fn classic_and_fast_have_constant_time_target_selection() {
        let ip = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let mut targets = vec![Target::new("one".into(), ip)];
        let mut ready = VecDeque::new();
        assert_eq!(select_target(&targets, &mut ready, None, false), Some(0));
        targets[0].sent = 100;
        assert_eq!(select_target(&targets, &mut ready, Some(100), false), None);
    }

    #[test]
    fn json_and_csv_escape_network_strings() {
        assert_eq!(json_escape("x\"\n"), "x\\\"\\n");
        assert_eq!(csv_escape("a\"b"), "\"a\"\"b\"");
    }

    fn fake_config() -> Config {
        let args: Vec<String> = ["-q", "-c", "1", "127.0.0.1"]
            .into_iter()
            .map(str::to_string)
            .collect();
        let cli::Parsed::Run(config) = cli::parse(&args).unwrap() else {
            panic!("expected run configuration");
        };
        config
    }

    fn fake_incoming(source: IpAddr, seq: u16, nonce: u64, token: u64) -> Received {
        Received {
            source,
            echo: linux::Echo {
                seq,
                nonce,
                token,
                bytes: 64,
            },
        }
    }

    #[test]
    fn invalid_source_nonce_and_sequence_are_not_accepted() {
        let config = fake_config();
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let addr = SocketAddr::new(ip, 0);
        let mut targets = vec![Target::new("loopback".into(), addr)];
        targets[0].sent = 1;
        targets[0].stats.on_send();
        let mut pending = HashMap::new();
        let start = Instant::now();
        pending.insert(
            42,
            Pending {
                target: 0,
                sent_at: start,
                deadline: start + Duration::from_secs(1),
                seq: 42,
                second: 0,
            },
        );
        let mut history = History::new(5);
        let mut recent = VecDeque::new();
        let _ = on_reply(
            &config,
            fake_incoming(ip, 42, 999, 42),
            123,
            &mut pending,
            &mut targets,
            &mut history,
            &mut recent,
        );
        let _ = on_reply(
            &config,
            fake_incoming(ip, 41, 123, 42),
            123,
            &mut pending,
            &mut targets,
            &mut history,
            &mut recent,
        );
        let _ = on_reply(
            &config,
            fake_incoming(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)), 42, 123, 42),
            123,
            &mut pending,
            &mut targets,
            &mut history,
            &mut recent,
        );
        assert_eq!(targets[0].stats.received, 0);
        assert!(pending.contains_key(&42));
    }

    #[test]
    fn valid_reply_is_counted_once_and_duplicate_is_tracked() {
        let config = fake_config();
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let addr = SocketAddr::new(ip, 0);
        let mut targets = vec![Target::new("loopback".into(), addr)];
        targets[0].sent = 1;
        targets[0].stats.on_send();
        let mut pending = HashMap::new();
        let start = Instant::now();
        pending.insert(
            42,
            Pending {
                target: 0,
                sent_at: start,
                deadline: start + Duration::from_secs(1),
                seq: 42,
                second: 0,
            },
        );
        let mut history = History::new(5);
        let mut recent = VecDeque::new();
        let _ = on_reply(
            &config,
            fake_incoming(ip, 42, 123, 42),
            123,
            &mut pending,
            &mut targets,
            &mut history,
            &mut recent,
        );
        assert_eq!(targets[0].stats.received, 1);
        assert!(pending.is_empty());
        let _ = on_reply(
            &config,
            fake_incoming(ip, 42, 123, 42),
            123,
            &mut pending,
            &mut targets,
            &mut history,
            &mut recent,
        );
        assert_eq!(targets[0].stats.received, 1);
        assert_eq!(targets[0].stats.duplicates, 1);
    }

    #[test]
    fn expired_probe_is_never_accepted_late() {
        let config = fake_config();
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let mut targets = vec![Target::new("loopback".into(), SocketAddr::new(ip, 0))];
        targets[0].stats.on_send();
        let mut pending = HashMap::new();
        let start = Instant::now();
        pending.insert(
            7,
            Pending {
                target: 0,
                sent_at: start - Duration::from_secs(2),
                deadline: start - Duration::from_secs(1),
                seq: 7,
                second: 0,
            },
        );
        let mut history = History::new(5);
        let mut recent = VecDeque::new();
        let _ = on_reply(
            &config,
            fake_incoming(ip, 7, 123, 7),
            123,
            &mut pending,
            &mut targets,
            &mut history,
            &mut recent,
        );
        assert_eq!(targets[0].stats.received, 0);
        assert!(pending.contains_key(&7));
    }

    #[test]
    fn completed_probe_queue_is_capped() {
        let mut q = VecDeque::new();
        for token in 0..10000 {
            note_recent(&mut q, token, 0, true);
        }
        assert_eq!(q.len(), 2048);
        assert_eq!(q.front().unwrap().0, 7952);
    }
}
