//! The wheel, drawn on the CPU into one premultiplied RGBA buffer.
//!
//! Every OS gets the same pixels, so the platform layer only has to put a buffer on
//! screen. Geometry is in window units: physical pixels on Windows, points on macOS, and
//! stage pixels on GNOME. `pixel_ratio` turns units into buffer pixels.

mod form;
pub mod geometry;
mod glyphs;
mod icons;
mod text;

use std::collections::HashMap;

use anyhow::{Context, Result};
use kile_wheel_core::slot::Slot;
use kile_wheel_platform::icons::IconImage;
use resvg::tiny_skia::{
    self, Color, FillRule, FilterQuality, Paint, Path, PathBuilder, Pixmap, PixmapPaint, Stroke,
    Transform,
};

use crate::anim;
use crate::form::{Line, Petal, RowValue, grid};
use crate::geom::{Point, Rect};
use crate::theme::{self, Colour, Metrics, Palette};
pub use glyphs::Glyph;
use geometry::{Wedges, on_circle, slot_bearing_rad, slot_edge_rad};
use glyphs::Glyphs;
use text::{Fonts, Style, TextPainter, TextRequest};

/// Pill under a running app's icon, as a fraction of the wedge's label box.
const RUNNING_PILL_WIDTH_RATIO: f32 = 0.22;
const RUNNING_PILL_FOCUSED_RATIO: f32 = 0.42;
const RUNNING_PILL_HEIGHT_PX: f32 = 2.5;
const RUNNING_PILL_GAP_PX: f32 = 5.0;
const MULTI_WINDOW_PILL_GAP_PX: f32 = 3.0;

/// Sits beside the cog, as a fraction of the hub radius.
const OVERFLOW_X_RATIO: f32 = 0.52;

/// How much of a form row the label column takes.
const FORM_LABEL_RATIO: f32 = 0.38;

/// Text only appears once the panel has room for it, so it never spills outside.
const FORM_TEXT_THRESHOLD: f32 = 0.55;

const FORM_EDGE_MARGIN_PX: f32 = 6.0;
const CARET_WIDTH_PX: f32 = 1.6;
const CARET_INSET_PX: f32 = 3.0;
/// Room kept to the right of the caret once a long value starts scrolling.
const CARET_SCROLL_MARGIN_PX: f32 = 14.0;
/// Keeps a value's text off the edge of its column.
const VALUE_PADDING_PX: f32 = 4.0;
/// Room around an icon in the picker grid, so the highlight shows around it.
const GRID_TILE_PADDING_PX: f32 = 4.0;

/// Where the open animation starts the wheel. Close enough to full size that the wheel is
/// readable on the first frame, far enough to read as arriving.
const APPEAR_FROM_SCALE: f32 = 0.88;

/// The appear scale is rounded to this many steps per unit, so text is only rasterised
/// at a handful of sizes and the glyph cache stays bounded.
const APPEAR_SCALE_STEPS: f32 = 100.0;

/// Antialiasing reaches a pixel past a shape's edge.
const ANTIALIAS_REACH_PX: f32 = 1.0;

/// How a slot should be drawn.
#[derive(Debug, Default, Clone, Copy)]
pub struct SlotView<'a> {
    pub label: &'a str,
    /// Cache key of an already uploaded icon.
    pub icon_key: Option<&'a str>,
    /// Mini glyph for the slot's type, or none for a slot the config left blank.
    pub type_glyph: Option<Glyph>,
    pub running: bool,
    pub focused: bool,
    pub window_count: usize,
    pub filled: bool,
}

/// What the middle of the wheel shows.
#[derive(Debug, Default, Clone, Copy)]
pub struct HubView<'a> {
    pub tab_line: &'a str,
    pub clock: Option<&'a str>,
    pub date: Option<&'a str>,
    /// Said out loud under the ring when apps outnumber the unpinned slots.
    pub overflow_note: Option<&'a str>,
}

/// Everything one frame needs.
#[derive(Debug)]
pub struct Frame<'a> {
    pub slots: [SlotView<'a>; Slot::COUNT],
    pub selected: Option<Slot>,
    /// How lit each wedge is, from 0 to 1, so the selection can move rather than jump.
    pub highlights: [f32; Slot::COUNT],
    /// How far the wheel has grown in, from 0 to 1.
    pub appear: f32,
    pub hub: HubView<'a>,
    pub edit_mode: bool,
    pub dragging: Option<Slot>,
    /// How many running apps had no unpinned slot to land in.
    pub overflow: usize,
    /// The open form, if there is one.
    pub form: Option<FormView<'a>>,
}

/// One open petal form, ready to draw.
#[derive(Debug)]
pub struct FormView<'a> {
    pub slot: Slot,
    pub title: &'a str,
    pub lines: &'a [Line],
    pub progress: f32,
    /// Drawn instead of the lines while an icon is being picked.
    pub grid: Option<&'a GridView>,
}

/// The icon grid's visible page, ready to draw.
#[derive(Debug)]
pub struct GridView {
    /// Icon keys of the visible tiles, in reading order. `None` has nothing to show.
    pub tiles: Vec<Option<String>>,
    pub highlighted: Option<usize>,
    pub caption: String,
}

/// The part of the buffer a frame drew into, in buffer pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// A finished frame: the whole buffer, and the part of it worth putting on screen.
#[derive(Debug)]
pub struct Drawn<'a> {
    pub pixmap: &'a Pixmap,
    pub region: Region,
}

/// Owns the buffer and everything that is expensive to rebuild per frame.
#[derive(Debug)]
pub struct Renderer {
    scene: Scene,
    pixmap: Pixmap,
    pixel_ratio: f32,
    text: TextPainter,
    glyphs: Glyphs,
    icons: HashMap<String, Pixmap>,
    /// Everything outside this is already clear, so the next frame only wipes this much.
    /// `None` means the whole buffer may hold ink.
    last_region: Option<Region>,
}

/// What a frame is drawn against: sizes, colours and the shapes built from them.
#[derive(Debug)]
struct Scene {
    size: f32,
    metrics: Metrics,
    palette: Palette,
    fonts: Fonts,
    wedges: Wedges,
    /// The part of the window that is on screen, in window units. `None` until the
    /// window has been placed, which means all of it.
    visible: Option<Rect>,
}

impl Renderer {
    /// `size` is the square window's side in window units.
    pub fn new(size: f32, pixel_ratio: f32, metrics: Metrics, palette: Palette) -> Result<Self> {
        Ok(Self {
            scene: Scene::new(size, metrics, palette),
            pixmap: buffer_for(size, pixel_ratio)?,
            pixel_ratio,
            text: TextPainter::new(),
            glyphs: Glyphs::default(),
            icons: HashMap::new(),
            last_region: None,
        })
    }

    pub fn metrics(&self) -> Metrics {
        self.scene.metrics
    }

    pub fn pixel_ratio(&self) -> f32 {
        self.pixel_ratio
    }

    /// Records which part of the window is on screen, so a petal is kept inside it.
    pub fn set_visible(&mut self, visible: Rect) {
        self.scene.visible = Some(visible);
    }

    /// Which form line is under `point`, in window units.
    ///
    /// The hit test lives here because the petal's geometry does, and a second copy in the
    /// app would be one more thing to keep in step. The appear transform is not undone: it
    /// has settled back to 1 by the time a click is possible.
    pub fn form_row_at(&self, view: &FormView<'_>, point: Point) -> Option<usize> {
        let petal = self.scene.petal_for(view);

        if let Some(grid) = view.grid {
            return (0..grid.tiles.len())
                .find(|index| grid::tile_bounds(&petal, *index).contains(point));
        }
        (0..view.lines.len()).find(|index| petal.row_bounds(*index).contains(point))
    }

    pub fn has_icon(&self, key: &str) -> bool {
        self.icons.contains_key(key)
    }

    /// Kept for the process lifetime, because there are only ever as many as there are
    /// pinned and running apps.
    pub fn upload_icon(&mut self, key: &str, image: &IconImage) -> Result<()> {
        let pixmap = icons::to_pixmap(image).with_context(|| {
            format!(
                "the icon for {key} is {}x{} but carries {} bytes",
                image.width,
                image.height,
                image.rgba.len()
            )
        })?;
        self.icons.insert(key.to_owned(), pixmap);
        Ok(())
    }

    /// Matches the buffer and the wedge outlines to a new size, which happens when the
    /// wheel moves to a monitor with a different scale.
    pub fn resize(
        &mut self,
        size: f32,
        pixel_ratio: f32,
        metrics: Metrics,
        palette: Palette,
    ) -> Result<()> {
        let visible = self.scene.visible;
        self.scene = Scene::new(size, metrics, palette);
        self.scene.visible = visible;

        let (width, _) = buffer_side(size, pixel_ratio);
        if width != self.pixmap.width() || pixel_ratio != self.pixel_ratio {
            self.pixmap = buffer_for(size, pixel_ratio)?;
            self.pixel_ratio = pixel_ratio;
            self.last_region = None;
        }
        Ok(())
    }

    pub fn draw(&mut self, frame: &Frame<'_>) -> Drawn<'_> {
        clear(&mut self.pixmap, self.last_region.take());

        let mut canvas = Canvas {
            pixmap: &mut self.pixmap,
            text: &mut self.text,
            glyphs: &mut self.glyphs,
            icons: &self.icons,
            view: View::identity(self.pixel_ratio),
            pixel_ratio: self.pixel_ratio,
            inked: Rect::default(),
        };
        self.scene.draw_wheel(&mut canvas, frame);

        let region = region_of(canvas.inked, &self.pixmap);
        self.last_region = Some(region);
        Drawn {
            pixmap: &self.pixmap,
            region,
        }
    }
}

impl Scene {
    fn new(size: f32, metrics: Metrics, palette: Palette) -> Self {
        Self {
            size,
            metrics,
            palette,
            fonts: Fonts::new(metrics),
            wedges: Wedges::build(centre_of(size), metrics),
            visible: None,
        }
    }

    fn petal_for(&self, view: &FormView<'_>) -> Petal {
        let rows = match view.grid {
            Some(_) => grid::FORM_ROWS,
            None => view.lines.len(),
        };

        Petal::new(
            centre_of(self.size),
            self.metrics.radius_px,
            slot_bearing_rad(view.slot),
            rows,
            view.progress,
            drawable_area(self.size, self.visible),
        )
    }

    fn draw_wheel(&self, canvas: &mut Canvas<'_>, frame: &Frame<'_>) {
        let centre = centre_of(self.size);
        let metrics = self.metrics;
        let palette = &self.palette;

        canvas.view = View::appearing(canvas.pixel_ratio, centre, frame.appear);

        canvas.fill_circle(centre, metrics.radius_px, palette.backdrop);
        self.fill_wedges(canvas, frame);
        self.draw_dividers(canvas, centre);
        canvas.stroke_circle(centre, metrics.radius_px, palette.ring, metrics.ring_stroke_px);

        for slot in Slot::all() {
            self.draw_slot(canvas, centre, slot, frame);
        }

        self.draw_hub(canvas, centre, frame);

        if let Some(view) = &frame.form {
            self.draw_form(canvas, centre, view);
        }
    }

    /// The petal grows from its slice, so it is obvious which slot is being edited.
    fn draw_form(&self, canvas: &mut Canvas<'_>, centre: Point, view: &FormView<'_>) {
        let petal = self.petal_for(view);
        let palette = &self.palette;

        if let Some(neck) = form::neck(centre, &petal) {
            canvas.fill_path(&neck, palette.hub);
            canvas.stroke_path(&neck, palette.accent_dim, self.metrics.divider_stroke_px);
        }
        if let Some(panel) = form::panel(&petal) {
            canvas.fill_path(&panel, palette.hub);
            canvas.stroke_path(&panel, palette.accent, self.metrics.selected_stroke_px);
        }

        // the panel is still growing, so there is no room for text yet
        if view.progress < FORM_TEXT_THRESHOLD {
            return;
        }

        canvas.text(view.title, petal.title_bounds(), self.fonts.hub_tab, palette.accent, None);
        if let Some(grid) = view.grid {
            self.draw_grid(canvas, &petal, grid);
            return;
        }
        for (index, line) in view.lines.iter().enumerate() {
            self.draw_form_line(canvas, &petal, index, line);
        }
    }

    fn draw_grid(&self, canvas: &mut Canvas<'_>, petal: &Petal, view: &GridView) {
        for (index, key) in view.tiles.iter().enumerate() {
            let tile = grid::tile_bounds(petal, index);

            if view.highlighted == Some(index) {
                canvas.fill_rect(tile, self.palette.wedge_selected, None);
            }
            if let Some(key) = key {
                canvas.bitmap(key, form::square_in(tile, GRID_TILE_PADDING_PX));
            }
        }

        canvas.text(
            &view.caption,
            grid::caption_bounds(petal),
            self.fonts.value,
            self.palette.label,
            None,
        );
    }

    fn draw_form_line(&self, canvas: &mut Canvas<'_>, petal: &Petal, index: usize, line: &Line) {
        let row = petal.row_bounds(index);

        if line.focused {
            canvas.fill_rect(row, self.palette.wedge_selected, None);
        }

        let colour = match line.focused {
            true => self.palette.label_selected,
            false => self.palette.label,
        };

        // an action is a button, so it spans the row rather than sitting in a label column
        if line.action {
            canvas.text(&line.label, row, self.fonts.number, colour, None);
            return;
        }

        let (label, value) = form::split_row(row, FORM_LABEL_RATIO);
        canvas.text(&line.label, label, self.fonts.number, self.palette.number, None);
        self.draw_value(canvas, value, line, colour);
    }

    /// The caret is drawn by measuring the text before it, which is why the value arrives
    /// already split in two.
    fn draw_value(&self, canvas: &mut Canvas<'_>, bounds: Rect, line: &Line, colour: Colour) {
        let Some(value) = &line.value else {
            return;
        };

        let (before, after) = match value {
            RowValue::Text { before, after } => (before, after),
            RowValue::Toggle(on) => {
                let text = match on {
                    true => "yes",
                    false => "no",
                };
                canvas.text(text, bounds, self.fonts.number, colour, None);
                return;
            }
            RowValue::Choice(chosen) => {
                canvas.text(&format!("< {chosen} >"), bounds, self.fonts.number, colour, None);
                return;
            }
        };

        let bounds = form::inset(bounds, VALUE_PADDING_PX);
        let text = format!("{before}{after}");

        // at rest a long value is cut short with an ellipsis, so it stays on its line
        if !line.focused {
            canvas.text(&text, bounds, self.fonts.value, colour, None);
            return;
        }

        // being edited it shows in full, sliding left under the caret so the part you are
        // typing is always in view; the clip keeps the overflow off the label column
        let caret_px = canvas.measure(before, self.fonts.editing);
        let scroll_px = form::caret_scroll(caret_px, bounds.width(), CARET_SCROLL_MARGIN_PX);
        // the editing style never wraps or cuts, so only the left edge matters here
        let slid = Rect {
            left: bounds.left - scroll_px,
            ..bounds
        };

        canvas.text(&text, slid, self.fonts.editing, colour, Some(bounds));
        self.draw_caret(canvas, bounds, caret_px - scroll_px);
    }

    /// `offset_px` is where the caret sits from the box's left edge, already scrolled.
    fn draw_caret(&self, canvas: &mut Canvas<'_>, bounds: Rect, offset_px: f32) {
        let x = bounds.left + offset_px;
        let caret = Rect {
            left: x,
            top: bounds.top + CARET_INSET_PX,
            right: x + CARET_WIDTH_PX,
            bottom: bounds.bottom - CARET_INSET_PX,
        };

        canvas.fill_rect(caret, self.palette.accent, Some(bounds));
    }

    fn fill_wedges(&self, canvas: &mut Canvas<'_>, frame: &Frame<'_>) {
        for slot in Slot::all() {
            let Some(wedge) = self.wedges.get(slot) else {
                continue;
            };
            let highlight = frame.highlights[slot.index()];
            let fill = blend(self.palette.wedge, self.palette.wedge_selected, highlight);

            canvas.fill_path(wedge, fill);

            // a wedge still fading out keeps its outline until it is dark again
            if highlight <= 0.0 && !frame.edit_mode {
                continue;
            }

            self.outline_wedge(canvas, wedge, slot, frame, highlight);
        }
    }

    /// The outline tracks the fill, so a wedge brightening does not get a hard edge before
    /// it has finished lighting up.
    fn outline_wedge(
        &self,
        canvas: &mut Canvas<'_>,
        wedge: &Path,
        slot: Slot,
        frame: &Frame<'_>,
        highlight: f32,
    ) {
        let dragged = frame.dragging == Some(slot);
        let outline = match dragged {
            true => self.palette.running,
            false => blend(self.palette.accent_dim, self.palette.accent, highlight),
        };
        let stroke = anim::lerp(
            self.metrics.edit_stroke_px,
            self.metrics.selected_stroke_px,
            highlight,
        );

        canvas.stroke_path(wedge, outline, stroke);
    }

    fn draw_dividers(&self, canvas: &mut Canvas<'_>, centre: Point) {
        for slot in Slot::all() {
            let bearing = slot_edge_rad(slot);
            let from = on_circle(centre, self.metrics.hub_radius_px, bearing);
            let to = on_circle(centre, self.metrics.radius_px, bearing);

            canvas.line(from, to, self.palette.divider, self.metrics.divider_stroke_px);
        }
    }

    fn draw_slot(&self, canvas: &mut Canvas<'_>, centre: Point, slot: Slot, frame: &Frame<'_>) {
        let view = frame.slots[slot.index()];
        let selected = frame.selected == Some(slot);
        let bearing = slot_bearing_rad(slot);
        let metrics = self.metrics;

        self.draw_slot_number(canvas, centre, slot, bearing, selected);

        if let Some(key) = view.icon_key {
            let at = on_circle(centre, metrics.icon_radius_px, bearing);
            canvas.bitmap(key, box_around(at, metrics.icon_size_px, metrics.icon_size_px));
        }

        let label_colour = match (view.filled, selected) {
            (false, _) => self.palette.label_empty,
            (true, true) => self.palette.label_selected,
            (true, false) => self.palette.label,
        };
        let label_centre = on_circle(centre, metrics.label_radius_px, bearing);
        canvas.text(
            &view.label.to_uppercase(),
            box_around(label_centre, metrics.label_box_px, metrics.label_size_px * 3.0),
            self.fonts.label,
            label_colour,
            None,
        );

        self.draw_indicators(canvas, centre, bearing, label_centre, view);
    }

    fn draw_slot_number(
        &self,
        canvas: &mut Canvas<'_>,
        centre: Point,
        slot: Slot,
        bearing: f32,
        selected: bool,
    ) {
        let colour = match selected {
            true => self.palette.accent,
            false => self.palette.number,
        };
        let at = on_circle(centre, self.metrics.number_radius_px, bearing);
        let box_px = self.metrics.number_size_px * 2.0;

        canvas.text(
            &(slot.index() + 1).to_string(),
            box_around(at, box_px, box_px),
            self.fonts.number,
            colour,
            None,
        );
    }

    /// Mirrors the taskbar: a pill under a running app, a longer brighter one when it has
    /// focus, a second pill for more than one window, and an accent mark for the type.
    ///
    /// The mark sits on the wedge's own bisector between the number and the icon, so it
    /// cannot collide with a neighbour's label however long the names get.
    fn draw_indicators(
        &self,
        canvas: &mut Canvas<'_>,
        centre: Point,
        bearing: f32,
        label_centre: Point,
        view: SlotView<'_>,
    ) {
        let metrics = self.metrics;

        if let Some(glyph) = view.type_glyph {
            let at = on_circle(centre, metrics.pin_radius_px, bearing);
            canvas.glyph(glyph, at, metrics.number_size_px, self.palette.accent);
        }

        if !view.running {
            return;
        }

        // the focused app gets the longer, brighter pill, as on the taskbar
        let (width, colour) = match view.focused {
            true => (
                metrics.label_box_px * RUNNING_PILL_FOCUSED_RATIO,
                self.palette.accent,
            ),
            false => (
                metrics.label_box_px * RUNNING_PILL_WIDTH_RATIO,
                self.palette.running,
            ),
        };
        let top = label_centre.y + metrics.label_size_px + RUNNING_PILL_GAP_PX;
        canvas.fill_rect(pill(label_centre.x, top, width), colour, None);

        if view.window_count > 1 {
            let second_top = top + RUNNING_PILL_HEIGHT_PX + MULTI_WINDOW_PILL_GAP_PX;
            let second_width = metrics.label_box_px * RUNNING_PILL_WIDTH_RATIO;
            canvas.fill_rect(pill(label_centre.x, second_top, second_width), colour, None);
        }
    }

    fn draw_hub(&self, canvas: &mut Canvas<'_>, centre: Point, frame: &Frame<'_>) {
        let metrics = self.metrics;
        let palette = &self.palette;

        canvas.fill_circle(centre, metrics.hub_radius_px, palette.hub);
        canvas.stroke_circle(centre, metrics.hub_radius_px, palette.accent, metrics.hub_stroke_px);

        let (tab_line, tab_colour) = match frame.edit_mode {
            true => (theme::EDIT_LABEL, palette.accent),
            false => (frame.hub.tab_line, palette.number),
        };
        self.draw_hub_line(
            canvas,
            centre,
            metrics.hub_tab_offset_px(),
            tab_line,
            self.fonts.hub_tab,
            tab_colour,
        );

        if let Some(clock) = frame.hub.clock {
            self.draw_hub_line(canvas, centre, 0.0, clock, self.fonts.hub_clock, palette.label_selected);
        }
        if let Some(date) = frame.hub.date {
            self.draw_hub_line(
                canvas,
                centre,
                metrics.hub_date_offset_px(),
                date,
                self.fonts.hub_date,
                palette.number,
            );
        }

        let cog_at = Point {
            x: centre.x,
            y: centre.y + metrics.cog_offset_px(),
        };
        canvas.glyph(theme::COG_GLYPH, cog_at, metrics.cog_size_px, palette.accent_dim);

        self.draw_overflow(canvas, centre, frame);
    }

    /// The glyph sits by the cog, and the note goes outside the ring, where there is room
    /// for a sentence without crowding the clock.
    fn draw_overflow(&self, canvas: &mut Canvas<'_>, centre: Point, frame: &Frame<'_>) {
        if frame.overflow == 0 {
            return;
        }
        let metrics = self.metrics;

        let at = Point {
            x: centre.x + metrics.hub_radius_px * OVERFLOW_X_RATIO,
            y: centre.y + metrics.cog_offset_px(),
        };
        canvas.glyph(theme::OVERFLOW_GLYPH, at, metrics.cog_size_px, self.palette.accent);

        let Some(note) = frame.hub.overflow_note else {
            return;
        };
        let note_at = Point {
            x: centre.x,
            y: centre.y + metrics.radius_px + metrics.overflow_note_gap_px(),
        };
        canvas.text(
            note,
            box_around(note_at, self.size, metrics.overflow_note_height_px()),
            self.fonts.hub_date,
            self.palette.accent_dim,
            None,
        );
    }

    fn draw_hub_line(
        &self,
        canvas: &mut Canvas<'_>,
        centre: Point,
        offset_y: f32,
        text: &str,
        style: Style,
        colour: Colour,
    ) {
        let at = Point {
            x: centre.x,
            y: centre.y + offset_y,
        };
        let radius = self.metrics.hub_radius_px;
        canvas.text(text, box_around(at, radius * 2.0, radius), style, colour, None);
    }
}

/// Window units to buffer pixels: a uniform scale and an offset.
#[derive(Debug, Clone, Copy)]
struct View {
    scale: f32,
    offset: Point,
}

impl View {
    fn identity(pixel_ratio: f32) -> Self {
        Self {
            scale: pixel_ratio,
            offset: Point::default(),
        }
    }

    /// The wheel grows into place from slightly small, which points the eye at the centre
    /// without delaying anything: input is live from the first frame.
    fn appearing(pixel_ratio: f32, centre: Point, progress: f32) -> Self {
        let grown = anim::lerp(APPEAR_FROM_SCALE, 1.0, progress);
        let grown = (grown * APPEAR_SCALE_STEPS).round() / APPEAR_SCALE_STEPS;

        Self {
            scale: pixel_ratio * grown,
            offset: Point {
                x: pixel_ratio * centre.x * (1.0 - grown),
                y: pixel_ratio * centre.y * (1.0 - grown),
            },
        }
    }

    fn transform(self) -> Transform {
        Transform::from_row(self.scale, 0.0, 0.0, self.scale, self.offset.x, self.offset.y)
    }

    fn rect(self, rect: Rect) -> Rect {
        Rect {
            left: rect.left * self.scale + self.offset.x,
            top: rect.top * self.scale + self.offset.y,
            right: rect.right * self.scale + self.offset.x,
            bottom: rect.bottom * self.scale + self.offset.y,
        }
    }
}

/// One frame's drawing surface, and a record of where it has drawn.
struct Canvas<'a> {
    pixmap: &'a mut Pixmap,
    text: &'a mut TextPainter,
    glyphs: &'a mut Glyphs,
    icons: &'a HashMap<String, Pixmap>,
    view: View,
    pixel_ratio: f32,
    /// In buffer pixels.
    inked: Rect,
}

impl Canvas<'_> {
    fn fill_path(&mut self, path: &Path, colour: Colour) {
        self.pixmap
            .fill_path(path, &paint(colour), FillRule::Winding, self.view.transform(), None);
        self.ink(path_bounds(path), 0.0);
    }

    fn stroke_path(&mut self, path: &Path, colour: Colour, width_px: f32) {
        let stroke = Stroke {
            width: width_px,
            ..Stroke::default()
        };
        self.pixmap
            .stroke_path(path, &paint(colour), &stroke, self.view.transform(), None);
        self.ink(path_bounds(path), width_px / 2.0);
    }

    fn fill_circle(&mut self, centre: Point, radius_px: f32, colour: Colour) {
        if let Some(circle) = PathBuilder::from_circle(centre.x, centre.y, radius_px) {
            self.fill_path(&circle, colour);
        }
    }

    fn stroke_circle(&mut self, centre: Point, radius_px: f32, colour: Colour, width_px: f32) {
        if let Some(circle) = PathBuilder::from_circle(centre.x, centre.y, radius_px) {
            self.stroke_path(&circle, colour, width_px);
        }
    }

    fn line(&mut self, from: Point, to: Point, colour: Colour, width_px: f32) {
        let mut builder = PathBuilder::new();
        builder.move_to(from.x, from.y);
        builder.line_to(to.x, to.y);
        if let Some(line) = builder.finish() {
            self.stroke_path(&line, colour, width_px);
        }
    }

    /// `clip` is in window units, like `rect`.
    fn fill_rect(&mut self, rect: Rect, colour: Colour, clip: Option<Rect>) {
        let rect = match clip {
            Some(clip) => rect.intersect(clip),
            None => rect,
        };
        let Some(shape) = tiny_skia::Rect::from_ltrb(rect.left, rect.top, rect.right, rect.bottom)
        else {
            return;
        };
        self.pixmap
            .fill_rect(shape, &paint(colour), self.view.transform(), None);
        self.ink(rect, 0.0);
    }

    fn text(&mut self, text: &str, bounds: Rect, style: Style, colour: Colour, clip: Option<Rect>) {
        if text.is_empty() {
            return;
        }
        let inked = self.text.draw(
            self.pixmap,
            TextRequest {
                text,
                bounds: self.view.rect(bounds),
                style,
                scale: self.view.scale,
                colour,
                clip: clip.map(|clip| self.view.rect(clip)),
            },
        );
        self.inked = self.inked.union(inked);
    }

    /// In window units, measured at the size the text is drawn at.
    fn measure(&mut self, text: &str, style: Style) -> f32 {
        self.text.measure(text, style, self.view.scale)
    }

    /// `size` is the glyph's side, in window units, centred on `centre`.
    fn glyph(&mut self, glyph: Glyph, centre: Point, size: f32, colour: Colour) {
        let bounds = box_around(centre, size, size);
        self.glyphs
            .draw(self.pixmap, glyph, colour, self.view.rect(bounds));
        self.ink(bounds, 0.0);
    }

    /// Draws nothing for an icon that was never uploaded, since a missing icon is not
    /// worth failing the frame over.
    fn bitmap(&mut self, key: &str, bounds: Rect) {
        let Some(icon) = self.icons.get(key) else {
            return;
        };
        let target = self.view.rect(bounds);
        let transform = Transform::from_row(
            target.width() / icon.width() as f32,
            0.0,
            0.0,
            target.height() / icon.height() as f32,
            target.left,
            target.top,
        );
        let paint = PixmapPaint {
            quality: FilterQuality::Bicubic,
            ..PixmapPaint::default()
        };

        self.pixmap
            .draw_pixmap(0, 0, icon.as_ref(), &paint, transform, None);
        self.ink(bounds, 0.0);
    }

    /// Records a shape's window-unit bounds as drawn, `reach` units further out for a
    /// stroke straddling its outline.
    fn ink(&mut self, bounds: Rect, reach: f32) {
        let pixels = self
            .view
            .rect(bounds.outset(reach))
            .outset(ANTIALIAS_REACH_PX);
        self.inked = self.inked.union(pixels);
    }
}

fn path_bounds(path: &Path) -> Rect {
    let bounds = path.bounds();
    Rect {
        left: bounds.left(),
        top: bounds.top(),
        right: bounds.right(),
        bottom: bounds.bottom(),
    }
}

fn paint(colour: Colour) -> Paint<'static> {
    let mut paint = Paint {
        anti_alias: true,
        ..Paint::default()
    };
    let clamp = |channel: f32| channel.clamp(0.0, 1.0);
    let colour = Color::from_rgba(clamp(colour.r), clamp(colour.g), clamp(colour.b), clamp(colour.a))
        .unwrap_or(Color::TRANSPARENT);
    paint.set_color(colour);
    paint
}

fn clear(pixmap: &mut Pixmap, region: Option<Region>) {
    const BYTES_PER_PIXEL: usize = 4;

    let Some(region) = region else {
        pixmap.fill(Color::TRANSPARENT);
        return;
    };
    let stride = pixmap.width() as usize * BYTES_PER_PIXEL;
    let start_x = region.x as usize * BYTES_PER_PIXEL;
    let row_bytes = region.width as usize * BYTES_PER_PIXEL;
    let data = pixmap.data_mut();
    for row in region.y..region.y + region.height {
        let start = row as usize * stride + start_x;
        data[start..start + row_bytes].fill(0);
    }
}

/// Rounded outward to whole pixels and kept inside the buffer.
fn region_of(inked: Rect, pixmap: &Pixmap) -> Region {
    let left = inked.left.floor().max(0.0) as u32;
    let top = inked.top.floor().max(0.0) as u32;
    let right = (inked.right.ceil().max(0.0) as u32).min(pixmap.width());
    let bottom = (inked.bottom.ceil().max(0.0) as u32).min(pixmap.height());

    Region {
        x: left.min(right),
        y: top.min(bottom),
        width: right.saturating_sub(left),
        height: bottom.saturating_sub(top),
    }
}

fn buffer_side(size: f32, pixel_ratio: f32) -> (u32, u32) {
    let side = (size * pixel_ratio).ceil().max(1.0) as u32;
    (side, side)
}

fn buffer_for(size: f32, pixel_ratio: f32) -> Result<Pixmap> {
    let (width, height) = buffer_side(size, pixel_ratio);
    Pixmap::new(width, height).with_context(|| {
        format!("could not allocate a {width}x{height} drawing buffer for the wheel")
    })
}

fn pill(centre_x: f32, top: f32, width: f32) -> Rect {
    Rect {
        left: centre_x - width / 2.0,
        top,
        right: centre_x + width / 2.0,
        bottom: top + RUNNING_PILL_HEIGHT_PX,
    }
}

/// The area a petal may use: the part of the window that is on screen, less a hair of
/// margin.
///
/// The window is sized for the furthest a petal can reach, so on a monitor shorter than
/// that it hangs off the top and bottom. Clamping to the window alone let a north or
/// south petal open off screen.
fn drawable_area(size: f32, visible: Option<Rect>) -> Rect {
    let window = Rect {
        left: 0.0,
        top: 0.0,
        right: size,
        bottom: size,
    };
    let on_screen = match visible {
        Some(visible) => window.intersect(visible),
        None => window,
    };

    Rect {
        left: on_screen.left + FORM_EDGE_MARGIN_PX,
        top: on_screen.top + FORM_EDGE_MARGIN_PX,
        right: on_screen.right - FORM_EDGE_MARGIN_PX,
        bottom: on_screen.bottom - FORM_EDGE_MARGIN_PX,
    }
}

fn centre_of(size: f32) -> Point {
    let half = size / 2.0;
    Point { x: half, y: half }
}

/// Mixes two palette colours, so a wedge can fade rather than switch.
fn blend(from: Colour, to: Colour, factor: f32) -> Colour {
    Colour {
        r: anim::lerp(from.r, to.r, factor),
        g: anim::lerp(from.g, to.g, factor),
        b: anim::lerp(from.b, to.b, factor),
        a: anim::lerp(from.a, to.a, factor),
    }
}

fn box_around(centre: Point, width_px: f32, height_px: f32) -> Rect {
    Rect {
        left: centre.x - width_px / 2.0,
        top: centre.y - height_px / 2.0,
        right: centre.x + width_px / 2.0,
        bottom: centre.y + height_px / 2.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn before_the_window_is_placed_the_whole_window_is_usable() {
        let area = drawable_area(1000.0, None);

        assert_eq!(area.left, FORM_EDGE_MARGIN_PX);
        assert_eq!(area.bottom, 1000.0 - FORM_EDGE_MARGIN_PX);
    }

    /// A window hanging off the top and bottom of the screen only lends its on-screen
    /// middle to a petal.
    #[test]
    fn a_window_hanging_off_the_screen_lends_only_its_visible_part() {
        let visible = Rect {
            left: -200.0,
            top: 245.0,
            right: 1800.0,
            bottom: 1385.0,
        };

        let area = drawable_area(1690.0, Some(visible));

        assert_eq!(area.top, 245.0 + FORM_EDGE_MARGIN_PX);
        assert_eq!(area.bottom, 1385.0 - FORM_EDGE_MARGIN_PX);
        // sideways the window is the tighter limit, so it still wins
        assert_eq!(area.left, FORM_EDGE_MARGIN_PX);
        assert_eq!(area.right, 1690.0 - FORM_EDGE_MARGIN_PX);
    }

    /// The real case end to end: every slice's petal, laid out against the visible part,
    /// stays on screen.
    #[test]
    fn every_petal_stays_on_a_short_screen() {
        let size = 1690.0;
        let visible = Rect {
            left: -115.0,
            top: 245.0,
            right: 1805.0,
            bottom: 1385.0,
        };
        let area = drawable_area(size, Some(visible));
        let centre = centre_of(size);

        for slot in Slot::all() {
            for rows in [3, 8, 12] {
                let petal = Petal::new(centre, 387.5, slot_bearing_rad(slot), rows, 1.0, area);
                assert!(petal.panel.top >= visible.top, "slot {} rows {rows}", slot.index());
                assert!(
                    petal.panel.bottom <= visible.bottom,
                    "slot {} rows {rows}",
                    slot.index()
                );
            }
        }
    }

    #[test]
    fn the_inked_region_is_kept_inside_the_buffer() {
        let pixmap = Pixmap::new(100, 100).expect("a small pixmap");
        let inked = Rect {
            left: -10.5,
            top: 20.2,
            right: 150.0,
            bottom: 40.7,
        };

        assert_eq!(
            region_of(inked, &pixmap),
            Region {
                x: 0,
                y: 20,
                width: 100,
                height: 21
            }
        );
    }

    /// Draws a busy frame and writes it next to the build, for eyeballing the renderer and
    /// timing it. Ignored by default because it loads every system font.
    #[test]
    #[ignore = "writes target/wheel-snapshot.png; run with --ignored"]
    fn snapshot_a_busy_frame() {
        const FRAMES: u32 = 30;
        let accent = kile_wheel_core::config::Rgb {
            red: 0xD8,
            green: 0xB3,
            blue: 0x6A,
        };
        let metrics = Metrics::new(1.25, 1.0);
        let size = metrics.window_size_px() as f32;
        let mut renderer =
            Renderer::new(size, 1.0, metrics, Palette::new(accent, 1.0)).expect("renderer");

        let labels = ["Code", "Firefox", "Files", "", "Terminal", "Jira", "Notes", ""];
        let slots = std::array::from_fn(|index| SlotView {
            label: labels[index],
            type_glyph: (!labels[index].is_empty()).then(|| theme::TYPE_GLYPHS[index % 7]),
            running: index % 2 == 0,
            focused: index == 0,
            window_count: index % 3,
            filled: !labels[index].is_empty(),
            ..SlotView::default()
        });
        let mut highlights = [0.0; Slot::COUNT];
        highlights[1] = 1.0;
        let frame = Frame {
            slots,
            selected: Slot::new(1),
            highlights,
            appear: 1.0,
            hub: HubView {
                tab_line: "1  MAIN",
                clock: Some("12:34"),
                date: Some("Sun 08 Nov"),
                overflow_note: Some("2 apps have nowhere to land."),
            },
            edit_mode: false,
            dragging: None,
            overflow: 2,
            form: None,
        };

        let started = std::time::Instant::now();
        for _ in 0..FRAMES {
            renderer.draw(&frame);
        }
        let per_frame = started.elapsed() / FRAMES;
        let drawn = renderer.draw(&frame);
        println!("{per_frame:?} per frame, region {:?}", drawn.region);

        let png = drawn.pixmap.encode_png().expect("encodes");
        std::fs::write(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/wheel-snapshot.png"),
            png,
        )
        .expect("writes");
    }

    #[test]
    fn nothing_inked_is_an_empty_region() {
        let pixmap = Pixmap::new(10, 10).expect("a small pixmap");

        assert_eq!(region_of(Rect::default(), &pixmap).width, 0);
    }
}
