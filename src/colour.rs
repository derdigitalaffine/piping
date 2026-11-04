//! Conditional ANSI text output; machine formats/pipes never receive escapes.
//! TUI renders its own palette, while classic commands use these status colors.

use std::io::{self, IsTerminal};
use std::sync::atomic::{AtomicBool, Ordering};

static ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn configured(allow: bool) {
    let enabled = allow
        && io::stdout().is_terminal()
        && std::env::var_os("NO_COLOR").is_none()
        && std::env::var("TERM").ok().as_deref() != Some("dumb");
    ACTIVE.store(enabled, Ordering::Relaxed);
}

pub fn style(s: &str) -> String {
    if !ACTIVE.load(Ordering::Relaxed) {
        return s.to_owned();
    }
    // Do not reinterpret network text as control sequences.
    let lower = s.to_ascii_lowercase();
    let code = if lower.contains("100.0%")
        || lower.contains("loss high")
        || lower.contains("failed")
        || lower.contains("timeout")
    {
        "1;31"
    } else if lower.contains("0.0% packet loss")
        || lower.contains("reachable")
        || lower.contains("reply")
        || lower.contains(" bytes from")
    {
        "1;32"
    } else if lower.contains("loss") || lower.contains("warning") || lower.contains("stale") {
        "1;33"
    } else if lower.starts_with("pipi") || lower.starts_with("piping") || lower.starts_with("rtt") {
        "1;36"
    } else {
        "37"
    };
    format!("\x1b[{code}m{s}\x1b[0m")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn color_off_is_literal_and_no_ansi() {
        configured(false);
        assert_eq!(style("raw,jsonl"), "raw,jsonl");
    }
}
