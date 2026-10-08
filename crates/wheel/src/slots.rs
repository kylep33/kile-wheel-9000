//! What each wheel slot holds.
//!
//! Nothing appears unless the config puts it there. Running apps only land in slots
//! marked as unpinned app slots, so a blank slot stays blank.

use std::path::Path;

use kile_wheel_core::config::Config;
use kile_wheel_core::item::{Item, ItemKind, Position};
use kile_wheel_core::layout::SlotMemory;
use kile_wheel_core::slot::{Slot, Tab};
use kile_wheel_platform::apps::{AppGroup, AppKey, WindowModel};
use kile_wheel_platform::paths::expand;
use windows::Win32::Foundation::HWND;

/// What activating a slot does.
#[derive(Debug, Clone)]
pub enum SlotTarget {
    /// A configured entry that launches, focuses or pastes.
    Pinned(Item),
    /// A running app sitting in an unpinned app slot.
    Running(AppKey),
    /// An unpinned app slot with nothing in it yet.
    Waiting,
}

/// One slot the config has given a purpose.
#[derive(Debug, Clone)]
pub struct SlotEntry {
    pub label: String,
    /// What the icon is extracted from, which doubles as its cache key.
    pub icon_source: Option<String>,
    /// The configured type, which is what picks the slice's mini glyph.
    pub kind: ItemKind,
    pub target: SlotTarget,
    pub windows: Vec<HWND>,
    pub focused: bool,
}

impl SlotEntry {
    pub fn is_running(&self) -> bool {
        !self.windows.is_empty()
    }

    /// An unpinned slot waiting for an app reads as empty, so it gets a quip rather than
    /// a name, while keeping its glyph so you can see it is spoken for.
    pub fn is_waiting(&self) -> bool {
        matches!(self.target, SlotTarget::Waiting)
    }
}

/// Every tab's slots, resolved for one open.
#[derive(Debug, Default)]
pub struct Layout {
    tabs: Vec<Vec<Option<SlotEntry>>>,
    overflow: usize,
}

impl Layout {
    pub fn build(config: &Config, model: &WindowModel, memory: &mut SlotMemory<AppKey>) -> Self {
        let pinned = config.pinned();
        let groups = model.groups();
        let focused = model.focused_key();

        let open_slots = unpinned_slots(config);
        let spare: Vec<AppKey> = groups
            .iter()
            .filter(|group| !pinned.values().any(|item| matches_app(item, group.key)))
            .map(|group| group.key.clone())
            .collect();

        let overflow = memory.reconcile(&spare, &open_slots);
        let tab_count = config.tab_count();
        let mut tabs = vec![vec![None; Slot::COUNT]; tab_count];

        for (position, item) in pinned {
            if position.tab.index() >= tab_count {
                continue;
            }

            let entry = match item.kind.is_unpinned() {
                true => unpinned_entry(position, memory, &groups, focused),
                false => pinned_entry(item, &groups, focused),
            };
            tabs[position.tab.index()][position.slot.index()] = Some(entry);
        }

        Self { tabs, overflow }
    }

    pub fn get(&self, tab: Tab, slot: Slot) -> Option<&SlotEntry> {
        self.tabs.get(tab.index())?.get(slot.index())?.as_ref()
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len().max(1)
    }

    /// How many running apps had nowhere to go.
    pub fn overflow(&self) -> usize {
        self.overflow
    }
}

/// The unpinned app slots, in the order they fill.
fn unpinned_slots(config: &Config) -> Vec<Position> {
    let mut slots: Vec<Position> = config
        .items
        .iter()
        .filter(|item| item.kind.is_unpinned())
        .filter_map(Item::position)
        .collect();

    slots.sort();
    slots
}

fn unpinned_entry(
    position: Position,
    memory: &SlotMemory<AppKey>,
    groups: &[AppGroup<'_>],
    focused: Option<&AppKey>,
) -> SlotEntry {
    let group = memory
        .at(position)
        .and_then(|key| groups.iter().find(|group| group.key == key));

    let Some(group) = group else {
        return SlotEntry {
            label: String::new(),
            icon_source: None,
            kind: ItemKind::UnpinnedApp,
            target: SlotTarget::Waiting,
            windows: Vec::new(),
            focused: false,
        };
    };

    SlotEntry {
        label: group_label(group),
        icon_source: Some(icon_path(group.key)),
        kind: ItemKind::UnpinnedApp,
        target: SlotTarget::Running(group.key.clone()),
        windows: group.windows.iter().map(|window| window.handle).collect(),
        focused: focused == Some(group.key),
    }
}

/// A pinned app shows the windows it already has open, so it behaves like the taskbar
/// rather than appearing twice.
fn pinned_entry(item: &Item, groups: &[AppGroup<'_>], focused: Option<&AppKey>) -> SlotEntry {
    let group = groups.iter().find(|group| matches_app(item, group.key));

    SlotEntry {
        label: item.name.clone(),
        icon_source: icon_source(item),
        kind: item.kind.clone(),
        target: SlotTarget::Pinned(item.clone()),
        windows: group
            .map(|group| group.windows.iter().map(|window| window.handle).collect())
            .unwrap_or_default(),
        focused: group.is_some_and(|group| focused == Some(group.key)),
    }
}

/// The icon comes from a user-supplied image when one is configured, otherwise from the
/// item's own target. A configured path is relative to the config folder, so the caller
/// resolves it before handing it over.
fn icon_source(item: &Item) -> Option<String> {
    if let Some(icon) = &item.icon {
        return Some(icon.clone());
    }
    item.kind.icon_source().map(expand)
}

fn icon_path(key: &AppKey) -> String {
    match key {
        AppKey::Executable(path) => path.display().to_string(),
        AppKey::UserModelId(id) => format!("shell:AppsFolder\\{id}"),
    }
}

/// Slot labels mirror the taskbar: the app, not the window title.
///
/// The executable is preferred over the grouping key, because a packaged-app id often
/// carries a profile suffix and reads badly, as in `MSEdge.UserData.Profile2`.
fn group_label(group: &AppGroup<'_>) -> String {
    if let Some(stem) = group.executable.and_then(Path::file_stem) {
        return stem.to_string_lossy().into_owned();
    }

    match group.key {
        AppKey::Executable(path) => path.display().to_string(),
        AppKey::UserModelId(id) => match id.rsplit_once('!') {
            Some((_, app)) => app.to_owned(),
            None => id.clone(),
        },
    }
}

/// Whether a pinned app item is the same app as a running group.
fn matches_app(item: &Item, key: &AppKey) -> bool {
    let ItemKind::App { target } = &item.kind else {
        return false;
    };
    let target = expand(target);

    match key {
        // Windows paths are case-insensitive, so comparing them as written would miss
        AppKey::Executable(path) => path
            .to_str()
            .is_some_and(|path| path.eq_ignore_ascii_case(&target)),
        AppKey::UserModelId(id) => target.eq_ignore_ascii_case(id) || target.ends_with(id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_item(name: &str, target: &str, tab: usize, slot: usize) -> Item {
        Item {
            name: name.to_owned(),
            icon: None,
            tab,
            slot,
            kind: ItemKind::App {
                target: target.to_owned(),
            },
        }
    }

    fn unpinned(tab: usize, slot: usize) -> Item {
        Item {
            name: "Open".to_owned(),
            icon: None,
            tab,
            slot,
            kind: ItemKind::UnpinnedApp,
        }
    }

    fn group<'a>(key: &'a AppKey, executable: Option<&'a Path>) -> AppGroup<'a> {
        AppGroup {
            key,
            executable,
            windows: Vec::new(),
        }
    }

    #[test]
    fn unpinned_slots_come_out_in_tab_then_slot_order() {
        let config = Config {
            items: vec![unpinned(2, 1), unpinned(1, 6), unpinned(1, 2)],
            ..Config::default()
        };

        let slots: Vec<(usize, usize)> = unpinned_slots(&config)
            .into_iter()
            .map(|position| (position.tab.number(), position.slot.index()))
            .collect();
        assert_eq!(slots, [(1, 2), (1, 6), (2, 1)]);
    }

    #[test]
    fn pinned_entries_are_not_unpinned_slots() {
        let config = Config {
            items: vec![app_item("Code", "code.exe", 1, 0), unpinned(1, 1)],
            ..Config::default()
        };

        assert_eq!(unpinned_slots(&config).len(), 1);
    }

    #[test]
    fn an_empty_config_opens_no_slots() {
        assert!(unpinned_slots(&Config::default()).is_empty());
    }

    #[test]
    fn a_pinned_app_matches_a_running_group_whatever_the_case() {
        let item = app_item("Code", r"C:\Apps\Code.exe", 1, 0);
        let key = AppKey::Executable(r"c:\apps\code.exe".into());

        assert!(matches_app(&item, &key));
    }

    #[test]
    fn a_pinned_app_does_not_match_a_different_executable() {
        let item = app_item("Code", r"C:\Apps\Code.exe", 1, 0);
        let key = AppKey::Executable(r"C:\Apps\Other.exe".into());

        assert!(!matches_app(&item, &key));
    }

    #[test]
    fn a_pinned_packaged_app_matches_by_user_model_id() {
        let item = app_item(
            "Terminal",
            "Microsoft.WindowsTerminal_8wekyb3d8bbwe!App",
            1,
            0,
        );
        let key = AppKey::UserModelId("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App".to_owned());

        assert!(matches_app(&item, &key));
    }

    #[test]
    fn an_unpinned_slot_never_matches_a_running_group() {
        let key = AppKey::Executable(r"C:\Apps\Code.exe".into());

        assert!(!matches_app(&unpinned(1, 0), &key));
    }

    #[test]
    fn a_configured_icon_wins_over_the_automatic_one() {
        let mut item = app_item("Code", r"C:\Apps\Code.exe", 1, 0);
        item.icon = Some("icons/code.png".to_owned());

        assert_eq!(icon_source(&item).as_deref(), Some("icons/code.png"));
    }

    #[test]
    fn without_a_configured_icon_the_target_is_used() {
        let item = app_item("Code", r"C:\Apps\Code.exe", 1, 0);

        assert_eq!(icon_source(&item).as_deref(), Some(r"C:\Apps\Code.exe"));
    }

    #[test]
    fn an_unpinned_slot_has_no_icon_of_its_own() {
        assert_eq!(icon_source(&unpinned(1, 0)), None);
    }

    #[test]
    fn an_executable_is_labelled_by_its_file_stem() {
        let key = AppKey::Executable(r"C:\Apps\Code.exe".into());
        let path = Path::new(r"C:\Apps\Code.exe");

        assert_eq!(group_label(&group(&key, Some(path))), "Code");
    }

    /// The whole reason the executable is carried alongside the grouping key.
    #[test]
    fn a_packaged_app_prefers_its_executable_over_its_id() {
        let key = AppKey::UserModelId("MSEdge.UserData.Profile2".to_owned());
        let path = Path::new(r"C:\Program Files\Edge\msedge.exe");

        assert_eq!(group_label(&group(&key, Some(path))), "msedge");
    }

    #[test]
    fn without_an_executable_the_id_is_trimmed_at_the_bang() {
        let key = AppKey::UserModelId("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App".to_owned());

        assert_eq!(group_label(&group(&key, None)), "App");
    }

    #[test]
    fn a_packaged_app_icon_goes_through_the_apps_folder() {
        let key = AppKey::UserModelId("Some.App_abc!App".to_owned());

        assert_eq!(icon_path(&key), r"shell:AppsFolder\Some.App_abc!App");
    }
}
