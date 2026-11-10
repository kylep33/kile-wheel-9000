//! Wedge and petal outlines as vector paths, in window units.

use kile_wheel_core::select::SECTOR_RAD;
use kile_wheel_core::slot::Slot;
use resvg::tiny_skia::{Path, PathBuilder};

use crate::geom::{Point, Rect};
use crate::theme::Metrics;

const HALF_SECTOR_RAD: f32 = SECTOR_RAD / 2.0;

/// A cubic is within a hair of a circle up to a quarter turn, so longer arcs are split.
const MAX_ARC_SEGMENT_RAD: f32 = std::f32::consts::FRAC_PI_2;

/// Control-point distance for a circular arc of `sweep` on a unit circle.
const ARC_HANDLE_RATIO: f32 = 4.0 / 3.0;

/// A point on a circle around `centre`, with zero bearing at north and the angle
/// increasing clockwise, matching how slots are numbered.
pub fn on_circle(centre: Point, radius_px: f32, bearing_rad: f32) -> Point {
    Point {
        x: centre.x + bearing_rad.sin() * radius_px,
        y: centre.y - bearing_rad.cos() * radius_px,
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

/// One annular sector per slot, built once per size rather than once per frame.
#[derive(Debug)]
pub struct Wedges {
    sectors: Vec<Path>,
}

impl Wedges {
    pub fn build(centre: Point, metrics: Metrics) -> Self {
        let sectors = Slot::all()
            .filter_map(|slot| {
                let start = slot_edge_rad(slot);
                annular_sector(
                    centre,
                    metrics.hub_radius_px,
                    metrics.radius_px,
                    start,
                    start + SECTOR_RAD,
                )
            })
            .collect();

        Self { sectors }
    }

    pub fn get(&self, slot: Slot) -> Option<&Path> {
        self.sectors.get(slot.index())
    }
}

/// A ring segment from `start_rad` clockwise to `end_rad`, hollowed out to `inner_px`.
/// `None` only for a degenerate shape, which tiny-skia refuses to build.
pub fn annular_sector(
    centre: Point,
    inner_px: f32,
    outer_px: f32,
    start_rad: f32,
    end_rad: f32,
) -> Option<Path> {
    let inner_start = on_circle(centre, inner_px, start_rad);
    let outer_start = on_circle(centre, outer_px, start_rad);
    let inner_end = on_circle(centre, inner_px, end_rad);

    let mut builder = PathBuilder::new();
    builder.move_to(inner_start.x, inner_start.y);
    builder.line_to(outer_start.x, outer_start.y);
    arc_to(&mut builder, centre, outer_px, start_rad, end_rad);
    builder.line_to(inner_end.x, inner_end.y);
    arc_to(&mut builder, centre, inner_px, end_rad, start_rad);
    builder.close();
    builder.finish()
}

/// A ring segment whose inner and outer arcs span different angles, which is the petal's
/// neck: narrow where it leaves the slice, wider where it meets the panel.
pub fn tapered_sector(
    centre: Point,
    inner_px: f32,
    outer_px: f32,
    bearing_rad: f32,
    inner_half_rad: f32,
    outer_half_rad: f32,
) -> Option<Path> {
    let inner_start = on_circle(centre, inner_px, bearing_rad - inner_half_rad);
    let outer_start = on_circle(centre, outer_px, bearing_rad - outer_half_rad);
    let inner_end = on_circle(centre, inner_px, bearing_rad + inner_half_rad);

    let mut builder = PathBuilder::new();
    builder.move_to(inner_start.x, inner_start.y);
    builder.line_to(outer_start.x, outer_start.y);
    arc_to(
        &mut builder,
        centre,
        outer_px,
        bearing_rad - outer_half_rad,
        bearing_rad + outer_half_rad,
    );
    builder.line_to(inner_end.x, inner_end.y);
    arc_to(
        &mut builder,
        centre,
        inner_px,
        bearing_rad + inner_half_rad,
        bearing_rad - inner_half_rad,
    );
    builder.close();
    builder.finish()
}

pub fn rounded_rect(rect: Rect, corner_px: f32) -> Option<Path> {
    let corner = corner_px.min(rect.width() / 2.0).min(rect.height() / 2.0).max(0.0);
    let Rect {
        left,
        top,
        right,
        bottom,
    } = rect;

    // each corner is a quarter turn, drawn clockwise from the top edge
    let mut builder = PathBuilder::new();
    builder.move_to(left + corner, top);
    builder.line_to(right - corner, top);
    corner_to(&mut builder, Point { x: right - corner, y: top + corner }, corner, 0.0);
    builder.line_to(right, bottom - corner);
    corner_to(
        &mut builder,
        Point {
            x: right - corner,
            y: bottom - corner,
        },
        corner,
        std::f32::consts::FRAC_PI_2,
    );
    builder.line_to(left + corner, bottom);
    corner_to(
        &mut builder,
        Point {
            x: left + corner,
            y: bottom - corner,
        },
        corner,
        std::f32::consts::PI,
    );
    builder.line_to(left, top + corner);
    corner_to(
        &mut builder,
        Point {
            x: left + corner,
            y: top + corner,
        },
        corner,
        3.0 * std::f32::consts::FRAC_PI_2,
    );
    builder.close();
    builder.finish()
}

/// A quarter turn clockwise around `centre`, starting at `from_rad` in bearing terms.
fn corner_to(builder: &mut PathBuilder, centre: Point, radius_px: f32, from_rad: f32) {
    arc_to(
        builder,
        centre,
        radius_px,
        from_rad,
        from_rad + std::f32::consts::FRAC_PI_2,
    );
}

/// Continues the path along a circle from one bearing to another. Clockwise when `to`
/// is larger, counter-clockwise when it is smaller.
fn arc_to(builder: &mut PathBuilder, centre: Point, radius_px: f32, from_rad: f32, to_rad: f32) {
    let sweep = to_rad - from_rad;
    let segments = (sweep.abs() / MAX_ARC_SEGMENT_RAD).ceil().max(1.0) as usize;
    let step = sweep / segments as f32;

    for index in 0..segments {
        let start = from_rad + step * index as f32;
        arc_segment(builder, centre, radius_px, start, start + step);
    }
}

fn arc_segment(builder: &mut PathBuilder, centre: Point, radius_px: f32, from: f32, to: f32) {
    let handle = ARC_HANDLE_RATIO * ((to - from) / 4.0).tan() * radius_px;
    let start = on_circle(centre, radius_px, from);
    let end = on_circle(centre, radius_px, to);

    // the tangent of on_circle at a bearing points along (cos, sin)
    builder.cubic_to(
        start.x + handle * from.cos(),
        start.y + handle * from.sin(),
        end.x - handle * to.cos(),
        end.y - handle * to.sin(),
        end.x,
        end.y,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use kile_wheel_core::slot::Direction;

    const CENTRE: Point = Point { x: 100.0, y: 100.0 };
    const RADIUS_PX: f32 = 50.0;
    const TOLERANCE_PX: f32 = 0.001;
    /// A cubic arc strays from the circle by about 0.03% of the radius.
    const ARC_TOLERANCE_PX: f32 = 0.05;

    #[test]
    fn zero_bearing_points_north() {
        let point = on_circle(CENTRE, RADIUS_PX, 0.0);

        assert!((point.x - CENTRE.x).abs() < TOLERANCE_PX);
        assert!((point.y - (CENTRE.y - RADIUS_PX)).abs() < TOLERANCE_PX);
    }

    #[test]
    fn bearings_increase_clockwise() {
        let east = on_circle(CENTRE, RADIUS_PX, slot_bearing_rad(Direction::East.slot()));

        assert!((east.x - (CENTRE.x + RADIUS_PX)).abs() < TOLERANCE_PX);
        assert!((east.y - CENTRE.y).abs() < TOLERANCE_PX);
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

    #[test]
    fn every_wedge_builds() {
        let metrics = Metrics::new(1.0, 1.0);
        let wedges = Wedges::build(CENTRE, metrics);

        assert!(Slot::all().all(|slot| wedges.get(slot).is_some()));
    }

    /// The whole wheel is a full turn of wedges, so its bounds are the outer circle's.
    #[test]
    fn a_half_turn_arc_reaches_the_circle_and_no_further() {
        let path = annular_sector(CENTRE, 10.0, RADIUS_PX, 0.0, std::f32::consts::PI)
            .expect("a half ring builds");
        let bounds = path.bounds();

        assert!((bounds.right() - (CENTRE.x + RADIUS_PX)).abs() < ARC_TOLERANCE_PX);
        assert!((bounds.top() - (CENTRE.y - RADIUS_PX)).abs() < ARC_TOLERANCE_PX);
    }

    #[test]
    fn a_rounded_rect_keeps_its_box() {
        let rect = Rect {
            left: 10.0,
            top: 20.0,
            right: 110.0,
            bottom: 70.0,
        };
        let bounds = rounded_rect(rect, 8.0).expect("builds").bounds();

        assert!((bounds.left() - rect.left).abs() < TOLERANCE_PX);
        assert!((bounds.bottom() - rect.bottom).abs() < TOLERANCE_PX);
    }
}
