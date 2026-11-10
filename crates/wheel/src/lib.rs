//! kile-wheel-9000: a hotkey-triggered radial app launcher for Windows, GNOME and macOS.
//!
//! The binary beside this library is a thin entry point. Everything else lives here so it
//! can be unit tested without the elevation or permissions the binary needs.

pub mod anim;
pub mod app;
pub mod form;
pub mod geom;
pub mod keyboard;
pub mod logging;
pub mod overlay;
pub mod render;
pub mod slots;
pub mod state;
pub mod theme;

use anyhow::{Context, Result};
use kile_wheel_core::config;
use kile_wheel_core::names::{APP_NAME, ConfigPaths, DISPLAY_NAME};
use kile_wheel_platform::instance::{self, Claim};
use kile_wheel_platform::paths;
use tracing::info;

use crate::app::App;

/// Starts up and runs until the loop ends.
pub fn run() -> Result<()> {
    let config_home = paths::config_home()?;
    let paths = ConfigPaths::new(&config_home);

    let _logging = logging::start(&paths.logs_dir())?;
    info!(
        version = env!("CARGO_PKG_VERSION"),
        "{DISPLAY_NAME} starting"
    );

    let Claim::Acquired(instance) =
        instance::claim(APP_NAME).context("could not claim the single-instance lock")?
    else {
        info!("another instance is already running, handing the open over to it");
        instance::signal_existing(APP_NAME);
        return Ok(());
    };

    kile_wheel_platform::init_main_thread()?;

    let config_file = paths.config_file();
    if config::write_default_if_missing(&config_file)
        .context("could not write the starting config")?
    {
        info!(path = %config_file.display(), "wrote the starting config");
    }

    let config = config::load(&config_file)
        .with_context(|| format!("could not read {}", config_file.display()))?;

    App::start(paths, config, instance)?.run()
}
