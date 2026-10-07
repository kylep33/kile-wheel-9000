//! kile-wheel-9000: a hotkey-triggered radial app launcher for Windows.
//!
//! The binary beside this library is a thin entry point. Everything else lives here so
//! it can be unit tested without the elevation the binary's manifest asks for.

pub mod app;
pub mod logging;
pub mod overlay;
pub mod render;
pub mod slots;
pub mod state;
pub mod theme;

use anyhow::{Context, Result};
use kile_wheel_core::names::{APP_NAME, ConfigPaths, DISPLAY_NAME};
use kile_wheel_platform::instance::{self, Claim};
use kile_wheel_platform::known_folders;
use tracing::info;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};

use crate::app::App;

/// Starts up and runs until the message loop ends.
pub fn run() -> Result<()> {
    let app_data = known_folders::roaming_app_data()
        .context("could not find %APPDATA%; the shell refused the folder lookup")?;
    let paths = ConfigPaths::new(&app_data);

    let _logging = logging::start(&paths.logs_dir())?;
    info!(
        version = env!("CARGO_PKG_VERSION"),
        "{DISPLAY_NAME} starting"
    );

    let Claim::Acquired(_instance) =
        instance::claim(APP_NAME).context("could not claim the single-instance mutex")?
    else {
        info!("another instance is already running, handing the open over to it");
        overlay::signal_existing();
        return Ok(());
    };

    // SAFETY: the shell calls that read app ids and launch programs need COM on this
    // thread, and this is the only place it is initialised.
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
        .ok()
        .context("could not initialise COM on the main thread")?;

    App::start()?.run()
}
