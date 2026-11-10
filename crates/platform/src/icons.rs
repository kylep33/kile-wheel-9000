//! Icon extraction and its in-memory cache.
//!
//! Picture files are decoded here, the same way on every OS. Anything else, an app, a
//! folder or a script, gets the icon the desktop itself shows for it.

use std::collections::HashMap;
use std::path::Path;

use tracing::debug;

use crate::icon_files::is_picture;
use crate::sys;

/// Room for a crisper icon than asked for, on a scaled display, without keeping a
/// full-size photo in memory.
const OVERSAMPLE: u32 = 2;

const BYTES_PER_PIXEL: usize = 4;
const ALPHA_OFFSET: usize = 3;
const MAX_CHANNEL: u32 = 255;

/// An icon's pixels: RGBA, premultiplied, rows top to bottom.
#[derive(Debug, Clone)]
pub struct IconImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Icons kept for the process lifetime, because there are only ever as many as there are
/// pinned and running apps.
#[derive(Debug, Default)]
pub struct IconCache {
    /// A miss is remembered too, so a source without an icon is only asked once.
    images: HashMap<String, Option<IconImage>>,
}

impl IconCache {
    /// `None` when the source has no icon that can be rendered.
    pub fn get(&mut self, source: &str, size_px: u32) -> Option<&IconImage> {
        if !self.images.contains_key(source) {
            self.images
                .insert(source.to_owned(), extract(source, size_px));
        }

        self.images.get(source)?.as_ref()
    }

    pub fn forget(&mut self, source: &str) {
        self.images.remove(source);
    }
}

fn extract(source: &str, size_px: u32) -> Option<IconImage> {
    let path = Path::new(source);
    match is_picture(path) {
        true => decode_picture(path, size_px),
        false => sys::icons::extract(source, size_px),
    }
}

pub(crate) fn decode_picture(path: &Path, size_px: u32) -> Option<IconImage> {
    let picture = match image::open(path) {
        Ok(picture) => picture,
        Err(error) => {
            debug!("no icon from {}: {error}", path.display());
            return None;
        }
    };

    let limit = size_px.saturating_mul(OVERSAMPLE).max(1);
    let fitted = match picture.width() > limit || picture.height() > limit {
        true => picture.thumbnail(limit, limit),
        false => picture,
    };
    let rgba = fitted.to_rgba8();

    Some(from_straight_rgba(rgba.width(), rgba.height(), rgba.into_raw()))
}

/// Most sources hand back straight alpha; drawing wants it premultiplied.
pub(crate) fn from_straight_rgba(width: u32, height: u32, mut rgba: Vec<u8>) -> IconImage {
    premultiply(&mut rgba);
    IconImage {
        width,
        height,
        rgba,
    }
}

fn premultiply(pixels: &mut [u8]) {
    let (pixels, _) = pixels.as_chunks_mut::<BYTES_PER_PIXEL>();
    for pixel in pixels {
        let alpha = u32::from(pixel[ALPHA_OFFSET]);
        for channel in &mut pixel[..ALPHA_OFFSET] {
            *channel = (u32::from(*channel) * alpha / MAX_CHANNEL) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transparent_pixel_premultiplies_to_nothing() {
        let mut pixels = vec![0xFF, 0xFF, 0xFF, 0x00];

        premultiply(&mut pixels);

        assert_eq!(pixels, [0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn an_opaque_pixel_premultiplies_to_itself() {
        let mut pixels = vec![0x10, 0x20, 0x30, 0xFF];

        premultiply(&mut pixels);

        assert_eq!(pixels, [0x10, 0x20, 0x30, 0xFF]);
    }

    #[test]
    fn a_half_transparent_pixel_scales_its_channels_by_alpha() {
        let mut pixels = vec![0xFF, 0x80, 0x40, 0x80];

        premultiply(&mut pixels);

        assert_eq!(pixels[..ALPHA_OFFSET], [0x80, 0x40, 0x20]);
        assert_eq!(pixels[ALPHA_OFFSET], 0x80);
    }

    #[test]
    fn a_missing_picture_has_no_icon() {
        assert!(
            IconCache::default()
                .get("/definitely/not/here.png", 32)
                .is_none()
        );
    }
}
