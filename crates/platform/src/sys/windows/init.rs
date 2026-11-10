//! COM, which the shell calls behind app ids, icons and launches need.

use thiserror::Error;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};

#[derive(Debug, Error)]
#[error("could not initialise COM on the main thread: {0}")]
pub struct InitError(#[source] windows::core::Error);

/// Single-threaded, because windows and drag and drop live on this thread too; a later
/// OLE init by a windowing layer then finds the apartment already set up.
pub fn main_thread() -> Result<(), InitError> {
    // SAFETY: called once, on the thread that then owns every window.
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
        .ok()
        .map_err(InitError)
}
