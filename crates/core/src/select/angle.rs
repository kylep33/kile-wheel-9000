//! Cursor angle to slot, so selection needs direction rather than aim.

use std::f32::consts::TAU;

use crate::slot::Slot;

/// One eighth of a turn, the angular width of a slot.
pub const SECTOR_RAD: f32 = TAU / Slot::COUNT as f32;
const HALF_SECTOR_RAD: f32 = SECTOR_RAD / 2.0;

/// Cursor position relative to the wheel center, in pixels, y growing downward.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Offset {
    pub x: f32,
    pub y: f32,
}

impl Offset {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn distance(self) -> f32 {
        self.x.hypot(self.y)
    }

    /// Zero at north, increasing clockwise, in `0..TAU`.
    fn bearing_rad(self) -> f32 {
        let raw = self.x.atan2(-self.y);
        if raw < 0.0 { raw + TAU } else { raw }
    }
}

/// The slot the cursor points at, or `None` inside the hub dead zone.
pub fn slot_at(offset: Offset, dead_zone_px: f32) -> Option<Slot> {
    if offset.distance() < dead_zone_px {
        return None;
    }

    // the half-sector shift centers slot 0 on north instead of starting it there
    let sector = (offset.bearing_rad() + HALF_SECTOR_RAD) / SECTOR_RAD;
    Some(Slot::wrapping(sector as usize))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slot::Direction;

    const NO_DEAD_ZONE_PX: f32 = 0.0;
    const HUB_RADIUS_PX: f32 = 40.0;
    const REACH_PX: f32 = 100.0;

    fn slot_toward(x: f32, y: f32) -> Slot {
        slot_at(Offset::new(x, y), NO_DEAD_ZONE_PX).expect("outside the dead zone")
    }

    #[test]
    fn cardinals_land_on_their_slots() {
        assert_eq!(slot_toward(0.0, -REACH_PX), Direction::North.slot());
        assert_eq!(slot_toward(REACH_PX, 0.0), Direction::East.slot());
        assert_eq!(slot_toward(0.0, REACH_PX), Direction::South.slot());
        assert_eq!(slot_toward(-REACH_PX, 0.0), Direction::West.slot());
    }

    #[test]
    fn diagonals_land_on_their_slots() {
        assert_eq!(
            slot_toward(REACH_PX, -REACH_PX),
            Direction::NorthEast.slot()
        );
        assert_eq!(slot_toward(REACH_PX, REACH_PX), Direction::SouthEast.slot());
        assert_eq!(
            slot_toward(-REACH_PX, REACH_PX),
            Direction::SouthWest.slot()
        );
        assert_eq!(
            slot_toward(-REACH_PX, -REACH_PX),
            Direction::NorthWest.slot()
        );
    }

    #[test]
    fn every_sector_center_maps_to_a_distinct_slot() {
        let hits: Vec<Slot> = (0..Slot::COUNT)
            .map(|index| {
                let bearing = SECTOR_RAD * index as f32;
                slot_toward(bearing.sin() * REACH_PX, -bearing.cos() * REACH_PX)
            })
            .collect();

        assert_eq!(hits, Slot::all().collect::<Vec<Slot>>());
    }

    #[test]
    fn a_bearing_just_short_of_north_still_picks_north() {
        let barely_west_of_north = -HALF_SECTOR_RAD * 0.99;
        let hit = slot_toward(
            barely_west_of_north.sin() * REACH_PX,
            -barely_west_of_north.cos() * REACH_PX,
        );

        assert_eq!(hit, Direction::North.slot());
    }

    #[test]
    fn the_dead_zone_reports_no_slot() {
        assert_eq!(slot_at(Offset::new(0.0, 0.0), HUB_RADIUS_PX), None);
        assert_eq!(
            slot_at(Offset::new(0.0, -HUB_RADIUS_PX + 1.0), HUB_RADIUS_PX),
            None
        );
    }

    #[test]
    fn the_dead_zone_edge_selects() {
        let hit = slot_at(Offset::new(0.0, -HUB_RADIUS_PX), HUB_RADIUS_PX);

        assert_eq!(hit, Some(Direction::North.slot()));
    }
}
