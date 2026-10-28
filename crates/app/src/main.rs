//! blitznir: the app, on Freya. The tray daemon and its panel without
//! arguments, the shared CLI (`--check`, `transcribe`, `dictate`) with them.
#![deny(unsafe_code)]

use blitznir_app::daemon;
use std::process::ExitCode;

fn main() -> ExitCode {
    blitznir::platform::before_gtk();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let done = if args.is_empty() {
        daemon::run()
    } else {
        blitznir::logging::init(None);
        blitznir::cli::run(&args)
    };
    match done {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            log::error!("{e}");
            ExitCode::FAILURE
        }
    }
}
