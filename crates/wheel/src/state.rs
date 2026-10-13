//! What the wheel is currently showing.

use kile_wheel_core::select::Held;
use kile_wheel_core::slot::{Cardinal, Slot, Tab};
use windows::Win32::Foundation::HWND;

use crate::anim::Animation;
use crate::slots::NewWindow;

/// One window of an app that has more than one open.
#[derive(Debug, Clone)]
pub struct WindowChoice {
    pub handle: HWND,
    pub title: String,
}

impl PartialEq for WindowChoice {
    fn eq(&self, other: &Self) -> bool {
        self.handle == other.handle
    }
}

impl Eq for WindowChoice {}

/// One slice of the sub-wheel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubChoice {
    Window(WindowChoice),
    /// Opens one more window of the app, rather than focusing one it already has.
    NewWindow(NewWindow),
}

/// What an app's sub-wheel shows, and how many windows had to be left off it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubWheel {
    pub choices: Vec<SubChoice>,
    pub hidden: usize,
}

impl SubWheel {
    /// The app's windows, oldest first, then a way to open another.
    ///
    /// The new-window slice always gets a slot, so with more windows than fit it is the
    /// last window that goes rather than the way to open one more. The hidden count says
    /// how many were left off, so none disappears without a word.
    pub fn new(windows: Vec<WindowChoice>, new_window: Option<NewWindow>) -> Self {
        let room = match new_window.is_some() {
            true => Slot::COUNT - 1,
            false => Slot::COUNT,
        };
        let hidden = windows.len().saturating_sub(room);

        let mut choices: Vec<SubChoice> = windows
            .into_iter()
            .take(room)
            .map(SubChoice::Window)
            .collect();
        choices.extend(new_window.map(SubChoice::NewWindow));

        Self { choices, hidden }
    }

    /// The new-window slice, wherever it landed, for Shift+Enter and middle click.
    pub fn new_window(&self) -> Option<&NewWindow> {
        self.choices.iter().find_map(|choice| match choice {
            SubChoice::NewWindow(new) => Some(new),
            SubChoice::Window(_) => None,
        })
    }
}

/// Which layer of the wheel is in front.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Wheel,
    /// One app's windows, reached by activating an app with more than one open.
    SubWheel(SubWheel),
}

/// Which device last set the selection. They are allowed to mix, so the last one to move
/// owns it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Pointer {
    #[default]
    Keys,
    Mouse,
}

/// What backing out one level did, so the caller knows whether to redraw or hide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Back {
    WentUp,
    Closed,
}

/// What the selection is moving from and to, so the renderer can light one wedge while the
/// other goes out instead of snapping between them.
#[derive(Debug, Default, Clone, Copy)]
pub struct Hover {
    pub from: Option<Slot>,
    pub to: Option<Slot>,
    pub moved: Animation,
}

impl Hover {
    /// How lit `slot` should be, from 0 to 1. Anything that is neither end of the move is
    /// unlit, and a slot that is both ends stays lit.
    pub fn highlight(&self, slot: Slot, duration_ms: u64) -> f32 {
        let progress = self.moved.progress(duration_ms);

        match (self.to == Some(slot), self.from == Some(slot)) {
            (true, _) => progress,
            (false, true) => 1.0 - progress,
            (false, false) => 0.0,
        }
    }

    fn running(&self, duration_ms: u64) -> bool {
        self.from != self.to && self.moved.running(duration_ms)
    }
}

/// Selection and mode.
#[derive(Debug, Default)]
pub struct WheelState {
    open: bool,
    tab: Tab,
    tab_count: usize,
    selected: Option<Slot>,
    held: Held,
    pointer: Pointer,
    mode: Mode,
    edit: bool,
    dragging: Option<Slot>,
    hover: Hover,
    /// Restarted whenever a whole new wheel appears, which is an open, a tab change or a
    /// step into or out of a sub-wheel.
    appeared: Animation,
}

impl WheelState {
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn selected(&self) -> Option<Slot> {
        self.selected
    }

    pub fn hover(&self) -> Hover {
        self.hover
    }

    /// How far the wheel has grown in, from 0 to 1.
    pub fn appear_progress(&self, duration_ms: u64) -> f32 {
        self.appeared.progress(duration_ms)
    }

    /// Whether any transition still needs frames drawn for it.
    pub fn animating(&self, duration_ms: u64) -> bool {
        self.appeared.running(duration_ms) || self.hover.running(duration_ms)
    }

    pub fn tab(&self) -> Tab {
        self.tab
    }

    pub fn mode(&self) -> &Mode {
        &self.mode
    }

    pub fn in_sub_wheel(&self) -> bool {
        matches!(self.mode, Mode::SubWheel(_))
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
        self.appeared.restart();
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
            self.selected = self.held.slot();
        }
        true
    }

    /// Selection follows the keys that are down, so this is the same whether the key is
    /// new or a key-repeat.
    pub fn press_direction(&mut self, key: Cardinal) {
        self.held.press(key);
        self.pointer = Pointer::Keys;
        let held = self.held.slot();
        self.set_selection(held);
    }

    /// Letting go falls back to whatever is still held, and letting go of everything
    /// deselects.
    ///
    /// A release after the mouse has taken over only updates the held set, because
    /// yanking the mouse's pick away would be a surprise.
    pub fn release_direction(&mut self, key: Cardinal) {
        self.held.release(key);
        if self.pointer != Pointer::Keys {
            return;
        }
        let held = self.held.slot();
        self.set_selection(held);
    }

    /// Mouse takes the selection over. Inside the hub there is no angle to read, so the
    /// current pick stands.
    pub fn aim_at(&mut self, slot: Option<Slot>) {
        let Some(slot) = slot else {
            return;
        };

        self.pointer = Pointer::Mouse;
        self.set_selection(Some(slot));
    }

    /// Edit mode belongs to the wheel, so a sub-wheel leaves it behind.
    pub fn enter_sub_wheel(&mut self, sub_wheel: SubWheel) {
        self.mode = Mode::SubWheel(sub_wheel);
        self.appeared.restart();
        self.edit = false;
        self.dragging = None;
        self.reset_selection();
    }

    /// Decides what backing out one level means, and reports it.
    ///
    /// Closing is left to the caller rather than done here, because the close sequence
    /// also has to tell the input hook to stop swallowing keys. Splitting that across two
    /// places is what let the wheel stay up with the keyboard captured.
    pub fn back(&mut self) -> Back {
        if self.in_sub_wheel() {
            self.mode = Mode::Wheel;
            self.appeared.restart();
            self.reset_selection();
            return Back::WentUp;
        }
        if self.edit {
            self.edit = false;
            self.dragging = None;
            return Back::WentUp;
        }

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
        self.set_selection(None);
        self.held.clear();
        self.pointer = Pointer::Keys;
    }

    /// The one place the selection changes, so the hover transition can never be missed.
    fn set_selection(&mut self, next: Option<Slot>) {
        if self.selected == next {
            return;
        }

        self.hover = Hover {
            from: self.selected,
            to: next,
            moved: Animation::started(),
        };
        self.selected = next;
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
    fn selection_follows_the_keys_held_down() {
        let mut state = opened();

        state.press_direction(Cardinal::East);
        assert_eq!(state.selected(), Some(Direction::East.slot()));

        state.press_direction(Cardinal::North);
        assert_eq!(state.selected(), Some(Direction::NorthEast.slot()));

        state.release_direction(Cardinal::North);
        assert_eq!(state.selected(), Some(Direction::East.slot()));

        state.release_direction(Cardinal::East);
        assert_eq!(state.selected(), None);
    }

    #[test]
    fn the_mouse_takes_the_selection_over() {
        let mut state = opened();
        state.press_direction(Cardinal::North);

        state.aim_at(Some(Direction::South.slot()));

        assert_eq!(state.selected(), Some(Direction::South.slot()));
    }

    /// Releasing a key after the mouse has taken over must not yank the pick away.
    #[test]
    fn a_release_after_the_mouse_moved_leaves_the_pick_alone() {
        let mut state = opened();
        state.press_direction(Cardinal::North);
        state.aim_at(Some(Direction::South.slot()));

        state.release_direction(Cardinal::North);

        assert_eq!(state.selected(), Some(Direction::South.slot()));
    }

    #[test]
    fn a_key_press_takes_the_selection_back_from_the_mouse() {
        let mut state = opened();
        state.aim_at(Some(Direction::South.slot()));

        state.press_direction(Cardinal::West);

        assert_eq!(state.selected(), Some(Direction::West.slot()));
    }

    #[test]
    fn aiming_into_the_hub_keeps_the_current_pick() {
        let mut state = opened();
        state.press_direction(Cardinal::West);

        state.aim_at(None);

        assert_eq!(state.selected(), Some(Direction::West.slot()));
    }

    #[test]
    fn opposite_keys_held_together_deselect() {
        let mut state = opened();

        state.press_direction(Cardinal::North);
        state.press_direction(Cardinal::South);

        assert_eq!(state.selected(), None);
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

    /// Switching tabs with nothing held leaves nothing selected on the new tab.
    #[test]
    fn switching_tabs_keeps_whatever_is_still_held() {
        let mut state = opened();
        state.press_direction(Cardinal::North);
        state.release_direction(Cardinal::North);

        state.select_tab(2);
        assert_eq!(state.selected(), None);

        state.press_direction(Cardinal::East);
        state.select_tab(3);
        assert_eq!(state.selected(), Some(Direction::East.slot()));
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
        state.enter_sub_wheel(SubWheel::default());

        assert_eq!(state.back(), Back::WentUp);
        assert!(state.is_open());
        assert_eq!(state.back(), Back::Closed);
    }

    #[test]
    fn backspace_leaves_edit_mode_before_closing() {
        let mut state = opened();
        state.toggle_edit();

        assert_eq!(state.back(), Back::WentUp);
        assert!(!state.edit_mode());
        assert_eq!(state.back(), Back::Closed);
    }

    /// `back` must not close the state itself. The caller owns closing, because closing
    /// also has to tell the input hook to stop swallowing keys; when this method did half
    /// of it, the caller's own guard skipped the other half and the keyboard stayed
    /// captured.
    #[test]
    fn back_reports_closed_without_closing() {
        let mut state = opened();

        assert_eq!(state.back(), Back::Closed);
        assert!(state.is_open());
    }

    #[test]
    fn closing_twice_is_harmless() {
        let mut state = opened();

        state.close();
        state.close();

        assert!(!state.is_open());
        assert_eq!(state.selected(), None);
    }

    #[test]
    fn a_sub_wheel_cannot_be_edited() {
        let mut state = opened();
        state.enter_sub_wheel(SubWheel::default());

        assert!(!state.toggle_edit());
        assert!(!state.select_tab(2));
    }

    #[test]
    fn entering_a_sub_wheel_drops_edit_mode() {
        let mut state = opened();
        state.toggle_edit();

        state.enter_sub_wheel(SubWheel::default());

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

    /// Animations off, so the transition is always finished and the lit wedge is the new one.
    const INSTANT_MS: u64 = 0;

    /// Long enough that the transition is still near its start.
    const SLOW_MS: u64 = 10_000;

    #[test]
    fn the_wedge_being_moved_to_is_lit_once_the_move_finishes() {
        let mut state = opened();
        state.press_direction(Cardinal::East);

        let hover = state.hover();
        assert_eq!(hover.highlight(Direction::East.slot(), INSTANT_MS), 1.0);
    }

    #[test]
    fn the_wedge_being_moved_off_gives_up_its_light() {
        let mut state = opened();
        state.press_direction(Cardinal::East);
        state.press_direction(Cardinal::North);

        let hover = state.hover();
        assert_eq!(hover.highlight(Direction::East.slot(), INSTANT_MS), 0.0);
    }

    #[test]
    fn a_wedge_the_move_does_not_touch_stays_dark() {
        let mut state = opened();
        state.press_direction(Cardinal::East);

        let hover = state.hover();
        assert_eq!(hover.highlight(Direction::South.slot(), INSTANT_MS), 0.0);
    }

    #[test]
    fn the_light_crosses_over_partway_through_the_move() {
        let mut state = opened();
        state.press_direction(Cardinal::East);
        state.press_direction(Cardinal::North);

        let hover = state.hover();
        let leaving = hover.highlight(Direction::East.slot(), SLOW_MS);
        let arriving = hover.highlight(Direction::North.slot(), SLOW_MS);

        assert!(
            leaving > arriving,
            "the old wedge should still be the brighter one"
        );
        assert!(
            (leaving + arriving - 1.0).abs() < f32::EPSILON,
            "the pair should sum to one"
        );
    }

    #[test]
    fn re_aiming_at_the_same_slot_does_not_restart_the_move() {
        let mut state = opened();
        state.aim_at(Some(Direction::East.slot()));
        let first = state.hover();

        state.aim_at(Some(Direction::East.slot()));

        assert_eq!(state.hover().from, first.from);
    }

    #[test]
    fn a_wheel_that_just_opened_is_still_growing_in() {
        let state = opened();

        assert!(state.appear_progress(SLOW_MS) < 1.0);
        assert!(state.animating(SLOW_MS));
    }

    #[test]
    fn nothing_is_animating_once_animations_are_switched_off() {
        let state = opened();

        assert_eq!(state.appear_progress(INSTANT_MS), 1.0);
        assert!(!state.animating(INSTANT_MS));
    }
}
