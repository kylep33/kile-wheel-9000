//! Layout helpers for drawing the petal form.

use resvg::tiny_skia::Path;

use super::geometry::{rounded_rect, tapered_sector};
use crate::geom::{Point, Rect};
use crate::form::Petal;

/// Narrow where it leaves the slice, so the join reads as a stem.
const NECK_TAPER: f32 = 0.45;

/// A neck from the slice out to the panel, built fresh each frame because it moves with
/// the open animation.
pub fn neck(centre: Point, petal: &Petal) -> Option<Path> {
    tapered_sector(
        centre,
        petal.neck_inner_px,
        petal.neck_outer_px,
        petal.bearing_rad,
        petal.neck_half_span_rad * NECK_TAPER,
        petal.neck_half_span_rad,
    )
}

pub fn panel(petal: &Petal) -> Option<Path> {
    rounded_rect(petal.panel, petal.corner_px)
}

/// Splits a row into its label and value halves, so the two line up down the form.
pub fn split_row(row: Rect, label_ratio: f32) -> (Rect, Rect) {
    let divide = row.left + row.width() * label_ratio;

    let label = Rect {
        right: divide,
        ..row
    };
    let value = Rect {
        left: divide,
        ..row
    };
    (label, value)
}

/// How far to slide a value being edited to the left, so the caret stays in its box.
///
/// Nothing moves while the caret fits, so short values sit still as you type. Once the
/// caret would pass the right edge, the text slides by exactly the overshoot plus a small
/// margin, keeping a few characters of context to the caret's right.
pub fn caret_scroll(caret_px: f32, box_width_px: f32, margin_px: f32) -> f32 {
    let visible = (box_width_px - margin_px).max(0.0);
    (caret_px - visible).max(0.0)
}

/// Pulls a box in on both sides, so text does not touch its edges.
pub fn inset(rect: Rect, by_px: f32) -> Rect {
    let by_px = by_px.min(rect.width() / 2.0).max(0.0);

    Rect {
        left: rect.left + by_px,
        right: rect.right - by_px,
        ..rect
    }
}

/// The largest square that fits inside `rect` once inset, centred, so an icon keeps its
/// shape in a tile that is not quite square.
pub fn square_in(rect: Rect, inset_px: f32) -> Rect {
    let side = (rect.width().min(rect.height()) - inset_px * 2.0).max(0.0);
    let centre_x = (rect.left + rect.right) / 2.0;
    let centre_y = (rect.top + rect.bottom) / 2.0;

    Rect {
        left: centre_x - side / 2.0,
        top: centre_y - side / 2.0,
        right: centre_x + side / 2.0,
        bottom: centre_y + side / 2.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROW: Rect = Rect {
        left: 100.0,
        top: 10.0,
        right: 300.0,
        bottom: 30.0,
    };

    #[test]
    fn a_row_splits_at_the_given_ratio() {
        let (label, value) = split_row(ROW, 0.4);

        assert_eq!(label.left, ROW.left);
        assert_eq!(label.right, 180.0);
        assert_eq!(value.left, 180.0);
        assert_eq!(value.right, ROW.right);
    }

    #[test]
    fn both_halves_keep_the_row_height() {
        let (label, value) = split_row(ROW, 0.5);

        assert_eq!((label.top, label.bottom), (ROW.top, ROW.bottom));
        assert_eq!((value.top, value.bottom), (ROW.top, ROW.bottom));
    }

    #[test]
    fn the_halves_meet_without_a_gap() {
        let (label, value) = split_row(ROW, 0.33);

        assert_eq!(label.right, value.left);
    }

    #[test]
    fn a_caret_that_fits_does_not_scroll() {
        assert_eq!(caret_scroll(0.0, 120.0, 10.0), 0.0);
        assert_eq!(caret_scroll(110.0, 120.0, 10.0), 0.0);
    }

    /// A long path scrolls left by just enough to keep the caret and a margin in view.
    #[test]
    fn a_caret_past_the_edge_scrolls_by_the_overshoot() {
        assert_eq!(caret_scroll(300.0, 120.0, 10.0), 190.0);
    }

    #[test]
    fn a_box_narrower_than_its_margin_scrolls_the_caret_to_the_edge() {
        assert_eq!(caret_scroll(50.0, 5.0, 10.0), 50.0);
    }

    #[test]
    fn a_square_fits_the_short_side_and_stays_centred() {
        let square = square_in(ROW, 2.0);

        assert_eq!(square.width(), 16.0);
        assert_eq!(square.height(), 16.0);
        assert_eq!((square.left + square.right) / 2.0, 200.0);
    }

    #[test]
    fn insetting_keeps_the_height_and_never_inverts() {
        let inner = inset(ROW, 6.0);
        assert_eq!((inner.left, inner.right), (106.0, 294.0));
        assert_eq!((inner.top, inner.bottom), (ROW.top, ROW.bottom));

        let squeezed = inset(ROW, 500.0);
        assert!(squeezed.left <= squeezed.right);
    }
}
