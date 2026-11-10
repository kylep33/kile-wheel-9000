//! What Linux and macOS do the same way: a socket for the single-instance hand-over,
//! `sh` for launching, and rfd for the one yes/no prompt.

pub mod dialog;
pub mod instance;
pub mod process;
