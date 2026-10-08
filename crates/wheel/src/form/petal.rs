//! Where the form sits: a neck rooted at the slice, opening into a readable panel.
//!
//! The container is radial so it is obvious which slice it grew from. The panel itself is
//! axis aligned, because text bent around an arc looks clever and reads badly.

use kile_wheel_core::select::SECTOR_RAD;
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows_numerics::Vector2;

use crate::render::geometry::on_circle;

/// How much wider than its slice the neck is, so the join reads as deliberate.
const NECK_SPAN_SECTORS: f32 = 0.55;

/// Where the panel's centre sits, as a multiple of the wheel radius.
const PANEL_CENTRE_REACH: f32 = 1.42;

/// Panel size as a multiple of the wheel radius.
const PANEL_WIDTH_RATIO: f32 = 1.05;
const PANEL_ROW_HEIGHT_RATIO: f32 = 0.085;
const PANEL_PADDING_RATIO: f32 = 0.05;
const PANEL_TITLE_RATIO: f32 = 0.09;
const PANEL_CORNER_RATIO: f32 = 0.035;

/// The furthest a petal reaches from the wheel centre, as a multiple of the radius.
/// The overlay window is sized from this, so a panel can never be clipped.
pub const PETAL_MAX_REACH_RATIO: f32 = 2.15;

/// Everything the renderer needs to draw one open form.
#[derive(Debug, Clone, Copy)]
pub struct Petal {
    /// Where the neck starts, on the wheel's rim.
    pub neck_inner_px: f32,
    pub neck_outer_px: f32,
    /// Angular half-width of the neck.
    pub neck_half_span_rad: f32,
    pub bearing_rad: f32,
    pub panel: D2D_RECT_F,
    pub corner_px: f32,
    pub title_height_px: f32,
    pub row_height_px: f32,
    pub padding_px: f32,
}

impl Petal {
    /// Lays the petal out for a slice, grown by `progress` from 0 to 1.
    ///
    /// `bounds` is the drawable area; the panel is kept inside it, so a slice near an edge
    /// gets a panel that has slid along rather than one that is cut off.
    pub fn new(
        centre: Vector2,
        radius_px: f32,
        bearing_rad: f32,
        rows: usize,
        progress: f32,
        bounds: D2D_RECT_F,
    ) -> Self {
        let grown = progress.clamp(0.0, 1.0);

        let width = radius_px * PANEL_WIDTH_RATIO;
        let padding = radius_px * PANEL_PADDING_RATIO;
        let title = radius_px * PANEL_TITLE_RATIO;
        let row = radius_px * PANEL_ROW_HEIGHT_RATIO;
        let height = title + row * rows as f32 + padding * 2.0;

        let reach = radius_px + (radius_px * PANEL_CENTRE_REACH - radius_px) * grown;
        let anchor = on_circle(centre, reach, bearing_rad);
        let panel = clamp_into(
            centred_rect(anchor, width * grown.max(f32::EPSILON), height * grown),
            bounds,
        );

        Self {
            neck_inner_px: radius_px,
            neck_outer_px: radius_px + (reach - radius_px).max(0.0),
            neck_half_span_rad: SECTOR_RAD * NECK_SPAN_SECTORS / 2.0,
            bearing_rad,
            panel,
            corner_px: radius_px * PANEL_CORNER_RATIO,
            title_height_px: title,
            row_height_px: row,
            padding_px: padding,
        }
    }

    /// The box one row occupies, counting from the first field under the title.
    pub fn row_bounds(&self, index: usize) -> D2D_RECT_F {
        let top = self.panel.top
            + self.padding_px
            + self.title_height_px
            + self.row_height_px * index as f32;

        D2D_RECT_F {
            left: self.panel.left + self.padding_px,
            top,
            right: self.panel.right - self.padding_px,
            bottom: top + self.row_height_px,
        }
    }

    pub fn title_bounds(&self) -> D2D_RECT_F {
        D2D_RECT_F {
            left: self.panel.left + self.padding_px,
            top: self.panel.top + self.padding_px,
            right: self.panel.right - self.padding_px,
            bottom: self.panel.top + self.padding_px + self.title_height_px,
        }
    }
}

fn centred_rect(centre: Vector2, width: f32, height: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left: centre.X - width / 2.0,
        top: centre.Y - height / 2.0,
        right: centre.X + width / 2.0,
        bottom: centre.Y + height / 2.0,
    }
}

/// Slides the rect until it fits, rather than shrinking it, so the text keeps its size.
fn clamp_into(rect: D2D_RECT_F, bounds: D2D_RECT_F) -> D2D_RECT_F {
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;

    let left = rect.left.min(bounds.right - width).max(bounds.left);
    let top = rect.top.min(bounds.bottom - height).max(bounds.top);

    D2D_RECT_F {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kile_wheel_core::slot::{Direction, Slot};

    const CENTRE: Vector2 = Vector2 { X: 600.0, Y: 600.0 };
    const RADIUS_PX: f32 = 300.0;
    const ROWS: usize = 6;

    fn window() -> D2D_RECT_F {
        D2D_RECT_F {
            left: 0.0,
            top: 0.0,
            right: 1200.0,
            bottom: 1200.0,
        }
    }

    fn petal_at(slot: Slot, progress: f32) -> Petal {
        let bearing = crate::render::geometry::slot_bearing_rad(slot);

        Petal::new(CENTRE, RADIUS_PX, bearing, ROWS, progress, window())
    }

    fn width_of(rect: D2D_RECT_F) -> f32 {
        rect.right - rect.left
    }

    fn height_of(rect: D2D_RECT_F) -> f32 {
        rect.bottom - rect.top
    }

    #[test]
    fn a_closed_petal_has_no_panel_to_speak_of() {
        let petal = petal_at(Direction::North.slot(), 0.0);

        assert!(height_of(petal.panel) < 1.0);
    }

    #[test]
    fn an_open_petal_has_a_panel_with_room_for_its_rows() {
        let petal = petal_at(Direction::North.slot(), 1.0);

        assert!(height_of(petal.panel) > petal.row_height_px * ROWS as f32);
        assert!(width_of(petal.panel) > 0.0);
    }

    #[test]
    fn the_neck_starts_on_the_rim() {
        let petal = petal_at(Direction::East.slot(), 1.0);

        assert_eq!(petal.neck_inner_px, RADIUS_PX);
        assert!(petal.neck_outer_px > petal.neck_inner_px);
    }

    #[test]
    fn the_neck_is_wider_than_nothing_but_narrower_than_its_slice() {
        let petal = petal_at(Direction::East.slot(), 1.0);

        assert!(petal.neck_half_span_rad > 0.0);
        assert!(petal.neck_half_span_rad < SECTOR_RAD / 2.0);
    }

    #[test]
    fn the_panel_grows_outward_as_it_opens() {
        let closed = height_of(petal_at(Direction::North.slot(), 0.2).panel);
        let open = height_of(petal_at(Direction::North.slot(), 1.0).panel);

        assert!(open > closed);
    }

    /// A slice pointing at the edge still has to show a whole panel.
    #[test]
    fn a_panel_never_leaves_the_drawable_area() {
        for slot in Slot::all() {
            let petal = petal_at(slot, 1.0);
            let bounds = window();

            assert!(petal.panel.left >= bounds.left, "slot {}", slot.index());
            assert!(petal.panel.top >= bounds.top, "slot {}", slot.index());
            assert!(petal.panel.right <= bounds.right, "slot {}", slot.index());
            assert!(petal.panel.bottom <= bounds.bottom, "slot {}", slot.index());
        }
    }

    #[test]
    fn clamping_slides_rather_than_shrinks() {
        let rect = D2D_RECT_F {
            left: -50.0,
            top: 10.0,
            right: 50.0,
            bottom: 60.0,
        };

        let clamped = clamp_into(rect, window());

        assert_eq!(clamped.left, 0.0);
        assert_eq!(width_of(clamped), width_of(rect));
        assert_eq!(height_of(clamped), height_of(rect));
    }

    #[test]
    fn rows_stack_downward_without_overlapping() {
        let petal = petal_at(Direction::North.slot(), 1.0);

        let first = petal.row_bounds(0);
        let second = petal.row_bounds(1);
        assert_eq!(second.top, first.bottom);
    }

    #[test]
    fn rows_sit_below_the_title() {
        let petal = petal_at(Direction::North.slot(), 1.0);

        assert!(petal.row_bounds(0).top >= petal.title_bounds().bottom);
    }

    #[test]
    fn rows_stay_inside_the_panel_width() {
        let petal = petal_at(Direction::South.slot(), 1.0);
        let row = petal.row_bounds(0);

        assert!(row.left > petal.panel.left);
        assert!(row.right < petal.panel.right);
    }

    /// The window is sized from this, so an under-estimate would clip a panel.
    #[test]
    fn the_declared_reach_covers_every_slice() {
        for slot in Slot::all() {
            let petal = petal_at(slot, 1.0);
            let furthest = [
                petal.panel.left - CENTRE.X,
                petal.panel.right - CENTRE.X,
                petal.panel.top - CENTRE.Y,
                petal.panel.bottom - CENTRE.Y,
            ]
            .into_iter()
            .map(f32::abs)
            .fold(0.0, f32::max);

            assert!(
                furthest <= RADIUS_PX * PETAL_MAX_REACH_RATIO,
                "slot {} reached {furthest}",
                slot.index()
            );
        }
    }
}
