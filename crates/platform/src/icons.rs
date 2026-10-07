//! Icon extraction and its in-memory cache.

use std::collections::HashMap;
use std::ffi::c_void;

use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits,
    GetObjectW, HBITMAP, ReleaseDC,
};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
};
use windows::core::HSTRING;

/// BGRA, which is the only layout Direct2D takes without a conversion step.
const BYTES_PER_PIXEL: usize = 4;
const ALPHA_OFFSET: usize = 3;
const MAX_CHANNEL: u32 = 255;
const BITS_PER_PIXEL: u16 = 32;
const SINGLE_PLANE: u16 = 1;
const FIRST_SCANLINE: u32 = 0;

/// An icon's pixels, premultiplied and ready to upload.
#[derive(Debug, Clone)]
pub struct IconImage {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

/// Icons kept for the process lifetime, because there are only ever as many as there are
/// pinned and running apps.
#[derive(Debug, Default)]
pub struct IconCache {
    /// A miss is remembered too, so a source without an icon is only asked once.
    images: HashMap<String, Option<IconImage>>,
}

impl IconCache {
    /// `None` when the source has no icon the shell can render.
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

/// Asks the shell to render the icon, which handles exes, shortcuts, folders, documents
/// and `shell:AppsFolder\<AppUserModelID>` alike.
fn extract(source: &str, size_px: u32) -> Option<IconImage> {
    let path = HSTRING::from(source);
    let wanted = SIZE {
        cx: size_px as i32,
        cy: size_px as i32,
    };

    // SAFETY: the path stays live for the call; the factory is released on drop.
    let factory: IShellItemImageFactory =
        unsafe { SHCreateItemFromParsingName(&path, None) }.ok()?;

    // SAFETY: BIGGERSIZEOK lets the shell hand back a crisper icon than we asked for.
    let bitmap = unsafe { factory.GetImage(wanted, SIIGBF_BIGGERSIZEOK | SIIGBF_ICONONLY) }.ok()?;

    let image = read_pixels(bitmap);

    // SAFETY: the bitmap came from GetImage and is ours to free.
    let _ = unsafe { DeleteObject(bitmap.into()) };
    image
}

fn read_pixels(bitmap: HBITMAP) -> Option<IconImage> {
    let (width, height) = dimensions(bitmap)?;

    let mut header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            // negative height asks for top-down rows, matching how Direct2D reads them
            biHeight: -(height as i32),
            biPlanes: SINGLE_PLANE,
            biBitCount: BITS_PER_PIXEL,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut pixels = vec![0u8; width as usize * height as usize * BYTES_PER_PIXEL];

    // SAFETY: a null window handle gives the screen DC, which is released below.
    let screen = unsafe { GetDC(None) };

    // SAFETY: the buffer is sized from the header, and both outlive the call.
    let copied = unsafe {
        GetDIBits(
            screen,
            bitmap,
            FIRST_SCANLINE,
            height,
            Some(pixels.as_mut_ptr().cast::<c_void>()),
            &raw mut header,
            DIB_RGB_COLORS,
        )
    };

    // SAFETY: releases the DC taken above.
    unsafe { ReleaseDC(None, screen) };

    if copied == 0 {
        return None;
    }

    premultiply(&mut pixels);
    Some(IconImage {
        width,
        height,
        bgra: pixels,
    })
}

fn dimensions(bitmap: HBITMAP) -> Option<(u32, u32)> {
    let mut info = BITMAP::default();

    // SAFETY: the size and pointer describe the same live BITMAP.
    let written = unsafe {
        GetObjectW(
            bitmap.into(),
            size_of::<BITMAP>() as i32,
            Some((&raw mut info).cast::<c_void>()),
        )
    };

    if written == 0 || info.bmWidth <= 0 || info.bmHeight <= 0 {
        return None;
    }
    Some((info.bmWidth as u32, info.bmHeight as u32))
}

/// The shell hands back straight alpha; Direct2D wants it premultiplied.
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
}
