//! Wheel: where it opens, how big, how it looks.

use kile_wheel_core::config::{AnimationSpeed, MonitorTarget, Rgb};

use crate::ui::Settings;

const MIN_SIZE: f32 = 0.5;
const MAX_SIZE: f32 = 2.5;
const MIN_OPACITY: f32 = 0.2;
const MAX_OPACITY: f32 = 1.0;

pub fn draw(settings: &mut Settings, ui: &mut egui::Ui) {
    super::heading(ui, "Wheel", "Where it opens, how big it is, how it looks.");

    let wheel = &mut settings.draft.working.wheel;

    super::row(ui, "Open on", |ui| {
        ui.selectable_value(
            &mut wheel.monitor,
            MonitorTarget::Cursor,
            "Monitor under cursor",
        );
        ui.selectable_value(
            &mut wheel.monitor,
            MonitorTarget::Primary,
            "Primary monitor",
        );
    });

    super::row(ui, "Center the cursor on open", |ui| {
        ui.checkbox(&mut wheel.center_cursor, "");
    });

    super::row(ui, "Open hotkey also closes", |ui| {
        ui.checkbox(&mut wheel.open_hotkey_closes, "");
    });
    ui.label("  Esc and Backspace always close, whatever this is set to.");
    ui.add_space(8.0);

    super::row(ui, "Animation", |ui| {
        ui.selectable_value(&mut wheel.animation, AnimationSpeed::Normal, "Normal");
        ui.selectable_value(&mut wheel.animation, AnimationSpeed::Fast, "Fast");
        ui.selectable_value(&mut wheel.animation, AnimationSpeed::Off, "Off");
    });

    super::row(ui, "Size", |ui| {
        ui.add(egui::Slider::new(&mut wheel.size, MIN_SIZE..=MAX_SIZE).fixed_decimals(2));
    });

    super::row(ui, "Opacity", |ui| {
        ui.add(egui::Slider::new(&mut wheel.opacity, MIN_OPACITY..=MAX_OPACITY).fixed_decimals(2));
    });

    draw_accent(ui, &mut wheel.accent);

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);
    draw_quips(ui, &mut wheel.empty_quips);
}

fn draw_accent(ui: &mut egui::Ui, accent: &mut String) {
    let parsed = Rgb::parse(accent);
    let mut colour = parsed
        .map(|rgb| egui::Color32::from_rgb(rgb.red, rgb.green, rgb.blue))
        .unwrap_or(egui::Color32::GRAY);

    super::row(ui, "Accent colour", |ui| {
        if ui.color_edit_button_srgba(&mut colour).changed() {
            *accent = Rgb {
                red: colour.r(),
                green: colour.g(),
                blue: colour.b(),
            }
            .to_string();
        }
        ui.add(egui::TextEdit::singleline(accent).desired_width(100.0));
    });

    if parsed.is_none() {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            "  Not a colour; the wheel will fall back to its own. Use #RRGGBB.",
        );
    }
}

/// Empty slots show one of these, so the wheel never has a blank wedge.
fn draw_quips(ui: &mut egui::Ui, quips: &mut Vec<String>) {
    ui.label("Empty slot quips");
    ui.label("  One is shown per empty slot.");
    ui.add_space(4.0);

    let mut remove = None;
    for (index, quip) in quips.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(quip).desired_width(320.0));
            if ui.button("Remove").clicked() {
                remove = Some(index);
            }
        });
    }
    if let Some(index) = remove {
        quips.remove(index);
    }

    if ui.button("Add a quip").clicked() {
        quips.push(String::new());
    }
}
