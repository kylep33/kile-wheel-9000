//! Bringing a window forward, or sending it away.

use thiserror::Error;

use crate::apps::WindowId;
use crate::sys;

#[derive(Debug, Error)]
#[error("{0}")]
pub struct ActivateError(pub(crate) String);

pub fn is_foreground(window: WindowId) -> bool {
    sys::activate::is_foreground(window)
}

pub fn minimize(window: WindowId) {
    sys::activate::minimize(window);
}

/// Restores the window first if it is minimized.
pub fn focus(window: WindowId) -> Result<(), ActivateError> {
    sys::activate::focus(window)
}
