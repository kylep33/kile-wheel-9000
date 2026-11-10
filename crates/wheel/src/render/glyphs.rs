//! The small line icons the wheel marks slices with. Lucide SVGs, drawn as vectors so they
//! stay sharp at every size and look the same on every OS.

use std::collections::HashMap;

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg;
use tracing::warn;

use crate::geom::Rect;
use crate::theme::Colour;

/// Lucide draws on a 24 unit grid.
const VIEWBOX_UNITS: f32 = 24.0;

/// Lucide strokes in `currentColor`, which is swapped for a real colour before parsing.
const CURRENT_COLOUR: &str = "\"currentColor\"";

const MAX_CHANNEL: f32 = 255.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Glyph {
    Cog,
    UnpinnedApp,
    App,
    Script,
    Url,
    Snippet,
    Folder,
    Terminal,
    NewWindow,
    Overflow,
}

impl Glyph {
    fn svg(self) -> &'static str {
        match self {
            Self::Cog => include_str!("../../assets/glyphs/settings.svg"),
            Self::UnpinnedApp => include_str!("../../assets/glyphs/app-window.svg"),
            Self::App => include_str!("../../assets/glyphs/pin.svg"),
            Self::Script => include_str!("../../assets/glyphs/code.svg"),
            Self::Url => include_str!("../../assets/glyphs/globe.svg"),
            Self::Snippet => include_str!("../../assets/glyphs/clipboard-paste.svg"),
            Self::Folder => include_str!("../../assets/glyphs/folder.svg"),
            Self::Terminal => include_str!("../../assets/glyphs/square-terminal.svg"),
            Self::NewWindow => include_str!("../../assets/glyphs/plus.svg"),
            Self::Overflow => include_str!("../../assets/glyphs/info.svg"),
        }
    }
}

/// Parsed glyphs per colour. There are only a handful of each, so nothing is evicted.
#[derive(Default)]
pub struct Glyphs {
    trees: HashMap<(Glyph, [u8; 4]), Option<usvg::Tree>>,
}

impl std::fmt::Debug for Glyphs {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Glyphs")
            .field("parsed", &self.trees.len())
            .finish()
    }
}

impl Glyphs {
    /// Draws the glyph filling `bounds`, which is in pixels.
    pub fn draw(&mut self, target: &mut Pixmap, glyph: Glyph, colour: Colour, bounds: Rect) {
        let key = (glyph, channels(colour));
        let tree = self
            .trees
            .entry(key)
            .or_insert_with(|| parse(glyph, colour))
            .as_ref();
        let Some(tree) = tree else {
            return;
        };

        let scale = bounds.width().min(bounds.height()) / VIEWBOX_UNITS;
        let transform = Transform::from_row(scale, 0.0, 0.0, scale, bounds.left, bounds.top);
        resvg::render(tree, transform, &mut target.as_mut());
    }
}

/// A bad glyph is logged once and then left blank, since a missing mini icon is not worth
/// failing a frame over.
fn parse(glyph: Glyph, colour: Colour) -> Option<usvg::Tree> {
    let [r, g, b, _] = channels(colour);
    let painted = glyph.svg().replace(
        CURRENT_COLOUR,
        &format!("\"#{r:02x}{g:02x}{b:02x}\" stroke-opacity=\"{}\"", colour.a),
    );

    match usvg::Tree::from_str(&painted, &usvg::Options::default()) {
        Ok(tree) => Some(tree),
        Err(error) => {
            warn!(?glyph, "the bundled glyph would not parse, so it is left blank: {error}");
            None
        }
    }
}

fn channels(colour: Colour) -> [u8; 4] {
    [colour.r, colour.g, colour.b, colour.a].map(|channel| (channel * MAX_CHANNEL).round() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Glyph; 10] = [
        Glyph::Cog,
        Glyph::UnpinnedApp,
        Glyph::App,
        Glyph::Script,
        Glyph::Url,
        Glyph::Snippet,
        Glyph::Folder,
        Glyph::Terminal,
        Glyph::NewWindow,
        Glyph::Overflow,
    ];

    const WHITE: Colour = Colour {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };

    #[test]
    fn every_bundled_glyph_parses() {
        for glyph in ALL {
            assert!(parse(glyph, WHITE).is_some(), "{glyph:?}");
        }
    }

    #[test]
    fn a_drawn_glyph_leaves_ink() {
        let mut target = Pixmap::new(32, 32).expect("a small pixmap");
        let bounds = Rect {
            left: 0.0,
            top: 0.0,
            right: 32.0,
            bottom: 32.0,
        };

        Glyphs::default().draw(&mut target, Glyph::Cog, WHITE, bounds);

        assert!(target.pixels().iter().any(|pixel| pixel.alpha() > 0));
    }
}
