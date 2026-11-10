//! Turning an icon's pixels into something the renderer can draw.

use kile_wheel_platform::icons::IconImage;
use resvg::tiny_skia::{IntSize, Pixmap};

/// `None` when the pixel buffer does not match the stated size, which would be a bug in
/// the platform layer rather than a bad icon file.
pub fn to_pixmap(image: &IconImage) -> Option<Pixmap> {
    let size = IntSize::from_wh(image.width, image.height)?;
    Pixmap::from_vec(image.rgba.clone(), size)
}
