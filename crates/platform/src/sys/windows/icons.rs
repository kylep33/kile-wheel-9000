//! Icons the shell renders, which covers exes, shortcuts, folders, documents and
//! `shell:AppsFolder\<AppUserModelID>` alike.

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

use crate::icons::{IconImage, from_straight_rgba};

const BYTES_PER_PIXEL: usize = 4;
const BITS_PER_PIXEL: u16 = 32;
const SINGLE_PLANE: u16 = 1;
const FIRST_SCANLINE: u32 = 0;
/// GDI hands back BGRA, which is RGBA with the first and third channels swapped.
const BLUE: usize = 0;
const RED: usize = 2;

pub fn extract(source: &str, size_px: u32) -> Option<IconImage> {
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
            // negative height asks for top-down rows
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

    bgra_to_rgba(&mut pixels);
    Some(from_straight_rgba(width, height, pixels))
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

fn bgra_to_rgba(pixels: &mut [u8]) {
    let (pixels, _) = pixels.as_chunks_mut::<BYTES_PER_PIXEL>();
    for pixel in pixels {
        pixel.swap(BLUE, RED);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blue_and_red_trade_places() {
        let mut pixels = vec![0x01, 0x02, 0x03, 0x04];

        bgra_to_rgba(&mut pixels);

        assert_eq!(pixels, [0x03, 0x02, 0x01, 0x04]);
    }

    #[test]
    fn the_shell_renders_an_icon_for_explorer() {
        super::super::init::main_thread().expect("the test thread can join an apartment");
        let explorer = crate::paths::expand(r"%SystemRoot%\explorer.exe");

        let icon = extract(&explorer, 32).expect("explorer has an icon");
        assert_eq!(icon.rgba.len(), (icon.width * icon.height) as usize * BYTES_PER_PIXEL);
    }
}
