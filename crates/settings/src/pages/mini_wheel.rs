//! A small copy of the wheel for the Items page, so slots are picked where they sit
//! instead of read off a list of compass codes.
//!
//! The geometry is kept apart from the painting, because hit-testing a click is the part
//! that can quietly be wrong and it is the part that can be tested without a window.

use egui::epaint::{Mesh, PathStroke};
use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use kile_wheel_core::slot::Slot;

/// The hole in the middle, as a share of the outer radius. Matches the real wheel's hub
/// closely enough to read as the same object.
const HUB_RATIO: f32 = 0.34;

/// The gap between neighbouring slices, in radians, so each one reads as its own target.
const SLICE_GAP_RAD: f32 = 0.035;

/// Arc segments per slice. Plenty at this size, and cheap.
const ARC_STEPS: usize = 14;

const SLICE_RAD: f32 = std::f32::consts::TAU / Slot::COUNT as f32;

/// Small enough to fit the settings window at its minimum height, big enough that the
/// labels still read.
const MIN_DIAMETER_PX: f32 = 200.0;
const MAX_DIAMETER_PX: f32 = 380.0;

/// How long a slice label may run before it is cut short. The slice is small, so the
/// full name lives in the panel beside it.
const LABEL_CHARS: usize = 11;

/// What one slice shows.
#[derive(Debug, Clone, Default)]
pub struct SliceView {
    pub label: String,
    /// The kind, in a word, under the label.
    pub caption: String,
    pub filled: bool,
    /// An unpinned slot is spoken for but not configured, so it draws in between.
    pub reserved: bool,
}

/// What the wheel reported back this frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WheelInput {
    pub clicked: Option<Slot>,
    pub double_clicked: Option<Slot>,
    /// A slice dragged onto another, as `(from, to)`.
    pub dropped: Option<(Slot, Slot)>,
}

/// Remembered across frames, because a drag starts on one and ends on another.
#[derive(Debug, Clone, Copy, Default)]
pub struct DragState {
    from: Option<Slot>,
}

/// Colours for one draw, taken from the theme and the user's accent.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub accent: Color32,
    pub empty: Color32,
    pub reserved: Color32,
    pub filled: Color32,
    pub text: Color32,
    pub faint_text: Color32,
    pub hub: Color32,
}

impl Palette {
    pub fn from_visuals(visuals: &egui::Visuals, accent: Color32) -> Self {
        let base = visuals.extreme_bg_color;
        Self {
            accent,
            empty: base,
            reserved: blend(base, visuals.widgets.inactive.bg_fill, 0.55),
            filled: blend(visuals.widgets.inactive.bg_fill, accent, 0.22),
            text: visuals.strong_text_color(),
            faint_text: visuals.weak_text_color(),
            hub: visuals.panel_fill,
        }
    }
}

/// Everything one draw of the wheel shows.
#[derive(Debug)]
pub struct WheelSpec<'a> {
    pub diameter_px: f32,
    pub slices: &'a [SliceView; Slot::COUNT],
    pub selected: Option<Slot>,
    pub hub_title: &'a str,
    pub hub_caption: &'a str,
    pub palette: Palette,
}

/// Draws the wheel and reports clicks, double-clicks and drops.
pub fn show(ui: &mut egui::Ui, spec: &WheelSpec<'_>, drag: &mut DragState) -> WheelInput {
    let WheelSpec {
        diameter_px,
        slices,
        selected,
        hub_title,
        hub_caption,
        palette,
    } = *spec;
    let (response, painter) =
        ui.allocate_painter(Vec2::splat(diameter_px), Sense::click_and_drag());
    let centre = response.rect.center();
    let outer = diameter_px / 2.0 - 2.0;
    let inner = outer * HUB_RATIO;

    let under = |pos: Option<Pos2>| pos.and_then(|pos| slot_at(pos - centre, inner, outer));
    let hovered = under(response.hover_pos());
    let pressed = under(response.interact_pointer_pos());

    let mut input = WheelInput::default();
    if response.drag_started() {
        drag.from = pressed;
    }
    if response.drag_stopped() {
        let to = under(ui.input(|input| input.pointer.latest_pos()));
        if let (Some(from), Some(to)) = (drag.from.take(), to)
            && from != to
        {
            input.dropped = Some((from, to));
        }
    }
    if response.double_clicked() {
        input.double_clicked = pressed;
    } else if response.clicked() {
        input.clicked = pressed;
    }

    let dragging = drag.from.filter(|_| response.dragged());

    for slot in Slot::all() {
        let view = &slices[slot.index()];
        let fill = slice_fill(view, &palette, Some(slot) == hovered);
        let (start, end) = slice_span(slot);

        painter.add(annulus_sector(centre, inner, outer, start, end, fill));

        let stroke = match (
            Some(slot) == selected,
            dragging.is_some() && Some(slot) == hovered,
        ) {
            (_, true) => Some(Stroke::new(2.5, palette.accent)),
            (true, false) => Some(Stroke::new(2.0, palette.accent)),
            (false, false) => None,
        };
        if let Some(stroke) = stroke {
            painter.add(Shape::closed_line(
                outline(centre, inner, outer, start, end),
                PathStroke::from(stroke),
            ));
        }

        draw_slice_text(
            &painter,
            centre,
            inner,
            outer,
            slot,
            view,
            &palette,
            dragging == Some(slot),
        );
    }

    painter.circle_filled(centre, inner - 4.0, palette.hub);
    painter.circle_stroke(centre, inner - 4.0, Stroke::new(1.0, palette.faint_text));
    painter.text(
        centre - Vec2::new(0.0, 8.0),
        egui::Align2::CENTER_CENTER,
        hub_title,
        egui::FontId::proportional(18.0),
        palette.accent,
    );
    painter.text(
        centre + Vec2::new(0.0, 12.0),
        egui::Align2::CENTER_CENTER,
        shorten(hub_caption, LABEL_CHARS),
        egui::FontId::proportional(12.0),
        palette.faint_text,
    );

    // the slice being dragged follows the pointer, so it is clear what is moving
    if let (Some(from), Some(pos)) = (dragging, ui.input(|input| input.pointer.latest_pos())) {
        let label = &slices[from.index()].label;
        if !label.is_empty() {
            painter.text(
                pos + Vec2::new(14.0, -14.0),
                egui::Align2::LEFT_BOTTOM,
                shorten(label, LABEL_CHARS),
                egui::FontId::proportional(13.0),
                palette.accent,
            );
        }
    }

    if hovered.is_some() {
        ui.ctx().set_cursor_icon(match dragging {
            Some(_) => egui::CursorIcon::Grabbing,
            None => egui::CursorIcon::PointingHand,
        });
    }
    input
}

fn slice_fill(view: &SliceView, palette: &Palette, hovered: bool) -> Color32 {
    let base = match (view.filled, view.reserved) {
        (true, _) => palette.filled,
        (false, true) => palette.reserved,
        (false, false) => palette.empty,
    };
    match hovered {
        true => blend(base, palette.accent, 0.18),
        false => base,
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_slice_text(
    painter: &egui::Painter,
    centre: Pos2,
    inner: f32,
    outer: f32,
    slot: Slot,
    view: &SliceView,
    palette: &Palette,
    lifted: bool,
) {
    let middle = point_at(centre, (inner + outer) / 2.0, bearing(slot));
    let compass = point_at(centre, outer - 10.0, bearing(slot));

    painter.text(
        compass,
        egui::Align2::CENTER_CENTER,
        compass_label(slot),
        egui::FontId::proportional(10.0),
        palette.faint_text,
    );

    if view.label.is_empty() {
        painter.text(
            middle,
            egui::Align2::CENTER_CENTER,
            "+",
            egui::FontId::proportional(18.0),
            palette.faint_text,
        );
        return;
    }

    let colour = match lifted {
        true => palette.faint_text,
        false => palette.text,
    };
    painter.text(
        middle - Vec2::new(0.0, 6.0),
        egui::Align2::CENTER_CENTER,
        shorten(&view.label, LABEL_CHARS),
        egui::FontId::proportional(13.0),
        colour,
    );
    painter.text(
        middle + Vec2::new(0.0, 9.0),
        egui::Align2::CENTER_CENTER,
        &view.caption,
        egui::FontId::proportional(10.0),
        palette.faint_text,
    );
}

/// Which slice a point falls in, given its offset from the centre.
///
/// Slot 0 is straight up and the rest run clockwise, matching the real wheel. The hub and
/// anything past the rim belong to no slice.
pub fn slot_at(offset: Vec2, inner: f32, outer: f32) -> Option<Slot> {
    let distance = offset.length();
    if distance < inner || distance > outer {
        return None;
    }

    // screen y grows downward, so north is -y and clockwise is +x
    let angle = offset.x.atan2(-offset.y).rem_euclid(std::f32::consts::TAU);
    let index = (angle / SLICE_RAD).round() as usize;
    Some(Slot::wrapping(index))
}

/// Clockwise bearing of a slice's centre from north, in radians.
pub fn bearing(slot: Slot) -> f32 {
    slot.index() as f32 * SLICE_RAD
}

/// A long name cut short with an ellipsis, counted in characters so it never splits one.
pub fn shorten(text: &str, max_chars: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }

    let kept: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

pub fn compass_label(slot: Slot) -> &'static str {
    crate::draft::slot_label(slot)
}

fn slice_span(slot: Slot) -> (f32, f32) {
    let centre = bearing(slot);
    let half = SLICE_RAD / 2.0 - SLICE_GAP_RAD;
    (centre - half, centre + half)
}

fn point_at(centre: Pos2, radius: f32, bearing: f32) -> Pos2 {
    centre + Vec2::new(bearing.sin(), -bearing.cos()) * radius
}

/// A filled ring segment. Not convex, so it is built from triangles rather than handed to
/// the polygon filler.
fn annulus_sector(
    centre: Pos2,
    inner: f32,
    outer: f32,
    start: f32,
    end: f32,
    fill: Color32,
) -> Mesh {
    let mut mesh = Mesh::default();

    for step in 0..=ARC_STEPS {
        let bearing = start + (end - start) * step as f32 / ARC_STEPS as f32;
        mesh.colored_vertex(point_at(centre, inner, bearing), fill);
        mesh.colored_vertex(point_at(centre, outer, bearing), fill);
    }
    for step in 0..ARC_STEPS as u32 {
        let base = step * 2;
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base + 1, base + 3, base + 2);
    }
    mesh
}

fn outline(centre: Pos2, inner: f32, outer: f32, start: f32, end: f32) -> Vec<Pos2> {
    let arc = |radius: f32| {
        (0..=ARC_STEPS).map(move |step| {
            let bearing = start + (end - start) * step as f32 / ARC_STEPS as f32;
            point_at(centre, radius, bearing)
        })
    };

    let mut points: Vec<Pos2> = arc(outer).collect();
    let mut back: Vec<Pos2> = arc(inner).collect();
    back.reverse();
    points.extend(back);
    points
}

fn blend(from: Color32, to: Color32, amount: f32) -> Color32 {
    let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * amount).round() as u8;
    Color32::from_rgb(
        mix(from.r(), to.r()),
        mix(from.g(), to.g()),
        mix(from.b(), to.b()),
    )
}

/// The size the wheel is drawn at for the space it has.
pub fn diameter_for(available: Rect) -> f32 {
    available
        .width()
        .min(available.height())
        .clamp(MIN_DIAMETER_PX, MAX_DIAMETER_PX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const INNER: f32 = 30.0;
    const OUTER: f32 = 100.0;

    fn at(slot: Slot, radius: f32) -> Vec2 {
        let bearing = bearing(slot);
        Vec2::new(bearing.sin(), -bearing.cos()) * radius
    }

    #[test]
    fn straight_up_is_north_and_the_slots_run_clockwise() {
        assert_eq!(slot_at(Vec2::new(0.0, -60.0), INNER, OUTER), Slot::new(0));
        assert_eq!(slot_at(Vec2::new(60.0, 0.0), INNER, OUTER), Slot::new(2));
        assert_eq!(slot_at(Vec2::new(0.0, 60.0), INNER, OUTER), Slot::new(4));
        assert_eq!(slot_at(Vec2::new(-60.0, 0.0), INNER, OUTER), Slot::new(6));
    }

    /// Clicking the middle of every slice has to land on that slice, or the page would
    /// select a neighbour.
    #[test]
    fn every_slice_centre_hits_its_own_slot() {
        for slot in Slot::all() {
            assert_eq!(
                slot_at(at(slot, 65.0), INNER, OUTER),
                Some(slot),
                "{slot:?}"
            );
        }
    }

    /// Just west of north is still north, not the last slot misread as slot 8.
    #[test]
    fn the_seam_at_north_wraps_back_to_the_first_slot() {
        assert_eq!(slot_at(Vec2::new(-5.0, -60.0), INNER, OUTER), Slot::new(0));
    }

    #[test]
    fn the_hub_and_beyond_the_rim_belong_to_no_slot() {
        assert_eq!(slot_at(Vec2::new(0.0, -10.0), INNER, OUTER), None);
        assert_eq!(slot_at(Vec2::new(0.0, -150.0), INNER, OUTER), None);
    }

    #[test]
    fn a_short_name_is_left_alone() {
        assert_eq!(shorten("Kiro", 11), "Kiro");
        assert_eq!(shorten("  Kiro  ", 11), "Kiro");
    }

    #[test]
    fn a_long_name_is_cut_to_fit_with_an_ellipsis() {
        let short = shorten("gauntlet: aevexos yocto units", 11);

        assert!(short.chars().count() <= 11, "{short}");
        assert!(short.ends_with('…'), "{short}");
        // a space is not left dangling before the ellipsis
        assert_eq!(short, "gauntlet:…");
    }

    /// Counting bytes would cut through a multi-byte character and panic.
    #[test]
    fn shortening_counts_characters_not_bytes() {
        let short = shorten("ééééééééééééééé", 5);

        assert_eq!(short, "éééé…");
    }

    #[test]
    fn the_wheel_size_stays_usable_in_any_window() {
        let tiny = Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 100.0));
        let huge = Rect::from_min_size(Pos2::ZERO, Vec2::new(2000.0, 2000.0));

        assert_eq!(diameter_for(tiny), MIN_DIAMETER_PX);
        assert_eq!(diameter_for(huge), MAX_DIAMETER_PX);
    }
}
