//! The notification area, menu bar or top bar icon, which is the only way to reach the
//! app without the hotkey.

use thiserror::Error;

use crate::event_loop::Waker;
use crate::sys;

/// Added to the tooltip while the wheel is off, so hovering says which it is.
const OFF_SUFFIX: &str = " (off)";

/// What the menu asks the app to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    OpenWheel,
    Settings,
    ReloadConfig,
    Quit,
    ToggleEnabled,
}

/// The menu top to bottom, `None` for a separator.
pub(crate) const MENU: [Option<Command>; 7] = [
    Some(Command::OpenWheel),
    Some(Command::Settings),
    None,
    Some(Command::ToggleEnabled),
    Some(Command::ReloadConfig),
    None,
    Some(Command::Quit),
];

impl Command {
    pub(crate) fn label(self, enabled: bool) -> &'static str {
        match self {
            Self::OpenWheel => "Open wheel",
            Self::Settings => "Settings",
            Self::ReloadConfig => "Reload config",
            Self::Quit => "Quit",
            Self::ToggleEnabled if enabled => "Disable",
            Self::ToggleEnabled => "Enable",
        }
    }

    /// Greyed out while the wheel is off, rather than clicking through to nothing.
    pub(crate) fn needs_enabled(self) -> bool {
        matches!(self, Self::OpenWheel)
    }

    /// Stable text ids for menus that key items by string.
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::OpenWheel => "open-wheel",
            Self::Settings => "settings",
            Self::ReloadConfig => "reload-config",
            Self::Quit => "quit",
            Self::ToggleEnabled => "toggle-enabled",
        }
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    pub(crate) fn from_id(id: &str) -> Option<Self> {
        MENU.into_iter()
            .flatten()
            .find(|command| command.id() == id)
    }
}

pub(crate) fn tooltip(name: &str, enabled: bool) -> String {
    match enabled {
        true => name.to_owned(),
        false => format!("{name}{OFF_SUFFIX}"),
    }
}

#[derive(Debug, Error)]
#[error("{0}")]
pub struct TrayError(pub(crate) String);

/// Removes the icon on drop.
#[derive(Debug)]
pub struct Tray {
    sys: sys::tray::Tray,
}

impl Tray {
    /// Menu choices arrive on the main loop as [`crate::event_loop::Notice::Tray`]. A
    /// plain click on the icon asks for the settings window.
    pub fn create(name: &str, icon_png: &[u8], waker: Waker) -> Result<Self, TrayError> {
        let icon = image::load_from_memory_with_format(icon_png, image::ImageFormat::Png)
            .map_err(|error| TrayError(format!("the bundled tray icon would not decode: {error}")))?
            .to_rgba8();

        Ok(Self {
            sys: sys::tray::Tray::create(name, icon, waker)?,
        })
    }

    /// Tells the icon what the app's state is, for the tooltip and the menu's toggle.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.sys.set_enabled(enabled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    #[test]
    fn every_command_round_trips_through_its_id() {
        for command in MENU.into_iter().flatten() {
            assert_eq!(Command::from_id(command.id()), Some(command));
        }
    }

    #[test]
    fn the_toggle_says_what_it_will_do() {
        assert_eq!(Command::ToggleEnabled.label(true), "Disable");
        assert_eq!(Command::ToggleEnabled.label(false), "Enable");
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    #[test]
    fn an_unknown_id_is_no_command() {
        assert_eq!(Command::from_id("nope"), None);
    }

    #[test]
    fn the_tooltip_says_when_the_wheel_is_off() {
        assert_eq!(tooltip("Wheel", true), "Wheel");
        assert_eq!(tooltip("Wheel", false), "Wheel (off)");
    }
}
