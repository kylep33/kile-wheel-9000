//! Regular apps and their standard windows, grouped by bundle id like the Dock does.

use std::path::PathBuf;

use objc2_app_kit::{NSApplicationActivationPolicy, NSRunningApplication, NSWorkspace};
use objc2_application_services::AXUIElement;

use super::{ACCESSIBILITY_HINT, ax};
use crate::apps::{AppKey, AppsError, WindowId, WindowInfo};

/// A window id carries its app's pid in the top half, so focusing it needs no lookup.
const PID_SHIFT: u32 = 32;
const WINDOW_MASK: u64 = u32::MAX as u64;

pub fn enumerate() -> Result<(Vec<WindowInfo>, Option<WindowId>), AppsError> {
    if !ax::trusted() {
        return Err(AppsError(format!("the window list is unavailable; {ACCESSIBILITY_HINT}")));
    }

    let workspace = NSWorkspace::sharedWorkspace();
    let mut windows = Vec::new();
    for app in workspace.runningApplications().iter() {
        if app.activationPolicy() != NSApplicationActivationPolicy::Regular {
            continue;
        }
        windows.extend(app_windows(&app));
    }

    Ok((windows, focused_window()))
}

pub fn describe(id: WindowId) -> Option<WindowInfo> {
    let (windows, _) = enumerate().ok()?;
    windows.into_iter().find(|window| window.id == id)
}

pub fn is_listed(id: WindowId) -> bool {
    describe(id).is_some()
}

pub fn title(id: WindowId) -> String {
    describe(id).map(|window| window.title).unwrap_or_default()
}

pub fn launch_target(key: &AppKey) -> String {
    match key {
        AppKey::Id(bundle_id) => bundle_id.clone(),
        AppKey::Executable(path) => path.to_string_lossy().into_owned(),
    }
}

pub fn pid_of(id: WindowId) -> i32 {
    (id.0 >> PID_SHIFT) as i32
}

/// The Accessibility element for one window, found again by its window-server id.
pub fn find_window(id: WindowId) -> Option<objc2_core_foundation::CFRetained<AXUIElement>> {
    let wanted = (id.0 & WINDOW_MASK) as u32;
    ax::windows(&ax::application(pid_of(id)))
        .into_iter()
        .find(|window| ax::window_id(window) == Some(wanted))
}

pub fn focused_window() -> Option<WindowId> {
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let pid = app.processIdentifier();
    let window = ax::element(&ax::application(pid), ax::FOCUSED_WINDOW)?;
    Some(window_id_of(pid, ax::window_id(&window)?))
}

fn app_windows(app: &NSRunningApplication) -> Vec<WindowInfo> {
    let pid = app.processIdentifier();
    let bundle_path = app
        .bundleURL()
        .and_then(|url| url.path())
        .map(|path| PathBuf::from(path.to_string()));
    let key = match (app.bundleIdentifier(), &bundle_path) {
        (Some(bundle_id), _) => AppKey::Id(bundle_id.to_string()),
        (None, Some(path)) => AppKey::Executable(path.clone()),
        (None, None) => return Vec::new(),
    };
    let name = app.localizedName().map(|name| name.to_string());

    ax::windows(&ax::application(pid))
        .iter()
        .filter(|window| ax::string(window, ax::SUBROLE).as_deref() == Some(ax::STANDARD_WINDOW))
        .filter_map(|window| {
            Some(WindowInfo {
                id: window_id_of(pid, ax::window_id(window)?),
                key: key.clone(),
                executable: bundle_path.clone(),
                name: name.clone(),
                title: ax::string(window, ax::TITLE).unwrap_or_default(),
            })
        })
        .collect()
}

fn window_id_of(pid: i32, window: u32) -> WindowId {
    WindowId(((pid as u32 as u64) << PID_SHIFT) | u64::from(window))
}
