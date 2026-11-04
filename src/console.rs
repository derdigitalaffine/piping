//! Interactive keyboard forms and menu for every pipi network diagnostic.
//! No shell, no implicit probes, bounded log buffer and second confirmation for ARP.

use crate::tui::{self, Screen};
use std::collections::VecDeque;
use std::ffi::{c_int, c_void};
use std::io::{self, BufRead, IsTerminal};
use std::mem::MaybeUninit;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::time::Instant;

const LOG_MAX: usize = 120;
// Preserve the next character when ESC is followed immediately by ordinary typing.
static BUFFERED_KEY: AtomicU8 = AtomicU8::new(0);
#[repr(C)]
#[derive(Clone, Copy)]
struct Termios {
    iflag: u32,
    oflag: u32,
    cflag: u32,
    lflag: u32,
    line: u8,
    cc: [u8; 32],
    ispeed: u32,
    ospeed: u32,
}
#[repr(C)]
struct PollFd {
    fd: c_int,
    events: i16,
    revents: i16,
}
unsafe extern "C" {
    fn tcgetattr(fd: c_int, value: *mut Termios) -> c_int;
    fn tcsetattr(fd: c_int, action: c_int, value: *const Termios) -> c_int;
    #[link_name = "read"]
    fn fd_read(fd: c_int, buf: *mut c_void, size: usize) -> isize;
    fn poll(fds: *mut PollFd, n: usize, millis: c_int) -> c_int;
}
struct RawInput(Termios);
impl RawInput {
    fn enable() -> Result<Self, String> {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err("pipi menu requires interactive stdin and stdout (TTY)".into());
        }
        let mut prior = MaybeUninit::<Termios>::uninit();
        // SAFETY: complete Linux termios is written into aligned, valid memory.
        if unsafe { tcgetattr(0, prior.as_mut_ptr()) } < 0 {
            return Err(format!("tcgetattr: {}", io::Error::last_os_error()));
        }
        // SAFETY: tcgetattr returned success and initialized all bytes.
        let prior = unsafe { prior.assume_init() };
        let mut raw = prior;
        raw.lflag &= !(0x0002 | 0x0008 | 0x0001 | 0x8000); // ICANON/ECHO/ISIG/IEXTEN
        raw.cc[6] = 1; // VMIN
        raw.cc[5] = 0; // VTIME
        // SAFETY: TCSANOW is zero; pointer is live Linux termios ABI.
        if unsafe { tcsetattr(0, 0, &raw) } < 0 {
            return Err(format!("tcsetattr: {}", io::Error::last_os_error()));
        }
        Ok(Self(prior))
    }
}
impl Drop for RawInput {
    fn drop(&mut self) {
        // SAFETY: restore the exact previous terminal state.
        unsafe {
            tcsetattr(0, 0, &self.0);
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    Up,
    Down,
    Left,
    Right,
    Tab,
    BackTab,
    Enter,
    Backspace,
    Esc,
    CtrlC,
    Start,
    Char(char),
}
fn available(ms: c_int) -> Result<bool, String> {
    let mut fd = PollFd {
        fd: 0,
        events: 1,
        revents: 0,
    };
    // SAFETY: poll receives one initialized writable fd.
    let rc = unsafe { poll(&mut fd, 1, ms) };
    if rc < 0 {
        let err = io::Error::last_os_error();
        return if err.kind() == io::ErrorKind::Interrupted {
            Ok(false)
        } else {
            Err(format!("stdin poll: {err}"))
        };
    }
    Ok(rc > 0 && fd.revents & 1 != 0)
}
fn byte() -> Result<u8, String> {
    let mut b = 0u8;
    // SAFETY: writable one-byte buffer remains live during syscall.
    if unsafe { fd_read(0, (&mut b as *mut u8).cast(), 1) } == 1 {
        Ok(b)
    } else {
        Err(format!("stdin read: {}", io::Error::last_os_error()))
    }
}
fn key(ms: c_int) -> Result<Option<Key>, String> {
    let buffered = BUFFERED_KEY.swap(0, Ordering::Relaxed);
    let b = if buffered != 0 {
        buffered
    } else {
        if !available(ms)? {
            return Ok(None);
        }
        byte()?
    };
    let k = match b {
        3 => Key::CtrlC,
        9 => Key::Tab,
        10 | 13 => Key::Enter,
        8 | 127 => Key::Backspace,
        18 => Key::Start,
        27 => {
            if !available(25)? {
                return Ok(Some(Key::Esc));
            }
            let second = byte()?;
            if second != b'[' && second != b'O' {
                BUFFERED_KEY.store(second, Ordering::Relaxed);
                return Ok(Some(Key::Esc));
            }
            if !available(25)? {
                return Ok(Some(Key::Esc));
            }
            match byte()? {
                b'A' => Key::Up,
                b'B' => Key::Down,
                b'C' => Key::Right,
                b'D' => Key::Left,
                b'Z' => Key::BackTab,
                b'1' => {
                    if available(15)? && byte()? == b'5' && available(15)? {
                        let _ = byte()?;
                        Key::Start
                    } else {
                        Key::Esc
                    }
                }
                _ => Key::Esc,
            }
        }
        32..=126 => Key::Char(char::from(b)),
        _ => return Ok(None),
    };
    Ok(Some(k))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Ping,
    Fast,
    Range,
    Smoke,
    Trace,
    Neighbors,
    Discover,
    Arp,
    AutoArp,
}
const MODES: [Mode; 9] = [
    Mode::Ping,
    Mode::Fast,
    Mode::Range,
    Mode::Smoke,
    Mode::Trace,
    Mode::Neighbors,
    Mode::Discover,
    Mode::Arp,
    Mode::AutoArp,
];
impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Ping => "PING",
            Self::Fast => "FAST",
            Self::Range => "RANGE",
            Self::Smoke => "SMOKE",
            Self::Trace => "TRACE",
            Self::Neighbors => "NEIGHBORHOOD",
            Self::Discover => "DISCOVER",
            Self::Arp => "ARP SCAN",
            Self::AutoArp => "LOCAL SCAN",
        }
    }
    fn note(self) -> &'static str {
        match self {
            Self::Ping => "ICMP echo and loss",
            Self::Fast => "Bounded high-rate ICMP",
            Self::Range => "CIDR / IP range sweep",
            Self::Smoke => "RTT/loss history chart",
            Self::Trace => "MTR-like hop telemetry + Smoke",
            Self::Neighbors => "Passive ARP/NDP visual map",
            Self::Discover => "Kernel ARP/NDP cache",
            Self::Arp => "ACTIVE: explicit on-link CIDR",
            Self::AutoArp => "ACTIVE: private on-link auto CIDR",
        }
    }
    fn active(self) -> bool {
        matches!(self, Self::Arp | Self::AutoArp)
    }
}
#[derive(Clone, Copy)]
enum Kind {
    Text,
    Choice(&'static [&'static str]),
}
#[derive(Clone)]
struct Field {
    key: &'static str,
    label: &'static str,
    value: String,
    kind: Kind,
    note: &'static str,
}
impl Field {
    fn text(key: &'static str, label: &'static str, value: &str, note: &'static str) -> Self {
        Self {
            key,
            label,
            value: value.into(),
            kind: Kind::Text,
            note,
        }
    }
    fn choice(
        key: &'static str,
        label: &'static str,
        value: &str,
        choices: &'static [&'static str],
        note: &'static str,
    ) -> Self {
        Self {
            key,
            label,
            value: value.into(),
            kind: Kind::Choice(choices),
            note,
        }
    }
    fn cycle(&mut self, reverse: bool) {
        if let Kind::Choice(items) = self.kind {
            let at = items.iter().position(|v| *v == self.value).unwrap_or(0);
            let next = if reverse {
                (at + items.len() - 1) % items.len()
            } else {
                (at + 1) % items.len()
            };
            self.value = items[next].into();
        }
    }
}
const IP: &[&str] = &["auto", "4", "6"];
const ALL: &[&str] = &["all", "4", "6"];
const BOOL: &[&str] = &["yes", "no"];
const GRAPH: &[&str] = &["unicode", "ascii"];
fn form(mode: Mode) -> Vec<Field> {
    let host = || Field::text("host", "Host / target", "127.0.0.1", "DNS host or IP");
    let family = || Field::choice("family", "IP family", "auto", IP, "auto / 4 / 6");
    let timeout = || Field::text("timeout", "Timeout sec", "2", "Per-probe limit");
    let iface = || Field::text("interface", "Interface", "", "e.g. eth0");
    let nf = || Field::choice("family", "IP family", "all", ALL, "ARP/NDP filter");
    let limit = || Field::text("results", "Max results", "128", "1..512 passive");
    match mode {
        Mode::Ping => vec![
            host(),
            Field::text("count", "Probes", "4", "blank=continuous"),
            Field::text("interval", "Interval sec", "1", "0.02..3600"),
            timeout(),
            family(),
        ],
        Mode::Fast => vec![
            host(),
            Field::text("count", "Probes", "100", "1..1,000,000"),
            Field::text("pps", "Rate pps", "20", "1..1000"),
            Field::text("jobs", "In flight", "64", "1..128"),
            timeout(),
            family(),
        ],
        Mode::Range => vec![
            Field::text(
                "host",
                "CIDR / range",
                "127.0.0.1/32",
                "Use authorized subnet",
            ),
            Field::text("count", "Probes/host", "1", "1..1,000,000"),
            Field::text("pps", "Rate pps", "20", "1..1000"),
            Field::text("jobs", "In flight", "32", "1..128"),
            Field::text("max-hosts", "Max hosts", "256", "1..4096"),
            Field::choice(
                "only-alive",
                "Only alive",
                "yes",
                BOOL,
                "Filter silent hosts",
            ),
            timeout(),
            family(),
        ],
        Mode::Smoke => vec![
            host(),
            Field::text("count", "Probes", "30", "blank=continuous"),
            Field::text("interval", "Interval sec", "0.25", "0.02..3600"),
            Field::text("window", "History sec", "60", "5..3600"),
            Field::choice("graph", "Chart", "unicode", GRAPH, "ASCII or blocks"),
            timeout(),
            family(),
        ],
        Mode::Trace => vec![
            host(),
            Field::text("count", "Cycles", "3", "blank=continuous"),
            Field::text("hops", "Max hops", "16", "1..32"),
            Field::text("interval", "Interval sec", "0.1", "0.05..5"),
            Field::text("timeout", "Hop timeout", "0.5", "0.1..5"),
            Field::choice("graph", "Chart", "unicode", GRAPH, "RTT/loss per hop"),
            family(),
        ],
        Mode::Neighbors => vec![
            iface(),
            nf(),
            limit(),
            Field::text("watch", "Refresh sec", "5", "passive; 2..120"),
            Field::choice("graph", "Map style", "unicode", GRAPH, "ASCII or Unicode"),
        ],
        Mode::Discover => vec![iface(), nf(), limit()],
        Mode::Arp => vec![
            Field::text(
                "host",
                "On-link CIDR",
                "192.168.1.0/24",
                "Only authorized IPv4",
            ),
            iface(),
            Field::text("pps", "Rate pps", "20", "1..100"),
            Field::text("max-hosts", "Max hosts", "256", "1..256"),
            Field::text("timeout", "Listen sec", "2", "0.1..5"),
        ],
        Mode::AutoArp => vec![
            iface(),
            limit(),
            Field::choice(
                "graph",
                "Map style",
                "unicode",
                GRAPH,
                "One-time, private CIDR",
            ),
        ],
    }
}
fn get<'a>(fields: &'a [Field], key: &str) -> &'a str {
    fields
        .iter()
        .find(|f| f.key == key)
        .map_or("", |f| f.value.trim())
}
fn flag(args: &mut Vec<String>, fields: &[Field], key: &str, name: &str) {
    if !get(fields, key).is_empty() {
        args.push(name.into());
        args.push(get(fields, key).into());
    }
}
fn arguments(mode: Mode, fields: &[Field]) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let fam = get(fields, "family");
    if (fam == "4" || fam == "6") && !matches!(mode, Mode::Neighbors | Mode::Discover) {
        args.push(format!("-{fam}"));
    }
    match mode {
        Mode::Ping | Mode::Fast | Mode::Range | Mode::Smoke => {
            if let Some(prefix) = match mode {
                Mode::Fast => Some("fast"),
                Mode::Range => Some("range"),
                Mode::Smoke => Some("monitor"),
                _ => None,
            } {
                args.insert(0, prefix.into());
            }
            for (k, v) in [
                ("count", "-c"),
                ("interval", "-i"),
                ("timeout", "-W"),
                ("pps", "--pps"),
                ("jobs", "--jobs"),
                ("max-hosts", "--max-hosts"),
                ("window", "--window"),
            ] {
                flag(&mut args, fields, k, v);
            }
            if mode == Mode::Range && get(fields, "only-alive") == "yes" {
                args.push("--only-alive".into());
            }
            if get(fields, "graph") == "ascii" {
                args.push("--ascii".into());
            }
            let target = get(fields, "host");
            if target.is_empty() {
                return Err("Enter a host / CIDR".into());
            }
            args.push(target.into());
            crate::cli::parse(&args).map_err(|e| e.to_string())?;
        }
        Mode::Trace => {
            args.insert(0, "trace".into());
            args.push("--smoke".into());
            args.push("--live-rows".into());
            for (k, v) in [
                ("count", "-c"),
                ("hops", "--max-hops"),
                ("interval", "-i"),
                ("timeout", "-W"),
            ] {
                flag(&mut args, fields, k, v);
            }
            if get(fields, "graph") == "ascii" {
                args.push("--ascii".into());
            }
            if get(fields, "host").is_empty() {
                return Err("Trace needs a host".into());
            }
            args.push(get(fields, "host").into());
        }
        Mode::Neighbors | Mode::AutoArp => {
            args.push("neighborhood".into());
            flag(&mut args, fields, "interface", "--interface");
            flag(&mut args, fields, "results", "--max-results");
            if mode == Mode::AutoArp {
                if get(fields, "interface").is_empty() {
                    return Err("Select interface before ARP scan".into());
                }
                args.push("--scan-local".into());
            } else {
                flag(&mut args, fields, "family", "--family");
                flag(&mut args, fields, "watch", "--watch");
            }
            if get(fields, "graph") == "ascii" {
                args.push("--ascii".into());
            }
        }
        Mode::Discover | Mode::Arp => {
            args.push("discover".into());
            flag(&mut args, fields, "interface", "--interface");
            if mode == Mode::Discover {
                flag(&mut args, fields, "family", "--family");
                flag(&mut args, fields, "results", "--max-results");
            } else {
                if get(fields, "interface").is_empty() {
                    return Err("Active ARP requires interface".into());
                }
                let target = get(fields, "host");
                if !target.contains('/') {
                    return Err("Enter explicit on-link CIDR".into());
                }
                args.push("--active".into());
                args.push(target.into());
                for (k, v) in [
                    ("pps", "--pps"),
                    ("max-hosts", "--max-hosts"),
                    ("timeout", "--timeout"),
                ] {
                    flag(&mut args, fields, k, v);
                }
            }
        }
    }
    Ok(args)
}
/// A bounded MTR table keyed by hop number, not a scrolling sequence of full
/// snapshots. Each TTL update overwrites its previous row, just like mtr.
#[derive(Default)]
struct TraceLive {
    cycle: u32,
    hops: Vec<String>,
}

impl TraceLive {
    fn update(&mut self, line: &str) -> bool {
        let Some(payload) = line.strip_prefix("@pipi-hop\t") else {
            return false;
        };
        let mut parts = payload.splitn(3, '\t');
        let (Some(ttl), Some(cycle), Some(row)) = (parts.next(), parts.next(), parts.next()) else {
            return false;
        };
        let (Ok(ttl), Ok(cycle)) = (ttl.parse::<usize>(), cycle.parse::<u32>()) else {
            return false;
        };
        if !(1..=32).contains(&ttl) || cycle == 0 || row.len() > 512 {
            return false;
        }
        self.cycle = cycle;
        if self.hops.len() < ttl {
            self.hops.resize(ttl, " ·  probing ...".to_owned());
        }
        self.hops[ttl - 1] = row.to_owned();
        true
    }
}

struct Running {
    child: Child,
    rx: Receiver<String>,
    lines: VecDeque<String>,
    total: u64,
    started: Instant,
    finished: Option<i32>,
    cmd: Vec<String>,
    trace: Option<TraceLive>,
}
fn pipe<R: io::Read + Send + 'static>(reader: R, tx: SyncSender<String>) {
    std::thread::spawn(move || {
        let mut reader = io::BufReader::new(reader);
        loop {
            let mut buf = Vec::with_capacity(256);
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => return,
                Ok(_) => {
                    // Keep TAB only for the internal, fixed-field per-hop
                    // protocol. All other control bytes (especially ESC)
                    // are removed before they reach the terminal renderer.
                    let line = String::from_utf8_lossy(&buf)
                        .chars()
                        .filter(|c| *c == '\t' || (!c.is_control() && *c != '\u{1b}'))
                        .take(512)
                        .collect::<String>();
                    if tx.send(line).is_err() {
                        return;
                    }
                }
            }
        }
    });
}
impl Running {
    fn spawn(args: Vec<String>) -> Result<Self, String> {
        let trace = if args.first().is_some_and(|arg| arg == "trace")
            && args.iter().any(|arg| arg == "--live-rows")
        {
            Some(TraceLive::default())
        } else {
            None
        };
        let exe = std::env::current_exe().map_err(|e| format!("binary path: {e}"))?;
        let mut child = Command::new(exe)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("run measurement: {e}"))?;
        let (tx, rx) = mpsc::sync_channel(128);
        if let Some(o) = child.stdout.take() {
            pipe(o, tx.clone());
        }
        if let Some(e) = child.stderr.take() {
            pipe(e, tx);
        }
        Ok(Self {
            child,
            rx,
            lines: VecDeque::with_capacity(LOG_MAX),
            total: 0,
            started: Instant::now(),
            finished: None,
            cmd: args,
            trace,
        })
    }
    fn update(&mut self) {
        for _ in 0..80 {
            match self.rx.try_recv() {
                Ok(line) => {
                    self.total += 1;
                    if self.trace.as_mut().is_some_and(|trace| trace.update(&line)) {
                        continue; // In-place MTR update: do not append to the log.
                    }
                    if self.lines.len() == LOG_MAX {
                        self.lines.pop_front();
                    }
                    self.lines.push_back(line);
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        if self.finished.is_none() {
            if let Ok(Some(code)) = self.child.try_wait() {
                self.finished = Some(code.code().unwrap_or(1));
            }
        }
    }
    fn stop(&mut self) {
        if self.finished.is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
            self.finished = Some(130);
        }
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        self.stop();
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Page {
    Home,
    Form,
    Confirm,
    Live,
}
struct Dashboard {
    page: Page,
    choice: usize,
    mode: Mode,
    fields: Vec<Field>,
    focus: usize,
    pending: Vec<String>,
    running: Option<Running>,
    notice: String,
}
impl Default for Dashboard {
    fn default() -> Self {
        Self {
            page: Page::Home,
            choice: 0,
            mode: Mode::Ping,
            fields: form(Mode::Ping),
            focus: 0,
            pending: Vec::new(),
            running: None,
            notice: "No packets sent before START".into(),
        }
    }
}
impl Dashboard {
    fn select(&mut self, m: Mode) {
        self.mode = m;
        self.fields = form(m);
        self.focus = 0;
        self.page = Page::Form;
        self.notice = format!("Configure {} / F5 or Ctrl-R to start", m.name());
    }
    fn prepare(&mut self) {
        match arguments(self.mode, &self.fields) {
            Ok(args) => {
                self.pending = args;
                if self.mode.active() {
                    self.page = Page::Confirm;
                    self.notice = "Active ARP sends packets only after confirmation".into();
                } else {
                    self.launch();
                }
            }
            Err(e) => self.notice = format!("INPUT ERROR: {e}"),
        }
    }
    fn launch(&mut self) {
        match Running::spawn(std::mem::take(&mut self.pending)) {
            Ok(run) => {
                self.running = Some(run);
                self.page = Page::Live;
            }
            Err(e) => {
                self.page = Page::Form;
                self.notice = e;
            }
        }
    }
    fn action(&mut self, k: Key) -> bool {
        if k == Key::CtrlC {
            if self.page == Page::Live {
                self.running = None;
                self.page = Page::Form;
                return false;
            }
            return true;
        }
        match self.page {
            Page::Home => match k {
                Key::Up | Key::BackTab => {
                    self.choice = (self.choice + MODES.len() - 1) % MODES.len()
                }
                Key::Down | Key::Tab => self.choice = (self.choice + 1) % MODES.len(),
                Key::Enter | Key::Right => self.select(MODES[self.choice]),
                Key::Esc | Key::Char('q') => return true,
                Key::Char(x @ '1'..='9') => {
                    let i = (x as u8 - b'1') as usize;
                    self.choice = i;
                    self.select(MODES[i]);
                }
                _ => {}
            },
            Page::Form => match k {
                Key::Esc => {
                    self.page = Page::Home;
                    self.notice = "Select module".into();
                }
                Key::Down | Key::Tab => self.focus = (self.focus + 1) % (self.fields.len() + 1),
                Key::Up | Key::BackTab => {
                    self.focus = (self.focus + self.fields.len()) % (self.fields.len() + 1)
                }
                Key::Start => self.prepare(),
                Key::Enter | Key::Right if self.focus == self.fields.len() => self.prepare(),
                Key::Enter | Key::Right
                    if matches!(self.fields[self.focus].kind, Kind::Choice(_)) =>
                {
                    self.fields[self.focus].cycle(false)
                }
                Key::Left if self.focus < self.fields.len() => self.fields[self.focus].cycle(true),
                Key::Backspace
                    if self.focus < self.fields.len()
                        && matches!(self.fields[self.focus].kind, Kind::Text) =>
                {
                    self.fields[self.focus].value.pop();
                }
                Key::Char(c)
                    if self.focus < self.fields.len()
                        && matches!(self.fields[self.focus].kind, Kind::Text)
                        && self.fields[self.focus].value.len() < 200 =>
                {
                    self.fields[self.focus].value.push(c);
                }
                _ => {}
            },
            Page::Confirm => match k {
                Key::Enter | Key::Char('y') | Key::Char('Y') => self.launch(),
                Key::Esc | Key::Char('n') | Key::Char('N') => {
                    self.pending.clear();
                    self.page = Page::Form;
                    self.notice = "Active scan cancelled; no packets sent".into();
                }
                _ => {}
            },
            Page::Live => match k {
                Key::Esc | Key::Char('q') => {
                    self.running = None;
                    self.page = Page::Form;
                    self.notice = "Stopped; edit fields or return to menu".into();
                }
                Key::Enter if self.running.as_ref().is_some_and(|r| r.finished.is_some()) => {
                    self.running = None;
                    self.page = Page::Form;
                }
                _ => {}
            },
        }
        false
    }
    fn draw(&self, w: usize, h: usize) -> (String, Vec<String>, String) {
        let mut rows = Vec::new();
        let (title, footer) = match self.page {
            Page::Home => (
                "pipi / NETWORK CONTROL CENTER".into(),
                "UP/DOWN select | 1-9 shortcut | Enter configure | q quit".into(),
            ),
            Page::Form => (
                format!("pipi / {} / FORM", self.mode.name()),
                "Tab/UP/DOWN field | type or arrows | F5/Ctrl-R START | Esc menu".into(),
            ),
            Page::Confirm => (
                "pipi / ACTIVE ARP / CONFIRM".into(),
                "Enter/Y = send ARP | Esc/N = cancel (default no scan)".into(),
            ),
            Page::Live => (
                if let Some(trace) = self.running.as_ref().and_then(|run| run.trace.as_ref()) {
                    format!("pipi / TRACE / CYCLE {} / LIVE", trace.cycle.max(1))
                } else {
                    format!("pipi / {} / LIVE", self.mode.name())
                },
                "q/Esc stop + return | Enter return when finished".into(),
            ),
        };
        match self.page {
            Page::Home => {
                rows.push("MODULE             DESCRIPTION".into());
                let visible = h.saturating_sub(9).clamp(1, MODES.len());
                let start = self
                    .choice
                    .saturating_sub(visible / 2)
                    .min(MODES.len().saturating_sub(visible));
                for (i, m) in MODES.iter().enumerate().skip(start).take(visible) {
                    rows.push(format!(
                        "{} [{}] {:<15} {}",
                        if i == self.choice { ">" } else { " " },
                        i + 1,
                        m.name(),
                        m.note()
                    ));
                }
                rows.push(format!("SELECTED  {}", MODES[self.choice].note()));
                rows.push("SAFETY    Passive until explicitly started".into());
            }
            Page::Form => {
                rows.push(format!(
                    "MODULE {}  /  {}",
                    self.mode.name(),
                    self.mode.note()
                ));
                rows.push("FIELD                VALUE / CONSTRAINT".into());
                let visible = h.saturating_sub(11).max(1);
                let start = self
                    .focus
                    .saturating_sub(visible / 2)
                    .min(self.fields.len().saturating_sub(visible));
                for (i, f) in self.fields.iter().enumerate().skip(start).take(visible) {
                    let mark = if i == self.focus { ">" } else { " " };
                    let val = if f.value.is_empty() {
                        "(blank)"
                    } else {
                        &f.value
                    };
                    if w >= 95 {
                        rows.push(format!(
                            "{} {:<16} [{:<19}]  │ {}",
                            mark,
                            f.label,
                            clip(val, 19),
                            f.note
                        ));
                    } else {
                        rows.push(format!(
                            "{} {:<16} [{}]",
                            mark,
                            f.label,
                            clip(val, w.saturating_sub(27))
                        ));
                    }
                }
                rows.push(format!(
                    "{} [ START MEASUREMENT ]",
                    if self.focus == self.fields.len() {
                        ">"
                    } else {
                        " "
                    }
                ));
                rows.push(if self.mode.active() {
                    "WARNING: packet scan requires confirmation".into()
                } else {
                    format!(
                        "HINT: {}",
                        self.fields.get(self.focus).map_or("START", |f| f.note)
                    )
                });
                rows.push(self.notice.clone());
            }
            Page::Confirm => {
                rows.push("WARNING: EXPLICIT ACTIVE NETWORK PROBES".into());
                rows.push("The selected interface will transmit ARP frames.".into());
                rows.push("Only use on your own or explicitly authorized network.".into());
                rows.push(format!("COMMAND: pipi {}", self.pending.join(" ")));
                rows.push("> [ SEND PACKETS ]      [ Esc CANCEL ]".into());
            }
            Page::Live => {
                if let Some(run) = self.running.as_ref() {
                    let state = match run.finished {
                        Some(0) => "COMPLETE / PASS",
                        Some(_) => "COMPLETE / CHECK RESULTS",
                        None => "RUNNING",
                    };
                    if let Some(trace) = run.trace.as_ref() {
                        // Stable, MTR-style hop grid. Earlier hops remain in place
                        // even while later ones are being probed in the next cycle.
                        rows.push(if w < 61 {
                            "HOP ADDRESS     LOSS AVG  SMOKE".into()
                        } else {
                            "HOP ADDRESS                LOSS  SENT  LAST   AVG  SMOKE".into()
                        });
                        rows.extend(trace.hops.iter().cloned());
                        if run.finished.is_some() {
                            rows.push(format!("STATE {state} | {} probes", run.total));
                        }
                        for line in run.lines.iter().rev().take(2).rev() {
                            rows.push(format!("NOTE {line}"));
                        }
                    } else {
                        rows.push(format!(
                            "STATE {} | {}s | {} events",
                            state,
                            run.started.elapsed().as_secs(),
                            run.total
                        ));
                        rows.push(format!(
                            "COMMAND pipi {}",
                            clip(&run.cmd.join(" "), w.saturating_sub(18))
                        ));
                        rows.push("LIVE TELEMETRY / MOST RECENT BELOW".into());
                        let visible = h.saturating_sub(10).max(1);
                        rows.extend(
                            run.lines
                                .iter()
                                .skip(run.lines.len().saturating_sub(visible))
                                .cloned(),
                        );
                        if run.finished.is_some() {
                            rows.push("FINISHED / Enter to configure again".into());
                        }
                    }
                }
            }
        }
        (title, rows, footer)
    }
}
fn clip(s: &str, n: usize) -> String {
    s.chars()
        .filter(|c| !c.is_control() && *c != '\u{1b}')
        .take(n)
        .collect()
}
pub fn run(flags: &[String]) -> Result<i32, String> {
    if flags.iter().any(|s| s != "--no-color") {
        return Err("menu accepts only --no-color; supply a HOST to use direct TUI".into());
    }
    let mut screen = Screen::enter()?;
    if flags.iter().any(|s| s == "--no-color") {
        screen.set_color(false);
    }
    let raw = RawInput::enable()?;
    crate::engine::install_signals();
    let mut app = Dashboard::default();
    loop {
        if crate::engine::interrupted() {
            break;
        }
        if let Some(run) = app.running.as_mut() {
            run.update();
        }
        let (w, h) = tui::dimensions();
        let (title, rows, footer) = app.draw(w, h);
        screen.show_lines(&title, &rows, &footer);
        if let Some(k) = key(if app.page == Page::Live { 120 } else { 250 })? {
            if app.action(k) {
                break;
            }
        }
    }
    drop(app);
    drop(raw);
    drop(screen);
    Ok(0)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn set(fields: &mut [Field], key: &str, value: &str) {
        fields.iter_mut().find(|f| f.key == key).unwrap().value = value.into();
    }
    #[test]
    fn trace_protocol_delimiters_survive_safe_pipe_filter() {
        let raw = "@pipi-hop\t1\t3\t 1  127.0.0.1\r\n";
        let filtered: String = raw.chars()
            .filter(|c| *c == '\t' || (!c.is_control() && *c != '\u{1b}'))
            .collect();
        let mut trace = TraceLive::default();
        assert!(trace.update(&filtered));
        assert_eq!(trace.cycle, 3);
        assert_eq!(trace.hops.len(), 1);
        assert!(!filtered.contains('\r'));
        assert!(!filtered.contains('\n'));
    }

    #[test]
    fn trace_menu_replaces_hop_instead_of_adding_snapshots() {
        let mut trace = TraceLive::default();
        assert!(trace.update("@pipi-hop\t1\t1\t 1  10.0.0.1   0%"));
        assert_eq!(trace.hops.len(), 1);
        assert!(trace.update("@pipi-hop\t2\t1\t 2  10.0.0.2  20%"));
        assert_eq!(trace.hops.len(), 2);
        assert!(trace.update("@pipi-hop\t1\t2\t 1  10.0.0.1  50%"));
        assert_eq!(trace.hops.len(), 2);
        assert!(trace.hops[0].contains("50%"));
        assert!(trace.hops[1].contains("20%"));
        assert_eq!(trace.cycle, 2);
        assert!(!trace.update("@pipi-hop\t33\t2\t invalid"));
        assert!(!trace.update("unrelated text"));
    }

    #[test]
    fn all_modes_are_in_menu_and_passive_on_open() {
        let app = Dashboard::default();
        assert_eq!(MODES.len(), 9);
        assert_eq!(app.page, Page::Home);
        assert!(app.running.is_none());
        assert!(app.pending.is_empty());
        for m in MODES {
            let mut a = Dashboard::default();
            a.select(m);
            let (title, rows, _) = a.draw(80, 24);
            assert!(title.contains(m.name()));
            assert!(rows.join("\n").contains("START"));
        }
    }
    #[test]
    fn all_commands_are_connected_and_active_requires_interface() {
        for (m, token) in [
            (Mode::Ping, "127.0.0.1"),
            (Mode::Fast, "fast"),
            (Mode::Range, "range"),
            (Mode::Smoke, "monitor"),
            (Mode::Trace, "trace"),
            (Mode::Neighbors, "neighborhood"),
            (Mode::Discover, "discover"),
        ] {
            assert!(arguments(m, &form(m)).unwrap().join(" ").contains(token));
        }
        let mut a = form(Mode::Arp);
        assert!(arguments(Mode::Arp, &a).is_err());
        set(&mut a, "interface", "eth0");
        assert!(
            arguments(Mode::Arp, &a)
                .unwrap()
                .contains(&"--active".into())
        );
        let mut b = form(Mode::AutoArp);
        assert!(arguments(Mode::AutoArp, &b).is_err());
        set(&mut b, "interface", "eth0");
        assert!(
            arguments(Mode::AutoArp, &b)
                .unwrap()
                .contains(&"--scan-local".into())
        );
    }
    #[test]
    fn second_confirmation_cancels_without_launch() {
        let mut app = Dashboard::default();
        app.select(Mode::Arp);
        set(&mut app.fields, "interface", "eth0");
        app.prepare();
        assert_eq!(app.page, Page::Confirm);
        assert!(app.running.is_none());
        app.action(Key::Esc);
        assert_eq!(app.page, Page::Form);
        assert!(app.pending.is_empty());
    }
    #[test]
    fn responsive_form_and_shortcuts() {
        let mut app = Dashboard::default();
        app.action(Key::Down);
        app.action(Key::Enter);
        assert_eq!(app.mode, Mode::Fast);
        app.action(Key::Tab);
        assert_eq!(app.focus, 1);
        for (w, h) in [(40, 12), (80, 24), (120, 40)] {
            for page in [Page::Home, Page::Form] {
                app.page = page;
                let (title, rows, foot) = app.draw(w, h);
                let frame = tui::panel_frame(&title, &rows, &foot, w, h);
                assert!(frame.lines().count() <= h);
                assert!(frame.lines().all(|l| l.chars().count() < w));
            }
        }
    }
    #[test]
    fn invalid_bounded_rate_does_not_start() {
        let mut a = form(Mode::Range);
        set(&mut a, "pps", "9999");
        assert!(arguments(Mode::Range, &a).is_err());
        set(&mut a, "pps", "20");
        assert!(arguments(Mode::Range, &a).is_ok());
    }
}
