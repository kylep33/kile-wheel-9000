//! Getting an icon's pixels onto the GPU.

use std::ffi::c_void;

use kile_wheel_platform::icons::IconImage;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_BITMAP_PROPERTIES1, ID2D1Bitmap1, ID2D1DeviceContext,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::core::Result;

/// Four bytes per pixel, which the platform layer already guarantees.
const BYTES_PER_PIXEL: u32 = 4;

/// Copies the pixels into a GPU bitmap. Done once per icon, not once per frame.
pub fn upload(context: &ID2D1DeviceContext, image: &IconImage) -> Result<ID2D1Bitmap1> {
    let size = D2D_SIZE_U {
        width: image.width,
        height: image.height,
    };
    let properties = D2D1_BITMAP_PROPERTIES1 {
        pixelFormat: D2D1_PIXEL_FORMAT {
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
        },
        ..Default::default()
    };

    // SAFETY: the slice is tightly packed BGRA of exactly this size, and Direct2D copies
    // it before the call returns.
    unsafe {
        context.CreateBitmap(
            size,
            Some(image.bgra.as_ptr() as *const c_void),
            image.width * BYTES_PER_PIXEL,
            &properties,
        )
    }
}
