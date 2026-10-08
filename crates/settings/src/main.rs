//! Entry point for the settings window.

// a release build is a plain GUI app with no console; a debug build keeps one
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::ExitCode;

use kile_wheel_core::names::DISPLAY_NAME;

fn main() -> ExitCode {
    let Err(error) = kile_wheel_9000_settings::run() else {
        return ExitCode::SUCCESS;
    };

    // there is no log file on this side, so the reason has to go somewhere visible
    eprintln!("{DISPLAY_NAME} settings could not start: {error:#}");
    rfd::MessageDialog::new()
        .set_title(format!("{DISPLAY_NAME} settings"))
        .set_description(format!("{error:#}"))
        .set_level(rfd::MessageLevel::Error)
        .show();
    ExitCode::FAILURE
}
