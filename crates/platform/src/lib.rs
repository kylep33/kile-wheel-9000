//! OS plumbing for kile-wheel-9000.
//!
//! The modules here are the one API the wheel and the settings window use. Each calls
//! into a backend under `sys` for Windows, GNOME on Linux, or macOS, so OS differences
//! stay in this crate.

pub mod activate;
pub mod apps;
pub mod clipboard;
pub mod config_watch;
pub mod dialog;
pub mod event_loop;
pub mod geom;
pub mod icon_files;
pub mod icons;
pub mod identity;
pub mod input;
pub mod installed;
pub mod instance;
pub mod launch;
pub mod monitor;
pub mod os;
pub mod overlay;
pub mod paths;
pub mod process;
pub mod send;
pub mod snippet;
pub mod sources;
pub mod tray;
pub mod watcher;

mod sys;

/// Per-thread setup some backends need before anything else runs on the main thread:
/// COM on Windows. Call it first thing in `main`.
pub fn init_main_thread() -> Result<(), sys::init::InitError> {
    sys::init::main_thread()
}
