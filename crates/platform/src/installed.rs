//! Apps the desktop already has pinned, offered so a pinned slot can be chosen from a
//! list instead of having its target typed out: the taskbar pins on Windows, the dash
//! favourites on GNOME, the Applications folder on macOS.

use kile_wheel_core::item::AppCandidate;

use crate::sys;

/// Sorted by name. Empty when nothing could be read, which only means the picker falls
/// back to a typed target.
pub fn app_candidates() -> Vec<AppCandidate> {
    sys::installed::app_candidates()
}
