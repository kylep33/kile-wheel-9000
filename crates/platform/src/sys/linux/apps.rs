//! The shell's own window list, grouped the way the dash groups it: by .desktop id.

use std::path::PathBuf;

use super::shell;
use crate::apps::{AppKey, AppsError, WindowId, WindowInfo};

/// What the extension reports per window: id, app id, app name, title, pid.
type WindowRow = (u64, String, String, String, u32);

/// The shell's "no window", since window ids start at one.
const NO_WINDOW: u64 = 0;

/// Windows the shell has no app for are keyed like this; there is nothing to launch.
const UNTRACKED_PREFIX: &str = "window:";

pub fn enumerate() -> Result<(Vec<WindowInfo>, Option<WindowId>), AppsError> {
    let rows: Vec<WindowRow> = shell::call("ListWindows", &()).map_err(AppsError)?;
    let focused: u64 = shell::call("FocusedWindow", &()).map_err(AppsError)?;

    let windows = rows.into_iter().map(window_info).collect();
    Ok((windows, (focused != NO_WINDOW).then_some(WindowId(focused))))
}

/// Only Windows reports single windows; GNOME always sends the whole list.
pub fn describe(id: WindowId) -> Option<WindowInfo> {
    let (windows, _) = enumerate().ok()?;
    windows.into_iter().find(|window| window.id == id)
}

pub fn is_listed(id: WindowId) -> bool {
    describe(id).is_some()
}

pub fn title(id: WindowId) -> String {
    describe(id).map(|window| window.title).unwrap_or_default()
}

pub fn launch_target(key: &AppKey) -> String {
    match key {
        AppKey::Id(id) => id.clone(),
        AppKey::Executable(path) => path.to_string_lossy().into_owned(),
    }
}

pub(super) fn is_untracked(id: &str) -> bool {
    id.starts_with(UNTRACKED_PREFIX)
}

fn window_info((id, app_id, name, title, pid): WindowRow) -> WindowInfo {
    WindowInfo {
        id: WindowId(id),
        key: AppKey::Id(app_id),
        executable: executable_of(pid),
        name: (!name.is_empty()).then_some(name),
        title,
    }
}

/// Unreadable for another user's process, which only costs the label its fallback.
fn executable_of(pid: u32) -> Option<PathBuf> {
    if pid == 0 {
        return None;
    }
    std::fs::read_link(format!("/proc/{pid}/exe")).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_keeps_the_app_name_and_drops_a_blank_one() {
        let named = window_info((7, "org.gnome.Nautilus.desktop".into(), "Files".into(), "Home".into(), 0));
        let unnamed = window_info((8, "window:xterm".into(), String::new(), "xterm".into(), 0));

        assert_eq!(named.name.as_deref(), Some("Files"));
        assert_eq!(named.key, AppKey::Id("org.gnome.Nautilus.desktop".into()));
        assert_eq!(unnamed.name, None);
    }

    #[test]
    fn our_own_executable_is_readable() {
        assert!(executable_of(std::process::id()).is_some());
    }

    #[test]
    fn a_desktop_id_launches_as_itself() {
        assert_eq!(
            launch_target(&AppKey::Id("firefox_firefox.desktop".into())),
            "firefox_firefox.desktop"
        );
        assert!(is_untracked("window:xterm"));
    }
}
