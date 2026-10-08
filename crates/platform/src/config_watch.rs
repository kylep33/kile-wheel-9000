//! Watching the config file, so a hand edit or a settings save lands without a restart.

use std::ffi::c_void;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

/// Stops watching on drop.
#[derive(Debug)]
pub struct ConfigWatcher {
    _watcher: RecommendedWatcher,
    changed: Arc<AtomicBool>,
}

impl ConfigWatcher {
    /// Watches the config's folder and posts `message` to `window` when the file changes.
    ///
    /// The folder is watched rather than the file, because the config is replaced by a
    /// temp file and a rename; a watch on the file itself would follow the old inode and
    /// go quiet after the first save.
    pub fn start(config_file: &Path, window: HWND, message: u32) -> notify::Result<Self> {
        let folder = config_file.parent().unwrap_or(Path::new(".")).to_path_buf();
        let name = config_file.file_name().map(std::ffi::OsString::from);

        let changed = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&changed);

        // HWND is not Send, so the raw value crosses to the watcher thread instead
        let target = window.0 as isize;

        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let Ok(event) = event else {
                    return;
                };
                if !touches(&event, name.as_deref()) {
                    return;
                }

                // several events per save are normal; one wake is enough for all of them
                if flag.swap(true, Ordering::AcqRel) {
                    return;
                }
                wake(target, message);
            })?;

        watcher.watch(&folder, RecursiveMode::NonRecursive)?;
        Ok(Self {
            _watcher: watcher,
            changed,
        })
    }

    /// Reports whether the config changed since the last call, and clears the flag.
    pub fn take_change(&self) -> bool {
        self.changed.swap(false, Ordering::AcqRel)
    }
}

fn touches(event: &notify::Event, name: Option<&std::ffi::OsStr>) -> bool {
    if matches!(event.kind, EventKind::Access(_)) {
        return false;
    }

    let Some(name) = name else {
        return true;
    };
    event
        .paths
        .iter()
        .any(|path| path.file_name() == Some(name))
}

fn wake(target: isize, message: u32) {
    if target == 0 {
        return;
    }
    let window = HWND(target as *mut c_void);

    // SAFETY: posting to a window that has gone away fails rather than misbehaves.
    let _ = unsafe { PostMessageW(Some(window), message, WPARAM(0), LPARAM(0)) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::path::PathBuf;

    fn event(kind: EventKind, path: &str) -> notify::Event {
        notify::Event {
            kind,
            paths: vec![PathBuf::from(path)],
            attrs: Default::default(),
        }
    }

    #[test]
    fn a_write_to_the_config_counts() {
        let event = event(
            EventKind::Modify(notify::event::ModifyKind::Any),
            r"C:\cfg\config.toml",
        );

        assert!(touches(&event, Some(OsStr::new("config.toml"))));
    }

    #[test]
    fn a_write_to_a_neighbour_does_not() {
        let event = event(
            EventKind::Modify(notify::event::ModifyKind::Any),
            r"C:\cfg\other.toml",
        );

        assert!(!touches(&event, Some(OsStr::new("config.toml"))));
    }

    /// Reading the file is not a change, and we read it ourselves on every reload.
    #[test]
    fn a_read_is_not_a_change() {
        let event = event(
            EventKind::Access(notify::event::AccessKind::Read),
            r"C:\cfg\config.toml",
        );

        assert!(!touches(&event, Some(OsStr::new("config.toml"))));
    }

    #[test]
    fn the_temp_file_rename_is_seen_as_a_config_change() {
        let event = event(
            EventKind::Create(notify::event::CreateKind::File),
            r"C:\cfg\config.toml",
        );

        assert!(touches(&event, Some(OsStr::new("config.toml"))));
    }
}
