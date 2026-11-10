//! The general pasteboard, as plain text.

use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
use objc2_foundation::NSString;

use crate::clipboard::ClipboardError;

pub fn read_text() -> Option<String> {
    let board = NSPasteboard::generalPasteboard();
    // SAFETY: the type constant is a static AppKit owns for the process lifetime.
    let text = board.stringForType(unsafe { NSPasteboardTypeString })?;
    Some(text.to_string()).filter(|text| !text.is_empty())
}

pub fn write_text(text: &str) -> Result<(), ClipboardError> {
    let board = NSPasteboard::generalPasteboard();
    board.clearContents();
    // SAFETY: as above.
    let written = board.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString });
    match written {
        true => Ok(()),
        false => Err(ClipboardError("the pasteboard would not take the text".to_owned())),
    }
}
