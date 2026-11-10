//! The topmost, click-through, never-focused surface the wheel is shown on.
//!
//! The wheel is drawn on the CPU into one premultiplied RGBA buffer, the same on every
//! OS, so all a backend has to do is put part of that buffer on screen at a position.
//! Focus has to stay with the app underneath, because snippet paste and window
//! activation both depend on it.

use thiserror::Error;

use crate::geom::ScreenPoint;
use crate::sys;

#[derive(Debug, Error)]
#[error("{0}")]
pub struct OverlayError(pub(crate) String);

/// One frame to put on screen: a region of a larger premultiplied RGBA buffer.
#[derive(Debug, Clone, Copy)]
pub struct FramePixels<'a> {
    /// The whole buffer, rows top to bottom, four bytes per pixel.
    pub rgba: &'a [u8],
    /// Width of the whole buffer, in pixels.
    pub buffer_width: u32,
    pub region_x: u32,
    pub region_y: u32,
    pub region_width: u32,
    pub region_height: u32,
    /// Where the region's top-left corner goes, in screen units.
    pub origin: ScreenPoint,
    /// Buffer pixels per screen unit.
    pub pixel_ratio: f32,
}

impl FramePixels<'_> {
    /// One row of the region.
    pub fn row(&self, index: u32) -> &[u8] {
        const BYTES_PER_PIXEL: usize = 4;

        let stride = self.buffer_width as usize * BYTES_PER_PIXEL;
        let start = (self.region_y + index) as usize * stride + self.region_x as usize * BYTES_PER_PIXEL;
        &self.rgba[start..start + self.region_width as usize * BYTES_PER_PIXEL]
    }

    /// The region's size in screen units, rounded up so nothing is cut off.
    pub fn size_units(&self) -> (i32, i32) {
        let to_units = |pixels: u32| (pixels as f32 / self.pixel_ratio).ceil() as i32;
        (to_units(self.region_width), to_units(self.region_height))
    }
}

#[derive(Debug)]
pub struct OverlaySurface {
    sys: sys::overlay::Surface,
}

impl OverlaySurface {
    /// Created hidden, so the first open only has to draw and show it.
    pub fn create() -> Result<Self, OverlayError> {
        Ok(Self {
            sys: sys::overlay::Surface::create()?,
        })
    }

    /// Replaces what is on screen, and moves it, in one step.
    pub fn present(&mut self, frame: &FramePixels<'_>) -> Result<(), OverlayError> {
        if frame.region_width == 0 || frame.region_height == 0 {
            return Ok(());
        }
        self.sys.present(frame)
    }

    pub fn show(&mut self) {
        self.sys.show();
    }

    pub fn hide(&mut self) {
        self.sys.hide();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_is_cut_from_the_region_not_the_whole_buffer() {
        // a 3x2 buffer whose pixels count up, with the right-hand 2x1 as the region
        let rgba: Vec<u8> = (0..24).collect();
        let frame = FramePixels {
            rgba: &rgba,
            buffer_width: 3,
            region_x: 1,
            region_y: 1,
            region_width: 2,
            region_height: 1,
            origin: ScreenPoint::default(),
            pixel_ratio: 1.0,
        };

        assert_eq!(frame.row(0), &rgba[16..24]);
    }

    #[test]
    fn a_retina_region_is_half_as_many_points() {
        let frame = FramePixels {
            rgba: &[],
            buffer_width: 0,
            region_x: 0,
            region_y: 0,
            region_width: 201,
            region_height: 100,
            origin: ScreenPoint::default(),
            pixel_ratio: 2.0,
        };

        assert_eq!(frame.size_units(), (101, 50));
    }
}
