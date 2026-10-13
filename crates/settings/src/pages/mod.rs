//! The settings pages, and the widgets they share.

mod about;
mod general;
mod hotkeys;
mod items;
mod tabs;
mod wheel;

pub use items::ItemEditor;

use crate::ui::Settings;

const LABEL_WIDTH_PX: f32 = 180.0;
const BROWSE_WIDTH_PX: f32 = 90.0;
const MIN_FIELD_WIDTH_PX: f32 = 120.0;

/// Section order, which is also the sidebar order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    General,
    Hotkeys,
    Wheel,
    Tabs,
    Items,
    About,
}

impl Page {
    pub const ALL: [Self; 6] = [
        Self::General,
        Self::Hotkeys,
        Self::Wheel,
        Self::Tabs,
        Self::Items,
        Self::About,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Hotkeys => "Hotkeys",
            Self::Wheel => "Wheel",
            Self::Tabs => "Tabs",
            Self::Items => "Items",
            Self::About => "About",
        }
    }
}

pub fn draw(settings: &mut Settings, ui: &mut egui::Ui) {
    match settings.page() {
        Page::General => general::draw(settings, ui),
        Page::Hotkeys => hotkeys::draw(settings, ui),
        Page::Wheel => wheel::draw(settings, ui),
        Page::Tabs => tabs::draw(settings, ui),
        Page::Items => items::draw(settings, ui),
        Page::About => about::draw(settings, ui),
    }
}

/// A page heading plus the one-line explanation under it.
pub fn heading(ui: &mut egui::Ui, title: &str, blurb: &str) {
    ui.heading(title);
    ui.label(blurb);
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
}

/// A labelled row, so every field on every page lines up. Contents wrap, because the
/// page only scrolls vertically and anything past the right edge is unreachable.
pub fn row(ui: &mut egui::Ui, label: &str, contents: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal_wrapped(|ui| {
        ui.add_sized(
            [LABEL_WIDTH_PX, ui.spacing().interact_size.y],
            egui::Label::new(label).truncate(),
        );
        contents(ui);
    });
}

/// A text field with a Browse button that fills it in.
pub fn path_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    pick: impl FnOnce() -> Option<std::path::PathBuf>,
) {
    row(ui, label, |ui| {
        ui.add(egui::TextEdit::singleline(value).desired_width(field_width_px(ui)));
        if !ui.button("Browse").clicked() {
            return;
        }
        if let Some(path) = pick() {
            *value = path.to_string_lossy().into_owned();
        }
    });
}

/// A path that cannot be edited, elided to one line with the whole thing on hover.
pub fn path_label(ui: &mut egui::Ui, path: &std::path::Path) {
    let text = path.to_string_lossy();

    ui.add(egui::Label::new(text.as_ref()).truncate())
        .on_hover_text(text.as_ref());
}

/// Leaves room for the Browse button, so a long path clips inside the field instead of
/// pushing the button off the window.
fn field_width_px(ui: &egui::Ui) -> f32 {
    (ui.available_width() - BROWSE_WIDTH_PX).max(MIN_FIELD_WIDTH_PX)
}

/// Keeps an optional string and a text field in step: blank means unset.
pub fn optional_text(ui: &mut egui::Ui, label: &str, value: &mut Option<String>) {
    let mut text = value.clone().unwrap_or_default();

    row(ui, label, |ui| {
        ui.add(egui::TextEdit::singleline(&mut text).desired_width(f32::INFINITY));
    });

    *value = Some(text).filter(|text| !text.trim().is_empty());
}

/// A combo box that writes the chosen name into an optional string, with a free-text
/// field beside it so a profile the list missed can still be typed in.
pub fn optional_choice(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<String>,
    choices: &[String],
    empty_hint: &str,
) {
    let mut text = value.clone().unwrap_or_default();

    row(ui, label, |ui| {
        ui.add(egui::TextEdit::singleline(&mut text).desired_width(220.0));

        if choices.is_empty() {
            ui.label(empty_hint);
            return;
        }
        egui::ComboBox::from_id_salt(label)
            .selected_text("Pick")
            .show_ui(ui, |ui| {
                for choice in choices {
                    if ui.selectable_label(text == *choice, choice).clicked() {
                        text = choice.clone();
                    }
                }
            });
    });

    *value = Some(text).filter(|text| !text.trim().is_empty());
}
