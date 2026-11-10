//! The clipboard through the shell, which owns it on Wayland.

use super::shell;
use crate::clipboard::ClipboardError;

pub fn read_text() -> Option<String> {
    let text: String = shell::call("GetClipboard", &()).ok()?;
    (!text.is_empty()).then_some(text)
}

pub fn write_text(text: &str) -> Result<(), ClipboardError> {
    shell::call::<_, ()>("SetClipboard", &(text,)).map_err(ClipboardError)
}
