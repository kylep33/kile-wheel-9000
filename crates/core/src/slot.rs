//! Wheel geometry: the fixed slot count and the directions that address it.

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

    pub fn index(self) -> usize {
        usize::from(self.0)
    }

    pub fn all() -> impl Iterator<Item = Self> {
        (0..Self::COUNT as u8).map(Self)
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
    fn all_yields_every_slot_once_in_order() {
        let indices: Vec<usize> = Slot::all().map(Slot::index).collect();

        assert_eq!(indices, (0..Slot::COUNT).collect::<Vec<usize>>());
    }

    #[test]
    fn perpendicular_pairs_cross_axes() {
        assert!(Cardinal::North.is_perpendicular_to(Cardinal::East));
        assert!(Cardinal::West.is_perpendicular_to(Cardinal::South));
        assert!(!Cardinal::North.is_perpendicular_to(Cardinal::South));
        assert!(!Cardinal::East.is_perpendicular_to(Cardinal::East));
    }
}
