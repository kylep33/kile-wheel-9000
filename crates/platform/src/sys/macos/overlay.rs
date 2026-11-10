//! A borderless, non-activating, click-through panel above every window, showing each
//! frame as its layer's contents.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSPanel, NSStatusWindowLevel, NSWindowCollectionBehavior,
    NSWindowStyleMask,
};
use objc2_core_foundation::{CFData, CFRetained};
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};

use super::monitor::primary_height;
use crate::overlay::{FramePixels, OverlayError};

const BITS_PER_CHANNEL: usize = 8;
const BYTES_PER_PIXEL: usize = 4;

/// Above normal windows and the menu bar, below the screen saver.
const LEVEL_ABOVE_STATUS: isize = 1;

#[derive(Debug)]
pub struct Surface {
    panel: Retained<NSPanel>,
    mtm: MainThreadMarker,
}

impl Surface {
    pub fn create() -> Result<Self, OverlayError> {
        let mtm = MainThreadMarker::new().ok_or_else(|| {
            OverlayError("the overlay has to be created on the main thread".to_owned())
        })?;

        let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
            NSPanel::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1.0, 1.0)),
            NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
            NSBackingStoreType::Buffered,
            false,
        );
        // SAFETY: the panel is owned here, so AppKit must not free it on close.
        unsafe { panel.setReleasedWhenClosed(false) };
        panel.setLevel(NSStatusWindowLevel + LEVEL_ABOVE_STATUS);
        panel.setOpaque(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        panel.setHasShadow(false);
        // every click goes to the tap, and the app underneath keeps focus
        panel.setIgnoresMouseEvents(true);
        panel.setHidesOnDeactivate(false);
        panel.setFloatingPanel(true);
        panel.setBecomesKeyOnlyIfNeeded(true);
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        let view = panel
            .contentView()
            .ok_or_else(|| OverlayError("the overlay panel has no content view".to_owned()))?;
        view.setWantsLayer(true);

        Ok(Self { panel, mtm })
    }

    pub fn present(&mut self, frame: &FramePixels<'_>) -> Result<(), OverlayError> {
        let image = image_of(frame)?;
        let layer = self
            .panel
            .contentView()
            .and_then(|view| view.layer())
            .ok_or_else(|| OverlayError("the overlay panel lost its layer".to_owned()))?;

        // SAFETY: a CGImage is a valid layer contents object, toll-free as an id.
        unsafe {
            let contents = &*(CFRetained::as_ptr(&image).as_ptr() as *const AnyObject);
            layer.setContents(Some(contents));
        }
        layer.setContentsScale(f64::from(frame.pixel_ratio));

        let (width, height) = frame.size_units();
        let bottom = primary_height(self.mtm) - f64::from(frame.origin.y + height);
        self.panel.setFrame_display(
            NSRect::new(
                NSPoint::new(f64::from(frame.origin.x), bottom),
                NSSize::new(f64::from(width), f64::from(height)),
            ),
            true,
        );
        Ok(())
    }

    pub fn show(&mut self) {
        self.panel.orderFrontRegardless();
    }

    pub fn hide(&mut self) {
        self.panel.orderOut(None);
    }
}

/// The region as a premultiplied RGBA image, copied so the buffer can be reused.
fn image_of(frame: &FramePixels<'_>) -> Result<CFRetained<CGImage>, OverlayError> {
    let mut pixels = Vec::with_capacity(
        frame.region_width as usize * frame.region_height as usize * BYTES_PER_PIXEL,
    );
    for row in 0..frame.region_height {
        pixels.extend_from_slice(frame.row(row));
    }

    let failed = || OverlayError("CoreGraphics would not build the frame image".to_owned());
    let data = CFData::from_bytes(&pixels);
    let provider = CGDataProvider::with_cf_data(Some(&data)).ok_or_else(failed)?;
    let space = CGColorSpace::new_device_rgb().ok_or_else(failed)?;
    // the default byte order is RGBA in memory, which is what the buffer holds
    let info = CGBitmapInfo(CGImageAlphaInfo::PremultipliedLast.0);

    // SAFETY: the sizes describe the copied buffer exactly, and no decode array is given.
    unsafe {
        CGImage::new(
            frame.region_width as usize,
            frame.region_height as usize,
            BITS_PER_CHANNEL,
            BITS_PER_CHANNEL * BYTES_PER_PIXEL,
            frame.region_width as usize * BYTES_PER_PIXEL,
            Some(&space),
            info,
            Some(&provider),
            std::ptr::null(),
            false,
            CGColorRenderingIntent::RenderingIntentDefault,
        )
    }
    .ok_or_else(failed)
}
