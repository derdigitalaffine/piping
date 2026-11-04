//! Compact color-aware terminal panels, dashboards and responsive layouts.
//! Uses only std + Linux ioctl; all raw input handling belongs in console.rs.

use std::cell::RefCell;
use std::ffi::{c_int, c_ulong, c_void};
use std::fmt::Write as _;
use std::io::{self, IsTerminal, Write};

const TIOCGWINSZ: c_ulong = 0x5413;

#[repr(C)]
#[derive(Default)]
struct WinSize {
    row: u16,
    col: u16,
    xpixel: u16,
    ypixel: u16,
}

unsafe extern "C" {
    fn ioctl(fd: c_int, request: c_ulong, arg: *mut c_void) -> c_int;
}

/// Stores only the currently visible rows (at most terminal height) so changed
/// lines can be overwritten in place. A resize invalidates the whole cache.
#[derive(Default)]
struct FrameCache {
    size: Option<(usize, usize)>,
    lines: Vec<String>,
}

pub struct Screen {
    color: bool,
    cache: RefCell<FrameCache>,
}

/// Generate a minimal cursor-addressed update, never appending a new frame.
/// A clear-screen is issued only for the first frame or a terminal resize.
/// The user-facing text has already been clipped/sanitized by panel_frame().
fn repaint(cache: &mut FrameCache, text: &str, size: (usize, usize), color: bool) -> String {
    let lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
    let reset = cache.size != Some(size);
    let mut output = String::new();
    if reset {
        output.push_str("\x1b[H\x1b[2J");
    }
    for index in 0..lines.len().max(cache.lines.len()) {
        let next = lines.get(index);
        if !reset && next == cache.lines.get(index) {
            continue;
        }
        // Cursor stays in the alternate screen; erase just this single row,
        // then paint its new content. No terminal scrollback or frame flicker.
        let _ = write!(output, "\x1b[{};1H\x1b[2K", index + 1);
        if let Some(line) = next {
            output.push_str(&tint(line, color));
        }
    }
    cache.lines = lines;
    cache.size = Some(size);
    output
}

pub fn dimensions() -> (usize, usize) {
    let mut ws = WinSize::default();
    // SAFETY: Linux TIOCGWINSZ initializes a correctly aligned winsize buffer.
    if unsafe { ioctl(1, TIOCGWINSZ, (&mut ws as *mut WinSize).cast()) } == 0
        && ws.col > 0
        && ws.row > 0
    {
        (usize::from(ws.col), usize::from(ws.row))
    } else {
        (80, 24)
    }
}

/// ANSI color only when explicitly supported. NO_COLOR wins even when empty.
pub fn supports_color(tty: bool, term: Option<&str>, no_color: bool) -> bool {
    tty && !no_color && term != Some("dumb") && term != Some("")
}
fn auto_color() -> bool {
    supports_color(
        io::stdout().is_terminal(),
        std::env::var("TERM").ok().as_deref(),
        std::env::var_os("NO_COLOR").is_some(),
    )
}

fn clean(s: &str, max: usize) -> String {
    s.chars()
        .filter(|c| !c.is_control() && *c != '\u{1b}')
        .take(max)
        .collect()
}

/// A pure no-ANSI renderer used by both output paths and viewport tests.
pub fn frame(title: &str, rows: &[String], footer: &str, width: usize, height: usize) -> String {
    if width < 2 || height < 2 {
        return String::new();
    }
    let mut output = String::new();
    output.push_str(&clean(title, width - 1));
    output.push('\n');
    let available = height.saturating_sub(2);
    if rows.len() > available && available >= 3 {
        output.push_str(&clean(&rows[0], width - 1));
        output.push('\n');
        let tail = available - 2;
        output.push_str(&clean(
            &format!("... {} earlier rows hidden", rows.len() - tail - 1),
            width - 1,
        ));
        output.push('\n');
        for row in rows.iter().skip(rows.len() - tail) {
            output.push_str(&clean(row, width - 1));
            output.push('\n');
        }
    } else if rows.len() > available && available > 0 {
        for row in rows.iter().skip(rows.len() - available) {
            output.push_str(&clean(row, width - 1));
            output.push('\n');
        }
    } else {
        for row in rows.iter().take(available) {
            output.push_str(&clean(row, width - 1));
            output.push('\n');
        }
    }
    output.push_str(&clean(footer, width - 1));
    output
}

fn border(width: usize, top: bool) -> String {
    let n = width.saturating_sub(3);
    let (left, right) = if top { ('╭', '╮') } else { ('╰', '╯') };
    format!("{left}{}{right}", "─".repeat(n))
}

fn pad_row(s: &str, width: usize) -> String {
    if width < 7 {
        return clean(s, width.saturating_sub(1));
    }
    let text = clean(s, width - 7);
    let used = text.chars().count();
    format!("│ {text}{} │", " ".repeat(width.saturating_sub(6 + used)))
}

/// Framed layout with guaranteed viewport bounds and deterministic text.
/// The first content row is retained as a column header when scrolling.
pub fn panel_frame(
    title: &str,
    rows: &[String],
    footer: &str,
    width: usize,
    height: usize,
) -> String {
    if width < 10 || height < 6 {
        return frame(title, rows, footer, width, height);
    }
    let inner_width = width.saturating_sub(1);
    let mut lines = Vec::with_capacity(height);
    lines.push(
        format!(
            "╭─ {} {}",
            clean(title, inner_width.saturating_sub(8)),
            "─".repeat(inner_width.saturating_sub(5 + title.chars().count()))
        )
        .chars()
        .take(inner_width)
        .collect::<String>(),
    );
    let slots = height.saturating_sub(5);
    if rows.len() > slots && slots > 2 {
        lines.push(pad_row(&rows[0], inner_width));
        lines.push(pad_row(
            &format!("⋮ {} older rows", rows.len() - slots + 2),
            inner_width,
        ));
        for row in rows.iter().skip(rows.len() - (slots - 2)) {
            lines.push(pad_row(row, inner_width));
        }
    } else {
        for row in rows.iter().take(slots) {
            lines.push(pad_row(row, inner_width));
        }
    }
    while lines.len() < height - 3 {
        lines.push(pad_row("", inner_width));
    }
    lines.push(border(inner_width, false));
    lines.push(clean(footer, inner_width));
    lines.join("\n")
}

fn tint(line: &str, allow: bool) -> String {
    if !allow {
        return line.to_owned();
    }
    let trimmed = line.trim();
    let ansi = if line.starts_with('╭') || line.starts_with('╰') || line.starts_with('┌') {
        "1;36" // cyan panel edges
    } else if trimmed.starts_with("│ ▶")
        || trimmed.starts_with("│ >")
        || trimmed.starts_with("▶")
        || trimmed.starts_with('>')
    {
        "1;30;46" // focused field: high contrast dark ink on cyan
    } else if trimmed.contains("FAILED")
        || trimmed.contains("FAIL ")
        || trimmed.contains("timeout")
        || trimmed.contains("LOSS HIGH")
    {
        "1;31"
    } else if trimmed.contains("PASS") || trimmed.contains("UP ") || trimmed.contains("reachable") {
        "1;32"
    } else if trimmed.contains("WARNING") || trimmed.contains("ACTIVE ARP") {
        "1;33"
    } else if trimmed.contains("pipi") || trimmed.contains("RTT ") || trimmed.contains("SMOKE") {
        "1;35"
    } else if trimmed.starts_with('│') {
        "37"
    } else {
        "36"
    };
    format!("\x1b[{ansi}m{line}\x1b[0m")
}

impl Screen {
    pub fn enter() -> Result<Self, String> {
        if !io::stdout().is_terminal() {
            return Err(
                "tui needs an interactive terminal; use ordinary CLI commands for pipes".into(),
            );
        }
        if std::env::var("TERM").ok().as_deref() == Some("dumb") {
            return Err(
                "TERM=dumb lacks fullscreen support; use pipi --help or a capable terminal".into(),
            );
        }
        print!("\x1b[?1049h\x1b[?25l");
        io::stdout().flush().map_err(|e| e.to_string())?;
        Ok(Self {
            color: auto_color(),
            cache: RefCell::new(FrameCache::default()),
        })
    }

    pub fn set_color(&mut self, enabled: bool) {
        let next = enabled && auto_color();
        if self.color != next {
            self.color = next;
            self.cache.borrow_mut().size = None;
        }
    }

    fn render(&self, data: &str, size: (usize, usize)) {
        let patch = repaint(&mut self.cache.borrow_mut(), data, size, self.color);
        if patch.is_empty() {
            return; // Identical frame: zero terminal output, even on idle ticks.
        }
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(patch.as_bytes()).and_then(|_| stdout.flush());
    }

    pub fn show_lines(&self, title: &str, rows: &[String], footer: &str) {
        let (w, h) = dimensions();
        self.render(&panel_frame(title, rows, footer, w, h), (w, h));
    }

    pub fn show(&self, label: &str, graph: &str, stats: &crate::stats::Stats) {
        let sent = stats.sent;
        let received = stats.received;
        let loss = stats.loss_percent();
        let (width, _) = dimensions();
        let mut rows = vec![
            format!(
                "PACKETS   Sent {:>7}  Recv {:>7}  Loss {:>5.1}%",
                sent, received, loss
            ),
            if received == 0 {
                "RTT       Waiting for first valid reply".to_owned()
            } else {
                format!(
                    "RTT       MIN {:.2}  AVG {:.2}  MAX {:.2}  P95 {:.2} ms",
                    stats.min,
                    stats.mean,
                    stats.max,
                    stats.percentile(0.95)
                )
            },
            format!(
                "JITTER    {:.2} ms  |  REALTIME  |  PACKET LOSS {}",
                stats.jitter(),
                if loss > 10.0 { "LOSS HIGH" } else { "OK" }
            ),
            "SMOKE / RTT DISTRIBUTION  (oldest -> newest)".to_owned(),
        ];
        rows.extend(graph.lines().map(|line| {
            // On smaller terminals retain seconds and the visualization; strip extended
            // metadata because the panel already shows aggregate statistics.
            if width < 86 {
                clean(line, width.saturating_sub(7))
            } else {
                line.to_owned()
            }
        }));
        self.show_lines(
            &format!("pipi / PING  {}", crate::cli::visible_host(label)),
            &rows,
            " Ctrl-C  stop  |  color: auto  |  source: Linux ICMP",
        );
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        print!("\x1b[0m\x1b[?25h\x1b[?1049l");
        let _ = io::stdout().flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incremental_mtr_updates_only_changed_hop_rows() {
        let mut cache = FrameCache::default();
        let a = "pipi TRACE\nHOP  ADDRESS LOSS\n 1   10.0.0.1 0%\nCtrl-C";
        let first = repaint(&mut cache, a, (80, 24), false);
        assert_eq!(first.matches("\x1b[2J").count(), 1);
        assert!(first.contains(" 1   10.0.0.1 0%"));
        assert_eq!(repaint(&mut cache, a, (80, 24), false), "");
        let b = a.replace("10.0.0.1 0%", "10.0.0.1 50%");
        let update = repaint(&mut cache, &b, (80, 24), false);
        assert!(!update.contains("\x1b[2J"));
        assert!(!update.contains("\x1b[1;1H"));
        assert!(!update.contains("\x1b[2;1H"));
        assert!(update.contains("\x1b[3;1H\x1b[2K"));
        assert!(update.contains("50%"));
        assert_eq!(update.matches("\x1b[2K").count(), 1);
        // Removed lines are erased individually rather than leaving stale hops.
        let removed = repaint(&mut cache, "pipi TRACE\nHOP  ADDRESS LOSS\nCtrl-C", (80, 24), false);
        assert!(!removed.contains("\x1b[2J"));
        assert!(removed.contains("\x1b[4;1H\x1b[2K"));
        assert_eq!(repaint(&mut cache, "same", (100, 24), false).matches("\x1b[2J").count(), 1);
    }

    #[test]
    fn no_color_tty_and_term_capability() {
        assert!(supports_color(true, Some("xterm-256color"), false));
        assert!(!supports_color(true, Some("xterm"), true));
        assert!(!supports_color(false, Some("xterm"), false));
        assert!(!supports_color(true, Some("dumb"), false));
    }

    #[test]
    fn viewport_limits_and_escape_safety() {
        let rows: Vec<_> = (0..100)
            .map(|i| format!("row{i} {}", "X".repeat(200)))
            .collect();
        for (w, h) in [(40, 12), (80, 24), (120, 40), (20, 6)] {
            for data in [
                frame("x\u{1b}[31m", &rows, "Ctrl-C", w, h),
                panel_frame("pipi\u{1b}[31m", &rows, "Ctrl-C", w, h),
            ] {
                assert!(data.lines().count() <= h);
                assert!(
                    data.lines().all(|line| line.chars().count() < w),
                    "viewport {w}x{h} overflow: {data}"
                );
                assert!(!data.contains('\u{1b}'));
            }
        }
    }

    #[test]
    fn current_rows_visible_on_small_terminal() {
        let hops: Vec<_> = (1..=32).map(|i| format!("HOP {i}")).collect();
        assert!(frame("pipi TRACE", &hops, "Ctrl-C", 40, 12).contains("HOP 32"));
        assert!(panel_frame("pipi TRACE", &hops, "Ctrl-C", 40, 12).contains("HOP 32"));
    }

    #[test]
    fn tiny_terminal_degrades_without_panic() {
        assert_eq!(frame("foo", &[], "bar", 1, 10), "");
        assert_eq!(frame("foo", &[], "bar", 20, 1), "");
    }
}
