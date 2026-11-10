//! Modal prompts, for the few choices that must not happen by accident.

use crate::sys;

/// Yes or no, taking no for an answer if the dialog could not be shown.
pub fn confirm(title: &str, message: &str) -> bool {
    sys::dialog::confirm(title, message)
}
