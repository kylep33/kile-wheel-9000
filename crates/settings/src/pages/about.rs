//! About: version, where things live, and the keys the wheel uses.

use kile_wheel_core::names::DISPLAY_NAME;

use crate::ui::Settings;

pub fn draw(settings: &mut Settings, ui: &mut egui::Ui) {
    super::heading(ui, "About", DISPLAY_NAME);

    super::row(ui, "Version", |ui| {
        ui.label(env!("CARGO_PKG_VERSION"));
    });

    let root = settings.draft.config_root().to_path_buf();
    let config_file = settings.draft.config_file();

    super::row(ui, "Config folder", |ui| {
        super::path_label(ui, &root);
    });
    super::row(ui, "", |ui| {
        if ui.button("Open config folder").clicked() {
            open(settings, &root);
        }
        if ui.button("Open config.toml").clicked() {
            open(settings, &config_file);
        }
    });

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);

    ui.label("Keys while the wheel is open");
    ui.label("  WASD or arrows   select by direction, following the keys you hold");
    ui.label("  Enter            activate the selected slot");
    ui.label("  Shift+Enter      launch a new instance");
    ui.label("  1 to 9           switch tab");
    ui.label("  Backspace        back out one level");
    ui.label("  Esc              close");
    ui.add_space(8.0);
    ui.label("Mouse");
    ui.label("  Move             selects by angle, no precision needed");
    ui.label("  Click            activate; the cog in the middle opens this window");
    ui.label("  Middle click     launch a new instance");
    ui.label("  Hold 400ms       toggle edit mode, then drag slots around");
}

/// Hands the path to the desktop rather than guessing at an editor.
fn open(settings: &mut Settings, path: &std::path::Path) {
    if let Err(error) = kile_wheel_platform::process::open(&path.to_string_lossy()) {
        settings.status = Some(format!("{error}"));
    }
}
