//! Editing the config as a document rather than as data, so comments survive.
//!
//! Assigning to a key that already exists keeps that key's surrounding comments, which
//! is why every setter writes key by key instead of replacing whole tables.

use std::path::{Path, PathBuf};

use toml_edit::{ArrayOfTables, DocumentMut, Item as TomlItem, Table, Value, value};

use super::{
    Config, ConfigError, General, HotkeyConfig, TabSettings, WheelSettings, write_atomically,
};
use crate::item::{Item, ItemKind, Position, SnippetMode};
use crate::slot::{Slot, Tab};

const GENERAL_TABLE: &str = "general";
const WHEEL_TABLE: &str = "wheel";
const HOTKEYS_ARRAY: &str = "hotkeys";
const TABS_ARRAY: &str = "tabs";
const ITEMS_ARRAY: &str = "items";

const TYPE_KEY: &str = "type";
const NAME_KEY: &str = "name";
const ICON_KEY: &str = "icon";
const TAB_KEY: &str = "tab";
const SLOT_KEY: &str = "slot";
const INDEX_KEY: &str = "index";
const BINDING_KEY: &str = "binding";
const ACTION_KEY: &str = "action";

/// The config file held open for editing.
#[derive(Debug, Clone)]
pub struct ConfigDocument {
    path: PathBuf,
    document: DocumentMut,
}

impl ConfigDocument {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_owned(),
            source,
        })?;

        let document = text
            .parse::<DocumentMut>()
            .map_err(|source| ConfigError::Document {
                path: path.to_owned(),
                source,
            })?;

        Ok(Self {
            path: path.to_owned(),
            document,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The document read back as data, so a caller can show what it is about to edit.
    pub fn config(&self) -> Result<Config, ConfigError> {
        let config: Config =
            toml::from_str(&self.document.to_string()).map_err(|source| ConfigError::Parse {
                path: self.path.clone(),
                source,
            })?;

        config.validate()?;
        Ok(config)
    }

    pub fn set_general(&mut self, general: &General) {
        let table = self.table_mut(GENERAL_TABLE);

        set_value(
            table,
            "start_with_windows",
            general.start_with_windows.into(),
        );
        set_value(
            table,
            "disable_in_fullscreen",
            general.disable_in_fullscreen.into(),
        );
        set_value(
            table,
            "minimize_if_focused",
            general.minimize_if_focused.into(),
        );
        set_value(table, "show_clock", general.show_clock.into());
    }

    pub fn set_wheel(&mut self, wheel: &WheelSettings) {
        let table = self.table_mut(WHEEL_TABLE);

        set_value(table, "monitor", monitor_name(wheel.monitor).into());
        set_value(table, "center_cursor", wheel.center_cursor.into());
        set_value(
            table,
            "center_cursor_on_tab_change",
            wheel.center_cursor_on_tab_change.into(),
        );
        set_value(
            table,
            "center_cursor_on_sub_wheel",
            wheel.center_cursor_on_sub_wheel.into(),
        );
        set_value(table, "animation", animation_name(wheel.animation).into());
        set_value(table, "open_hotkey_closes", wheel.open_hotkey_closes.into());
        set_value(table, "size", f64::from(wheel.size).into());
        set_value(table, "opacity", f64::from(wheel.opacity).into());
        set_value(table, "accent", wheel.accent.as_str().into());

        let mut quips = toml_edit::Array::new();
        for quip in &wheel.empty_quips {
            quips.push(quip.as_str());
        }
        set_value(table, "empty_quips", quips.into());

        let mut hidden = toml_edit::Array::new();
        for app in &wheel.hidden_apps {
            hidden.push(app.as_str());
        }
        set_value(table, "hidden_apps", hidden.into());
    }

    /// Replaces the hotkey list wholesale, because its entries have no stable identity.
    /// Comments written between hotkey entries are lost; comments elsewhere are not.
    pub fn set_hotkeys(&mut self, hotkeys: &[HotkeyConfig]) {
        let array = self.array_mut(HOTKEYS_ARRAY);
        array.clear();

        for hotkey in hotkeys {
            let mut table = Table::new();
            table.insert(BINDING_KEY, value(hotkey.binding.as_str()));
            table.insert(ACTION_KEY, value(action_name(hotkey.action)));
            array.push(table);
        }
    }

    /// Replaces the tab list wholesale, for the same reason as the hotkeys.
    pub fn set_tabs(&mut self, tabs: &[TabSettings]) {
        let array = self.array_mut(TABS_ARRAY);
        array.clear();

        for tab in tabs {
            let mut table = Table::new();
            table.insert(INDEX_KEY, value(tab.index as i64));
            insert_optional(&mut table, NAME_KEY, tab.name.as_deref());
            insert_optional(&mut table, ICON_KEY, tab.icon.as_deref());
            array.push(table);
        }
    }

    /// Adds the item, or replaces whatever already sits in its slot.
    pub fn upsert_item(&mut self, item: &Item) -> Result<(), ConfigError> {
        let Some(position) = item.position() else {
            return Err(ConfigError::OutOfRange {
                name: item.name.clone(),
                tab: item.tab,
                slot: item.slot,
                max_tab: Tab::MAX_COUNT,
                max_slot: Slot::COUNT - 1,
            });
        };

        let table = item_table(item);
        let array = self.array_mut(ITEMS_ARRAY);

        match index_at(array, position) {
            Some(index) => {
                if let Some(existing) = array.get_mut(index) {
                    *existing = table;
                }
            }
            None => array.push(table),
        }
        Ok(())
    }

    /// Reports whether anything was there to remove.
    pub fn remove_item_at(&mut self, position: Position) -> bool {
        let array = self.array_mut(ITEMS_ARRAY);
        let Some(index) = index_at(array, position) else {
            return false;
        };

        array.remove(index);
        true
    }

    /// Moves the item at `from` to `to`, trading places with anything already there.
    ///
    /// Reports whether there was anything to move.
    pub fn move_item(&mut self, from: Position, to: Position) -> bool {
        if from == to {
            return false;
        }

        let array = self.array_mut(ITEMS_ARRAY);
        let Some(moved) = index_at(array, from) else {
            return false;
        };
        let displaced = index_at(array, to);

        set_position(array, moved, to);
        if let Some(displaced) = displaced {
            set_position(array, displaced, from);
        }
        true
    }

    pub fn save(&self) -> Result<(), ConfigError> {
        write_atomically(&self.path, &self.document.to_string())
    }

    /// An absent or wrong-typed table is replaced, since there is nothing to preserve.
    fn table_mut(&mut self, key: &str) -> &mut Table {
        let entry = self
            .document
            .entry(key)
            .or_insert(TomlItem::Table(Table::new()));

        if !entry.is_table() {
            *entry = TomlItem::Table(Table::new());
        }
        entry.as_table_mut().expect("just made sure it is a table")
    }

    fn array_mut(&mut self, key: &str) -> &mut ArrayOfTables {
        let entry = self
            .document
            .entry(key)
            .or_insert(TomlItem::ArrayOfTables(ArrayOfTables::new()));

        if !entry.is_array_of_tables() {
            *entry = TomlItem::ArrayOfTables(ArrayOfTables::new());
        }
        entry
            .as_array_of_tables_mut()
            .expect("just made sure it is an array of tables")
    }
}

/// Replaces a value while keeping the key's comments and spacing.
///
/// Plain `Table::insert` throws both away, which would eat the comments in the shipped
/// default the first time the settings window saved.
fn set_value(table: &mut Table, key: &str, new_value: Value) {
    let Some(existing) = table.get_mut(key) else {
        table.insert(key, TomlItem::Value(new_value));
        return;
    };

    let kept = existing.as_value().map(|value| value.decor().clone());
    let mut replacement = new_value;
    if let Some(kept) = kept {
        *replacement.decor_mut() = kept;
    }
    *existing = TomlItem::Value(replacement);
}

/// Must match the serde tag on `ItemKind`; the round-trip test below guards the pair.
fn type_tag(kind: &ItemKind) -> &'static str {
    match kind {
        ItemKind::UnpinnedApp => "unpinned_app",
        ItemKind::App { .. } => "app",
        ItemKind::Script { .. } => "script",
        ItemKind::Url { .. } => "url",
        ItemKind::Snippet { .. } => "snippet",
        ItemKind::Folder { .. } => "folder",
        ItemKind::Terminal { .. } => "terminal",
    }
}

fn item_table(item: &Item) -> Table {
    let mut table = Table::new();

    table.insert(TYPE_KEY, value(type_tag(&item.kind)));
    table.insert(NAME_KEY, value(item.name.as_str()));
    insert_optional(&mut table, ICON_KEY, item.icon.as_deref());
    insert_kind(&mut table, &item.kind);

    // position last, so the interesting fields read first
    table.insert(TAB_KEY, value(item.tab as i64));
    table.insert(SLOT_KEY, value(item.slot as i64));
    table
}

fn insert_kind(table: &mut Table, kind: &ItemKind) {
    match kind {
        // the type tag is the whole entry; the wheel decides what sits here
        ItemKind::UnpinnedApp => {}
        ItemKind::App { target } => {
            table.insert("target", value(target.as_str()));
        }
        ItemKind::Script {
            path,
            exec,
            cwd,
            hidden,
        } => {
            table.insert("path", value(path.as_str()));
            table.insert("exec", value(exec.as_str()));
            insert_optional(table, "cwd", cwd.as_deref());
            if *hidden {
                table.insert("hidden", value(true));
            }
        }
        ItemKind::Url {
            url,
            browser,
            browser_args,
        } => {
            table.insert("url", value(url.as_str()));
            insert_optional(table, "browser", browser.as_deref());
            insert_optional(table, "browser_args", browser_args.as_deref());
        }
        ItemKind::Snippet { text, mode } => {
            table.insert("text", value(text.as_str()));
            table.insert("mode", value(snippet_mode_name(*mode)));
        }
        ItemKind::Folder { path } => {
            table.insert("path", value(path.as_str()));
        }
        ItemKind::Terminal {
            profile,
            distro,
            cwd,
        } => {
            insert_optional(table, "profile", profile.as_deref());
            insert_optional(table, "distro", distro.as_deref());
            insert_optional(table, "cwd", cwd.as_deref());
        }
    }
}

/// Leaves the key out entirely rather than writing an empty string, so the file stays
/// readable and the serde default stays in charge.
fn insert_optional(table: &mut Table, key: &str, text: Option<&str>) {
    match text {
        Some(text) => {
            table.insert(key, value(text));
        }
        None => {
            table.remove(key);
        }
    }
}

fn set_position(array: &mut ArrayOfTables, index: usize, position: Position) {
    let Some(table) = array.get_mut(index) else {
        return;
    };

    table.insert(TAB_KEY, value(position.tab.number() as i64));
    table.insert(SLOT_KEY, value(position.slot.index() as i64));
}

fn index_at(array: &ArrayOfTables, position: Position) -> Option<usize> {
    array
        .iter()
        .position(|table| table_position(table) == Some(position))
}

fn table_position(table: &Table) -> Option<Position> {
    let tab = usize::try_from(table.get(TAB_KEY)?.as_integer()?).ok()?;
    let slot = usize::try_from(table.get(SLOT_KEY)?.as_integer()?).ok()?;

    Some(Position::new(Tab::from_number(tab)?, Slot::new(slot)?))
}

fn monitor_name(monitor: super::MonitorTarget) -> &'static str {
    match monitor {
        super::MonitorTarget::Cursor => "cursor",
        super::MonitorTarget::Primary => "primary",
    }
}

fn animation_name(animation: super::AnimationSpeed) -> &'static str {
    match animation {
        super::AnimationSpeed::Normal => "normal",
        super::AnimationSpeed::Fast => "fast",
        super::AnimationSpeed::Off => "off",
    }
}

fn action_name(action: crate::binding::Action) -> &'static str {
    match action {
        crate::binding::Action::Open => "open",
        crate::binding::Action::Close => "close",
        crate::binding::Action::Toggle => "toggle",
    }
}

fn snippet_mode_name(mode: SnippetMode) -> &'static str {
    match mode {
        SnippetMode::Clipboard => "clipboard",
        SnippetMode::Type => "type",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DEFAULT_CONFIG;

    fn document_from(text: &str) -> ConfigDocument {
        ConfigDocument {
            path: PathBuf::from("config.toml"),
            document: text.parse().expect("test fixture must parse"),
        }
    }

    fn sample(name: &str, tab: usize, slot: usize) -> Item {
        Item {
            name: name.to_owned(),
            icon: None,
            tab,
            slot,
            kind: ItemKind::App {
                target: "code.exe".to_owned(),
            },
        }
    }

    #[test]
    fn upserting_then_reading_back_round_trips_every_kind() {
        let kinds = [
            ItemKind::UnpinnedApp,
            ItemKind::App {
                target: "a.exe".to_owned(),
            },
            ItemKind::Script {
                path: "s.sh".to_owned(),
                exec: "bash \"{path}\"".to_owned(),
                cwd: Some("C:\\w".to_owned()),
                hidden: true,
            },
            ItemKind::Url {
                url: "https://example.test".to_owned(),
                browser: Some("msedge.exe".to_owned()),
                browser_args: Some("--profile-directory=Work".to_owned()),
            },
            ItemKind::Snippet {
                text: "line one\nline two".to_owned(),
                mode: SnippetMode::Type,
            },
            ItemKind::Folder {
                path: "C:\\p".to_owned(),
            },
            ItemKind::Terminal {
                profile: Some("Ubuntu".to_owned()),
                distro: Some("Ubuntu".to_owned()),
                cwd: Some("~".to_owned()),
            },
        ];

        let mut document = document_from("");
        for (slot, kind) in kinds.iter().enumerate() {
            let item = Item {
                name: format!("item {slot}"),
                icon: Some("icons/x.png".to_owned()),
                tab: 1,
                slot,
                kind: kind.clone(),
            };
            document.upsert_item(&item).expect("in range");
        }

        let read_back = document.config().expect("written items must parse back");
        let written: Vec<ItemKind> = read_back.items.into_iter().map(|item| item.kind).collect();
        assert_eq!(written, kinds);
    }

    #[test]
    fn comments_outside_the_edited_key_survive() {
        let mut document = document_from(
            "# keep me\n[general]\n# and me\nshow_clock = true  # inline too\nminimize_if_focused = true\n",
        );

        document.set_general(&General {
            show_clock: false,
            ..General::default()
        });

        let written = document.document.to_string();
        assert!(written.contains("# keep me"), "{written}");
        assert!(written.contains("# and me"), "{written}");
        assert!(written.contains("# inline too"), "{written}");
        assert!(written.contains("show_clock = false"), "{written}");
    }

    #[test]
    fn editing_the_shipped_default_keeps_its_comments() {
        let mut document = document_from(DEFAULT_CONFIG);

        document.set_wheel(&WheelSettings {
            center_cursor: false,
            ..WheelSettings::default()
        });

        let written = document.document.to_string();
        assert!(written.contains("# kile-wheel-9000 config."), "{written}");
        assert!(
            written.contains("# \"cursor\" opens on the monitor under the pointer"),
            "{written}"
        );
    }

    #[test]
    fn moving_onto_an_empty_slot_just_moves() {
        let mut document = document_from("");
        document
            .upsert_item(&sample("Code", 1, 0))
            .expect("in range");

        let from = Position::new(Tab::FIRST, Slot::new(0).expect("slot 0"));
        let to = Position::new(Tab::FIRST, Slot::new(5).expect("slot 5"));
        assert!(document.move_item(from, to));

        let config = document.config().expect("parses");
        assert_eq!(config.items[0].slot, 5);
    }

    #[test]
    fn moving_onto_a_filled_slot_trades_places() {
        let mut document = document_from("");
        document
            .upsert_item(&sample("North", 1, 0))
            .expect("in range");
        document
            .upsert_item(&sample("East", 1, 2))
            .expect("in range");

        let north = Position::new(Tab::FIRST, Slot::new(0).expect("slot 0"));
        let east = Position::new(Tab::FIRST, Slot::new(2).expect("slot 2"));
        assert!(document.move_item(north, east));

        let config = document.config().expect("parses");
        let placed: Vec<(&str, usize)> = config
            .items
            .iter()
            .map(|item| (item.name.as_str(), item.slot))
            .collect();
        assert_eq!(placed, [("North", 2), ("East", 0)]);
    }

    #[test]
    fn moving_across_tabs_keeps_both_items() {
        let mut document = document_from("");
        document
            .upsert_item(&sample("Code", 1, 0))
            .expect("in range");

        let from = Position::new(Tab::FIRST, Slot::new(0).expect("slot 0"));
        let to = Position::new(
            Tab::from_number(3).expect("tab 3"),
            Slot::new(7).expect("slot 7"),
        );
        assert!(document.move_item(from, to));

        let config = document.config().expect("parses");
        assert_eq!((config.items[0].tab, config.items[0].slot), (3, 7));
    }

    #[test]
    fn moving_nothing_reports_nothing_moved() {
        let mut document = document_from("");
        let from = Position::new(Tab::FIRST, Slot::new(0).expect("slot 0"));
        let to = Position::new(Tab::FIRST, Slot::new(1).expect("slot 1"));

        assert!(!document.move_item(from, to));
    }

    #[test]
    fn removing_reports_whether_anything_went() {
        let mut document = document_from("");
        document
            .upsert_item(&sample("Code", 1, 0))
            .expect("in range");
        let position = Position::new(Tab::FIRST, Slot::new(0).expect("slot 0"));

        assert!(document.remove_item_at(position));
        assert!(!document.remove_item_at(position));
    }

    #[test]
    fn upserting_over_a_slot_replaces_rather_than_duplicates() {
        let mut document = document_from("");
        document
            .upsert_item(&sample("First", 1, 0))
            .expect("in range");
        document
            .upsert_item(&sample("Second", 1, 0))
            .expect("in range");

        let config = document.config().expect("parses");
        assert_eq!(config.items.len(), 1);
        assert_eq!(config.items[0].name, "Second");
    }

    #[test]
    fn editing_the_shipped_default_leaves_it_valid() {
        let mut document = document_from(DEFAULT_CONFIG);

        document
            .upsert_item(&sample("Code", 2, 3))
            .expect("in range");
        document.set_wheel(&WheelSettings {
            center_cursor: false,
            ..WheelSettings::default()
        });

        let config = document
            .config()
            .expect("the edited default must still parse");
        assert!(!config.wheel.center_cursor);
        assert_eq!(config.tab_count(), 2);
    }
}
