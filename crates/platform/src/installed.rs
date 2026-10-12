//! The apps already pinned to the Windows taskbar, offered so a pinned slot can be chosen
//! from a list instead of having its path typed out.
//!
//! The shortcuts are handed on as-is rather than resolved to their targets. Explorer opens
//! a `.lnk` exactly as a double-click would, so resolving would only add a way to get the
//! answer wrong, and would lose the arguments and working directory the shortcut carries.

use kile_wheel_core::item::AppCandidate;
use std::path::{Path, PathBuf};
use tracing::debug;

use crate::known_folders;

/// Where Windows keeps taskbar pins. Still under the old Quick Launch path, which is why
/// it reads oddly.
const PIN_DIR: &str = r"Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar";

const SHORTCUT_EXTENSION: &str = "lnk";

/// Taskbar pins, by name. Empty when the folder is missing or unreadable, which is not an
/// error: it only means the picker falls back to a typed path.
pub fn taskbar_pins() -> Vec<AppCandidate> {
    let Some(directory) = pin_dir() else {
        return Vec::new();
    };

    let entries = match std::fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) => {
            debug!(
                "no taskbar pins to offer from {}: {error}",
                directory.display()
            );
            return Vec::new();
        }
    };

    let mut pins: Vec<AppCandidate> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| candidate_of(&entry.path()))
        .collect();

    pins.sort_by(|left, right| left.name.cmp(&right.name));
    pins
}

fn pin_dir() -> Option<PathBuf> {
    known_folders::roaming_app_data()
        .ok()
        .map(|roaming| roaming.join(PIN_DIR))
}

/// The shortcut's file name is its label, which is what the taskbar shows on hover.
fn candidate_of(path: &Path) -> Option<AppCandidate> {
    let extension = path.extension()?.to_str()?;
    if !extension.eq_ignore_ascii_case(SHORTCUT_EXTENSION) {
        return None;
    }

    Some(AppCandidate {
        name: path.file_stem()?.to_str()?.to_owned(),
        target: path.to_str()?.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shortcut_becomes_a_candidate_named_after_its_file() {
        let found = candidate_of(Path::new(r"C:\pins\Google Chrome.lnk"))
            .expect("a lnk file is a candidate");

        assert_eq!(found.name, "Google Chrome");
        assert_eq!(found.target, r"C:\pins\Google Chrome.lnk");
    }

    /// The folder holds the odd stray file, and a non-shortcut is not launchable the same
    /// way, so it is skipped rather than offered.
    #[test]
    fn anything_that_is_not_a_shortcut_is_skipped() {
        assert_eq!(candidate_of(Path::new(r"C:\pins\desktop.ini")), None);
        assert_eq!(candidate_of(Path::new(r"C:\pins\notes")), None);
    }

    #[test]
    fn the_extension_is_matched_whatever_its_case() {
        assert!(candidate_of(Path::new(r"C:\pins\Thing.LNK")).is_some());
    }

    /// Reading the real folder must never panic or throw, whatever is in it.
    #[test]
    fn listing_the_pins_is_safe_on_this_machine() {
        let pins = taskbar_pins();

        assert!(
            pins.iter().all(|pin| !pin.name.is_empty()),
            "a nameless pin would draw as a blank line"
        );
    }
}
