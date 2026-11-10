//! GNOME on Wayland: evdev and uinput for input, the kile-wheel-9000 shell extension for
//! windows, monitors, the pointer, the clipboard and the overlay.

pub mod activate;
pub mod apps;
pub mod clipboard;
mod desktop;
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
mod shell;
pub mod tray;
mod uinput;
pub mod watcher;

pub use super::unix::{dialog, instance, process};
