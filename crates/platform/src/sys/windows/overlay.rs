//! A layered popup fed by `UpdateLayeredWindow`.
//!
//! Layered windows take per-pixel premultiplied alpha straight from a DIB, so the CPU
//! buffer goes on screen with one copy and no GPU plumbing. Transparent to clicks and
//! never activated: the hooks deliver all input, and focus stays with the app underneath.

use std::ffi::c_void;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, HBITMAP, HDC,
    HGDIOBJ, SelectObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, HWND_TOPMOST, RegisterClassExW, SW_HIDE,
    SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos, ShowWindow,
    ULW_ALPHA, UpdateLayeredWindow, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{HSTRING, PCWSTR};

use crate::overlay::{FramePixels, OverlayError};

const WINDOW_CLASS: &str = "kile-wheel-9000.overlay";

const BYTES_PER_PIXEL: usize = 4;
const BITS_PER_PIXEL: u16 = 32;
const SINGLE_PLANE: u16 = 1;
const OPAQUE: u8 = 255;
/// The DIB is BGRA, the buffer RGBA.
const RED: usize = 0;
const BLUE: usize = 2;

#[derive(Debug)]
pub struct Surface {
    window: HWND,
    dib: Dib,
}

impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: the window is ours and is not used again.
        let _ = unsafe { DestroyWindow(self.window) };
    }
}

impl Surface {
    pub fn create() -> Result<Self, OverlayError> {
        let window = create_window().map_err(|error| {
            OverlayError(format!(
                "could not create the overlay window; the window class was refused: {error}"
            ))
        })?;
        let dib = Dib::new(1, 1)?;
        Ok(Self { window, dib })
    }

    pub fn present(&mut self, frame: &FramePixels<'_>) -> Result<(), OverlayError> {
        if frame.region_width > self.dib.width || frame.region_height > self.dib.height {
            let width = frame.region_width.max(self.dib.width);
            let height = frame.region_height.max(self.dib.height);
            self.dib = Dib::new(width, height)?;
        }
        self.dib.fill(frame);

        let origin = POINT {
            x: frame.origin.x,
            y: frame.origin.y,
        };
        let size = SIZE {
            cx: frame.region_width as i32,
            cy: frame.region_height as i32,
        };
        let source = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: OPAQUE,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };

        // SAFETY: every pointer targets a live local, and the DIB stays selected into its
        // DC for as long as the surface lives.
        unsafe {
            UpdateLayeredWindow(
                self.window,
                None,
                Some(&origin),
                Some(&size),
                Some(self.dib.dc),
                Some(&source),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
        }
        .map_err(|error| OverlayError(format!("UpdateLayeredWindow refused the frame: {error}")))
    }

    pub fn show(&mut self) {
        // SAFETY: SW_SHOWNOACTIVATE leaves the app underneath focused, and the second call
        // only restacks; neither moves focus.
        unsafe {
            let _ = ShowWindow(self.window, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(
                self.window,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            );
        }
    }

    pub fn hide(&mut self) {
        // SAFETY: hides our own window.
        let _ = unsafe { ShowWindow(self.window, SW_HIDE) };
    }
}

/// A top-down BGRA DIB selected into a memory DC, regrown only when a frame outgrows it.
#[derive(Debug)]
struct Dib {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u8,
    width: u32,
    height: u32,
}

impl Drop for Dib {
    fn drop(&mut self) {
        // SAFETY: the original object goes back before the DIB and DC are freed.
        unsafe {
            SelectObject(self.dc, self.previous);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

impl Dib {
    fn new(width: u32, height: u32) -> Result<Self, OverlayError> {
        let header = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                // negative height gives top-down rows, matching the buffer
                biHeight: -(height as i32),
                biPlanes: SINGLE_PLANE,
                biBitCount: BITS_PER_PIXEL,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        // SAFETY: a memory DC compatible with the screen; freed by Drop.
        let dc = unsafe { CreateCompatibleDC(None) };
        if dc.is_invalid() {
            return Err(OverlayError(
                "could not create a memory DC for the overlay".to_owned(),
            ));
        }

        let mut bits: *mut c_void = std::ptr::null_mut();
        // SAFETY: the header and out-pointer are live for the call.
        let bitmap =
            unsafe { CreateDIBSection(Some(dc), &header, DIB_RGB_COLORS, &raw mut bits, None, 0) };
        let bitmap = match bitmap {
            Ok(bitmap) if !bits.is_null() => bitmap,
            _ => {
                // SAFETY: the DC was created above and nothing else holds it.
                let _ = unsafe { DeleteDC(dc) };
                return Err(OverlayError(format!(
                    "could not allocate a {width}x{height} overlay bitmap; the system is out of GDI memory"
                )));
            }
        };

        // SAFETY: both handles are live; the previous object is restored on drop.
        let previous = unsafe { SelectObject(dc, bitmap.into()) };

        Ok(Self {
            dc,
            bitmap,
            previous,
            bits: bits.cast(),
            width,
            height,
        })
    }

    fn fill(&mut self, frame: &FramePixels<'_>) {
        let stride = self.width as usize * BYTES_PER_PIXEL;
        let length = stride * self.height as usize;

        // SAFETY: the DIB section is `length` bytes and lives as long as `self`.
        let target = unsafe { std::slice::from_raw_parts_mut(self.bits, length) };

        for row in 0..frame.region_height {
            let source = frame.row(row);
            let start = row as usize * stride;
            let line = &mut target[start..start + source.len()];
            line.copy_from_slice(source);

            let (pixels, _) = line.as_chunks_mut::<BYTES_PER_PIXEL>();
            for pixel in pixels {
                pixel.swap(RED, BLUE);
            }
        }
    }
}

fn create_window() -> windows::core::Result<HWND> {
    let class = HSTRING::from(WINDOW_CLASS);

    // SAFETY: passing None asks for this executable's own module handle.
    let instance = unsafe { GetModuleHandleW(None) }?;

    let descriptor = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(window_proc),
        hInstance: instance.into(),
        lpszClassName: PCWSTR(class.as_ptr()),
        ..Default::default()
    };

    // SAFETY: the descriptor and its strings stay live for the duration of the call.
    if unsafe { RegisterClassExW(&descriptor) } == 0 {
        return Err(windows::core::Error::from_thread());
    }

    // SAFETY: the class is registered and the strings stay live for the call.
    unsafe {
        CreateWindowExW(
            // layered for per-pixel alpha, transparent so clicks fall through to the hook,
            // noactivate to keep focus underneath, toolwindow to stay out of alt-tab
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            PCWSTR(class.as_ptr()),
            PCWSTR(class.as_ptr()),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(instance.into()),
            None,
        )
    }
}

/// All input comes through the hooks, so the window only has to exist.
unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: forwards the values Windows gave us to the default handler.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}
