//! Single-instance guard. A second launch hands over to the running instance, which
//! opens its wheel, so a desktop shortcut to the app doubles as an open button.

use thiserror::Error;

use crate::event_loop::Waker;
use crate::sys;

#[derive(Debug, Error)]
#[error("{0}")]
pub struct InstanceError(pub(crate) String);

/// Holds the claim for the process lifetime.
#[derive(Debug)]
pub struct InstanceGuard {
    sys: sys::instance::Guard,
}

impl InstanceGuard {
    pub(crate) fn from_sys(sys: sys::instance::Guard) -> Self {
        Self { sys }
    }

    /// From here on a second launch arrives as [`crate::event_loop::Notice::OpenWheel`].
    pub fn listen(&mut self, waker: Waker) -> Result<(), InstanceError> {
        self.sys.listen(waker)
    }
}

/// Outcome of trying to become the one running instance.
#[derive(Debug)]
pub enum Claim {
    Acquired(InstanceGuard),
    AlreadyRunning,
}

pub fn claim(app_name: &str) -> Result<Claim, InstanceError> {
    sys::instance::claim(app_name)
}

/// Asks the running instance to open its wheel. `false` when none could be reached.
pub fn signal_existing(app_name: &str) -> bool {
    sys::instance::signal_existing(app_name)
}
