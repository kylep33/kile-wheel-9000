//! Small safe wrappers over the Accessibility API, which is how one app sees and moves
//! another app's windows on macOS.

use std::ptr::NonNull;

use objc2_application_services::{AXError, AXUIElement};
use objc2_core_foundation::{CFArray, CFBoolean, CFRetained, CFString, CFType};

/// An app that stops answering must not stall the wheel for the default six seconds.
const MESSAGING_TIMEOUT_S: f32 = 0.1;

pub const WINDOWS: &str = "AXWindows";
pub const TITLE: &str = "AXTitle";
pub const SUBROLE: &str = "AXSubrole";
pub const MINIMIZED: &str = "AXMinimized";
pub const MAIN: &str = "AXMain";
pub const FOCUSED_WINDOW: &str = "AXFocusedWindow";
pub const FULL_SCREEN: &str = "AXFullScreen";
pub const RAISE: &str = "AXRaise";
pub const STANDARD_WINDOW: &str = "AXStandardWindow";

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    /// Private, but the only way from an Accessibility window to its window-server id,
    /// and what every window manager on macOS relies on.
    fn _AXUIElementGetWindow(element: &AXUIElement, window: *mut u32) -> AXError;
}

pub fn trusted() -> bool {
    // SAFETY: a plain query with no arguments.
    unsafe { objc2_application_services::AXIsProcessTrusted() }
}

pub fn application(pid: i32) -> CFRetained<AXUIElement> {
    // SAFETY: any pid is accepted; a dead one just answers nothing.
    let app = unsafe { AXUIElement::new_application(pid) };
    // SAFETY: the element is live, and a failed timeout only keeps the default.
    let _ = unsafe { app.set_messaging_timeout(MESSAGING_TIMEOUT_S) };
    app
}

pub fn attribute(element: &AXUIElement, name: &'static str) -> Option<CFRetained<CFType>> {
    let name = CFString::from_static_str(name);
    let mut value: *const CFType = std::ptr::null();

    // SAFETY: the out-pointer targets a live local, and a returned value is +1 retained.
    let error = unsafe { element.copy_attribute_value(&name, NonNull::from(&mut value)) };
    if error != AXError::Success {
        return None;
    }
    // SAFETY: Copy-rule ownership is handed over on success.
    NonNull::new(value.cast_mut()).map(|value| unsafe { CFRetained::from_raw(value) })
}

pub fn string(element: &AXUIElement, name: &'static str) -> Option<String> {
    let value = attribute(element, name)?.downcast::<CFString>().ok()?;
    Some(value.to_string())
}

pub fn boolean(element: &AXUIElement, name: &'static str) -> Option<bool> {
    let value = attribute(element, name)?.downcast::<CFBoolean>().ok()?;
    Some(value.as_bool())
}

pub fn element(element: &AXUIElement, name: &'static str) -> Option<CFRetained<AXUIElement>> {
    attribute(element, name)?.downcast::<AXUIElement>().ok()
}

pub fn set_boolean(element: &AXUIElement, name: &'static str, value: bool) -> bool {
    let name = CFString::from_static_str(name);
    let value: &CFType = CFBoolean::new(value);

    // SAFETY: both values are live CF objects for the duration of the call.
    unsafe { element.set_attribute_value(&name, value) == AXError::Success }
}

pub fn perform(element: &AXUIElement, action: &'static str) -> bool {
    let action = CFString::from_static_str(action);
    // SAFETY: the action name is a live CFString.
    unsafe { element.perform_action(&action) == AXError::Success }
}

pub fn windows(app: &AXUIElement) -> Vec<CFRetained<AXUIElement>> {
    let Some(list) = attribute(app, WINDOWS).and_then(|value| value.downcast::<CFArray>().ok())
    else {
        return Vec::new();
    };
    // SAFETY: AXWindows is documented to hold only AXUIElements.
    let list = unsafe { CFRetained::cast_unchecked::<CFArray<AXUIElement>>(list) };
    list.iter().collect()
}

pub fn window_id(window: &AXUIElement) -> Option<u32> {
    let mut id = 0;
    // SAFETY: the out-pointer targets a live local.
    let error = unsafe { _AXUIElementGetWindow(window, &raw mut id) };
    (error == AXError::Success && id != 0).then_some(id)
}
