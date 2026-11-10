//! Raising, focusing and minimizing another app's window through Accessibility.

use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};

use super::{ACCESSIBILITY_HINT, apps, ax};
use crate::activate::ActivateError;
use crate::apps::WindowId;

pub fn is_foreground(id: WindowId) -> bool {
    apps::focused_window() == Some(id)
}

pub fn minimize(id: WindowId) {
    if let Some(window) = apps::find_window(id) {
        ax::set_boolean(&window, ax::MINIMIZED, true);
    }
}

pub fn focus(id: WindowId) -> Result<(), ActivateError> {
    let window = apps::find_window(id).ok_or_else(|| {
        ActivateError(format!(
            "window {:#x} could not be found to bring forward; it may have closed, or {ACCESSIBILITY_HINT}",
            id.0
        ))
    })?;

    if ax::boolean(&window, ax::MINIMIZED) == Some(true) {
        ax::set_boolean(&window, ax::MINIMIZED, false);
    }
    ax::perform(&window, ax::RAISE);
    ax::set_boolean(&window, ax::MAIN, true);

    let activated = NSRunningApplication::runningApplicationWithProcessIdentifier(apps::pid_of(id))
        .is_some_and(|app| app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows));
    match activated {
        true => Ok(()),
        false => Err(ActivateError(format!(
            "macOS would not bring the app owning window {:#x} forward",
            id.0
        ))),
    }
}
