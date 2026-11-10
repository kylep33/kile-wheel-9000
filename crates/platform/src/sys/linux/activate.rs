//! Focus and minimize through the shell, which switches workspace if it has to.

use tracing::debug;

use super::shell;
use crate::activate::ActivateError;
use crate::apps::WindowId;

pub fn is_foreground(id: WindowId) -> bool {
    match shell::call::<_, u64>("FocusedWindow", &()) {
        Ok(focused) => focused == id.0,
        Err(error) => {
            debug!("could not ask which window has focus: {error}");
            false
        }
    }
}

pub fn minimize(id: WindowId) {
    if let Err(error) = shell::call::<_, ()>("Minimize", &(id.0,)) {
        debug!("could not minimize window {}: {error}", id.0);
    }
}

pub fn focus(id: WindowId) -> Result<(), ActivateError> {
    shell::call::<_, ()>("Activate", &(id.0,)).map_err(ActivateError)
}
