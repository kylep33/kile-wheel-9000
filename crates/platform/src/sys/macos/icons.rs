//! The icon Finder shows, for an app, a folder or any file.

use objc2::AllocAnyThread;
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSWorkspace};
use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};

use crate::icons::{IconImage, from_straight_rgba};

pub fn extract(source: &str, size_px: u32) -> Option<IconImage> {
    let workspace = NSWorkspace::sharedWorkspace();
    let path = match source.starts_with('/') {
        true => source.to_owned(),
        // a bundle id, as a running app is known by
        false => workspace
            .URLForApplicationWithBundleIdentifier(&NSString::from_str(source))?
            .path()?
            .to_string(),
    };

    let image = workspace.iconForFile(&NSString::from_str(&path));
    let side = f64::from(size_px.max(1));
    image.setSize(NSSize::new(side, side));

    let mut rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(side, side));
    // SAFETY: the rect is a live local; no context or hints are needed.
    let bitmap = unsafe { image.CGImageForProposedRect_context_hints(&mut rect, None, None) }?;

    // PNG is the one lossless form AppKit hands back without a pixel-format dance
    let rep = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &bitmap);
    // SAFETY: an empty property dictionary asks for the defaults.
    let png = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) }?;
    let decoded = image::load_from_memory_with_format(&png.to_vec(), image::ImageFormat::Png)
        .ok()?
        .to_rgba8();

    Some(from_straight_rgba(decoded.width(), decoded.height(), decoded.into_raw()))
}
