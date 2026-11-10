//! Product name and the config folder layout derived from it.

use std::path::{Path, PathBuf};

/// Rename the product here and in `app-common.ps1`.
pub const APP_NAME: &str = "kile-wheel-9000";

/// Shown in the tray, the settings window and installer output.
pub const DISPLAY_NAME: &str = "Kile Wheel 9000";

/// The settings window, installed beside the wheel.
#[cfg(windows)]
pub const SETTINGS_EXE: &str = "kile-wheel-9000-settings.exe";
#[cfg(not(windows))]
pub const SETTINGS_EXE: &str = "kile-wheel-9000-settings";

const CONFIG_FILE: &str = "config.toml";
const ICONS_DIR: &str = "icons";
const LOGS_DIR: &str = "logs";

/// Everything under `<config home>/<APP_NAME>/`.
#[derive(Debug, Clone)]
pub struct ConfigPaths {
    root: PathBuf,
}

impl ConfigPaths {
    /// `config_home` is the OS's per-user config folder, resolved by the platform crate:
    /// `%APPDATA%`, `~/.config` or `~/Library/Application Support`.
    pub fn new(config_home: &Path) -> Self {
        Self {
            root: config_home.join(APP_NAME),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config_file(&self) -> PathBuf {
        self.root.join(CONFIG_FILE)
    }

    pub fn icons_dir(&self) -> PathBuf {
        self.root.join(ICONS_DIR)
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join(LOGS_DIR)
    }

    /// Icon paths in the config are stored relative so the folder ports between machines.
    pub fn resolve_icon(&self, relative: &Path) -> PathBuf {
        self.root.join(relative)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_hang_off_the_app_folder() {
        let paths = ConfigPaths::new(Path::new("/home/someone/.config"));

        assert!(paths.root().ends_with(APP_NAME));
        assert_eq!(paths.config_file(), paths.root().join("config.toml"));
        assert_eq!(paths.icons_dir(), paths.root().join("icons"));
        assert_eq!(paths.logs_dir(), paths.root().join("logs"));
    }

    #[test]
    fn relative_icons_resolve_under_the_config_folder() {
        let paths = ConfigPaths::new(Path::new("/appdata"));

        assert_eq!(
            paths.resolve_icon(Path::new("icons/jira.png")),
            paths.root().join("icons/jira.png")
        );
    }
}
