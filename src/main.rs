//! piping: Linux-first ICMP echo diagnostics with deliberately bounded advanced modes.

#[cfg(not(target_os = "linux"))]
compile_error!("piping currently supports Linux only (SOCK_DGRAM ICMP ping sockets)");

mod arp;
mod cli;
mod discovery;
mod engine;
mod graph;
mod linux;
mod neighbor;
mod neighborhood;
mod trace;
mod trace_socket;
mod range;
mod stats;
#[cfg(feature = "tui")]
mod tui;

use cli::Parsed;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("discover") => result(discovery::run(&args[1..])),
        Some("neighborhood") => result(neighborhood::run(&args[1..])),
        Some("trace") => result(trace::run(&args[1..])),
        _ => {
            match cli::parse(&args) {
                Ok(Parsed::Help) => { print!("{}", cli::help()); 0 }
                Ok(Parsed::Version) => {
                    println!("pipi {}", env!("CARGO_PKG_VERSION"));
                    0
                }
                Ok(Parsed::Run(config)) => result(engine::run(config)),
                Err(e) => {
                    eprintln!("pipi: {e}\nTry 'pipi --help'.");
                    2
                }
            }
        }
    };
    std::process::exit(code);
}

fn result(outcome: Result<i32, String>) -> i32 {
    match outcome {
        Ok(code) => code,
        Err(e) => {
            eprintln!("pipi: {e}");
            2
        }
    }
}
