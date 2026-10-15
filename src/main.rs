//! One Herdr plugin, many features. Each feature lives in its own folder and
//! is reached as `dagadbm <feature> [args]`; code more than one feature needs
//! moves to `src/shared/`.

mod notify;

use std::env;
use std::process::ExitCode;

const USAGE: &str = "Usage:\n  dagadbm <feature> [args]\n\nFeatures:\n  notify    Clickable macOS notifications for agent panes\n\nRun `dagadbm <feature> --help` for a feature's options.";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("dagadbm: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("notify") => notify::run(args),
        Some("-h" | "--help") => {
            println!("dagadbm {}\n\n{USAGE}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("-V" | "--version") => {
            println!("dagadbm {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some(other) => Err(format!("unknown feature: {other}\n\n{USAGE}")),
        None => Err(USAGE.to_string()),
    }
}
