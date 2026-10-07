//! What the wheel is currently showing.

use kile_wheel_core::select::Latch;
use kile_wheel_core::slot::{Cardinal, Slot};

/// Selection state. Mouse and keyboard both write it; whichever moved last wins.
#[derive(Debug, Default)]
pub struct WheelState {
    open: bool,
    selected: Option<Slot>,
    latch: Option<Latch>,
}

impl WheelState {
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn selected(&self) -> Option<Slot> {
        self.selected
    }

    /// Each open starts with nothing selected, so a stale pick cannot be activated.
    pub fn open(&mut self) {
        self.open = true;
        self.selected = None;
        self.latch = None;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.selected = None;
        self.latch = None;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use kile_wheel_core::slot::Direction;

    #[test]
    fn opening_clears_any_previous_pick() {
        let mut state = WheelState::default();
        state.open();
        state.press_direction(Cardinal::East);
        state.close();

        state.open();

        assert_eq!(state.selected(), None);
    }

    #[test]
    fn the_mouse_resets_the_latch_so_keys_start_over() {
        let mut state = WheelState::default();
        state.open();
        state.press_direction(Cardinal::North);
        state.aim_at(Some(Direction::South.slot()));

        state.press_direction(Cardinal::East);

        assert_eq!(state.selected(), Some(Direction::East.slot()));
    }

    #[test]
    fn aiming_into_the_hub_keeps_the_current_pick() {
        let mut state = WheelState::default();
        state.open();
        state.press_direction(Cardinal::West);

        state.aim_at(None);

        assert_eq!(state.selected(), Some(Direction::West.slot()));
    }

    #[test]
    fn keys_still_latch_diagonals_through_the_state() {
        let mut state = WheelState::default();
        state.open();

        state.press_direction(Cardinal::South);
        state.press_direction(Cardinal::East);

        assert_eq!(state.selected(), Some(Direction::SouthEast.slot()));
    }
}
