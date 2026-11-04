//! pipi: Linux-first ICMP echo diagnostics with deliberately bounded advanced modes.

#[cfg(not(target_os = "linux"))]
compile_error!("pipi currently supports Linux only (SOCK_DGRAM ICMP ping sockets)");

mod arp;
mod cli;
mod colour;
mod discovery;
mod engine;
mod graph;
mod linux;
mod neighbor;
mod neighborhood;
mod range;
mod stats;
mod trace;
mod trace_socket;
#[cfg(feature = "tui")]
mod tui;
#[cfg(feature = "tui")]
mod console;

use cli::Parsed;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let menu = args.is_empty() || args.first().is_some_and(|x| x == "tui")
        && args.iter().skip(1).all(|x| x == "--no-color");
    let code = if menu {
        result(open_menu(if args.is_empty() { &[] } else { &args[1..] }))
    } else { match args.first().map(String::as_str) {
        Some("discover") => result(discovery::run(&args[1..])),
        Some("neighborhood") => result(neighborhood::run(&args[1..])),
        Some("trace") => result(trace::run(&args[1..])),
        _ => match cli::parse(&args) {
            Ok(Parsed::Help) => {
                print!("{}", cli::help());
                0
            }
            Ok(Parsed::Version) => {
                println!("pipi {}", env!("CARGO_PKG_VERSION"));
                0
            }
            Ok(Parsed::Run(config)) => result(engine::run(config)),
            Err(e) => {
                eprintln!("pipi: {e}\nTry 'pipi --help'.");
                2
            }
        },
    } };
    std::process::exit(code);
}

#[cfg(feature = "tui")]
fn open_menu(args: &[String]) -> Result<i32, String> {
    console::run(args)
}

#[cfg(not(feature = "tui"))]
fn open_menu(_args: &[String]) -> Result<i32, String> {
    Err("interactive menu was not built; cargo install --path . --features tui".into())
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
