//! General: how the wheel behaves outside the wheel itself.

use crate::ui::Settings;

pub fn draw(settings: &mut Settings, ui: &mut egui::Ui) {
    super::heading(
        ui,
        "General",
        "How the wheel behaves outside the wheel itself.",
    );

    let general = &mut settings.draft.working.general;

    super::row(ui, "Ignore hotkeys in fullscreen", |ui| {
        ui.checkbox(&mut general.disable_in_fullscreen, "");
    });
    ui.label("  Leaves games and video alone while they have focus.");
    ui.add_space(8.0);

    super::row(ui, "Minimize if already focused", |ui| {
        ui.checkbox(&mut general.minimize_if_focused, "");
    });
    ui.label("  Activating the app that already has focus minimizes it, like the taskbar.");
    ui.add_space(8.0);

    super::row(ui, "Show the clock", |ui| {
        ui.checkbox(&mut general.show_clock, "");
    });
    ui.label("  Puts the time and date in the middle of the wheel.");
    ui.add_space(16.0);

    ui.separator();
    ui.add_space(8.0);

    // this one is not ours to change: the logon task runs with highest privileges, which
    // an unelevated settings window cannot register
    ui.add_enabled_ui(false, |ui| {
        super::row(ui, "Start with Windows", |ui| {
            ui.checkbox(&mut general.start_with_windows, "");
        });
    });
    ui.label("  Managed by install.ps1, which registers the elevated logon task.");
    ui.label("  Run uninstall.ps1 to remove it.");
}
