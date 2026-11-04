//! piping: Linux-first ICMP echo diagnostics with deliberately bounded advanced modes.

#[cfg(not(target_os = "linux"))]
compile_error!("piping currently supports Linux only (SOCK_DGRAM ICMP ping sockets)");

mod cli;
mod engine;
mod graph;
mod linux;
mod range;
mod stats;
#[cfg(feature = "tui")]
mod tui;

use cli::Parsed;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match cli::parse(&args) {
        Ok(Parsed::Help) => {
            print!("{}", cli::help());
            0
        }
        Ok(Parsed::Version) => {
            println!("piping {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Ok(Parsed::Run(config)) => match engine::run(config) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("piping: {e}");
                2
            }
        },
        Err(e) => {
            eprintln!("piping: {e}\nTry 'piping --help'.");
            2
        }
    };
    std::process::exit(code);
}
