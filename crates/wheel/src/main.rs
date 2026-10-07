//! Entry point. The work lives in the library beside this file.

// a release build is a tray app with no console; a debug build keeps one for quick runs
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::ExitCode;

use kile_wheel_core::names::DISPLAY_NAME;
use tracing::error;

fn main() -> ExitCode {
    let Err(error) = kile_wheel_9000::run() else {
        return ExitCode::SUCCESS;
    };

    // the log file may not exist yet, so the reason goes to stderr too
    error!("{error:#}");
    eprintln!("{DISPLAY_NAME} could not start: {error:#}");
    ExitCode::FAILURE
}
