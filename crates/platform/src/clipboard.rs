//! Reading text off the clipboard, so paste works in the wheel's own fields, and writing
//! it, which is how a snippet reaches the window that had focus.

use thiserror::Error;

use crate::sys;

#[derive(Debug, Error)]
#[error("{0}")]
pub struct ClipboardError(pub(crate) String);

/// `None` when the clipboard holds no text, or another process has it locked.
pub fn read_text() -> Option<String> {
    sys::clipboard::read_text()
}

pub fn write_text(text: &str) -> Result<(), ClipboardError> {
    sys::clipboard::write_text(text)
}
