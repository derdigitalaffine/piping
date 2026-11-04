//! Strict, dependency-free command-line parser. This is an intentionally documented
//! subset of iputils ping, not a drop-in clone.

use std::fmt;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Classic,
    Fast,
    Range,
    Monitor,
    Tui,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    Text,
    Jsonl,
    Csv,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub mode: Mode,
    pub target: String,
    pub count: Option<u32>,
    pub interval: Duration,
    pub timeout: Duration,
    pub family: Option<bool>, // Some(true) = IPv6, Some(false) = IPv4.
    pub quiet: bool,
    pub numeric: bool,
    pub ttl: Option<u8>,
    pub size: usize,
    pub jobs: usize,
    pub max_hosts: usize,
    pub only_alive: bool,
    pub require_all: bool,
    pub window: usize,
    pub ascii: bool,
    pub no_color: bool,
    pub output: Output,
}

#[derive(Debug)]
pub enum Parsed {
    Run(Config),
    Help,
    Version,
}

#[derive(Debug)]
pub struct CliError(pub String);

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for CliError {}

fn value(args: &[String], idx: &mut usize, name: &str) -> Result<String, CliError> {
    *idx += 1;
    args.get(*idx)
        .cloned()
        .ok_or_else(|| CliError(format!("{name} requires a value")))
}

fn number<T>(name: &str, raw: &str) -> Result<T, CliError>
where
    T: std::str::FromStr,
{
    raw.parse::<T>()
        .map_err(|_| CliError(format!("invalid {name}: {raw}")))
}

fn seconds(name: &str, raw: &str) -> Result<Duration, CliError> {
    let v: f64 = number(name, raw)?;
    if !v.is_finite() || !(0.02..=3600.0).contains(&v) {
        return Err(CliError(format!(
            "{name} must be between 0.02 and 3600 seconds"
        )));
    }
    Ok(Duration::from_secs_f64(v))
}

pub fn parse(args: &[String]) -> Result<Parsed, CliError> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        return Ok(Parsed::Help);
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        return Ok(Parsed::Version);
    }
    let mut i = 0;
    let mut mode = Mode::Classic;
    if let Some(first) = args.first() {
        mode = match first.as_str() {
            "fast" => Mode::Fast,
            "range" => Mode::Range,
            "monitor" => Mode::Monitor,
            "tui" => Mode::Tui,
            _ => Mode::Classic,
        };
        if mode != Mode::Classic {
            i += 1;
        }
    }

    let mut target: Option<String> = None;
    let mut count = None;
    let mut interval = None;
    let mut timeout = Duration::from_secs(2);
    let mut family = None;
    let mut quiet = false;
    let mut numeric = false;
    let mut ttl = None;
    let mut size = 56_usize;
    let mut pps: Option<u32> = None;
    let mut jobs = None;
    let mut max_hosts = 256_usize;
    let mut only_alive = false;
    let mut require_all = false;
    let mut window = 60_usize;
    let mut ascii = false;
    let mut no_color = false;
    let mut output = Output::Text;
    let mut positional = false;
    while i < args.len() {
        let a = &args[i];
        if !positional && a == "--" {
            positional = true;
        } else if !positional && (a == "-c" || a == "--count") {
            let s = value(args, &mut i, a)?;
            let n: u32 = number("count", &s)?;
            if n == 0 || n > 1_000_000 {
                return Err(CliError("count must be within 1..=1000000".into()));
            }
            count = Some(n);
        } else if !positional && (a == "-i" || a == "--interval") {
            interval = Some(seconds("interval", &value(args, &mut i, a)?)?);
        } else if !positional && (a == "-W" || a == "--timeout") {
            timeout = seconds("timeout", &value(args, &mut i, a)?)?;
        } else if !positional && a == "-4" {
            if family == Some(true) {
                return Err(CliError("-4 conflicts with -6".into()));
            }
            family = Some(false);
        } else if !positional && a == "-6" {
            if family == Some(false) {
                return Err(CliError("-6 conflicts with -4".into()));
            }
            family = Some(true);
        } else if !positional && (a == "-n" || a == "--numeric") {
            numeric = true;
        } else if !positional && (a == "-q" || a == "--quiet") {
            quiet = true;
        } else if !positional && a == "--ttl" {
            let n: u16 = number("ttl", &value(args, &mut i, a)?)?;
            if !(1..=255).contains(&n) {
                return Err(CliError("ttl must be in 1..=255".into()));
            }
            ttl = Some(n as u8);
        } else if !positional && a == "--size" {
            size = number("size", &value(args, &mut i, a)?)?;
            if !(16..=1400).contains(&size) {
                return Err(CliError("ICMP payload size must be 16..=1400 bytes".into()));
            }
        } else if !positional && a == "--pps" {
            let n: u32 = number("pps", &value(args, &mut i, a)?)?;
            if !(1..=1000).contains(&n) {
                return Err(CliError("pps must be within 1..=1000".into()));
            }
            pps = Some(n);
        } else if !positional && a == "--jobs" {
            let n: usize = number("jobs", &value(args, &mut i, a)?)?;
            if !(1..=128).contains(&n) {
                return Err(CliError("jobs must be within 1..=128".into()));
            }
            jobs = Some(n);
        } else if !positional && a == "--max-hosts" {
            max_hosts = number("max-hosts", &value(args, &mut i, a)?)?;
            if !(1..=4096).contains(&max_hosts) {
                return Err(CliError("max-hosts must be within 1..=4096".into()));
            }
        } else if !positional && a == "--only-alive" {
            only_alive = true;
        } else if !positional && a == "--require-all" {
            require_all = true;
        } else if !positional && a == "--window" {
            window = number("window", &value(args, &mut i, a)?)?;
            if !(5..=3600).contains(&window) {
                return Err(CliError("window must be within 5..=3600 seconds".into()));
            }
        } else if !positional && a == "--ascii" {
            ascii = true;
        } else if !positional && a == "--no-color" {
            no_color = true;
        } else if !positional && a == "--output" {
            output = match value(args, &mut i, a)?.as_str() {
                "text" => Output::Text,
                "jsonl" => Output::Jsonl,
                "csv" => Output::Csv,
                other => return Err(CliError(format!("unsupported output: {other}"))),
            };
        } else if !positional && a.starts_with('-') {
            return Err(CliError(format!("unknown option: {a}")));
        } else if target.is_some() {
            return Err(CliError(format!("unexpected positional argument: {a}")));
        } else {
            target = Some(a.clone());
        }
        i += 1;
    }
    let target = target.ok_or_else(|| CliError("missing HOST or range target".into()))?;
    if target.trim().is_empty() {
        return Err(CliError("empty host".into()));
    }
    if pps.is_some() && !matches!(mode, Mode::Fast | Mode::Range) {
        return Err(CliError(
            "--pps is supported only for fast and range".into(),
        ));
    }
    if interval.is_some() && matches!(mode, Mode::Fast | Mode::Range) {
        return Err(CliError(
            "use --pps rather than --interval for fast/range".into(),
        ));
    }
    if jobs.is_some() && !matches!(mode, Mode::Fast | Mode::Range) {
        return Err(CliError(
            "--jobs is supported only for fast and range".into(),
        ));
    }
    if (only_alive || require_all || max_hosts != 256) && mode != Mode::Range {
        return Err(CliError(
            "--only-alive/--require-all/--max-hosts require range".into(),
        ));
    }
    if window != 60 && !matches!(mode, Mode::Monitor | Mode::Tui) {
        return Err(CliError("--window requires monitor or tui".into()));
    }
    if matches!(mode, Mode::Tui) && output != Output::Text {
        return Err(CliError("tui requires text output".into()));
    }
    if mode == Mode::Range && family == Some(true) && target.contains('-') {
        return Err(CliError(
            "IPv4 endpoint ranges cannot be combined with -6".into(),
        ));
    }
    let interval = match mode {
        Mode::Fast | Mode::Range => Duration::from_secs_f64(1.0 / f64::from(pps.unwrap_or(20))),
        Mode::Monitor | Mode::Tui => interval.unwrap_or(Duration::from_millis(250)),
        Mode::Classic => interval.unwrap_or(Duration::from_secs(1)),
    };
    let count = count.or(match mode {
        Mode::Fast => Some(100),
        Mode::Range => Some(1),
        _ => None,
    });
    Ok(Parsed::Run(Config {
        mode,
        target,
        count,
        interval,
        timeout,
        family,
        quiet,
        numeric,
        ttl,
        size,
        jobs: jobs.unwrap_or(if mode == Mode::Range {
            32
        } else if mode == Mode::Fast {
            64
        } else {
            16
        }),
        max_hosts,
        only_alive,
        require_all,
        window,
        ascii,
        no_color,
        output,
    }))
}

/// Run potentially blocking system DNS resolution under a fixed deadline.
/// The resolver thread is intentionally detached on timeout: the CLI exits on error,
/// and never blocks the main process indefinitely on an unresponsive NSS backend.
fn with_deadline<T, F>(timeout: Duration, task: F) -> Result<T, CliError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, CliError> + Send + 'static,
{
    use std::sync::mpsc::{self, RecvTimeoutError};
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("piping-dns-resolver".into())
        .spawn(move || {
            let _ = tx.send(task());
        })
        .map_err(|e| CliError(format!("cannot start resolver thread: {e}")))?;
    match rx.recv_timeout(timeout) {
        Ok(result) => result,
        Err(RecvTimeoutError::Timeout) => Err(CliError(format!(
            "DNS resolution timed out after {:.0} seconds",
            timeout.as_secs_f64()
        ))),
        Err(RecvTimeoutError::Disconnected) => {
            Err(CliError("DNS resolver thread terminated unexpectedly".into()))
        }
    }
}

pub fn resolve(host: &str, family: Option<bool>) -> Result<SocketAddr, CliError> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        if family.is_some_and(|v6| v6 != ip.is_ipv6()) {
            return Err(CliError(format!("no matching IP family for {}", visible_host(host))));
        }
        return Ok(SocketAddr::new(ip, 0));
    }
    let target = host.to_owned();
    let addresses = with_deadline(Duration::from_secs(5), move || {
        (target.as_str(), 0_u16)
            .to_socket_addrs()
            .map(|iter| iter.collect::<Vec<_>>())
            .map_err(|e| CliError(format!("cannot resolve {}: {e}", visible_host(&target))))
    })?;
    let mut first = None;
    for addr in addresses {
        if family.is_some_and(|v6| v6 != addr.is_ipv6()) {
            continue;
        }
        if first.is_none() {
            first = Some(addr);
        }
        if family.is_none() && addr.is_ipv4() {
            return Ok(addr);
        }
    }
    first.ok_or_else(|| CliError(format!("no matching IP address for {}", visible_host(host))))
}

pub fn help() -> &'static str {
    concat!(
        "piping 0.1.0 — Linux ICMP Echo diagnostics (Rust)\n",
        "USAGE:\n",
        "  piping [OPTIONS] HOST                 Classic ping, Ctrl-C for summary\n",
        "  piping fast [OPTIONS] HOST            Bounded high-frequency ping\n",
        "  piping range [OPTIONS] CIDR|START-END ICMP echo sweep\n",
        "  piping monitor [OPTIONS] HOST         Smoke-style streaming RTT graph\n",
        "  piping tui [OPTIONS] HOST             Optional fullscreen UI (feature tui)\n\n",
        "COMMON:\n",
        "  -c, --count N       Number of probes (range: per host)\n",
        "  -i, --interval SEC  Time between probes, classic/monitor/tui\n",
        "  -W, --timeout SEC   Per-probe response timeout (default 2)\n",
        "  -4 / -6             Force IPv4 / IPv6\n",
        "  -n, --numeric       Never reverse-resolve addresses\n",
        "  -q, --quiet         Only print statistics\n",
        "  --ttl N             IPv4 TTL or IPv6 hop limit (1..255)\n",
        "  --size N            ICMP data bytes (16..1400; default 56)\n",
        "  --output FORMAT     text|jsonl|csv\n",
        "  --ascii             Force ASCII smoke graph\n",
        "  --no-color          Disable terminal colors (also honors NO_COLOR)\n\n",
        "FAST/RANGE:\n",
        "  --pps N             Global rate cap (1..1000; default 20)\n",
        "  --jobs N            Max outstanding probes (1..128)\n",
        "  --max-hosts N       Range target cap (1..4096; default 256)\n",
        "  --only-alive        Hide nonresponding range hosts\n",
        "  --require-all       Range exit 0 only if every host responds\n",
        "MONITOR/TUI:\n",
        "  --window N          Rolling graph history in seconds (5..3600)\n\n",
        "EXIT: 0 = reply, 1 = no reply, 2 = syntax/network/socket error\n",
        "Ping sockets depend on Linux kernel/group permissions; no auto-sudo.\n"
    )
}

pub fn visible_host(host: &str) -> String {
    // Never echo terminal-control characters supplied through CLI/DNS.
    host.chars()
        .filter(|c| !c.is_control() && *c != '\u{1b}')
        .take(256)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn classic_is_default_and_infinite() {
        let Parsed::Run(c) = parse(&args(&["example.com"])).unwrap() else {
            panic!()
        };
        assert_eq!(c.mode, Mode::Classic);
        assert_eq!(c.count, None);
        assert_eq!(c.size, 56);
    }

    #[test]
    fn fast_and_range_have_bounded_defaults() {
        let Parsed::Run(f) = parse(&args(&["fast", "127.0.0.1"])).unwrap() else {
            panic!()
        };
        assert_eq!(f.count, Some(100));
        assert_eq!(f.interval, Duration::from_millis(50));
        let Parsed::Run(r) = parse(&args(&["range", "192.0.2.0/30"])).unwrap() else {
            panic!()
        };
        assert_eq!(r.count, Some(1));
    }

    #[test]
    fn reject_unsafe_or_ambiguous_flags() {
        for test in [
            args(&["fast", "x", "--pps", "0"]),
            args(&["fast", "x", "--pps", "1001"]),
            args(&["range", "0.0.0.0/0", "--max-hosts", "99999"]),
            args(&["-4", "-6", "x"]),
            args(&["-c", "0", "x"]),
            args(&["--size", "0", "x"]),
            args(&["--interval", "nan", "x"]),
            args(&["fast", "x", "-i", "1"]),
            args(&["x", "--pps", "5"]),
        ] {
            assert!(parse(&test).is_err(), "{test:?}");
        }
    }

    #[test]
    fn command_help_does_not_require_host() {
        assert!(matches!(parse(&args(&["--help"])), Ok(Parsed::Help)));
        assert!(matches!(
            parse(&args(&["fast", "--version"])),
            Ok(Parsed::Version)
        ));
    }

    #[test]
    fn numeric_ip_bypasses_dns_and_obeys_family() {
        assert!(resolve("127.0.0.1", Some(false)).unwrap().is_ipv4());
        assert!(resolve("::1", Some(true)).unwrap().is_ipv6());
        assert!(resolve("::1", Some(false)).is_err());
    }

    #[test]
    fn bounded_resolver_is_not_stalled_by_slow_nss() {
        let result: Result<u32, CliError> = with_deadline(Duration::from_millis(10), || {
            std::thread::sleep(Duration::from_millis(50));
            Ok(7)
        });
        assert!(result.unwrap_err().to_string().contains("timed out"));
    }

    #[test]
    fn terminal_sequences_removed() {
        assert_eq!(visible_host("a\u{1b}[31m\nb"), "a[31mb");
    }
}
