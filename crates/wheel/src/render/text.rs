//! Text shaping and rasterising on the CPU, so every OS draws the same pixels.

use cosmic_text::{
    Align, Attrs, Buffer, Ellipsize, EllipsizeHeightLimit, Family, FontSystem,
    Metrics as LineMetrics, Shaping, SwashCache, SwashContent, Weight, Wrap,
};
use std::collections::HashMap;

use resvg::tiny_skia::Pixmap;

use crate::geom::Rect;
use crate::theme::{self, Colour, Metrics};

/// Gap between baselines as a multiple of the font size, close to what DirectWrite and
/// Core Text pick for UI fonts.
const LINE_HEIGHT_RATIO: f32 = 1.25;

const SEMI_BOLD: Weight = Weight(600);
const BYTES_PER_PIXEL: usize = 4;
const ALPHA: usize = 3;
const MAX_CHANNEL: f32 = 255.0;

/// How a run of text sits in its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Flow {
    /// Centred both ways, wrapping onto more lines when it has to.
    Centred,
    /// One left-aligned line, vertically centred, cut short with an ellipsis.
    Ellipsis,
    /// One left-aligned line that is never cut, so it can scroll under a caret.
    Full,
}

#[derive(Debug, Clone, Copy)]
pub struct Style {
    /// In window units; the renderer scales it to pixels.
    pub size: f32,
    pub weight: Weight,
    pub flow: Flow,
}

/// Every style the wheel needs, sized for one set of metrics.
#[derive(Debug, Clone, Copy)]
pub struct Fonts {
    pub label: Style,
    pub number: Style,
    pub hub_tab: Style,
    pub hub_clock: Style,
    pub hub_date: Style,
    /// A form value at rest: one line, cut short with an ellipsis.
    pub value: Style,
    /// A form value being edited: one line, never cut, so it can scroll under the caret.
    pub editing: Style,
}

impl Fonts {
    pub fn new(metrics: Metrics) -> Self {
        Self {
            label: centred(metrics.label_size_px, SEMI_BOLD),
            number: centred(metrics.number_size_px, Weight::NORMAL),
            hub_tab: centred(metrics.hub_tab_size_px, SEMI_BOLD),
            hub_clock: centred(metrics.hub_clock_size_px, SEMI_BOLD),
            hub_date: centred(metrics.hub_date_size_px, Weight::NORMAL),
            value: line(metrics.number_size_px, Flow::Ellipsis),
            editing: line(metrics.number_size_px, Flow::Full),
        }
    }
}

fn centred(size: f32, weight: Weight) -> Style {
    Style {
        size,
        weight,
        flow: Flow::Centred,
    }
}

fn line(size: f32, flow: Flow) -> Style {
    Style {
        size,
        weight: Weight::NORMAL,
        flow,
    }
}

/// Shaped lines kept between frames. A wheel shows a few dozen strings that rarely change,
/// so this only grows past the limit while typing, and then starts over.
const MAX_CACHED_LAYOUTS: usize = 512;

/// Everything that changes how a string is shaped. Floats go in as bits so the key can
/// be hashed; the sizes come from fixed metrics, so equal sizes are bit-equal.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct LayoutKey {
    text: String,
    size_bits: u32,
    weight: u16,
    flow: Flow,
    width_bits: Option<u32>,
}

/// Owns the font database, the glyph cache and the shaped lines, which are all expensive
/// to build.
pub struct TextPainter {
    system: FontSystem,
    cache: SwashCache,
    layouts: HashMap<LayoutKey, Buffer>,
}

impl std::fmt::Debug for TextPainter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("TextPainter").finish_non_exhaustive()
    }
}

impl TextPainter {
    /// Loads the system fonts, which takes a while, so this runs once at startup.
    pub fn new() -> Self {
        Self {
            system: FontSystem::new(),
            cache: SwashCache::new(),
            layouts: HashMap::new(),
        }
    }

    /// Draws `text` into `bounds`, both in pixels, and returns the pixels it inked.
    /// `scale` turns the style's window units into pixels.
    pub fn draw(&mut self, target: &mut Pixmap, request: TextRequest<'_>) -> Rect {
        let key = self.layout(
            request.text,
            request.style,
            request.scale,
            Some(request.bounds.width()),
        );
        let buffer = &self.layouts[&key];
        let block_height = block_height(buffer);
        let top = request.bounds.top + (request.bounds.height() - block_height) / 2.0;
        let clip = match request.clip {
            Some(clip) => target_bounds(target).intersect(clip),
            None => target_bounds(target),
        };

        let mut inked = Rect::default();
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                let physical = glyph.physical((request.bounds.left, top + run.line_y), 1.0);
                let Some(image) = self.cache.get_image(&mut self.system, physical.cache_key) else {
                    continue;
                };
                let at = (
                    physical.x + image.placement.left,
                    physical.y - image.placement.top,
                );
                inked = inked.union(blit(target, image, at, request.colour, clip));
            }
        }
        inked
    }

    /// Width of one line of `text`, in window units.
    pub fn measure(&mut self, text: &str, style: Style, scale: f32) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let line_style = Style {
            flow: Flow::Full,
            ..style
        };
        let key = self.layout(text, line_style, scale, None);
        let widest_px = self.layouts[&key]
            .layout_runs()
            .map(|run| run.line_w)
            .fold(0.0, f32::max);
        widest_px / scale
    }

    /// Shapes on first sight only, and hands back the key the result is filed under.
    fn layout(&mut self, text: &str, style: Style, scale: f32, width_px: Option<f32>) -> LayoutKey {
        let size_px = style.size * scale;
        let width_px = match style.flow {
            Flow::Full => None,
            Flow::Centred | Flow::Ellipsis => width_px,
        };
        let key = LayoutKey {
            text: text.to_owned(),
            size_bits: size_px.to_bits(),
            weight: style.weight.0,
            flow: style.flow,
            width_bits: width_px.map(f32::to_bits),
        };
        if self.layouts.contains_key(&key) {
            return key;
        }
        if self.layouts.len() >= MAX_CACHED_LAYOUTS {
            self.layouts.clear();
        }

        let buffer = shape(&mut self.system, text, style, size_px, width_px);
        self.layouts.insert(key.clone(), buffer);
        key
    }
}

fn shape(
    system: &mut FontSystem,
    text: &str,
    style: Style,
    size_px: f32,
    width_px: Option<f32>,
) -> Buffer {
    let mut buffer = Buffer::new(system, LineMetrics::new(size_px, size_px * LINE_HEIGHT_RATIO));

    let (wrap, align, ellipsize) = match style.flow {
        Flow::Centred => (Wrap::WordOrGlyph, Align::Center, Ellipsize::None),
        Flow::Ellipsis => (
            Wrap::None,
            Align::Left,
            Ellipsize::End(EllipsizeHeightLimit::Lines(1)),
        ),
        Flow::Full => (Wrap::None, Align::Left, Ellipsize::None),
    };

    buffer.set_wrap(wrap);
    buffer.set_ellipsize(ellipsize);
    buffer.set_size(width_px, None);
    let attrs = Attrs::new()
        .family(Family::Name(theme::LABEL_FONT))
        .weight(style.weight);
    buffer.set_text(text, &attrs, Shaping::Advanced, Some(align));
    buffer.shape_until_scroll(system, false);
    buffer
}

/// One `draw` call's worth of inputs.
#[derive(Debug, Clone, Copy)]
pub struct TextRequest<'a> {
    pub text: &'a str,
    pub bounds: Rect,
    pub style: Style,
    pub scale: f32,
    pub colour: Colour,
    /// Pixels outside this are left alone, for a value scrolling under its caret.
    pub clip: Option<Rect>,
}

fn block_height(buffer: &Buffer) -> f32 {
    buffer
        .layout_runs()
        .map(|run| run.line_top + run.line_height)
        .fold(0.0, f32::max)
}

fn target_bounds(target: &Pixmap) -> Rect {
    Rect {
        left: 0.0,
        top: 0.0,
        right: target.width() as f32,
        bottom: target.height() as f32,
    }
}

/// Composites one glyph over the target, source-over on premultiplied pixels.
fn blit(
    target: &mut Pixmap,
    image: &cosmic_text::SwashImage,
    at: (i32, i32),
    colour: Colour,
    clip: Rect,
) -> Rect {
    let width = image.placement.width as i32;
    let height = image.placement.height as i32;
    let left = at.0.max(clip.left.ceil() as i32);
    let top = at.1.max(clip.top.ceil() as i32);
    let right = (at.0 + width).min(clip.right.floor() as i32);
    let bottom = (at.1 + height).min(clip.bottom.floor() as i32);
    if left >= right || top >= bottom {
        return Rect::default();
    }

    let stride = target.width() as usize * BYTES_PER_PIXEL;
    let pixels = target.data_mut();
    for y in top..bottom {
        for x in left..right {
            let source = ((y - at.1) * width + (x - at.0)) as usize;
            let Some(ink) = glyph_pixel(image, source, colour) else {
                continue;
            };
            let offset = y as usize * stride + x as usize * BYTES_PER_PIXEL;
            over(&mut pixels[offset..offset + BYTES_PER_PIXEL], ink);
        }
    }

    Rect {
        left: left as f32,
        top: top as f32,
        right: right as f32,
        bottom: bottom as f32,
    }
}

/// The glyph's colour at one pixel, premultiplied, or `None` where it has no ink.
fn glyph_pixel(image: &cosmic_text::SwashImage, index: usize, colour: Colour) -> Option<[f32; 4]> {
    match image.content {
        SwashContent::Mask => {
            let coverage = f32::from(*image.data.get(index)?) / MAX_CHANNEL;
            premultiplied(colour.r, colour.g, colour.b, coverage * colour.a)
        }
        // emoji keep their own colours, faded by ours
        SwashContent::Color => {
            let pixel = image.data.get(index * BYTES_PER_PIXEL..(index + 1) * BYTES_PER_PIXEL)?;
            let channel = |at: usize| f32::from(pixel[at]) / MAX_CHANNEL;
            premultiplied(channel(0), channel(1), channel(2), channel(ALPHA) * colour.a)
        }
        SwashContent::SubpixelMask => {
            let pixel = image.data.get(index * BYTES_PER_PIXEL..(index + 1) * BYTES_PER_PIXEL)?;
            let coverage = pixel[..ALPHA].iter().map(|value| f32::from(*value)).sum::<f32>()
                / (MAX_CHANNEL * ALPHA as f32);
            premultiplied(colour.r, colour.g, colour.b, coverage * colour.a)
        }
    }
}

fn premultiplied(r: f32, g: f32, b: f32, a: f32) -> Option<[f32; 4]> {
    (a > 0.0).then_some([r * a, g * a, b * a, a])
}

fn over(pixel: &mut [u8], ink: [f32; 4]) {
    let keep = 1.0 - ink[ALPHA];
    for (channel, source) in pixel.iter_mut().zip(ink) {
        let blended = source * MAX_CHANNEL + f32::from(*channel) * keep;
        *channel = blended.round().clamp(0.0, MAX_CHANNEL) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ink_over_a_clear_pixel_is_the_ink() {
        let mut pixel = [0u8; 4];
        over(&mut pixel, [0.5, 0.0, 0.0, 0.5]);

        assert_eq!(pixel, [128, 0, 0, 128]);
    }

    #[test]
    fn opaque_ink_replaces_whatever_was_there() {
        let mut pixel = [10u8, 20, 30, 255];
        over(&mut pixel, [1.0, 1.0, 1.0, 1.0]);

        assert_eq!(pixel, [255, 255, 255, 255]);
    }

    #[test]
    fn no_coverage_is_no_ink() {
        assert_eq!(premultiplied(1.0, 1.0, 1.0, 0.0), None);
    }
}
