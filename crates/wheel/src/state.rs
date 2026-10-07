//! What the wheel is currently showing.

use kile_wheel_core::select::Latch;
use kile_wheel_core::slot::{Cardinal, Slot, Tab};
use windows::Win32::Foundation::HWND;

/// One window of an app that has more than one open.
#[derive(Debug, Clone)]
pub struct WindowChoice {
    pub handle: HWND,
    pub title: String,
}

/// Which layer of the wheel is in front.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Wheel,
    /// One app's windows, reached by activating an app with more than one open.
    SubWheel { windows: Vec<WindowChoice> },
}

impl PartialEq for WindowChoice {
    fn eq(&self, other: &Self) -> bool {
        self.handle == other.handle
    }
}

impl Eq for WindowChoice {}

/// What backing out one level did, so the caller knows whether to redraw or hide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Back {
    WentUp,
    Closed,
}

/// Selection and mode. Mouse and keyboard both write it; whichever moved last wins.
#[derive(Debug, Default)]
pub struct WheelState {
    open: bool,
    tab: Tab,
    tab_count: usize,
    selected: Option<Slot>,
    latch: Option<Latch>,
    mode: Mode,
    edit: bool,
    dragging: Option<Slot>,
}

impl WheelState {
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn selected(&self) -> Option<Slot> {
        self.selected
    }

    pub fn tab(&self) -> Tab {
        self.tab
    }

    pub fn mode(&self) -> &Mode {
        &self.mode
    }

    pub fn in_sub_wheel(&self) -> bool {
        matches!(self.mode, Mode::SubWheel { .. })
    }

    pub fn edit_mode(&self) -> bool {
        self.edit
    }

    pub fn dragging(&self) -> Option<Slot> {
        self.dragging
    }

    /// Each open starts on tab one with nothing selected, so a stale pick from last time
    /// can never be activated by a stray Enter.
    pub fn open(&mut self, tab_count: usize) {
        self.open = true;
        self.tab_count = tab_count.max(1);
        self.tab = Tab::FIRST;
        self.reset_selection();
        self.mode = Mode::Wheel;
        self.edit = false;
        self.dragging = None;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.reset_selection();
        self.mode = Mode::Wheel;
        self.edit = false;
        self.dragging = None;
    }

    /// Reports whether the tab changed, so a no-op number key does not force a redraw.
    pub fn select_tab(&mut self, number: usize) -> bool {
        if self.in_sub_wheel() || number > self.tab_count {
            return false;
        }

        let Some(tab) = Tab::from_number(number) else {
            return false;
        };
        if tab == self.tab {
            return false;
        }

        self.tab = tab;
        // a dragged item follows you to the new tab; otherwise the pick starts over
        if self.dragging.is_none() {
            self.reset_selection();
        }
        true
    }

    pub fn press_direction(&mut self, key: Cardinal) {
        let latch = Latch::press(self.latch, key);

        self.latch = Some(latch);
        self.selected = Some(latch.slot());
    }

    /// Mouse takes the selection over, so the next direction key starts a fresh latch.
    /// Inside the hub there is no angle to read, so the current pick stands.
    pub fn aim_at(&mut self, slot: Option<Slot>) {
        let Some(slot) = slot else {
            return;
        };

        self.latch = None;
        self.selected = Some(slot);
    }

    /// Edit mode belongs to the wheel, so a sub-wheel leaves it behind.
    pub fn enter_sub_wheel(&mut self, windows: Vec<WindowChoice>) {
        self.mode = Mode::SubWheel { windows };
        self.edit = false;
        self.dragging = None;
        self.reset_selection();
    }

    /// Backs out exactly one level: sub-wheel to wheel, edit mode to normal, and from
    /// the plain wheel all the way shut.
    pub fn back(&mut self) -> Back {
        if self.in_sub_wheel() {
            self.mode = Mode::Wheel;
            self.reset_selection();
            return Back::WentUp;
        }
        if self.edit {
            self.edit = false;
            self.dragging = None;
            return Back::WentUp;
        }

        self.close();
        Back::Closed
    }

    /// Reports whether edit mode is now on.
    pub fn toggle_edit(&mut self) -> bool {
        if self.in_sub_wheel() {
            return false;
        }

        self.edit = !self.edit;
        self.dragging = None;
        self.edit
    }

    pub fn begin_drag(&mut self, slot: Slot) {
        if !self.edit {
            return;
        }
        self.dragging = Some(slot);
    }

    /// Gives back where the drag started, so the caller can move the item.
    pub fn end_drag(&mut self) -> Option<Slot> {
        self.dragging.take()
    }

    fn reset_selection(&mut self) {
        self.selected = None;
        self.latch = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kile_wheel_core::slot::Direction;

    const THREE_TABS: usize = 3;

    fn opened() -> WheelState {
        let mut state = WheelState::default();
        state.open(THREE_TABS);
        state
    }

    #[test]
    fn opening_clears_any_previous_pick() {
        let mut state = opened();
        state.press_direction(Cardinal::East);
        state.close();

        state.open(THREE_TABS);

        assert_eq!(state.selected(), None);
        assert_eq!(state.tab(), Tab::FIRST);
    }

    #[test]
    fn the_mouse_resets_the_latch_so_keys_start_over() {
        let mut state = opened();
        state.press_direction(Cardinal::North);
        state.aim_at(Some(Direction::South.slot()));

        state.press_direction(Cardinal::East);

        assert_eq!(state.selected(), Some(Direction::East.slot()));
    }

    #[test]
    fn aiming_into_the_hub_keeps_the_current_pick() {
        let mut state = opened();
        state.press_direction(Cardinal::West);

        state.aim_at(None);

        assert_eq!(state.selected(), Some(Direction::West.slot()));
    }

    #[test]
    fn keys_still_latch_diagonals_through_the_state() {
        let mut state = opened();

        state.press_direction(Cardinal::South);
        state.press_direction(Cardinal::East);

        assert_eq!(state.selected(), Some(Direction::SouthEast.slot()));
    }

    #[test]
    fn number_keys_switch_tabs_inside_the_configured_count() {
        let mut state = opened();

        assert!(state.select_tab(3));
        assert_eq!(state.tab().number(), 3);
        assert!(!state.select_tab(4));
        assert_eq!(state.tab().number(), 3);
    }

    #[test]
    fn reselecting_the_current_tab_changes_nothing() {
        let mut state = opened();

        assert!(!state.select_tab(1));
    }

    #[test]
    fn switching_tabs_clears_the_pick() {
        let mut state = opened();
        state.press_direction(Cardinal::North);

        state.select_tab(2);

        assert_eq!(state.selected(), None);
    }

    #[test]
    fn a_dragged_item_keeps_its_selection_across_tabs() {
        let mut state = opened();
        state.toggle_edit();
        state.press_direction(Cardinal::North);
        state.begin_drag(Direction::North.slot());

        state.select_tab(2);

        assert_eq!(state.selected(), Some(Direction::North.slot()));
        assert_eq!(state.dragging(), Some(Direction::North.slot()));
    }

    #[test]
    fn backspace_leaves_a_sub_wheel_before_closing() {
        let mut state = opened();
        state.enter_sub_wheel(Vec::new());

        assert_eq!(state.back(), Back::WentUp);
        assert!(state.is_open());
        assert_eq!(state.back(), Back::Closed);
        assert!(!state.is_open());
    }

    #[test]
    fn backspace_leaves_edit_mode_before_closing() {
        let mut state = opened();
        state.toggle_edit();

        assert_eq!(state.back(), Back::WentUp);
        assert!(!state.edit_mode());
        assert_eq!(state.back(), Back::Closed);
    }

    #[test]
    fn a_sub_wheel_cannot_be_edited() {
        let mut state = opened();
        state.enter_sub_wheel(Vec::new());

        assert!(!state.toggle_edit());
        assert!(!state.select_tab(2));
    }

    #[test]
    fn entering_a_sub_wheel_drops_edit_mode() {
        let mut state = opened();
        state.toggle_edit();

        state.enter_sub_wheel(Vec::new());

        assert!(!state.edit_mode());
    }

    #[test]
    fn dragging_only_starts_in_edit_mode() {
        let mut state = opened();

        state.begin_drag(Direction::North.slot());
        assert_eq!(state.dragging(), None);

        state.toggle_edit();
        state.begin_drag(Direction::North.slot());
        assert_eq!(state.dragging(), Some(Direction::North.slot()));
    }

    #[test]
    fn ending_a_drag_hands_back_where_it_started() {
        let mut state = opened();
        state.toggle_edit();
        state.begin_drag(Direction::East.slot());

        assert_eq!(state.end_drag(), Some(Direction::East.slot()));
        assert_eq!(state.end_drag(), None);
    }
}
