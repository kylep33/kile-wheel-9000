//! WASD selection that latches, so keys never need to be held together.

use crate::binding::{KeyCode, keys};
use crate::slot::{Cardinal, Direction, Slot};

/// A latched direction. Build it with [`Latch::press`], which keeps the diagonal
/// variant's pair sorted by axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Latch {
    Cardinal(Cardinal),
    Diagonal { vertical: Cardinal, horizontal: Cardinal },
}

impl Latch {
    /// A perpendicular key extends a single cardinal into a diagonal. Anything else,
    /// including the same key, its opposite, or any key while a diagonal is latched,
    /// starts fresh on the pressed key.
    pub fn press(current: Option<Self>, key: Cardinal) -> Self {
        match current {
            Some(Self::Cardinal(held)) if held.is_perpendicular_to(key) => {
                Self::diagonal(held, key)
            }
            _ => Self::Cardinal(key),
        }
    }

    pub fn direction(self) -> Direction {
        match self {
            Self::Cardinal(cardinal) => cardinal.direction(),
            Self::Diagonal {
                vertical,
                horizontal,
            } => diagonal_direction(vertical, horizontal),
        }
    }

    pub fn slot(self) -> Slot {
        self.direction().slot()
    }

    fn diagonal(a: Cardinal, b: Cardinal) -> Self {
        if a.is_vertical() {
            return Self::Diagonal {
                vertical: a,
                horizontal: b,
            };
        }
        Self::Diagonal {
            vertical: b,
            horizontal: a,
        }
    }
}

fn diagonal_direction(vertical: Cardinal, horizontal: Cardinal) -> Direction {
    match (vertical == Cardinal::North, horizontal == Cardinal::East) {
        (true, true) => Direction::NorthEast,
        (false, true) => Direction::SouthEast,
        (false, false) => Direction::SouthWest,
        (true, false) => Direction::NorthWest,
    }
}

/// The direction a key drives the latch toward. Arrows mirror WASD.
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

    /// Feeds a press sequence through the latch and reports where it lands.
    fn after(keys: &[Cardinal]) -> Direction {
        let mut latch = None;
        for key in keys {
            latch = Some(Latch::press(latch, *key));
        }
        latch.expect("sequence must hold at least one key").direction()
    }

    #[test]
    fn a_single_key_selects_its_cardinal() {
        assert_eq!(after(&[Cardinal::North]), Direction::North);
        assert_eq!(after(&[Cardinal::East]), Direction::East);
        assert_eq!(after(&[Cardinal::South]), Direction::South);
        assert_eq!(after(&[Cardinal::West]), Direction::West);
    }

    #[test]
    fn perpendicular_keys_latch_a_diagonal_in_either_order() {
        assert_eq!(after(&[Cardinal::North, Cardinal::East]), Direction::NorthEast);
        assert_eq!(after(&[Cardinal::East, Cardinal::North]), Direction::NorthEast);
    }

    #[test]
    fn every_diagonal_is_reachable() {
        assert_eq!(after(&[Cardinal::North, Cardinal::East]), Direction::NorthEast);
        assert_eq!(after(&[Cardinal::South, Cardinal::East]), Direction::SouthEast);
        assert_eq!(after(&[Cardinal::South, Cardinal::West]), Direction::SouthWest);
        assert_eq!(after(&[Cardinal::North, Cardinal::West]), Direction::NorthWest);
    }

    #[test]
    fn repeating_a_key_falls_back_to_its_cardinal() {
        assert_eq!(after(&[Cardinal::North, Cardinal::North]), Direction::North);
        assert_eq!(
            after(&[Cardinal::North, Cardinal::East, Cardinal::North]),
            Direction::North
        );
        assert_eq!(
            after(&[Cardinal::North, Cardinal::East, Cardinal::East]),
            Direction::East
        );
    }

    #[test]
    fn the_opposite_key_starts_a_fresh_selection() {
        assert_eq!(after(&[Cardinal::North, Cardinal::South]), Direction::South);
        assert_eq!(after(&[Cardinal::East, Cardinal::West]), Direction::West);
    }

    #[test]
    fn a_third_key_starts_a_fresh_selection() {
        assert_eq!(
            after(&[Cardinal::North, Cardinal::East, Cardinal::West]),
            Direction::West
        );
        assert_eq!(
            after(&[Cardinal::North, Cardinal::East, Cardinal::South]),
            Direction::South
        );
    }

    #[test]
    fn a_diagonal_can_be_rebuilt_after_a_reset() {
        assert_eq!(
            after(&[Cardinal::North, Cardinal::East, Cardinal::South, Cardinal::West]),
            Direction::SouthWest
        );
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
        assert_eq!(cardinal_for(keys::W), Some(Cardinal::North));
        assert_eq!(cardinal_for(keys::D), Some(Cardinal::East));
        assert_eq!(cardinal_for(keys::S), Some(Cardinal::South));
        assert_eq!(cardinal_for(keys::A), Some(Cardinal::West));
        assert_eq!(cardinal_for(keys::ENTER), None);
    }
}
