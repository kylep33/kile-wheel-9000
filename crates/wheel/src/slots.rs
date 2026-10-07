//! What each wheel slot shows and what activating it does.

use kile_wheel_core::slot::Slot;
use kile_wheel_platform::apps::{AppGroup, AppKey, WindowModel};
use windows::Win32::Foundation::HWND;

/// A filled slot. Phase 1 fills these from running apps only.
#[derive(Debug, Clone)]
pub struct SlotContent {
    pub label: String,
    pub windows: Vec<HWND>,
}

/// The eight slots of the current tab.
#[derive(Debug)]
pub struct SlotRow {
    slots: [Option<SlotContent>; Slot::COUNT],
}

impl Default for SlotRow {
    fn default() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
        }
    }
}

impl SlotRow {
    /// Fills slots with running apps in discovery order, which is what lets a slot keep
    /// its app for as long as that app runs.
    pub fn from_running(model: &WindowModel) -> Self {
        let groups = model.groups();
        let mut groups = groups.iter();

        Self {
            slots: std::array::from_fn(|_| {
                groups.next().map(|group| SlotContent {
                    label: label_for(group),
                    windows: group.windows.iter().map(|window| window.handle).collect(),
                })
            }),
        }
    }

    pub fn get(&self, slot: Slot) -> Option<&SlotContent> {
        self.slots[slot.index()].as_ref()
    }
}

/// Slot labels mirror the taskbar: the app, not the window title.
fn label_for(group: &AppGroup<'_>) -> String {
    match group.key {
        AppKey::Executable(path) => match path.file_stem() {
            Some(stem) => stem.to_string_lossy().into_owned(),
            None => path.display().to_string(),
        },
        AppKey::UserModelId(id) => match id.rsplit_once('!') {
            Some((_, app)) => app.to_owned(),
            None => id.clone(),
        },
    }
}
