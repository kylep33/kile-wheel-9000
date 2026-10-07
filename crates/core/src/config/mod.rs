//! The config file: its shape, its shipped default, and how edits go back in.
//!
//! Reads go through serde. Writes go through `toml_edit` against the existing document,
//! so hand-written comments and ordering survive a settings-window edit.

mod document;

pub use document::ConfigDocument;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

use crate::binding::{Action, Binding, BindingTable, Hotkey, ParseBindingError};
use crate::item::{Item, Position};
use crate::slot::{Slot, Tab};

/// Written on first run. Commented, because it doubles as the documentation.
pub const DEFAULT_CONFIG: &str = include_str!("default.toml");

/// Accent colour used when the config does not name a readable one.
const FALLBACK_ACCENT: Rgb = Rgb {
    red: 0xD8,
    green: 0xB3,
    blue: 0x6A,
};

const HEX_PREFIX: char = '#';
const HEX_RADIX: u32 = 16;
const HEX_PAIR_LEN: usize = 2;
const RGB_PAIRS: usize = 3;

/// Keeps a part-written file from ever being the config.
const TEMP_SUFFIX: &str = "toml.tmp";

const MIN_SCALE: f32 = 0.5;
const MAX_SCALE: f32 = 2.5;
const MIN_OPACITY: f32 = 0.2;
const MAX_OPACITY: f32 = 1.0;

/// Which monitor the wheel opens on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MonitorTarget {
    #[default]
    Cursor,
    Primary,
}

/// How long the open, hover and confirm animations take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnimationSpeed {
    Normal,
    #[default]
    Fast,
    Off,
}

impl AnimationSpeed {
    /// Milliseconds a transition is allowed. The spec ceiling is 100 ms.
    pub fn duration_ms(self) -> u64 {
        match self {
            Self::Normal => 100,
            Self::Fast => 60,
            Self::Off => 0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct General {
    pub start_with_windows: bool,
    pub disable_in_fullscreen: bool,
    pub minimize_if_focused: bool,
    pub show_clock: bool,
}

impl Default for General {
    fn default() -> Self {
        Self {
            start_with_windows: true,
            disable_in_fullscreen: true,
            minimize_if_focused: true,
            show_clock: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct WheelSettings {
    pub monitor: MonitorTarget,
    pub center_cursor: bool,
    pub animation: AnimationSpeed,
    pub open_hotkey_closes: bool,
    pub size: f32,
    pub opacity: f32,
    pub accent: String,
    pub empty_quips: Vec<String>,
}

impl Default for WheelSettings {
    fn default() -> Self {
        Self {
            monitor: MonitorTarget::default(),
            center_cursor: true,
            animation: AnimationSpeed::default(),
            open_hotkey_closes: false,
            size: 1.0,
            opacity: 1.0,
            accent: format!("{FALLBACK_ACCENT}"),
            empty_quips: default_quips(),
        }
    }
}

impl WheelSettings {
    /// Clamped, because a config typo should not make the wheel unusable.
    pub fn scale(&self) -> f32 {
        self.size.clamp(MIN_SCALE, MAX_SCALE)
    }

    pub fn opacity(&self) -> f32 {
        self.opacity.clamp(MIN_OPACITY, MAX_OPACITY)
    }

    pub fn accent(&self) -> Rgb {
        Rgb::parse(&self.accent).unwrap_or(FALLBACK_ACCENT)
    }
}

fn default_quips() -> Vec<String> {
    [
        "FEED ME",
        "I'M HUNGRY",
        "*crickets*",
        "vacancy",
        "404: app not found",
        "room for rent",
        "insert app here",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Rgb {
    /// Accepts `#RRGGBB` or `RRGGBB`.
    pub fn parse(text: &str) -> Option<Self> {
        let digits = text.trim().trim_start_matches(HEX_PREFIX);
        if digits.len() != RGB_PAIRS * HEX_PAIR_LEN {
            return None;
        }

        let mut channels = [0u8; RGB_PAIRS];
        for (channel, pair) in channels
            .iter_mut()
            .zip(digits.as_bytes().chunks(HEX_PAIR_LEN))
        {
            let pair = std::str::from_utf8(pair).ok()?;
            *channel = u8::from_str_radix(pair, HEX_RADIX).ok()?;
        }

        Some(Self {
            red: channels[0],
            green: channels[1],
            blue: channels[2],
        })
    }
}

impl std::fmt::Display for Rgb {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{HEX_PREFIX}{:02X}{:02X}{:02X}",
            self.red, self.green, self.blue
        )
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TabSettings {
    /// Tab number as written in the config, counting from 1.
    pub index: usize,

    #[serde(default)]
    pub name: Option<String>,

    /// Relative to the config folder.
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HotkeyConfig {
    pub binding: String,
    pub action: Action,
}

/// Everything the config file holds.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub wheel: WheelSettings,
    pub hotkeys: Vec<HotkeyConfig>,
    pub tabs: Vec<TabSettings>,
    pub items: Vec<Item>,
}

impl Config {
    /// Parses every binding string, so a typo names itself rather than going quiet.
    pub fn binding_table(&self) -> Result<BindingTable, ConfigError> {
        let mut hotkeys = Vec::with_capacity(self.hotkeys.len());

        for entry in &self.hotkeys {
            let binding: Binding =
                entry
                    .binding
                    .parse()
                    .map_err(|source| ConfigError::Binding {
                        binding: entry.binding.clone(),
                        source,
                    })?;
            hotkeys.push(Hotkey {
                binding,
                action: entry.action,
            });
        }

        Ok(BindingTable::new(hotkeys))
    }

    /// Pinned items keyed by where they sit, which is also how edits find them.
    pub fn pinned(&self) -> HashMap<Position, &Item> {
        self.items
            .iter()
            .filter_map(|item| Some((item.position()?, item)))
            .collect()
    }

    /// How many tabs to show: enough for every configured tab and pinned item, at
    /// least one, never more than the number keys can reach.
    pub fn tab_count(&self) -> usize {
        let configured = self.tabs.iter().map(|tab| tab.index);
        let used = self.items.iter().map(|item| item.tab);

        configured
            .chain(used)
            .filter(|number| *number >= 1 && *number <= Tab::MAX_COUNT)
            .max()
            .unwrap_or(1)
    }

    pub fn tab(&self, tab: Tab) -> Option<&TabSettings> {
        self.tabs
            .iter()
            .find(|settings| settings.index == tab.number())
    }

    /// Rejects anything that would silently misplace an item.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let mut claimed: HashMap<Position, &str> = HashMap::new();

        for item in &self.items {
            let Some(position) = item.position() else {
                return Err(ConfigError::OutOfRange {
                    name: item.name.clone(),
                    tab: item.tab,
                    slot: item.slot,
                    max_tab: Tab::MAX_COUNT,
                    max_slot: Slot::COUNT - 1,
                });
            };

            if let Some(first) = claimed.insert(position, &item.name) {
                return Err(ConfigError::DuplicateSlot {
                    tab: item.tab,
                    slot: item.slot,
                    first: first.to_owned(),
                    second: item.name.clone(),
                });
            }
        }

        self.binding_table().map(|_| ())
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not read {path}: {source}; check the file exists and is readable")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not write {path}: {source}; check the folder is writable")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "{path} is not valid TOML: {source}; fix the file, the last good config is still in use"
    )]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("{path} could not be opened for editing: {source}; fix the TOML and try again")]
    Document {
        path: PathBuf,
        #[source]
        source: toml_edit::TomlError,
    },

    #[error("hotkey \"{binding}\" could not be read: {source}")]
    Binding {
        binding: String,
        #[source]
        source: ParseBindingError,
    },

    #[error(
        "tab {tab} slot {slot} is claimed by both \"{first}\" and \"{second}\"; give one of them a different slot"
    )]
    DuplicateSlot {
        tab: usize,
        slot: usize,
        first: String,
        second: String,
    },

    #[error(
        "\"{name}\" sits at tab {tab} slot {slot}, which is off the wheel; tabs run 1 to {max_tab} and slots 0 to {max_slot}"
    )]
    OutOfRange {
        name: String,
        tab: usize,
        slot: usize,
        max_tab: usize,
        max_slot: usize,
    },
}

/// Reads and validates the config.
pub fn load(path: &Path) -> Result<Config, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_owned(),
        source,
    })?;

    let config: Config = toml::from_str(&text).map_err(|source| ConfigError::Parse {
        path: path.to_owned(),
        source,
    })?;

    config.validate()?;
    Ok(config)
}

/// Writes the shipped default and reports whether it had to.
///
/// Never touches an existing file, so a hand-tuned config survives a reinstall.
pub fn write_default_if_missing(path: &Path) -> Result<bool, ConfigError> {
    if path.exists() {
        return Ok(false);
    }

    write_atomically(path, DEFAULT_CONFIG)?;
    Ok(true)
}

/// Writes through a sibling temp file then renames, so a crash can never leave half a
/// config behind.
pub fn write_atomically(path: &Path, text: &str) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| ConfigError::Write {
            path: parent.to_owned(),
            source,
        })?;
    }

    let temp = path.with_extension(TEMP_SUFFIX);
    std::fs::write(&temp, text).map_err(|source| ConfigError::Write {
        path: temp.clone(),
        source,
    })?;

    std::fs::rename(&temp, path).map_err(|source| ConfigError::Write {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::ItemKind;

    #[test]
    fn the_shipped_default_parses_and_validates() {
        let config: Config = toml::from_str(DEFAULT_CONFIG).expect("the default must parse");

        config.validate().expect("the default must validate");
        assert_eq!(config.tab_count(), 1);
        assert!(!config.wheel.empty_quips.is_empty());
    }

    #[test]
    fn the_shipped_default_binds_caps_lock_to_toggle() {
        let config: Config = toml::from_str(DEFAULT_CONFIG).expect("the default must parse");
        let table = config.binding_table().expect("bindings must compile");

        let pressed = Binding::for_key_press(crate::binding::keys::CAPS_LOCK, Default::default());
        assert_eq!(table.action_for(pressed, false), Some(Action::Toggle));
    }

    /// The flat item shape the spec documents has to keep working.
    #[test]
    fn every_item_type_parses_from_the_flat_shape() {
        let text = r#"
            [[items]]
            type = "app"
            name = "Code"
            target = "C:\\code.exe"
            tab = 1
            slot = 0

            [[items]]
            type = "script"
            name = "Deploy"
            path = "d.sh"
            exec = "bash \"{path}\""
            tab = 1
            slot = 1

            [[items]]
            type = "url"
            name = "Jira"
            url = "https://example.test"
            browser = "msedge.exe"
            tab = 1
            slot = 2

            [[items]]
            type = "snippet"
            name = "Sig"
            text = "Thanks,\nKyle"
            mode = "type"
            tab = 1
            slot = 3

            [[items]]
            type = "folder"
            name = "Projects"
            path = "C:\\p"
            tab = 1
            slot = 4

            [[items]]
            type = "terminal"
            name = "Ubuntu"
            profile = "Ubuntu"
            distro = "Ubuntu"
            cwd = "~"
            tab = 1
            slot = 5
        "#;

        let config: Config = toml::from_str(text).expect("flat items must parse");

        let kinds: Vec<&str> = config
            .items
            .iter()
            .map(|item| item.kind.type_name())
            .collect();
        assert_eq!(
            kinds,
            ["App", "Script", "URL", "Snippet", "Folder", "Terminal"]
        );
        config.validate().expect("positions are all distinct");
    }

    #[test]
    fn two_items_in_one_slot_is_rejected_by_name() {
        let text = r#"
            [[items]]
            type = "folder"
            name = "First"
            path = "C:\\a"
            tab = 2
            slot = 3

            [[items]]
            type = "folder"
            name = "Second"
            path = "C:\\b"
            tab = 2
            slot = 3
        "#;

        let config: Config = toml::from_str(text).expect("parses");
        let error = config.validate().expect_err("a clash must be reported");

        let message = error.to_string();
        assert!(
            message.contains("First") && message.contains("Second"),
            "{message}"
        );
    }

    #[test]
    fn an_off_wheel_position_is_rejected() {
        let text = r#"
            [[items]]
            type = "folder"
            name = "Nope"
            path = "C:\\a"
            tab = 12
            slot = 0
        "#;

        let config: Config = toml::from_str(text).expect("parses");

        assert!(matches!(
            config.validate(),
            Err(ConfigError::OutOfRange { .. })
        ));
    }

    #[test]
    fn a_bad_binding_names_itself() {
        let text = r#"
            [[hotkeys]]
            binding = "Ctrl+Nonsense"
            action = "toggle"
        "#;

        let config: Config = toml::from_str(text).expect("parses");
        let error = config.binding_table().expect_err("must be rejected");

        assert!(error.to_string().contains("Ctrl+Nonsense"));
    }

    #[test]
    fn tab_count_covers_the_highest_tab_in_use() {
        let text = r#"
            [[items]]
            type = "folder"
            name = "Far"
            path = "C:\\a"
            tab = 4
            slot = 0
        "#;

        let config: Config = toml::from_str(text).expect("parses");

        assert_eq!(config.tab_count(), 4);
    }

    #[test]
    fn accent_parses_with_or_without_a_hash() {
        assert_eq!(
            Rgb::parse("#D8B36A"),
            Some(Rgb {
                red: 0xD8,
                green: 0xB3,
                blue: 0x6A
            })
        );
        assert_eq!(Rgb::parse("d8b36a"), Rgb::parse("#D8B36A"));
        assert_eq!(Rgb::parse("nope"), None);
    }

    #[test]
    fn a_bad_accent_falls_back_to_the_shipped_one() {
        let settings = WheelSettings {
            accent: "not a colour".to_owned(),
            ..WheelSettings::default()
        };

        assert_eq!(settings.accent(), FALLBACK_ACCENT);
    }

    #[test]
    fn size_and_opacity_are_clamped_to_something_usable() {
        let settings = WheelSettings {
            size: 99.0,
            opacity: -1.0,
            ..WheelSettings::default()
        };

        assert_eq!(settings.scale(), MAX_SCALE);
        assert_eq!(settings.opacity(), MIN_OPACITY);
    }

    #[test]
    fn pinned_items_are_keyed_by_position() {
        let item = Item {
            name: "Code".to_owned(),
            icon: None,
            tab: 1,
            slot: 0,
            kind: ItemKind::App {
                target: "code.exe".to_owned(),
            },
        };
        let config = Config {
            items: vec![item],
            ..Config::default()
        };

        let pinned = config.pinned();
        let position = Position::new(Tab::FIRST, Slot::new(0).expect("slot 0"));
        assert_eq!(
            pinned.get(&position).map(|item| item.name.as_str()),
            Some("Code")
        );
    }
}
