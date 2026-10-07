//! The things a wheel slot can hold.

use serde::Deserialize;

use crate::slot::{Slot, Tab};

/// Substituted with the script's path when the exec template is expanded.
pub const PATH_PLACEHOLDER: &str = "{path}";

/// Ready-made exec templates offered by the settings window. Still freely editable.
pub const SCRIPT_PRESETS: &[(&str, &str)] = &[
    (
        "Git Bash in Windows Terminal",
        r#"wt.exe -- "C:\Program Files\Git\bin\bash.exe" "{path}""#,
    ),
    ("PowerShell 7", r#"pwsh.exe -NoExit -File "{path}""#),
    (
        "Windows PowerShell",
        r#"powershell.exe -NoExit -File "{path}""#,
    ),
    ("Command Prompt", r#"cmd.exe /k "{path}""#),
    (
        "WSL in Windows Terminal",
        r#"wt.exe wsl.exe -- bash "{path}""#,
    ),
];

/// How a snippet reaches the focused window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SnippetMode {
    /// Save the clipboard, set the snippet, send Ctrl+V, put the clipboard back.
    #[default]
    Clipboard,
    /// Send the text as keystrokes.
    Type,
}

/// What activating a slot does. The tag matches the `type` key in the config.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ItemKind {
    /// Behaves like a taskbar button: launch, focus, minimize, or offer the windows.
    App {
        target: String,
    },

    Script {
        path: String,
        exec: String,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        hidden: bool,
    },

    Url {
        url: String,
        #[serde(default)]
        browser: Option<String>,
        #[serde(default)]
        browser_args: Option<String>,
    },

    Snippet {
        text: String,
        #[serde(default)]
        mode: SnippetMode,
    },

    Folder {
        path: String,
    },

    Terminal {
        #[serde(default)]
        profile: Option<String>,
        #[serde(default)]
        distro: Option<String>,
        #[serde(default)]
        cwd: Option<String>,
    },
}

impl ItemKind {
    /// Name shown in the settings window's type picker.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::App { .. } => "App",
            Self::Script { .. } => "Script",
            Self::Url { .. } => "URL",
            Self::Snippet { .. } => "Snippet",
            Self::Folder { .. } => "Folder",
            Self::Terminal { .. } => "Terminal",
        }
    }

    /// The path an automatic icon is extracted from, when there is one.
    pub fn icon_source(&self) -> Option<&str> {
        match self {
            Self::App { target } => Some(target),
            Self::Script { path, .. } | Self::Folder { path } => Some(path),
            Self::Url { browser, .. } => browser.as_deref(),
            Self::Snippet { .. } | Self::Terminal { .. } => None,
        }
    }
}

/// One pinned entry from the config.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Item {
    pub name: String,

    /// Relative to the config folder, so the folder ports between machines.
    #[serde(default)]
    pub icon: Option<String>,

    /// Tab number as written in the config, counting from 1.
    pub tab: usize,

    /// Slot index, 0 at north going clockwise.
    pub slot: usize,

    #[serde(flatten)]
    pub kind: ItemKind,
}

impl Item {
    /// `None` when the config names a tab or slot that does not exist.
    pub fn position(&self) -> Option<Position> {
        Some(Position {
            tab: Tab::from_number(self.tab)?,
            slot: Slot::new(self.slot)?,
        })
    }
}

/// Where something sits on the wheel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position {
    pub tab: Tab,
    pub slot: Slot,
}

impl Position {
    pub fn new(tab: Tab, slot: Slot) -> Self {
        Self { tab, slot }
    }
}

/// Expands a script item's exec template.
pub fn expand_exec(exec: &str, path: &str) -> String {
    exec.replace(PATH_PLACEHOLDER, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_path_placeholder_is_substituted_everywhere_it_appears() {
        let expanded = expand_exec(r#"wt.exe -d "{path}" bash "{path}""#, r"C:\s\go.sh");

        assert_eq!(expanded, r#"wt.exe -d "C:\s\go.sh" bash "C:\s\go.sh""#);
    }

    #[test]
    fn every_preset_mentions_the_placeholder() {
        for (name, exec) in SCRIPT_PRESETS {
            assert!(
                exec.contains(PATH_PLACEHOLDER),
                "preset \"{name}\" would ignore the script path"
            );
        }
    }

    #[test]
    fn a_position_outside_the_wheel_is_rejected() {
        let item = Item {
            name: "nope".to_owned(),
            icon: None,
            tab: 10,
            slot: 0,
            kind: ItemKind::Folder {
                path: "C:\\".to_owned(),
            },
        };

        assert_eq!(item.position(), None);
    }

    #[test]
    fn tab_one_slot_zero_is_the_first_position() {
        let item = Item {
            name: "first".to_owned(),
            icon: None,
            tab: 1,
            slot: 0,
            kind: ItemKind::Folder {
                path: "C:\\".to_owned(),
            },
        };

        assert_eq!(
            item.position(),
            Some(Position::new(
                Tab::FIRST,
                Slot::new(0).expect("slot 0 exists")
            ))
        );
    }
}
