//! WASD selection from the keys actually held down.
//!
//! Selection follows the physical keys: hold D for east, add W for north-east, let go of
//! W and it falls back to east, let go of everything and nothing is selected. Opposite
//! keys cancel on their own axis, the way movement input usually does.

use crate::binding::{KeyCode, keys};
use crate::slot::{Cardinal, Direction, Slot};

/// Which direction keys are down right now.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Held {
    north: bool,
    east: bool,
    south: bool,
    west: bool,
}

impl Held {
    /// Idempotent, because key repeat sends the same press over and over.
    pub fn press(&mut self, key: Cardinal) {
        *self.flag(key) = true;
    }

    pub fn release(&mut self, key: Cardinal) {
        *self.flag(key) = false;
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn is_empty(self) -> bool {
        !(self.north || self.east || self.south || self.west)
    }

    /// `None` when nothing is held, or when every held key is cancelled by its opposite.
    pub fn direction(self) -> Option<Direction> {
        match (self.vertical(), self.horizontal()) {
            (Some(vertical), Some(horizontal)) => Some(diagonal(vertical, horizontal)),
            (Some(vertical), None) => Some(vertical.direction()),
            (None, Some(horizontal)) => Some(horizontal.direction()),
            (None, None) => None,
        }
    }

    pub fn slot(self) -> Option<Slot> {
        self.direction().map(Direction::slot)
    }

    fn flag(&mut self, key: Cardinal) -> &mut bool {
        match key {
            Cardinal::North => &mut self.north,
            Cardinal::East => &mut self.east,
            Cardinal::South => &mut self.south,
            Cardinal::West => &mut self.west,
        }
    }

    fn vertical(self) -> Option<Cardinal> {
        match (self.north, self.south) {
            (true, false) => Some(Cardinal::North),
            (false, true) => Some(Cardinal::South),
            _ => None,
        }
    }

    fn horizontal(self) -> Option<Cardinal> {
        match (self.east, self.west) {
            (true, false) => Some(Cardinal::East),
            (false, true) => Some(Cardinal::West),
            _ => None,
        }
    }
}

fn diagonal(vertical: Cardinal, horizontal: Cardinal) -> Direction {
    match (vertical == Cardinal::North, horizontal == Cardinal::East) {
        (true, true) => Direction::NorthEast,
        (false, true) => Direction::SouthEast,
        (false, false) => Direction::SouthWest,
        (true, false) => Direction::NorthWest,
    }
}

/// The direction a key drives selection toward. Arrows mirror WASD.
pub fn cardinal_for(key: KeyCode) -> Option<Cardinal> {
    match key {
        keys::W | keys::ARROW_UP => Some(Cardinal::North),
        keys::D | keys::ARROW_RIGHT => Some(Cardinal::East),
        keys::S | keys::ARROW_DOWN => Some(Cardinal::South),
        keys::A | keys::ARROW_LEFT => Some(Cardinal::West),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs a press and release script, reporting where selection lands at the end.
    /// `+` presses, `-` releases.
    fn after(script: &[(char, Cardinal)]) -> Option<Direction> {
        let mut held = Held::default();
        for (action, key) in script {
            match action {
                '+' => held.press(*key),
                _ => held.release(*key),
            }
        }
        held.direction()
    }

    const NORTH: Cardinal = Cardinal::North;
    const EAST: Cardinal = Cardinal::East;
    const SOUTH: Cardinal = Cardinal::South;
    const WEST: Cardinal = Cardinal::West;

    #[test]
    fn nothing_held_selects_nothing() {
        assert_eq!(Held::default().direction(), None);
        assert!(Held::default().is_empty());
    }

    #[test]
    fn one_key_selects_its_cardinal() {
        assert_eq!(after(&[('+', NORTH)]), Some(Direction::North));
        assert_eq!(after(&[('+', EAST)]), Some(Direction::East));
        assert_eq!(after(&[('+', SOUTH)]), Some(Direction::South));
        assert_eq!(after(&[('+', WEST)]), Some(Direction::West));
    }

    /// The exact sequence this model exists for.
    #[test]
    fn adding_then_dropping_a_key_falls_back_to_what_is_still_held() {
        assert_eq!(after(&[('+', EAST)]), Some(Direction::East));
        assert_eq!(
            after(&[('+', EAST), ('+', NORTH)]),
            Some(Direction::NorthEast)
        );
        assert_eq!(
            after(&[('+', EAST), ('+', NORTH), ('-', NORTH)]),
            Some(Direction::East)
        );
        assert_eq!(
            after(&[('+', EAST), ('+', NORTH), ('-', NORTH), ('-', EAST)]),
            None
        );
    }

    #[test]
    fn press_order_does_not_matter() {
        assert_eq!(
            after(&[('+', NORTH), ('+', EAST)]),
            after(&[('+', EAST), ('+', NORTH)])
        );
    }

    #[test]
    fn every_diagonal_is_reachable() {
        assert_eq!(
            after(&[('+', NORTH), ('+', EAST)]),
            Some(Direction::NorthEast)
        );
        assert_eq!(
            after(&[('+', SOUTH), ('+', EAST)]),
            Some(Direction::SouthEast)
        );
        assert_eq!(
            after(&[('+', SOUTH), ('+', WEST)]),
            Some(Direction::SouthWest)
        );
        assert_eq!(
            after(&[('+', NORTH), ('+', WEST)]),
            Some(Direction::NorthWest)
        );
    }

    #[test]
    fn opposite_keys_cancel_on_their_own_axis() {
        assert_eq!(after(&[('+', NORTH), ('+', SOUTH)]), None);
        assert_eq!(after(&[('+', EAST), ('+', WEST)]), None);
    }

    #[test]
    fn a_cancelled_axis_leaves_the_other_one_in_charge() {
        assert_eq!(
            after(&[('+', NORTH), ('+', SOUTH), ('+', EAST)]),
            Some(Direction::East)
        );
        assert_eq!(
            after(&[('+', EAST), ('+', WEST), ('+', NORTH)]),
            Some(Direction::North)
        );
    }

    #[test]
    fn all_four_held_selects_nothing() {
        assert_eq!(
            after(&[('+', NORTH), ('+', EAST), ('+', SOUTH), ('+', WEST)]),
            None
        );
    }

    #[test]
    fn dropping_one_of_a_cancelled_pair_revives_the_other() {
        assert_eq!(
            after(&[('+', NORTH), ('+', SOUTH), ('-', NORTH)]),
            Some(Direction::South)
        );
    }

    /// Key repeat sends the same press many times over.
    #[test]
    fn pressing_a_held_key_again_changes_nothing() {
        assert_eq!(
            after(&[('+', EAST), ('+', EAST), ('+', EAST)]),
            Some(Direction::East)
        );
    }

    /// A release can arrive for a key we never saw pressed, after the wheel reopens.
    #[test]
    fn releasing_a_key_that_was_not_held_is_harmless() {
        assert_eq!(after(&[('-', EAST)]), None);
        assert_eq!(after(&[('+', NORTH), ('-', EAST)]), Some(Direction::North));
    }

    #[test]
    fn clear_drops_everything() {
        let mut held = Held::default();
        held.press(NORTH);
        held.press(EAST);

        held.clear();

        assert!(held.is_empty());
        assert_eq!(held.direction(), None);
    }

    #[test]
    fn arrows_drive_the_same_cardinals_as_wasd() {
        assert_eq!(cardinal_for(keys::W), cardinal_for(keys::ARROW_UP));
        assert_eq!(cardinal_for(keys::D), cardinal_for(keys::ARROW_RIGHT));
        assert_eq!(cardinal_for(keys::S), cardinal_for(keys::ARROW_DOWN));
        assert_eq!(cardinal_for(keys::A), cardinal_for(keys::ARROW_LEFT));
    }

    #[test]
    fn wasd_maps_to_the_compass() {
        assert_eq!(cardinal_for(keys::W), Some(NORTH));
        assert_eq!(cardinal_for(keys::D), Some(EAST));
        assert_eq!(cardinal_for(keys::S), Some(SOUTH));
        assert_eq!(cardinal_for(keys::A), Some(WEST));
        assert_eq!(cardinal_for(keys::ENTER), None);
    }
}
