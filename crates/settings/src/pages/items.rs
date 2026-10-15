//! Items: everything pinned to a slot, of every type.
//!
//! Shown as a small copy of the wheel, one tab at a time, so a slot is picked where it
//! actually sits. Click a slice to see it, double-click to edit, drag one onto another to
//! swap them.

use kile_wheel_core::item::{Item, ItemKind, SCRIPT_PRESETS, SnippetMode};
use kile_wheel_core::slot::{Slot, Tab};
use kile_wheel_platform::paths::expand;

use super::mini_wheel::{self, DragState, Palette, SliceView, WheelSpec};
use crate::draft::slot_label;
use crate::ui::Settings;

/// Below this the wheel and its details stack instead of sitting side by side. Room for
/// the smallest wheel, a gap and the details column.
const SIDE_BY_SIDE_MIN_PX: f32 = 540.0;
const DETAILS_MIN_PX: f32 = 260.0;
/// Kept free under the wheel, so it never sits flush against the save bar.
const WHEEL_BOTTOM_ROOM_PX: f32 = 12.0;

/// Which tab the page is showing and which slice is picked.
#[derive(Debug, Clone, Copy)]
pub struct ItemsView {
    tab: Tab,
    selected: Option<Slot>,
    drag: DragState,
}

impl Default for ItemsView {
    fn default() -> Self {
        Self {
            tab: Tab::FIRST,
            selected: Slot::new(0),
            drag: DragState::default(),
        }
    }
}

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

/// What a frame of the wheel view asked for. Gathered first and applied after, because
/// the drawing borrows the items the actions change.
#[derive(Debug, Default)]
struct Requests {
    edit: Option<usize>,
    add: Option<(Slot, usize)>,
    remove: Option<usize>,
    swap: Option<(Slot, Slot)>,
}

fn draw_list(settings: &mut Settings, ui: &mut egui::Ui) {
    super::heading(
        ui,
        "Items",
        "Click a slot to see it, double-click to edit, drag one onto another to swap them.",
    );

    cycle_tabs_with_keys(settings, ui);
    draw_tab_strip(settings, ui);
    ui.add_space(12.0);

    let mut requests = Requests::default();
    let side_by_side = ui.available_width() >= SIDE_BY_SIDE_MIN_PX;

    match side_by_side {
        true => {
            ui.horizontal_top(|ui| {
                draw_wheel(settings, ui, &mut requests, DETAILS_MIN_PX + 16.0);
                ui.add_space(16.0);
                ui.vertical(|ui| {
                    ui.set_min_width(DETAILS_MIN_PX);
                    draw_details(settings, ui, &mut requests);
                });
            });
        }
        false => {
            ui.vertical_centered(|ui| draw_wheel(settings, ui, &mut requests, 0.0));
            ui.add_space(12.0);
            draw_details(settings, ui, &mut requests);
        }
    }

    apply(settings, requests);
}

/// Left and right step through the tabs while nothing is being typed into.
fn cycle_tabs_with_keys(settings: &mut Settings, ui: &egui::Ui) {
    let typing = ui.ctx().memory(|memory| memory.focused().is_some());
    if typing {
        return;
    }

    let (back, forward) = ui.input(|input| {
        (
            input.key_pressed(egui::Key::ArrowLeft) || input.key_pressed(egui::Key::PageUp),
            input.key_pressed(egui::Key::ArrowRight) || input.key_pressed(egui::Key::PageDown),
        )
    });
    match (back, forward) {
        (true, false) => settings.items_view.tab = step_tab(settings.items_view.tab, -1),
        (false, true) => settings.items_view.tab = step_tab(settings.items_view.tab, 1),
        _ => {}
    }
}

fn draw_tab_strip(settings: &mut Settings, ui: &mut egui::Ui) {
    let current = settings.items_view.tab;

    ui.horizontal_wrapped(|ui| {
        if ui
            .button("◀")
            .on_hover_text("Previous tab (Left)")
            .clicked()
        {
            settings.items_view.tab = step_tab(current, -1);
        }

        for number in 1..=Tab::MAX_COUNT {
            let Some(tab) = Tab::from_number(number) else {
                continue;
            };
            let used = items_on(&settings.draft.working.items, tab);
            let label = tab_chip_label(settings, tab, used);

            let chip = egui::Button::selectable(tab == current, label);
            let response = ui.add(chip).on_hover_text(match used {
                0 => "Nothing here yet".to_owned(),
                1 => "1 slot used".to_owned(),
                count => format!("{count} slots used"),
            });
            if response.clicked() {
                settings.items_view.tab = tab;
            }
        }

        if ui.button("▶").on_hover_text("Next tab (Right)").clicked() {
            settings.items_view.tab = step_tab(current, 1);
        }
    });
}

/// A used tab says how full it is; an empty one is just its number, dimmed.
fn tab_chip_label(settings: &Settings, tab: Tab, used: usize) -> egui::RichText {
    let name = tab_name(settings, tab);
    let text = match name {
        Some(name) => format!("{} {}", tab.number(), mini_wheel::shorten(&name, 12)),
        None => tab.number().to_string(),
    };

    match used {
        0 => egui::RichText::new(text).weak(),
        _ => egui::RichText::new(format!("{text} · {used}")),
    }
}

/// `beside_px` is what the details column needs next to the wheel, so the wheel leaves it.
fn draw_wheel(settings: &mut Settings, ui: &mut egui::Ui, requests: &mut Requests, beside_px: f32) {
    let tab = settings.items_view.tab;
    let items = &settings.draft.working.items;

    let slices: [SliceView; Slot::COUNT] = std::array::from_fn(|index| {
        let slot = Slot::new(index).expect("index is below the slot count");
        match item_at(items, tab, slot).map(|index| &items[index]) {
            Some(item) => SliceView {
                label: item.name.clone(),
                caption: caption(&item.kind).to_owned(),
                filled: !item.kind.is_unpinned(),
                reserved: item.kind.is_unpinned(),
            },
            None => SliceView::default(),
        }
    });

    let accent = settings.draft.working.wheel.accent();
    let palette = Palette::from_visuals(
        ui.visuals(),
        egui::Color32::from_rgb(accent.red, accent.green, accent.blue),
    );
    let hub_title = format!("TAB {}", tab.number());
    let hub_caption = tab_name(settings, tab).unwrap_or_default();
    // the page scrolls, so its own height is unbounded; size against what is on screen
    let visible_height = ui.clip_rect().bottom() - ui.cursor().top() - WHEEL_BOTTOM_ROOM_PX;
    let side = (ui.available_width() - beside_px).min(visible_height);
    let diameter = mini_wheel::diameter_for(egui::Rect::from_min_size(
        ui.cursor().min,
        egui::Vec2::splat(side),
    ));

    let spec = WheelSpec {
        diameter_px: diameter,
        slices: &slices,
        selected: settings.items_view.selected,
        hub_title: &hub_title,
        hub_caption: &hub_caption,
        palette,
    };
    let input = mini_wheel::show(ui, &spec, &mut settings.items_view.drag);

    if let Some(slot) = input.clicked {
        settings.items_view.selected = Some(slot);
    }
    if let Some(slot) = input.double_clicked {
        settings.items_view.selected = Some(slot);
        match item_at(&settings.draft.working.items, tab, slot) {
            Some(index) => requests.edit = Some(index),
            // an empty slot opens on the commonest thing to put there
            None => requests.add = Some((slot, PINNED_APP_TYPE)),
        }
    }
    if let Some((from, to)) = input.dropped {
        requests.swap = Some((from, to));
    }
}

/// `ItemKind::NAMES` index of a pinned app, the default for a double-clicked empty slot.
const PINNED_APP_TYPE: usize = 1;

fn draw_details(settings: &mut Settings, ui: &mut egui::Ui, requests: &mut Requests) {
    let tab = settings.items_view.tab;
    let Some(slot) = settings.items_view.selected else {
        ui.weak("Pick a slot on the wheel.");
        return;
    };

    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_width(DETAILS_MIN_PX);
        ui.weak(format!("Tab {} · {}", tab.number(), slot_label(slot)));
        ui.add_space(4.0);

        let index = item_at(&settings.draft.working.items, tab, slot);
        match index {
            Some(index) => draw_filled(ui, &settings.draft.working.items[index], index, requests),
            None => draw_empty(ui, slot, requests),
        }
    });

    ui.add_space(8.0);
    ui.weak("Left and Right switch tabs. Changes wait for Save.");
}

fn draw_filled(ui: &mut egui::Ui, item: &Item, index: usize, requests: &mut Requests) {
    // a long name is cut to the panel and shown in full on hover, never pushed sideways
    ui.add(egui::Label::new(egui::RichText::new(&item.name).heading()).truncate())
        .on_hover_text(&item.name);
    ui.label(item.kind.type_name());

    if let Some(detail) = summary(&item.kind) {
        ui.add_space(4.0);
        ui.add(egui::Label::new(egui::RichText::new(&detail).monospace().weak()).truncate())
            .on_hover_text(&detail);
    }

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        if ui.button("Edit").clicked() {
            requests.edit = Some(index);
        }
        if ui.button("Remove").clicked() {
            requests.remove = Some(index);
        }
    });

    ui.add_space(6.0);
    ui.weak("Drag it onto another slot to swap them.");
}

fn draw_empty(ui: &mut egui::Ui, slot: Slot, requests: &mut Requests) {
    ui.heading("Empty");
    ui.label("Put something here:");
    ui.add_space(6.0);

    ui.horizontal_wrapped(|ui| {
        for (kind, name) in ItemKind::NAMES.iter().enumerate() {
            if ui.button(*name).clicked() {
                requests.add = Some((slot, kind));
            }
        }
    });
}

fn apply(settings: &mut Settings, requests: Requests) {
    let tab = settings.items_view.tab;

    if let Some((from, to)) = requests.swap {
        swap_slots(&mut settings.draft.working.items, tab, from, to);
        settings.items_view.selected = Some(to);
    }
    if let Some(index) = requests.remove {
        settings.draft.working.items.remove(index);
    }
    if let Some(index) = requests.edit {
        settings.editor = Some(ItemEditor {
            index: Some(index),
            item: settings.draft.working.items[index].clone(),
        });
        return;
    }
    if let Some((slot, kind)) = requests.add {
        let (Some(name), Some(kind)) = (ItemKind::NAMES.get(kind), ItemKind::blank(kind)) else {
            return;
        };
        settings.editor = Some(ItemEditor {
            index: None,
            item: Item {
                name: (*name).to_owned(),
                icon: None,
                tab: tab.number(),
                slot: slot.index(),
                kind,
            },
        });
    }
}

/// Where in the item list a slot's item is.
fn item_at(items: &[Item], tab: Tab, slot: Slot) -> Option<usize> {
    items
        .iter()
        .position(|item| item.tab == tab.number() && item.slot == slot.index())
}

fn items_on(items: &[Item], tab: Tab) -> usize {
    items.iter().filter(|item| item.tab == tab.number()).count()
}

/// Swaps whatever sits in two slots of one tab. Either may be empty, which makes it a
/// move.
fn swap_slots(items: &mut [Item], tab: Tab, from: Slot, to: Slot) {
    for item in items.iter_mut().filter(|item| item.tab == tab.number()) {
        if item.slot == from.index() {
            item.slot = to.index();
        } else if item.slot == to.index() {
            item.slot = from.index();
        }
    }
}

/// Wraps at both ends, so the arrows never dead-end.
fn step_tab(tab: Tab, by: isize) -> Tab {
    let count = Tab::MAX_COUNT as isize;
    let index = (tab.index() as isize + by).rem_euclid(count) as usize;
    Tab::from_index(index).unwrap_or(Tab::FIRST)
}

fn tab_name(settings: &Settings, tab: Tab) -> Option<String> {
    settings
        .draft
        .working
        .tab(tab)
        .and_then(|settings| settings.name.clone())
        .filter(|name| !name.trim().is_empty())
}

/// One word under a slice's label, since the full type name does not fit.
fn caption(kind: &ItemKind) -> &'static str {
    match kind {
        ItemKind::UnpinnedApp => "unpinned",
        ItemKind::App { .. } => "app",
        ItemKind::Script { .. } => "script",
        ItemKind::Url { .. } => "url",
        ItemKind::Snippet { .. } => "snippet",
        ItemKind::Folder { .. } => "folder",
        ItemKind::Terminal { .. } => "terminal",
    }
}

/// The one line that says what the item does, for the details panel.
fn summary(kind: &ItemKind) -> Option<String> {
    let text = match kind {
        ItemKind::UnpinnedApp => return Some("Filled by whatever is running".to_owned()),
        ItemKind::App { target } => target.clone(),
        ItemKind::Script { path, .. } | ItemKind::Folder { path } => path.clone(),
        ItemKind::Url { url, .. } => url.clone(),
        ItemKind::Snippet { text, .. } => text.lines().next().unwrap_or_default().to_owned(),
        ItemKind::Terminal {
            profile, distro, ..
        } => [profile.as_deref(), distro.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · "),
    };
    (!text.trim().is_empty()).then_some(text)
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

    draw_common(ui, &mut editor.item, settings);
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

    // land on the item where it now sits, which may be a different tab than before
    if let Some(position) = editor.item.position() {
        settings.items_view.tab = position.tab;
        settings.items_view.selected = Some(position.slot);
    }
}

/// Fields every type has: the name, the icon, and where it sits.
fn draw_common(ui: &mut egui::Ui, item: &mut Item, settings: &mut Settings) {
    super::row(ui, "Name", |ui| {
        ui.add(egui::TextEdit::singleline(&mut item.name).desired_width(f32::INFINITY));
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

    let icons_dir = settings.draft.icons_dir();
    let root = settings.draft.config_root().to_path_buf();
    let own_icon = item.kind.icon_source().map(expand);
    super::icon_field(
        ui,
        &mut settings.icon_picker,
        &icons_dir,
        &root,
        "item",
        own_icon.as_deref(),
        &mut item.icon,
    );
}

fn draw_kind(ui: &mut egui::Ui, settings: &Settings, kind: &mut ItemKind) {
    match kind {
        ItemKind::UnpinnedApp => draw_unpinned(ui),
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

/// Nothing to configure: the wheel decides what sits here.
fn draw_unpinned(ui: &mut egui::Ui) {
    ui.label("This slot is held open for whatever happens to be running.");
    ui.label("The app that lands here keeps the slot until it closes.");
    ui.add_space(8.0);
    ui.label("Add more of these to make room for more running apps.");
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

    /// The picker reads its labels straight from core, so a blank of each type has to come
    /// back as that same type.
    #[test]
    fn every_type_in_the_picker_builds_itself() {
        for (index, name) in ItemKind::NAMES.iter().enumerate() {
            let blank = ItemKind::blank(index).expect("every listed type has a blank");
            assert_eq!(blank.type_name(), *name);
        }
    }

    #[test]
    fn there_is_no_type_past_the_list() {
        assert!(ItemKind::blank(ItemKind::NAMES.len()).is_none());
    }

    fn folder(name: &str, tab: usize, slot: usize) -> Item {
        Item {
            name: name.to_owned(),
            icon: None,
            tab,
            slot,
            kind: ItemKind::Folder {
                path: "C:\\".to_owned(),
            },
        }
    }

    fn slot(index: usize) -> Slot {
        Slot::new(index).expect("a real slot")
    }

    fn tab(number: usize) -> Tab {
        Tab::from_number(number).expect("a real tab")
    }

    #[test]
    fn dragging_one_item_onto_another_swaps_them() {
        let mut items = vec![folder("a", 1, 0), folder("b", 1, 4)];

        swap_slots(&mut items, tab(1), slot(0), slot(4));

        assert_eq!((items[0].slot, items[1].slot), (4, 0));
    }

    #[test]
    fn dragging_onto_an_empty_slot_moves_the_item() {
        let mut items = vec![folder("a", 1, 0)];

        swap_slots(&mut items, tab(1), slot(0), slot(5));

        assert_eq!(items[0].slot, 5);
    }

    /// The wheel only shows one tab, so a swap on it must leave every other tab alone.
    #[test]
    fn a_swap_never_touches_another_tab() {
        let mut items = vec![folder("a", 1, 0), folder("other", 2, 4)];

        swap_slots(&mut items, tab(1), slot(0), slot(4));

        assert_eq!((items[1].tab, items[1].slot), (2, 4));
    }

    #[test]
    fn a_slot_is_found_on_its_own_tab_only() {
        let items = vec![folder("a", 1, 3), folder("b", 2, 3)];

        assert_eq!(item_at(&items, tab(2), slot(3)), Some(1));
        assert_eq!(item_at(&items, tab(1), slot(4)), None);
        assert_eq!(items_on(&items, tab(1)), 1);
    }

    #[test]
    fn the_tab_arrows_wrap_at_both_ends() {
        assert_eq!(step_tab(tab(1), -1), tab(Tab::MAX_COUNT));
        assert_eq!(step_tab(tab(Tab::MAX_COUNT), 1), tab(1));
        assert_eq!(step_tab(tab(3), 1), tab(4));
    }

    #[test]
    fn a_summary_says_what_the_item_does() {
        let app = ItemKind::App {
            target: r"C:\Kiro.lnk".to_owned(),
        };
        let snippet = ItemKind::Snippet {
            text: "first line\nsecond".to_owned(),
            mode: SnippetMode::Clipboard,
        };

        assert_eq!(summary(&app).as_deref(), Some(r"C:\Kiro.lnk"));
        assert_eq!(summary(&snippet).as_deref(), Some("first line"));
        assert_eq!(
            summary(&ItemKind::Folder {
                path: "  ".to_owned()
            }),
            None
        );
    }

    #[test]
    fn every_type_has_a_caption() {
        for index in 0..ItemKind::NAMES.len() {
            let kind = ItemKind::blank(index).expect("every listed type has a blank");
            assert!(!caption(&kind).is_empty());
        }
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
