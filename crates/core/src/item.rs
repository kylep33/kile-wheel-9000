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
    /// A slot held open for whatever happens to be running. The wheel fills it, and the
    /// app it picks keeps the slot until that app closes.
    UnpinnedApp,

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
    /// Every type, in the order the wheel and the settings window offer them.
    pub const NAMES: [&'static str; 7] = [
        "Unpinned app slot",
        "Pinned app",
        "Script",
        "URL",
        "Snippet",
        "Folder",
        "Terminal",
    ];

    /// Name shown wherever a type is picked or listed.
    pub fn type_name(&self) -> &'static str {
        Self::NAMES[self.type_index()]
    }

    /// Position in [`Self::NAMES`], which is also the picker order.
    pub fn type_index(&self) -> usize {
        match self {
            Self::UnpinnedApp => 0,
            Self::App { .. } => 1,
            Self::Script { .. } => 2,
            Self::Url { .. } => 3,
            Self::Snippet { .. } => 4,
            Self::Folder { .. } => 5,
            Self::Terminal { .. } => 6,
        }
    }

    /// An empty value of the type at `type_index`, for a freshly picked slot.
    pub fn blank(type_index: usize) -> Option<Self> {
        let blank = match type_index {
            0 => Self::UnpinnedApp,
            1 => Self::App {
                target: String::new(),
            },
            2 => Self::Script {
                path: String::new(),
                exec: SCRIPT_PRESETS[0].1.to_owned(),
                cwd: None,
                hidden: false,
            },
            3 => Self::Url {
                url: String::new(),
                browser: None,
                browser_args: None,
            },
            4 => Self::Snippet {
                text: String::new(),
                mode: SnippetMode::Clipboard,
            },
            5 => Self::Folder {
                path: String::new(),
            },
            6 => Self::Terminal {
                profile: None,
                distro: None,
                cwd: None,
            },
            _ => return None,
        };
        Some(blank)
    }

    /// Whether the wheel fills this slot rather than the config.
    pub fn is_unpinned(&self) -> bool {
        matches!(self, Self::UnpinnedApp)
    }

    /// Whether this is a pinned app, which is the one type the app picker can offer.
    pub fn is_app(&self) -> bool {
        matches!(self, Self::App { .. })
    }

    /// A pinned app with nothing chosen yet, for the typed-path route.
    pub fn blank_app() -> Self {
        Self::App {
            target: String::new(),
        }
    }

    /// The path an automatic icon is extracted from, when there is one.
    pub fn icon_source(&self) -> Option<&str> {
        match self {
            Self::App { target } => Some(target),
            Self::Script { path, .. } | Self::Folder { path } => Some(path),
            Self::Url { browser, .. } => browser.as_deref(),
            Self::UnpinnedApp | Self::Snippet { .. } | Self::Terminal { .. } => None,
        }
    }
}

/// An app the picker can offer, so a pinned app can be chosen from a list instead of
/// having its path typed out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppCandidate {
    pub name: String,
    /// Whatever the shell launches it by: a shortcut, an executable, or a shell item.
    pub target: String,
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
            // one past the last tab, written from the constant so it cannot go stale
            tab: Tab::MAX_COUNT + 1,
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
