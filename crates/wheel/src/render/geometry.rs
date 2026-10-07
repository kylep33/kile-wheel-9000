//! The eight wedge outlines, built once per size rather than once per frame.

use kile_wheel_core::select::SECTOR_RAD;
use kile_wheel_core::slot::Slot;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_SIZE_F, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_SMALL, D2D1_SWEEP_DIRECTION, D2D1_SWEEP_DIRECTION_CLOCKWISE,
    D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE, ID2D1Factory1, ID2D1PathGeometry1,
};
use windows::core::Result;
use windows_numerics::Vector2;

use crate::theme::Metrics;

const HALF_SECTOR_RAD: f32 = SECTOR_RAD / 2.0;
const NO_ROTATION_DEG: f32 = 0.0;

/// A point on a circle around `centre`, with zero bearing at north and the angle
/// increasing clockwise, matching how slots are numbered.
pub fn on_circle(centre: Vector2, radius_px: f32, bearing_rad: f32) -> Vector2 {
    Vector2 {
        X: centre.X + bearing_rad.sin() * radius_px,
        Y: centre.Y - bearing_rad.cos() * radius_px,
    }
}

/// Bearing of the middle of a slot's wedge.
pub fn slot_bearing_rad(slot: Slot) -> f32 {
    SECTOR_RAD * slot.index() as f32
}

/// Bearing of the divider that starts a slot's wedge.
pub fn slot_edge_rad(slot: Slot) -> f32 {
    slot_bearing_rad(slot) - HALF_SECTOR_RAD
}

/// One annular sector per slot, in window coordinates.
#[derive(Debug)]
pub struct Wedges {
    sectors: Vec<ID2D1PathGeometry1>,
}

impl Wedges {
    pub fn build(factory: &ID2D1Factory1, centre: Vector2, metrics: Metrics) -> Result<Self> {
        let mut sectors = Vec::with_capacity(Slot::COUNT);
        for slot in Slot::all() {
            sectors.push(annular_sector(
                factory,
                centre,
                metrics.hub_radius_px,
                metrics.radius_px,
                slot_edge_rad(slot),
            )?);
        }

        Ok(Self { sectors })
    }

    pub fn get(&self, slot: Slot) -> Option<&ID2D1PathGeometry1> {
        self.sectors.get(slot.index())
    }
}

/// A wedge running one sector clockwise from `start_rad`, hollowed out by the hub.
fn annular_sector(
    factory: &ID2D1Factory1,
    centre: Vector2,
    inner_px: f32,
    outer_px: f32,
    start_rad: f32,
) -> Result<ID2D1PathGeometry1> {
    let end_rad = start_rad + SECTOR_RAD;

    let inner_start = on_circle(centre, inner_px, start_rad);
    let inner_end = on_circle(centre, inner_px, end_rad);
    let outer_start = on_circle(centre, outer_px, start_rad);
    let outer_end = on_circle(centre, outer_px, end_rad);

    // SAFETY: the sink is opened, filled and closed before the geometry is returned.
    unsafe {
        let geometry = factory.CreatePathGeometry()?;
        let sink = geometry.Open()?;

        sink.BeginFigure(inner_start, D2D1_FIGURE_BEGIN_FILLED);
        sink.AddLine(outer_start);
        sink.AddArc(&arc(outer_end, outer_px, D2D1_SWEEP_DIRECTION_CLOCKWISE));
        sink.AddLine(inner_end);
        sink.AddArc(&arc(
            inner_start,
            inner_px,
            D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE,
        ));
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        sink.Close()?;

        Ok(geometry)
    }
}

fn arc(to: Vector2, radius_px: f32, direction: D2D1_SWEEP_DIRECTION) -> D2D1_ARC_SEGMENT {
    D2D1_ARC_SEGMENT {
        point: to,
        size: D2D_SIZE_F {
            width: radius_px,
            height: radius_px,
        },
        rotationAngle: NO_ROTATION_DEG,
        sweepDirection: direction,
        // a slot is an eighth of a turn, so the short way round is always right
        arcSize: D2D1_ARC_SIZE_SMALL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kile_wheel_core::slot::Direction;

    const CENTRE: Vector2 = Vector2 { X: 100.0, Y: 100.0 };
    const RADIUS_PX: f32 = 50.0;
    const TOLERANCE_PX: f32 = 0.001;

    #[test]
    fn zero_bearing_points_north() {
        let point = on_circle(CENTRE, RADIUS_PX, 0.0);

        assert!((point.X - CENTRE.X).abs() < TOLERANCE_PX);
        assert!((point.Y - (CENTRE.Y - RADIUS_PX)).abs() < TOLERANCE_PX);
    }

    #[test]
    fn bearings_increase_clockwise() {
        let east = on_circle(CENTRE, RADIUS_PX, slot_bearing_rad(Direction::East.slot()));

        assert!((east.X - (CENTRE.X + RADIUS_PX)).abs() < TOLERANCE_PX);
        assert!((east.Y - CENTRE.Y).abs() < TOLERANCE_PX);
    }

    #[test]
    fn a_wedge_is_centred_on_its_slot_bearing() {
        let slot = Direction::SouthEast.slot();

        let span = slot_bearing_rad(slot) - slot_edge_rad(slot);
        assert!((span - HALF_SECTOR_RAD).abs() < TOLERANCE_PX);
    }

    #[test]
    fn the_eight_wedges_tile_the_whole_turn() {
        let total: f32 = Slot::all().map(|_| SECTOR_RAD).sum();

        assert!((total - std::f32::consts::TAU).abs() < TOLERANCE_PX);
    }
}
