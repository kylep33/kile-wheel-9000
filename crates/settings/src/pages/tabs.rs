//! Tabs: the nine pages the number keys reach.

use kile_wheel_core::slot::Tab;

use crate::ui::Settings;

pub fn draw(settings: &mut Settings, ui: &mut egui::Ui) {
    super::heading(
        ui,
        "Tabs",
        "Up to nine, selected with the number keys while the wheel is open.",
    );

    let icons_dir = settings.draft.icons_dir();
    let mut remove = None;

    for (index, tab) in settings.draft.working.tabs.iter_mut().enumerate() {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Tab {}", tab.index));
                if ui.button("Remove").clicked() {
                    remove = Some(index);
                }
            });

            super::optional_text(ui, "Name", &mut tab.name);
            draw_icon(ui, &icons_dir, &mut tab.icon);
        });
        ui.add_space(8.0);
    }

    if let Some(index) = remove {
        settings.draft.working.tabs.remove(index);
    }

    draw_add(settings, ui);
}

fn draw_add(settings: &mut Settings, ui: &mut egui::Ui) {
    let used: Vec<usize> = settings
        .draft
        .working
        .tabs
        .iter()
        .map(|tab| tab.index)
        .collect();
    let next = (1..=Tab::MAX_COUNT).find(|number| !used.contains(number));

    let Some(next) = next else {
        ui.label(format!("All {} tabs are configured.", Tab::MAX_COUNT));
        return;
    };

    if ui.button(format!("Add tab {next}")).clicked() {
        settings.draft.ensure_tab(next);
    }
}

/// Icon paths are stored relative to the config folder, so the whole folder stays
/// portable between machines.
fn draw_icon(ui: &mut egui::Ui, icons_dir: &std::path::Path, icon: &mut Option<String>) {
    let mut text = icon.clone().unwrap_or_default();

    super::path_row(ui, "Icon", &mut text, || {
        rfd::FileDialog::new()
            .set_directory(icons_dir)
            .add_filter(
                "Images",
                &["png", "jpg", "jpeg", "bmp", "ico", "gif", "webp"],
            )
            .pick_file()
    });

    *icon = Some(text).filter(|text| !text.trim().is_empty());
}
