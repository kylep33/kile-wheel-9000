//! Synthesising keystrokes into whatever window has focus.

use thiserror::Error;

use crate::sys;

/// Says what was refused and why, in the OS's own terms.
#[derive(Debug, Error)]
#[error("{0}")]
pub struct SendError(pub(crate) String);

/// Sends the OS's paste shortcut: Ctrl+V, or Cmd+V on macOS.
pub fn paste() -> Result<(), SendError> {
    sys::send::paste()
}

/// Types `text` as keystrokes. Newlines go as Enter, because a bare line feed is ignored
/// by most controls.
pub fn text(body: &str) -> Result<(), SendError> {
    sys::send::text(body)
}
