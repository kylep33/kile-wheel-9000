//! What each wheel slot holds, and how running apps fill the gaps around pinned items.

use std::collections::HashSet;
use std::path::Path;

use kile_wheel_core::config::Config;
use kile_wheel_core::item::{Item, ItemKind, Position};
use kile_wheel_core::layout::{self, SlotMemory};
use kile_wheel_core::slot::{Slot, Tab};
use kile_wheel_platform::apps::{AppGroup, AppKey, WindowModel};
use kile_wheel_platform::paths::expand;
use windows::Win32::Foundation::HWND;

/// Shown in the last slot of the last tab when everything else is full.
pub const MORE_LABEL: &str = "More...";

/// What activating a slot does.
#[derive(Debug, Clone)]
pub enum SlotTarget {
    Pinned(Item),
    Running(AppKey),
    /// Everything that did not fit anywhere.
    More,
}

/// One filled slot.
#[derive(Debug, Clone)]
pub struct SlotEntry {
    pub label: String,
    /// What the icon is extracted from, which doubles as its cache key.
    pub icon_source: Option<String>,
    pub target: SlotTarget,
    pub windows: Vec<HWND>,
    pub focused: bool,
    pub pinned: bool,
}

impl SlotEntry {
    pub fn is_running(&self) -> bool {
        !self.windows.is_empty()
    }
}

/// Every tab's slots, resolved for one open.
#[derive(Debug, Default)]
pub struct Layout {
    tabs: Vec<Vec<Option<SlotEntry>>>,
}

impl Layout {
    /// Pinned items take their configured slots; running apps fill what is left, keeping
    /// whatever slot they already held.
    pub fn build(config: &Config, model: &WindowModel, memory: &mut SlotMemory<AppKey>) -> Self {
        let pinned = config.pinned();
        let groups = model.groups();
        let focused = model.focused_key();

        let unpinned: Vec<AppKey> = groups
            .iter()
            .filter(|group| !pinned.values().any(|item| matches_app(item, group.key)))
            .map(|group| group.key.clone())
            .collect();

        let tab_count = needed_tabs(config, pinned.len(), unpinned.len());
        let reserved: HashSet<Position> = pinned.keys().copied().collect();
        let overflowed = memory.reconcile(&unpinned, &reserved, tab_count);

        let mut tabs = vec![vec![None; Slot::COUNT]; tab_count];

        for position in layout::positions(tab_count) {
            let entry = if let Some(item) = pinned.get(&position) {
                Some(pinned_entry(item, model, focused))
            } else if let Some(key) = memory.at(position) {
                groups
                    .iter()
                    .find(|group| group.key == key)
                    .map(|group| running_entry(group, focused))
            } else {
                None
            };

            if let Some(entry) = entry {
                tabs[position.tab.index()][position.slot.index()] = Some(entry);
            }
        }

        if !overflowed.is_empty() {
            place_more(&mut tabs, tab_count);
        }

        Self { tabs }
    }

    pub fn get(&self, tab: Tab, slot: Slot) -> Option<&SlotEntry> {
        self.tabs.get(tab.index())?.get(slot.index())?.as_ref()
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }
}

/// Enough tabs for everything, capped at what the number keys can reach.
///
/// Pinned items can sit anywhere, so this counts them rather than packing them, which
/// errs on the side of one extra tab rather than hiding an app.
fn needed_tabs(config: &Config, pinned: usize, unpinned: usize) -> usize {
    let wanted = (pinned + unpinned).div_ceil(Slot::COUNT).max(1);

    config.tab_count().max(wanted).min(Tab::MAX_COUNT)
}

fn place_more(tabs: &mut [Vec<Option<SlotEntry>>], tab_count: usize) {
    let Some(position) = layout::overflow_position(tab_count) else {
        return;
    };

    tabs[position.tab.index()][position.slot.index()] = Some(SlotEntry {
        label: MORE_LABEL.to_owned(),
        icon_source: None,
        target: SlotTarget::More,
        windows: Vec::new(),
        focused: false,
        pinned: false,
    });
}

/// A pinned app shows the windows it already has open, so it behaves like the taskbar
/// rather than appearing twice.
fn pinned_entry(item: &Item, model: &WindowModel, focused: Option<&AppKey>) -> SlotEntry {
    let group = model
        .groups()
        .into_iter()
        .find(|group| matches_app(item, group.key));

    let windows = group
        .as_ref()
        .map(|group| group.windows.iter().map(|window| window.handle).collect())
        .unwrap_or_default();
    let is_focused = group
        .as_ref()
        .is_some_and(|group| Some(group.key) == focused);

    SlotEntry {
        label: item.name.clone(),
        icon_source: icon_source(item),
        target: SlotTarget::Pinned(item.clone()),
        windows,
        focused: is_focused,
        pinned: true,
    }
}

fn running_entry(group: &AppGroup<'_>, focused: Option<&AppKey>) -> SlotEntry {
    SlotEntry {
        label: group_label(group),
        icon_source: Some(icon_path(group.key)),
        target: SlotTarget::Running(group.key.clone()),
        windows: group.windows.iter().map(|window| window.handle).collect(),
        focused: focused == Some(group.key),
        pinned: false,
    }
}

/// The icon comes from the item's own target, or from a user-supplied image when one is
/// configured. The config path is relative to the config folder, so the caller resolves
/// it before handing it over.
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
    use kile_wheel_core::config::Config;

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

    fn group<'a>(key: &'a AppKey, executable: Option<&'a Path>) -> AppGroup<'a> {
        AppGroup {
            key,
            executable,
            windows: Vec::new(),
        }
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
    fn a_non_app_item_never_matches_a_running_group() {
        let item = Item {
            name: "Projects".to_owned(),
            icon: None,
            tab: 1,
            slot: 0,
            kind: ItemKind::Folder {
                path: r"C:\p".to_owned(),
            },
        };
        let key = AppKey::Executable(r"C:\p".into());

        assert!(!matches_app(&item, &key));
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
    fn a_snippet_has_no_icon_to_extract() {
        let item = Item {
            name: "Sig".to_owned(),
            icon: None,
            tab: 1,
            slot: 0,
            kind: ItemKind::Snippet {
                text: "hi".to_owned(),
                mode: Default::default(),
            },
        };

        assert_eq!(icon_source(&item), None);
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

    #[test]
    fn one_tab_is_enough_for_an_empty_config() {
        assert_eq!(needed_tabs(&Config::default(), 0, 0), 1);
    }

    #[test]
    fn more_apps_than_one_tab_holds_grows_the_tab_count() {
        assert_eq!(needed_tabs(&Config::default(), 0, Slot::COUNT + 1), 2);
    }

    #[test]
    fn the_tab_count_never_passes_the_number_keys() {
        let crowded = Slot::COUNT * (Tab::MAX_COUNT + 4);

        assert_eq!(needed_tabs(&Config::default(), 0, crowded), Tab::MAX_COUNT);
    }
}
