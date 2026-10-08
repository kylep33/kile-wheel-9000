//! Settings window for kile-wheel-9000.
//!
//! Edits are held in memory and written on Save, so a half-finished change never reaches
//! the config the wheel is reading.

mod draft;
mod pages;
mod ui;

use anyhow::{Context, Result};
use kile_wheel_core::names::{ConfigPaths, DISPLAY_NAME};
use kile_wheel_platform::known_folders;

use crate::ui::Settings;

const WINDOW_WIDTH_PX: f32 = 860.0;
const WINDOW_HEIGHT_PX: f32 = 640.0;
const MIN_WIDTH_PX: f32 = 640.0;
const MIN_HEIGHT_PX: f32 = 480.0;

pub fn run() -> Result<()> {
    let app_data = known_folders::roaming_app_data()
        .context("could not find %APPDATA%; the shell refused the folder lookup")?;
    let paths = ConfigPaths::new(&app_data);

    let settings = Settings::load(paths)?;
    let viewport = egui::ViewportBuilder::default()
        .with_title(format!("{DISPLAY_NAME} settings"))
        .with_inner_size([WINDOW_WIDTH_PX, WINDOW_HEIGHT_PX])
        .with_min_inner_size([MIN_WIDTH_PX, MIN_HEIGHT_PX]);

    eframe::run_native(
        DISPLAY_NAME,
        eframe::NativeOptions {
            viewport,
            ..eframe::NativeOptions::default()
        },
        Box::new(|_| Ok(Box::new(settings))),
    )
    .map_err(|error| anyhow::anyhow!("could not open the settings window: {error}"))
}
