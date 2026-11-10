//! macOS sets up AppKit with the event loop, so there is nothing to do first.

use std::convert::Infallible;

use thiserror::Error;

#[derive(Debug, Error)]
#[error("unreachable: there is no main-thread setup on macOS")]
pub struct InitError(Infallible);

pub fn main_thread() -> Result<(), InitError> {
    Ok(())
}
