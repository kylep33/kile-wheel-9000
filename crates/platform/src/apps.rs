//! The set of windows the taskbar or dock would show, grouped by app.
//!
//! Built once at startup and kept current by the watcher, so opening the wheel never
//! enumerates.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::sys;
use crate::watcher::WindowEvent;

/// A window, as the OS names it. Only meaningful to the backend that handed it out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowId(pub u64);

/// How windows are grouped into one wheel slot, matching the taskbar or dock.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AppKey {
    /// What the desktop groups by: an AppUserModelID on Windows, a .desktop id on GNOME,
    /// a bundle id on macOS.
    Id(String),
    /// Executable path, for anything the desktop has no id for.
    Executable(PathBuf),
}

/// One window as the backend reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    pub id: WindowId,
    pub key: AppKey,
    /// Path of the owning executable, even when grouping went by id. Used for labels and
    /// the hidden-apps list.
    pub executable: Option<PathBuf>,
    /// The app's own display name, where the desktop knows it.
    pub name: Option<String>,
    pub title: String,
}

/// One taskbar-visible window.
#[derive(Debug, Clone, Copy)]
pub struct WindowRef<'a> {
    pub id: WindowId,
    pub title: &'a str,
}

/// An app and its open windows, oldest first.
#[derive(Debug, Clone)]
pub struct AppGroup<'a> {
    pub key: &'a AppKey,
    pub executable: Option<&'a Path>,
    pub name: Option<&'a str>,
    pub windows: Vec<WindowRef<'a>>,
}

#[derive(Debug, Error)]
#[error("{0}")]
pub struct AppsError(pub(crate) String);

#[derive(Debug)]
struct TrackedWindow {
    info: WindowInfo,
    /// Discovery order, which is what keeps group and window order stable.
    sequence: u64,
}

/// Taskbar-visible windows and which one has focus.
#[derive(Debug, Default)]
pub struct WindowModel {
    tracked: HashMap<WindowId, TrackedWindow>,
    focused: Option<WindowId>,
    next_sequence: u64,
}

impl WindowModel {
    /// One full sweep. Called at startup only.
    pub fn rebuild(&mut self) -> Result<(), AppsError> {
        let (windows, focused) = sys::apps::enumerate()?;
        self.replace(windows, focused);
        Ok(())
    }

    pub fn apply(&mut self, event: WindowEvent) {
        match event {
            WindowEvent::Changed(id) => self.refresh(id),
            WindowEvent::Vanished(id) => self.remove(id),
            WindowEvent::Focused(id) => self.set_focused(id),
            WindowEvent::Snapshot { windows, focused } => self.replace(windows, focused),
        }
    }

    /// Takes a whole new list, keeping each known window's place in the order.
    fn replace(&mut self, windows: Vec<WindowInfo>, focused: Option<WindowId>) {
        let mut kept = HashMap::with_capacity(windows.len());
        for info in windows {
            let sequence = match self.tracked.get(&info.id) {
                Some(known) => known.sequence,
                None => self.take_sequence(),
            };
            kept.insert(info.id, TrackedWindow { info, sequence });
        }

        self.tracked = kept;
        self.focused = focused.filter(|id| self.tracked.contains_key(id));
    }

    /// Re-checks one window, since one can become taskbar-worthy after it is created,
    /// which is how packaged apps appear.
    fn refresh(&mut self, id: WindowId) {
        if let Some(tracked) = self.tracked.get_mut(&id) {
            match sys::apps::is_listed(id) {
                true => tracked.info.title = sys::apps::title(id),
                false => self.remove(id),
            }
            return;
        }

        if let Some(info) = sys::apps::describe(id) {
            let sequence = self.take_sequence();
            self.tracked.insert(id, TrackedWindow { info, sequence });
        }
    }

    fn remove(&mut self, id: WindowId) {
        self.tracked.remove(&id);
        if self.focused == Some(id) {
            self.focused = None;
        }
    }

    fn set_focused(&mut self, id: WindowId) {
        self.refresh(id);
        self.focused = self.tracked.contains_key(&id).then_some(id);
    }

    fn take_sequence(&mut self) -> u64 {
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        sequence
    }

    /// Groups ordered by when each app's first window appeared, so a close elsewhere
    /// never reorders them.
    pub fn groups(&self) -> Vec<AppGroup<'_>> {
        let mut by_key: HashMap<&AppKey, Vec<&TrackedWindow>> = HashMap::new();
        for tracked in self.tracked.values() {
            by_key.entry(&tracked.info.key).or_default().push(tracked);
        }

        let mut ordered: Vec<(u64, AppGroup<'_>)> = by_key
            .into_iter()
            .map(|(key, windows)| build_group(key, windows))
            .collect();

        ordered.sort_by_key(|(first_seen, _)| *first_seen);
        ordered.into_iter().map(|(_, group)| group).collect()
    }

    pub fn focused_key(&self) -> Option<&AppKey> {
        let focused = self.focused?;
        self.tracked.get(&focused).map(|tracked| &tracked.info.key)
    }

    pub fn focused_window(&self) -> Option<WindowId> {
        self.focused
    }

    pub fn title_of(&self, id: WindowId) -> Option<&str> {
        self.tracked.get(&id).map(|tracked| tracked.info.title.as_str())
    }
}

fn build_group<'a>(key: &'a AppKey, mut windows: Vec<&'a TrackedWindow>) -> (u64, AppGroup<'a>) {
    windows.sort_by_key(|tracked| tracked.sequence);

    let first = windows
        .first()
        .expect("a group is only built from at least one window");
    let first_seen = first.sequence;

    let group = AppGroup {
        key,
        executable: first.info.executable.as_deref(),
        name: first.info.name.as_deref(),
        windows: windows
            .into_iter()
            .map(|tracked| WindowRef {
                id: tracked.info.id,
                title: &tracked.info.title,
            })
            .collect(),
    };
    (first_seen, group)
}

/// What the desktop launches a running app by, for pinning it or opening another window.
pub fn launch_target(key: &AppKey) -> String {
    sys::apps::launch_target(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(id: u64, app: &str) -> WindowInfo {
        WindowInfo {
            id: WindowId(id),
            key: AppKey::Id(app.to_owned()),
            executable: None,
            name: None,
            title: format!("window {id}"),
        }
    }

    #[test]
    fn a_snapshot_groups_windows_by_app() {
        let mut model = WindowModel::default();
        model.apply(WindowEvent::Snapshot {
            windows: vec![window(1, "a"), window(2, "b"), window(3, "a")],
            focused: Some(WindowId(2)),
        });

        let groups = model.groups();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].windows.len(), 2);
        assert_eq!(model.focused_key(), Some(&AppKey::Id("b".to_owned())));
    }

    /// A later snapshot must not shuffle the slots apps already sit in.
    #[test]
    fn a_new_snapshot_keeps_the_order_apps_appeared_in() {
        let mut model = WindowModel::default();
        model.apply(WindowEvent::Snapshot {
            windows: vec![window(1, "a"), window(2, "b")],
            focused: None,
        });
        model.apply(WindowEvent::Snapshot {
            windows: vec![window(3, "c"), window(2, "b"), window(1, "a")],
            focused: None,
        });

        let order: Vec<&AppKey> = model.groups().iter().map(|group| group.key).collect();
        assert_eq!(
            order,
            [
                &AppKey::Id("a".to_owned()),
                &AppKey::Id("b".to_owned()),
                &AppKey::Id("c".to_owned())
            ]
        );
    }

    #[test]
    fn focus_on_a_window_that_is_gone_is_dropped() {
        let mut model = WindowModel::default();
        model.apply(WindowEvent::Snapshot {
            windows: vec![window(1, "a")],
            focused: Some(WindowId(9)),
        });

        assert_eq!(model.focused_window(), None);
    }
}
