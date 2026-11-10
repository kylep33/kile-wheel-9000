//! Win32.

pub mod activate;
pub mod apps;
pub mod clipboard;
pub mod dialog;
pub mod event_loop;
pub mod icons;
pub mod identity;
pub mod init;
pub mod input;
pub mod installed;
pub mod instance;
pub mod launch;
pub mod monitor;
pub mod overlay;
pub mod process;
pub mod send;
pub mod watcher;
mod window_class;
mod window_text;

pub use super::tray_icon as tray;
