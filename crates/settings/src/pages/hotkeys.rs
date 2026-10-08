//! Hotkeys: what opens, closes and toggles the wheel.

use kile_wheel_core::binding::Action;
use kile_wheel_core::config::HotkeyConfig;

use crate::draft::binding_problem;
use crate::ui::Settings;

pub fn draw(settings: &mut Settings, ui: &mut egui::Ui) {
    super::heading(
        ui,
        "Hotkeys",
        "Any number of bindings. Esc and Backspace always work and need no binding.",
    );

    let hotkeys = &mut settings.draft.working.hotkeys;
    let mut remove = None;

    for (index, hotkey) in hotkeys.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut hotkey.binding).desired_width(200.0));
            draw_action(ui, index, &mut hotkey.action);
            if ui.button("Remove").clicked() {
                remove = Some(index);
            }
        });

        if let Some(problem) = binding_problem(&hotkey.binding) {
            ui.colored_label(ui.visuals().error_fg_color, format!("  {problem}"));
        }
        ui.add_space(4.0);
    }

    if let Some(index) = remove {
        hotkeys.remove(index);
    }

    if ui.button("Add a hotkey").clicked() {
        hotkeys.push(HotkeyConfig {
            binding: String::new(),
            action: Action::Toggle,
        });
    }

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);
    draw_help(ui);
}

fn draw_action(ui: &mut egui::Ui, index: usize, action: &mut Action) {
    egui::ComboBox::from_id_salt(("action", index))
        .selected_text(action_name(*action))
        .show_ui(ui, |ui| {
            ui.selectable_value(action, Action::Toggle, "Toggle");
            ui.selectable_value(action, Action::Open, "Open");
            ui.selectable_value(action, Action::Close, "Close");
        });
}

fn action_name(action: Action) -> &'static str {
    match action {
        Action::Open => "Open",
        Action::Close => "Close",
        Action::Toggle => "Toggle",
    }
}

fn draw_help(ui: &mut egui::Ui) {
    ui.label("How to write a binding");
    ui.label("  A key name: CapsLock, F13, Esc, Space, Insert, 7, K");
    ui.label("  A combo: Ctrl+Shift+K, Win+Space");
    ui.label("  Modifiers on their own: Alt+Shift, Ctrl+Alt");
    ui.label("  A mouse button: XButton1, XButton2, MouseMiddle, Ctrl+XButton1");
    ui.label("  Anything unnamed: vk(0x5B)");
    ui.add_space(8.0);
    ui.label("Notes");
    ui.label("  A modifier-only combo fires on whichever key completes it, in any order.");
    ui.label("  Close bindings only fire while the wheel is open, so they stay usable elsewhere.");
    ui.label("  Alt+Shift is the Windows keyboard-layout switch; binding it takes that over.");
}
