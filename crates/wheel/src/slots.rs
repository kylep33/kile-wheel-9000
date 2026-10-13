//! What each wheel slot holds.
//!
//! Nothing appears unless the config puts it there. Running apps only land in slots
//! marked as unpinned app slots, so a blank slot stays blank.

use std::path::Path;

use kile_wheel_core::config::Config;
use kile_wheel_core::item::{Item, ItemKind, Position};
use kile_wheel_core::layout::SlotMemory;
use kile_wheel_core::slot::{Slot, Tab};
#[cfg(test)]
use kile_wheel_platform::apps::WindowRef;
use kile_wheel_platform::apps::{AppGroup, AppKey, WindowModel};
use kile_wheel_platform::identity::{AppIdentity, IdentityCache};
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

    /// What starts another window of this slot's app. `None` for anything that is not an
    /// app, since a script or a url has no windows to add to.
    pub fn new_window(&self) -> Option<NewWindow> {
        let launch = match &self.target {
            SlotTarget::Pinned(item) if matches!(item.kind, ItemKind::App { .. }) => {
                item.kind.clone()
            }
            SlotTarget::Running(key) => ItemKind::App {
                target: launch_target(key),
            },
            SlotTarget::Pinned(_) | SlotTarget::Waiting => return None,
        };

        Some(NewWindow {
            label: format!("New {} window", self.label),
            icon_source: self.icon_source.clone(),
            launch,
        })
    }
}

/// The sub-wheel entry that opens one more window of the app, as the taskbar's jump list
/// does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewWindow {
    pub label: String,
    pub icon_source: Option<String>,
    pub launch: ItemKind,
}

/// What the shell launches a running app by.
///
/// Packaged apps go through the Apps folder, because their executable path is not what
/// starts them.
pub fn launch_target(key: &AppKey) -> String {
    match key {
        AppKey::Executable(path) => path.to_string_lossy().into_owned(),
        AppKey::UserModelId(id) => format!(r"shell:AppsFolder\{id}"),
    }
}

/// Every tab's slots, resolved for one open.
#[derive(Debug, Default)]
pub struct Layout {
    tabs: Vec<Vec<Option<SlotEntry>>>,
    overflow: usize,
}

impl Layout {
    pub fn build(
        config: &Config,
        model: &WindowModel,
        memory: &mut SlotMemory<AppKey>,
        identities: &mut IdentityCache,
    ) -> Self {
        let pinned: Vec<(Position, &Item, Option<AppIdentity>)> = config
            .pinned()
            .into_iter()
            .map(|(position, item)| (position, item, identity_of(item, identities)))
            .collect();
        let groups = model.groups();
        let focused = model.focused_key();

        // a running app that is already pinned shows in its pinned slot, so it must not
        // also claim an unpinned one
        let open_slots = unpinned_slots(config);
        let spare: Vec<AppKey> = groups
            .iter()
            .filter(|group| {
                !pinned
                    .iter()
                    .filter_map(|(_, _, identity)| identity.as_ref())
                    .any(|identity| claims(identity, group))
            })
            .map(|group| group.key.clone())
            .collect();

        let overflow = memory.reconcile(&spare, &open_slots);

        // every tab is allocated, not just the ones the config mentions, so switching to an
        // untouched tab lands on a blank wheel instead of doing nothing
        let mut tabs = vec![vec![None; Slot::COUNT]; Tab::MAX_COUNT];

        for (position, item, identity) in &pinned {
            let position = *position;
            let entry = match item.kind.is_unpinned() {
                true => unpinned_entry(position, memory, &groups, focused),
                false => pinned_entry(item, identity.as_ref(), &groups, focused),
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
///
/// Every matching group contributes, because one pin can stand for several: Chrome keys
/// each profile's windows by its own id, while its pin names chrome.exe.
fn pinned_entry(
    item: &Item,
    identity: Option<&AppIdentity>,
    groups: &[AppGroup<'_>],
    focused: Option<&AppKey>,
) -> SlotEntry {
    let owned: Vec<&AppGroup<'_>> = match identity {
        Some(identity) => groups
            .iter()
            .filter(|group| claims(identity, group))
            .collect(),
        None => Vec::new(),
    };

    SlotEntry {
        label: item.name.clone(),
        icon_source: icon_source(item),
        kind: item.kind.clone(),
        target: SlotTarget::Pinned(item.clone()),
        windows: owned
            .iter()
            .flat_map(|group| group.windows.iter().map(|window| window.handle))
            .collect(),
        focused: owned.iter().any(|group| focused == Some(group.key)),
    }
}

/// Only an app item stands for running windows; a script or a url never does.
fn identity_of(item: &Item, identities: &mut IdentityCache) -> Option<AppIdentity> {
    let ItemKind::App { target } = &item.kind else {
        return None;
    };
    Some(identities.identity(target).clone())
}

fn claims(identity: &AppIdentity, group: &AppGroup<'_>) -> bool {
    identity.matches(group.key, group.executable)
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
    launch_target(key)
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

    fn window(handle: isize) -> WindowRef<'static> {
        WindowRef {
            handle: HWND(handle as *mut _),
            title: "",
        }
    }

    fn running<'a>(
        key: &'a AppKey,
        executable: Option<&'a Path>,
        handles: &[isize],
    ) -> AppGroup<'a> {
        AppGroup {
            key,
            executable,
            windows: handles.iter().map(|handle| window(*handle)).collect(),
        }
    }

    fn identity(item: &Item) -> Option<AppIdentity> {
        let ItemKind::App { target } = &item.kind else {
            return None;
        };
        Some(AppIdentity::unresolved(target))
    }

    #[test]
    fn a_pinned_app_claims_its_running_group_whatever_the_case() {
        let item = app_item("Code", r"C:\Apps\Code.exe", 1, 0);
        let key = AppKey::Executable(r"c:\apps\code.exe".into());

        let identity = identity(&item).expect("an app has an identity");
        assert!(claims(&identity, &group(&key, None)));
    }

    #[test]
    fn a_pinned_app_does_not_claim_a_different_executable() {
        let item = app_item("Code", r"C:\Apps\Code.exe", 1, 0);
        let key = AppKey::Executable(r"C:\Apps\Other.exe".into());

        let identity = identity(&item).expect("an app has an identity");
        assert!(!claims(&identity, &group(&key, None)));
    }

    #[test]
    fn a_pinned_packaged_app_claims_its_group_by_user_model_id() {
        let item = app_item(
            "Terminal",
            "Microsoft.WindowsTerminal_8wekyb3d8bbwe!App",
            1,
            0,
        );
        let key = AppKey::UserModelId("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App".to_owned());

        let identity = identity(&item).expect("an app has an identity");
        assert!(claims(&identity, &group(&key, None)));
    }

    #[test]
    fn an_unpinned_slot_stands_for_no_running_app() {
        let mut identities = IdentityCache::default();

        assert_eq!(identity_of(&unpinned(1, 0), &mut identities), None);
    }

    /// The pinned slot is where an already-pinned app's windows go, so selecting it offers
    /// them instead of starting yet another copy.
    #[test]
    fn a_pinned_app_collects_its_running_windows() {
        let item = app_item("Code", r"C:\Apps\Code.exe", 1, 0);
        let code = AppKey::Executable(r"C:\Apps\Code.exe".into());
        let other = AppKey::Executable(r"C:\Apps\Other.exe".into());
        let groups = [running(&code, None, &[1, 2]), running(&other, None, &[3])];

        let entry = pinned_entry(&item, identity(&item).as_ref(), &groups, Some(&code));

        assert_eq!(entry.windows, [window(1).handle, window(2).handle]);
        assert!(entry.focused);
    }

    /// One pin can stand for several groups, as Chrome does with a group per profile.
    #[test]
    fn a_pinned_app_collects_windows_from_every_group_it_claims() {
        let item = app_item("Chrome", r"C:\Chrome\chrome.exe", 1, 0);
        let exe = Path::new(r"C:\Chrome\chrome.exe");
        let first = AppKey::UserModelId("Chrome".to_owned());
        let second = AppKey::UserModelId("Chrome.UserData.Profile2".to_owned());
        let groups = [
            running(&first, Some(exe), &[1]),
            running(&second, Some(exe), &[2]),
        ];

        let entry = pinned_entry(&item, identity(&item).as_ref(), &groups, None);

        assert_eq!(entry.windows.len(), 2);
    }

    #[test]
    fn a_pinned_app_that_is_not_running_has_no_windows() {
        let item = app_item("Code", r"C:\Apps\Code.exe", 1, 0);
        let other = AppKey::Executable(r"C:\Apps\Other.exe".into());
        let groups = [running(&other, None, &[3])];

        let entry = pinned_entry(&item, identity(&item).as_ref(), &groups, None);

        assert!(entry.windows.is_empty());
        assert!(!entry.is_running());
    }

    #[test]
    fn a_pinned_app_offers_a_new_window_through_its_own_target() {
        let item = app_item("Kiro", r"C:\pins\Kiro.lnk", 1, 0);
        let entry = pinned_entry(&item, identity(&item).as_ref(), &[], None);

        let new = entry.new_window().expect("an app can open another window");
        assert_eq!(new.label, "New Kiro window");
        assert_eq!(new.launch, item.kind);
    }

    #[test]
    fn a_running_app_offers_a_new_window_through_its_launch_target() {
        let key = AppKey::UserModelId("Some.App_abc!App".to_owned());
        let entry = unpinned_entry(
            Position {
                tab: Tab::FIRST,
                slot: Slot::new(0).expect("slot 0"),
            },
            &SlotMemory::default(),
            &[],
            None,
        );
        assert_eq!(
            entry.new_window(),
            None,
            "an empty slot has nothing to launch"
        );

        let entry = SlotEntry {
            label: "App".to_owned(),
            target: SlotTarget::Running(key),
            ..entry
        };
        let new = entry
            .new_window()
            .expect("a running app can open another window");
        assert_eq!(
            new.launch,
            ItemKind::App {
                target: r"shell:AppsFolder\Some.App_abc!App".to_owned()
            }
        );
    }

    /// Only apps have windows to add to; a script or a url would just run again.
    #[test]
    fn anything_that_is_not_an_app_offers_no_new_window() {
        let item = Item {
            name: "Jira".to_owned(),
            icon: None,
            tab: 1,
            slot: 0,
            kind: ItemKind::Url {
                url: "https://example.com".to_owned(),
                browser: None,
                browser_args: None,
            },
        };
        let entry = pinned_entry(&item, None, &[], None);

        assert_eq!(entry.new_window(), None);
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
