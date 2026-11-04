//! Opt-in ANSI alternate-screen display. No external runtime or privileged I/O.
//! Standard input remains in cooked mode: Ctrl-C exits; no raw-mode restoration risk.

use std::io::{self, IsTerminal, Write};

pub struct Screen;

impl Screen {
    pub fn enter() -> Result<Self, String> {
        if !io::stdout().is_terminal() {
            return Err("tui needs an interactive terminal; use monitor for pipes".into());
        }
        print!("\x1b[?1049h\x1b[?25l");
        io::stdout().flush().map_err(|e| e.to_string())?;
        Ok(Self)
    }

    pub fn show(&self, label: &str, graph: &str, sent: u64, received: u64) {
        print!(
            "\x1b[H\x1b[2Jpiping — {}   (Ctrl-C to exit)\n{}\n\nsent {} / received {}\n",
            label, graph, sent, received
        );
        let _ = io::stdout().flush();
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        print!("\x1b[?25h\x1b[?1049l");
        let _ = io::stdout().flush();
    }
}
