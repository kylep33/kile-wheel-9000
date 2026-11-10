//! One backend per desktop. Each provides the same functions and types under the same
//! module names, which the neutral modules in the crate root call into.

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub mod tray_icon;

#[cfg(unix)]
pub mod unix;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use self::windows::*;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use self::linux::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use self::macos::*;

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
compile_error!("kile-wheel-9000 supports Windows, Linux (GNOME) and macOS only");
