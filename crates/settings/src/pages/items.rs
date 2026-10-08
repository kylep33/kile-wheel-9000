//! Items: everything pinned to a slot, of every type.

use kile_wheel_core::item::{Item, ItemKind, SCRIPT_PRESETS, SnippetMode};
use kile_wheel_core::slot::{Slot, Tab};

use crate::draft::slot_label;
use crate::ui::Settings;

/// Builds the starting shape for a new item of one type.
type MakeKind = fn() -> ItemKind;

/// Every type a new item can be, with what it starts out as.
const TYPES: &[(&str, MakeKind)] = &[
    ("App", || ItemKind::App {
        target: String::new(),
    }),
    ("Script", || ItemKind::Script {
        path: String::new(),
        exec: SCRIPT_PRESETS[0].1.to_owned(),
        cwd: None,
        hidden: false,
    }),
    ("URL", || ItemKind::Url {
        url: String::new(),
        browser: None,
        browser_args: None,
    }),
    ("Snippet", || ItemKind::Snippet {
        text: String::new(),
        mode: SnippetMode::default(),
    }),
    ("Folder", || ItemKind::Folder {
        path: String::new(),
    }),
    ("Terminal", || ItemKind::Terminal {
        profile: None,
        distro: None,
        cwd: None,
    }),
];

/// The item being added or edited, held apart from the config so Cancel is possible.
#[derive(Debug, Clone)]
pub struct ItemEditor {
    /// Where it came from, or `None` while adding.
    index: Option<usize>,
    item: Item,
}

pub fn draw(settings: &mut Settings, ui: &mut egui::Ui) {
    if settings.editor.is_some() {
        draw_editor(settings, ui);
        return;
    }
    draw_list(settings, ui);
}

fn draw_list(settings: &mut Settings, ui: &mut egui::Ui) {
    super::heading(
        ui,
        "Items",
        "Everything pinned to a slot. Unpinned running apps fill whatever is left.",
    );

    if settings.draft.working.items.is_empty() {
        ui.label("Nothing pinned yet. Every slot is filled by whatever is running.");
        ui.add_space(8.0);
    }

    let mut edit = None;
    let mut remove = None;

    for (index, item) in settings.draft.working.items.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(position_text(item));
            ui.label(format!("{:<9}", item.kind.type_name()));
            ui.label(&item.name);

            if ui.button("Edit").clicked() {
                edit = Some(index);
            }
            if ui.button("Remove").clicked() {
                remove = Some(index);
            }
        });
    }

    if let Some(index) = remove {
        settings.draft.working.items.remove(index);
    }
    if let Some(index) = edit {
        settings.editor = Some(ItemEditor {
            index: Some(index),
            item: settings.draft.working.items[index].clone(),
        });
        return;
    }

    ui.add_space(12.0);
    draw_add(settings, ui);
}

fn draw_add(settings: &mut Settings, ui: &mut egui::Ui) {
    let Some(position) = settings.draft.first_free_position() else {
        ui.label("Every slot on every tab is taken.");
        return;
    };

    ui.horizontal(|ui| {
        ui.label("Add");
        for (name, make) in TYPES {
            if !ui.button(*name).clicked() {
                continue;
            }
            settings.editor = Some(ItemEditor {
                index: None,
                item: Item {
                    name: String::new(),
                    icon: None,
                    tab: position.tab.number(),
                    slot: position.slot.index(),
                    kind: make(),
                },
            });
        }
    });
}

fn draw_editor(settings: &mut Settings, ui: &mut egui::Ui) {
    let Some(mut editor) = settings.editor.take() else {
        return;
    };

    let title = match editor.index {
        Some(_) => "Edit item",
        None => "New item",
    };
    super::heading(ui, title, editor.item.kind.type_name());

    draw_common(ui, &mut editor.item, &settings.draft.icons_dir());
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
    draw_kind(ui, settings, &mut editor.item.kind);

    ui.add_space(16.0);
    let problem = name_problem(&editor.item);
    if let Some(problem) = &problem {
        ui.colored_label(ui.visuals().error_fg_color, problem);
    }

    let mut closed = false;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(problem.is_none(), egui::Button::new("Done"))
            .clicked()
        {
            commit(settings, &editor);
            closed = true;
        }
        if ui.button("Cancel").clicked() {
            closed = true;
        }
    });

    if !closed {
        settings.editor = Some(editor);
    }
}

fn commit(settings: &mut Settings, editor: &ItemEditor) {
    match editor.index {
        Some(index) => settings.draft.working.items[index] = editor.item.clone(),
        None => settings.draft.working.items.push(editor.item.clone()),
    }
    settings.draft.ensure_tab(editor.item.tab);
}

/// Fields every type has: the name, the icon, and where it sits.
fn draw_common(ui: &mut egui::Ui, item: &mut Item, icons_dir: &std::path::Path) {
    super::row(ui, "Name", |ui| {
        ui.add(egui::TextEdit::singleline(&mut item.name).desired_width(320.0));
    });

    super::row(ui, "Tab", |ui| {
        for number in 1..=Tab::MAX_COUNT {
            ui.selectable_value(&mut item.tab, number, number.to_string());
        }
    });

    super::row(ui, "Slot", |ui| {
        for slot in Slot::all() {
            ui.selectable_value(&mut item.slot, slot.index(), slot_label(slot));
        }
    });

    let mut icon = item.icon.clone().unwrap_or_default();
    super::path_row(ui, "Icon", &mut icon, || {
        rfd::FileDialog::new()
            .set_directory(icons_dir)
            .add_filter(
                "Images",
                &["png", "jpg", "jpeg", "bmp", "ico", "gif", "webp"],
            )
            .pick_file()
    });
    item.icon = Some(icon).filter(|text| !text.trim().is_empty());
    ui.label("  Leave blank to use the icon Windows already has for the target.");
}

fn draw_kind(ui: &mut egui::Ui, settings: &Settings, kind: &mut ItemKind) {
    match kind {
        ItemKind::App { target } => draw_app(ui, target),
        ItemKind::Script {
            path,
            exec,
            cwd,
            hidden,
        } => draw_script(ui, path, exec, cwd, hidden),
        ItemKind::Url {
            url,
            browser,
            browser_args,
        } => draw_url(ui, url, browser, browser_args),
        ItemKind::Snippet { text, mode } => draw_snippet(ui, text, mode),
        ItemKind::Folder { path } => draw_folder(ui, path),
        ItemKind::Terminal {
            profile,
            distro,
            cwd,
        } => draw_terminal(ui, settings, profile, distro, cwd),
    }
}

fn draw_app(ui: &mut egui::Ui, target: &mut String) {
    super::path_row(ui, "Target", target, || {
        rfd::FileDialog::new()
            .add_filter("Programs and shortcuts", &["exe", "lnk", "bat", "cmd"])
            .pick_file()
    });
    ui.label("  An exe, a shortcut, or shell:AppsFolder\\<AppUserModelID> for a Store app.");
    ui.label("  Environment variables such as %LOCALAPPDATA% are expanded.");
}

fn draw_script(
    ui: &mut egui::Ui,
    path: &mut String,
    exec: &mut String,
    cwd: &mut Option<String>,
    hidden: &mut bool,
) {
    super::path_row(ui, "Script", path, || rfd::FileDialog::new().pick_file());

    super::row(ui, "Run with", |ui| {
        ui.add(egui::TextEdit::singleline(exec).desired_width(f32::INFINITY));
    });
    ui.label("  {path} is replaced with the script above.");

    super::row(ui, "Presets", |ui| {
        for (name, template) in SCRIPT_PRESETS {
            if ui.button(*name).clicked() {
                *exec = (*template).to_owned();
            }
        }
    });

    super::optional_text(ui, "Working directory", cwd);
    super::row(ui, "Run with no window", |ui| {
        ui.checkbox(hidden, "");
    });
}

fn draw_url(
    ui: &mut egui::Ui,
    url: &mut String,
    browser: &mut Option<String>,
    browser_args: &mut Option<String>,
) {
    super::row(ui, "URL", |ui| {
        ui.add(egui::TextEdit::singleline(url).desired_width(f32::INFINITY));
    });

    let mut chosen = browser.clone().unwrap_or_default();
    super::path_row(ui, "Browser", &mut chosen, || {
        rfd::FileDialog::new()
            .add_filter("Programs", &["exe"])
            .pick_file()
    });
    *browser = Some(chosen).filter(|text| !text.trim().is_empty());
    ui.label("  Leave blank to use whichever browser Windows opens links with.");

    super::optional_text(ui, "Browser arguments", browser_args);
    ui.label("  For a specific Chrome or Edge profile: --profile-directory=\"Profile 2\"");
}

fn draw_snippet(ui: &mut egui::Ui, text: &mut String, mode: &mut SnippetMode) {
    super::row(ui, "Text", |ui| {
        ui.add(
            egui::TextEdit::multiline(text)
                .desired_rows(6)
                .desired_width(f32::INFINITY),
        );
    });

    super::row(ui, "Deliver by", |ui| {
        ui.selectable_value(mode, SnippetMode::Clipboard, "Clipboard paste");
        ui.selectable_value(mode, SnippetMode::Type, "Typing it out");
    });
    ui.label("  Clipboard paste puts your old clipboard back afterwards.");

    ui.add_space(8.0);
    ui.colored_label(
        ui.visuals().warn_fg_color,
        "Snippets are stored as plain text in config.toml. Keep secrets out of them.",
    );
}

fn draw_folder(ui: &mut egui::Ui, path: &mut String) {
    super::path_row(ui, "Folder", path, || rfd::FileDialog::new().pick_folder());
    ui.label("  Environment variables such as %USERPROFILE% are expanded.");
}

fn draw_terminal(
    ui: &mut egui::Ui,
    settings: &Settings,
    profile: &mut Option<String>,
    distro: &mut Option<String>,
    cwd: &mut Option<String>,
) {
    super::optional_choice(
        ui,
        "Profile",
        profile,
        &settings.terminal_profiles,
        "Windows Terminal settings not found",
    );
    super::optional_choice(
        ui,
        "WSL distro",
        distro,
        &settings.wsl_distros,
        "No WSL distributions found",
    );
    super::optional_text(ui, "Start in", cwd);
    ui.label("  A Windows path, or a Linux path when a WSL distro is set.");
}

fn position_text(item: &Item) -> String {
    let slot = Slot::new(item.slot).map(slot_label).unwrap_or("?");

    format!("{}:{:<3}", item.tab, slot)
}

/// Says what is stopping Done, rather than leaving it greyed out for no visible reason.
fn name_problem(item: &Item) -> Option<String> {
    if item.name.trim().is_empty() {
        return Some("Give it a name; that is what the wheel shows.".to_owned());
    }
    if item.position().is_none() {
        return Some("That tab and slot are not on the wheel.".to_owned());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_type_starts_out_as_that_type() {
        let names: Vec<&str> = TYPES.iter().map(|(_, make)| make().type_name()).collect();
        let labels: Vec<&str> = TYPES.iter().map(|(name, _)| *name).collect();

        assert_eq!(names, labels);
    }

    #[test]
    fn a_nameless_item_is_rejected() {
        let item = Item {
            name: "  ".to_owned(),
            icon: None,
            tab: 1,
            slot: 0,
            kind: ItemKind::Folder {
                path: "C:\\".to_owned(),
            },
        };

        assert!(name_problem(&item).is_some());
    }

    #[test]
    fn an_off_wheel_position_is_rejected() {
        let item = Item {
            name: "Fine".to_owned(),
            icon: None,
            tab: 99,
            slot: 0,
            kind: ItemKind::Folder {
                path: "C:\\".to_owned(),
            },
        };

        assert!(name_problem(&item).is_some());
    }

    #[test]
    fn a_complete_item_passes() {
        let item = Item {
            name: "Projects".to_owned(),
            icon: None,
            tab: 2,
            slot: 3,
            kind: ItemKind::Folder {
                path: "C:\\p".to_owned(),
            },
        };

        assert_eq!(name_problem(&item), None);
    }
}
