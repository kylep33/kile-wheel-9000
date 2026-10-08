//! The config being edited, held apart from the file until Save.

use anyhow::{Context, Result};
use kile_wheel_core::binding::Binding;
use kile_wheel_core::config::{self, Config, ConfigDocument, HotkeyConfig, TabSettings};
use kile_wheel_core::item::{Item, Position};
use kile_wheel_core::names::ConfigPaths;
use kile_wheel_core::slot::{Slot, Tab};

/// A working copy of the config plus whether it differs from what is on disk.
#[derive(Debug)]
pub struct Draft {
    paths: ConfigPaths,
    saved: Config,
    pub working: Config,
}

impl Draft {
    pub fn load(paths: ConfigPaths) -> Result<Self> {
        let file = paths.config_file();
        config::write_default_if_missing(&file)
            .with_context(|| format!("could not create {}", file.display()))?;

        let saved =
            config::load(&file).with_context(|| format!("could not read {}", file.display()))?;

        Ok(Self {
            paths,
            working: saved.clone(),
            saved,
        })
    }

    pub fn config_file(&self) -> std::path::PathBuf {
        self.paths.config_file()
    }

    pub fn icons_dir(&self) -> std::path::PathBuf {
        self.paths.icons_dir()
    }

    pub fn config_root(&self) -> &std::path::Path {
        self.paths.root()
    }

    /// Whether anything is waiting to be written.
    pub fn is_dirty(&self) -> bool {
        !same_config(&self.saved, &self.working)
    }

    /// Why the working copy could not be saved, if anything.
    pub fn problem(&self) -> Option<String> {
        self.working.validate().err().map(|error| error.to_string())
    }

    /// Writes only the sections that changed, so comments elsewhere are left untouched.
    pub fn save(&mut self) -> Result<()> {
        self.working
            .validate()
            .context("the settings have a problem that has to be fixed first")?;

        let file = self.config_file();
        let mut document = ConfigDocument::load(&file)
            .with_context(|| format!("could not open {} for editing", file.display()))?;

        document.set_general(&self.working.general);
        document.set_wheel(&self.working.wheel);

        if !same_hotkeys(&self.saved.hotkeys, &self.working.hotkeys) {
            document.set_hotkeys(&self.working.hotkeys);
        }
        if !same_tabs(&self.saved.tabs, &self.working.tabs) {
            document.set_tabs(&self.working.tabs);
        }
        self.write_items(&mut document)?;

        document
            .save()
            .with_context(|| format!("could not write {}", file.display()))?;
        self.saved = self.working.clone();
        Ok(())
    }

    pub fn revert(&mut self) {
        self.working = self.saved.clone();
    }

    /// Removes what is gone and upserts what is left, so an item's comments survive an
    /// unrelated edit.
    fn write_items(&self, document: &mut ConfigDocument) -> Result<()> {
        let kept: Vec<Position> = self
            .working
            .items
            .iter()
            .filter_map(Item::position)
            .collect();

        for item in &self.saved.items {
            let Some(position) = item.position() else {
                continue;
            };
            if !kept.contains(&position) {
                document.remove_item_at(position);
            }
        }

        for item in &self.working.items {
            document.upsert_item(item)?;
        }
        Ok(())
    }

    /// The first free slot, so a new item never lands on top of an existing one.
    pub fn first_free_position(&self) -> Option<Position> {
        let taken: Vec<Position> = self
            .working
            .items
            .iter()
            .filter_map(Item::position)
            .collect();

        kile_wheel_core::layout::positions(Tab::MAX_COUNT)
            .find(|position| !taken.contains(position))
    }

    /// Adds a tab entry if the number does not have one yet.
    pub fn ensure_tab(&mut self, number: usize) {
        if self.working.tabs.iter().any(|tab| tab.index == number) {
            return;
        }

        self.working.tabs.push(TabSettings {
            index: number,
            name: None,
            icon: None,
        });
        self.working.tabs.sort_by_key(|tab| tab.index);
    }
}

/// Reports whether a binding string would be accepted, so the UI can say so as it is
/// typed rather than only on Save.
pub fn binding_problem(text: &str) -> Option<String> {
    text.parse::<Binding>().err().map(|error| error.to_string())
}

pub fn slot_label(slot: Slot) -> &'static str {
    const COMPASS: [&str; Slot::COUNT] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];

    COMPASS[slot.index()]
}

/// `Config` holds no `PartialEq` because it carries user text that is compared field by
/// field here instead.
fn same_config(left: &Config, right: &Config) -> bool {
    same_general(&left.general, &right.general)
        && same_wheel(&left.wheel, &right.wheel)
        && same_hotkeys(&left.hotkeys, &right.hotkeys)
        && same_tabs(&left.tabs, &right.tabs)
        && left.items == right.items
}

fn same_general(left: &config::General, right: &config::General) -> bool {
    left.start_with_windows == right.start_with_windows
        && left.disable_in_fullscreen == right.disable_in_fullscreen
        && left.minimize_if_focused == right.minimize_if_focused
        && left.show_clock == right.show_clock
}

fn same_wheel(left: &config::WheelSettings, right: &config::WheelSettings) -> bool {
    left.monitor == right.monitor
        && left.center_cursor == right.center_cursor
        && left.animation == right.animation
        && left.open_hotkey_closes == right.open_hotkey_closes
        && left.size.to_bits() == right.size.to_bits()
        && left.opacity.to_bits() == right.opacity.to_bits()
        && left.accent == right.accent
        && left.empty_quips == right.empty_quips
}

fn same_hotkeys(left: &[HotkeyConfig], right: &[HotkeyConfig]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(a, b)| a.binding == b.binding && a.action == b.action)
}

fn same_tabs(left: &[TabSettings], right: &[TabSettings]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(a, b)| a.index == b.index && a.name == b.name && a.icon == b.icon)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_good_binding_reports_no_problem() {
        assert_eq!(binding_problem("Alt+Shift"), None);
        assert_eq!(binding_problem("CapsLock"), None);
    }

    #[test]
    fn a_bad_binding_explains_itself() {
        let problem = binding_problem("Ctrl+Nonsense").expect("must be rejected");

        assert!(problem.contains("Nonsense"), "{problem}");
    }

    #[test]
    fn slot_labels_run_clockwise_from_north() {
        assert_eq!(slot_label(Slot::new(0).expect("slot 0")), "N");
        assert_eq!(slot_label(Slot::new(2).expect("slot 2")), "E");
        assert_eq!(slot_label(Slot::new(7).expect("slot 7")), "NW");
    }
}
