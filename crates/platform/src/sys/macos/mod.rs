//! macOS: an event tap for input, Accessibility for windows, a non-activating panel for
//! the overlay.
//!
//! Input and window control both need the Accessibility permission, granted in System
//! Settings, Privacy & Security, Accessibility.

pub mod activate;
pub mod apps;
mod ax;
pub mod clipboard;
pub mod event_loop;
pub mod icons;
pub mod identity;
pub mod init;
pub mod input;
pub mod installed;
mod keymap;
pub mod launch;
pub mod monitor;
pub mod overlay;
pub mod send;
pub mod watcher;

pub use super::tray_icon as tray;
pub use super::unix::{dialog, instance, process};

/// Where to grant the one permission everything here needs.
const ACCESSIBILITY_HINT: &str = "grant kile-wheel-9000 Accessibility access in System Settings, Privacy & Security, Accessibility, then start it again";
