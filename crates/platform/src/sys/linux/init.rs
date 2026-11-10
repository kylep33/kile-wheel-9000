//! Linux needs no per-thread setup.

use std::convert::Infallible;

use thiserror::Error;

#[derive(Debug, Error)]
#[error("unreachable: there is no main-thread setup on Linux")]
pub struct InitError(Infallible);

pub fn main_thread() -> Result<(), InitError> {
    Ok(())
}
