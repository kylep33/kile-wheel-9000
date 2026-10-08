//! Drawing the petal form.

use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_SMALL, D2D1_ROUNDED_RECT, D2D1_SWEEP_DIRECTION_CLOCKWISE,
    D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE, ID2D1Factory1, ID2D1PathGeometry1,
};
use windows::core::Result;
use windows_numerics::Vector2;

use super::geometry::on_circle;
use crate::form::Petal;

/// Narrow where it leaves the slice, so the join reads as a stem.
const NECK_TAPER: f32 = 0.45;

/// A neck from the slice out to the panel, built fresh each frame because it moves with
/// the open animation.
pub fn neck(factory: &ID2D1Factory1, centre: Vector2, petal: &Petal) -> Result<ID2D1PathGeometry1> {
    let inner_half = petal.neck_half_span_rad * NECK_TAPER;
    let outer_half = petal.neck_half_span_rad;

    let inner_start = on_circle(centre, petal.neck_inner_px, petal.bearing_rad - inner_half);
    let inner_end = on_circle(centre, petal.neck_inner_px, petal.bearing_rad + inner_half);
    let outer_start = on_circle(centre, petal.neck_outer_px, petal.bearing_rad - outer_half);
    let outer_end = on_circle(centre, petal.neck_outer_px, petal.bearing_rad + outer_half);

    // SAFETY: the sink is opened, filled and closed before the geometry is returned.
    unsafe {
        let geometry = factory.CreatePathGeometry()?;
        let sink = geometry.Open()?;

        sink.BeginFigure(inner_start, D2D1_FIGURE_BEGIN_FILLED);
        sink.AddLine(outer_start);
        sink.AddArc(&arc(
            outer_end,
            petal.neck_outer_px,
            D2D1_SWEEP_DIRECTION_CLOCKWISE,
        ));
        sink.AddLine(inner_end);
        sink.AddArc(&arc(
            inner_start,
            petal.neck_inner_px,
            D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE,
        ));
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        sink.Close()?;

        Ok(geometry)
    }
}

pub fn panel(petal: &Petal) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT {
        rect: petal.panel,
        radiusX: petal.corner_px,
        radiusY: petal.corner_px,
    }
}

/// Splits a row into its label and value halves, so the two line up down the form.
pub fn split_row(row: D2D_RECT_F, label_ratio: f32) -> (D2D_RECT_F, D2D_RECT_F) {
    let divide = row.left + (row.right - row.left) * label_ratio;

    let label = D2D_RECT_F {
        right: divide,
        ..row
    };
    let value = D2D_RECT_F {
        left: divide,
        ..row
    };
    (label, value)
}

fn arc(
    to: Vector2,
    radius_px: f32,
    direction: windows::Win32::Graphics::Direct2D::D2D1_SWEEP_DIRECTION,
) -> D2D1_ARC_SEGMENT {
    D2D1_ARC_SEGMENT {
        point: to,
        size: windows::Win32::Graphics::Direct2D::Common::D2D_SIZE_F {
            width: radius_px,
            height: radius_px,
        },
        rotationAngle: 0.0,
        sweepDirection: direction,
        // the neck is a sliver of a turn, so the short way round is always right
        arcSize: D2D1_ARC_SIZE_SMALL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROW: D2D_RECT_F = D2D_RECT_F {
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
}
