//! Wheel geometry: the fixed slot count, the tabs that hold them, and the directions
//! that address them.

/// A position on the wheel. Slot 0 is north and the rest run clockwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Slot(u8);

impl Slot {
    /// The wheel is always this many slots, whatever the content.
    pub const COUNT: usize = 8;

    /// Wraps, so a sector index past the last slot lands back on the first.
    pub fn wrapping(index: usize) -> Self {
        Self((index % Self::COUNT) as u8)
    }

    pub fn new(index: usize) -> Option<Self> {
        (index < Self::COUNT).then_some(Self(index as u8))
    }

    pub fn index(self) -> usize {
        usize::from(self.0)
    }

    pub fn all() -> impl Iterator<Item = Self> {
        (0..Self::COUNT as u8).map(Self)
    }
}

/// A page of eight slots. Numbered from 1 because that is how the config spells it and
/// how the number keys select it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tab(u8);

impl Tab {
    /// One per number key along the top row, 1 through 9 then 0 for the tenth.
    pub const MAX_COUNT: usize = 10;
    pub const FIRST: Self = Self(0);

    pub fn from_number(number: usize) -> Option<Self> {
        let index = number.checked_sub(1)?;
        Self::from_index(index)
    }

    pub fn from_index(index: usize) -> Option<Self> {
        (index < Self::MAX_COUNT).then_some(Self(index as u8))
    }

    pub fn index(self) -> usize {
        usize::from(self.0)
    }

    pub fn number(self) -> usize {
        self.index() + 1
    }
}

/// Compass direction of a slot. Declaration order is slot order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
}

impl Direction {
    pub fn slot(self) -> Slot {
        Slot(self as u8)
    }
}

/// One of the four direction keys. Diagonals are built by combining two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cardinal {
    North,
    East,
    South,
    West,
}

impl Cardinal {
    pub fn is_vertical(self) -> bool {
        matches!(self, Self::North | Self::South)
    }

    pub fn is_perpendicular_to(self, other: Self) -> bool {
        self.is_vertical() != other.is_vertical()
    }

    pub fn direction(self) -> Direction {
        match self {
            Self::North => Direction::North,
            Self::East => Direction::East,
            Self::South => Direction::South,
            Self::West => Direction::West,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_order_matches_slot_order() {
        assert_eq!(Direction::North.slot().index(), 0);
        assert_eq!(Direction::East.slot().index(), 2);
        assert_eq!(Direction::South.slot().index(), 4);
        assert_eq!(Direction::NorthWest.slot().index(), 7);
    }

    #[test]
    fn slot_index_wraps_past_the_last_slot() {
        assert_eq!(Slot::wrapping(Slot::COUNT).index(), 0);
        assert_eq!(Slot::wrapping(Slot::COUNT + 3).index(), 3);
    }

    #[test]
    fn slot_new_rejects_out_of_range() {
        assert_eq!(Slot::new(Slot::COUNT - 1).map(Slot::index), Some(7));
        assert_eq!(Slot::new(Slot::COUNT), None);
    }

    #[test]
    fn all_yields_every_slot_once_in_order() {
        let indices: Vec<usize> = Slot::all().map(Slot::index).collect();

        assert_eq!(indices, (0..Slot::COUNT).collect::<Vec<usize>>());
    }

    #[test]
    fn tabs_are_numbered_from_one() {
        assert_eq!(Tab::from_number(1), Some(Tab::FIRST));
        assert_eq!(Tab::from_number(1).map(Tab::index), Some(0));
        assert_eq!(Tab::from_number(9).map(Tab::index), Some(8));
    }

    #[test]
    fn tab_numbers_outside_one_to_nine_are_rejected() {
        assert_eq!(Tab::from_number(0), None);
        assert_eq!(Tab::from_number(Tab::MAX_COUNT + 1), None);
    }

    #[test]
    fn perpendicular_pairs_cross_axes() {
        assert!(Cardinal::North.is_perpendicular_to(Cardinal::East));
        assert!(Cardinal::West.is_perpendicular_to(Cardinal::South));
        assert!(!Cardinal::North.is_perpendicular_to(Cardinal::South));
        assert!(!Cardinal::East.is_perpendicular_to(Cardinal::East));
    }
}
