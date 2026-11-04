//! Minimal, dependency-free, viewport-sized terminal dashboard.
//! Alternate screen is opt-in; tty settings stay cooked and are not modified.

use std::ffi::{c_int, c_ulong};
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
    fn ioctl(fd: c_int, request: c_ulong, arg: *mut WinSize) -> c_int;
}

pub struct Screen;

pub fn dimensions() -> (usize, usize) {
    let mut ws = WinSize::default();
    // SAFETY: Linux TIOCGWINSZ reads a 4 x u16 initialized writable struct.
    if unsafe { ioctl(1, TIOCGWINSZ, &mut ws) } == 0 && ws.col > 0 && ws.row > 0 {
        (usize::from(ws.col), usize::from(ws.row))
    } else {
        (80, 24)
    }
}

/// All headers/network-derived strings are scrubbed; never allow escape injection.
/// Keep one column free to prevent cursor-wrap and cap output to the visible height.
fn clip(line: &str, width: usize) -> String {
    let mut output = String::new();
    for c in line.chars().filter(|c| !c.is_control() && *c != '\u{1b}') {
        if output.chars().count() >= width.saturating_sub(1) {
            break;
        }
        output.push(c);
    }
    output
}

pub fn frame(title: &str, rows: &[String], footer: &str, width: usize, height: usize) -> String {
    if width < 2 || height < 2 {
        return String::new();
    }
    let mut output = String::new();
    output.push_str(&clip(title, width));
    output.push('\n');
    let available = height.saturating_sub(2);
    // When a table exceeds the viewport prefer its first rows, not a scrolling flood.
    for row in rows.iter().take(available) {
        output.push_str(&clip(row, width));
        output.push('\n');
    }
    output.push_str(&clip(footer, width));
    output
}

impl Screen {
    pub fn enter() -> Result<Self, String> {
        if !io::stdout().is_terminal() {
            return Err("tui needs an interactive terminal; use monitor/trace/neighborhood without --tui for pipes".into());
        }
        print!("\x1b[?1049h\x1b[?25l");
        io::stdout().flush().map_err(|e| e.to_string())?;
        Ok(Self)
    }

    pub fn show_lines(&self, title: &str, rows: &[String], footer: &str) {
        let (width, height) = dimensions();
        let data = frame(title, rows, footer, width, height);
        print!("\x1b[H\x1b[2J{data}");
        let _ = io::stdout().flush();
    }

    /// Keep the existing ping TUI call site, but bound it to the terminal viewport.
    pub fn show(&self, label: &str, graph: &str, stats: &crate::stats::Stats) {
        let sent = stats.sent;
        let received = stats.received;
        let loss = if sent == 0 { 0.0 } else {
            100.0 * sent.saturating_sub(received) as f64 / sent as f64
        };
        let (width, height) = size();
        let graph_lines: Vec<_> = graph.lines().map(str::to_owned).collect();
        let visible = height.saturating_sub(5);
        let mut rows = vec![format!(
            "Sent {:>6}  Rx {:>6}  Loss {:>5.1}%  |  last {} graph buckets",
            sent, received, loss, graph_lines.len()
        )];
        rows.push(if received == 0 {
            "RTT waiting for first valid reply".to_owned()
        } else {
            format!(
                "RTT min {:.2}  avg {:.2}  max {:.2}  p95 {:.2}  jitter {:.2} ms",
                stats.min, stats.mean, stats.max, stats.percentile(0.95), stats.jitter()
            )
        });
        rows.push("RTT distribution (older -> newer):".to_owned());
        rows.extend(graph_lines.iter().skip(graph_lines.len().saturating_sub(visible)).cloned());
        let data = frame(
            &format!("pipi | PING {}", label),
            &rows,
            "Ctrl-C exit  |  monotonic RTT  |  no remote telemetry",
            width,
            height,
        );
        print!("\x1b[H\x1b[2J{data}");
        let _ = io::stdout().flush();
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        print!("\x1b[?25h\x1b[?1049l");
        let _ = io::stdout().flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_40_by_12_and_80_by_24_never_overflow() {
        let lines: Vec<_> = (0..80).map(|i| format!("line {i:03} - {}", "x".repeat(300))).collect();
        for (width, height) in [(40, 12), (80, 24), (120, 40), (20, 6)] {
            let f = frame("network \x1b[31m title", &lines, "Ctrl-C", width, height);
            assert!(f.lines().count() <= height);
            assert!(f.lines().all(|line| line.chars().count() < width));
            assert!(!f.contains('\x1b'));
        }
    }

    #[test]
    fn empty_tiny_viewports_do_not_panic() {
        assert_eq!(frame("foo", &[], "bar", 1, 10), "");
        assert_eq!(frame("foo", &[], "bar", 20, 1), "");
    }
}
