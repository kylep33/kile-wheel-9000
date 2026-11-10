//! Which top-level windows the taskbar would show, and what each belongs to.

use std::ffi::c_void;
use std::path::Path;

use windows::Win32::Foundation::{HWND, LPARAM, TRUE};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GA_ROOT, GW_OWNER, GWL_EXSTYLE, GetAncestor, GetForegroundWindow, GetWindow,
    GetWindowLongPtrW, IsWindowVisible, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};
use windows::core::BOOL;

use super::window_class::{is_shell_class, is_uwp_host};
use super::window_text::{executable_path, has_title};
use crate::apps::{AppKey, AppsError, WindowId, WindowInfo};

/// Packaged apps launch through the Apps folder, because their exe is not what starts them.
const APPS_FOLDER_PREFIX: &str = r"shell:AppsFolder\";

pub fn enumerate() -> Result<(Vec<WindowInfo>, Option<WindowId>), AppsError> {
    let windows = enumerate_windows()
        .map_err(|error| AppsError(format!("EnumWindows failed: {error}")))?
        .into_iter()
        .filter_map(|window| describe(id_of(window)))
        .collect();

    // SAFETY: returns a borrowed handle or null, no pointers involved.
    let focused = unsafe { GetForegroundWindow() };
    Ok((windows, (!focused.is_invalid()).then(|| id_of(focused))))
}

/// `None` when the taskbar would not show the window.
pub fn describe(id: WindowId) -> Option<WindowInfo> {
    let window = handle(id);
    if !is_taskbar_window(window) {
        return None;
    }

    let executable = executable_path(window);
    let key = app_key(window, executable.as_deref())?;
    Some(WindowInfo {
        id,
        key,
        executable,
        name: None,
        title: title(id),
    })
}

pub fn is_listed(id: WindowId) -> bool {
    is_taskbar_window(handle(id))
}

pub fn title(id: WindowId) -> String {
    super::window_text::title(handle(id))
}

pub fn launch_target(key: &AppKey) -> String {
    match key {
        AppKey::Executable(path) => path.to_string_lossy().into_owned(),
        AppKey::Id(id) => format!("{APPS_FOLDER_PREFIX}{id}"),
    }
}

pub(crate) fn handle(id: WindowId) -> HWND {
    HWND(id.0 as usize as *mut c_void)
}

pub(crate) fn id_of(window: HWND) -> WindowId {
    WindowId(window.0 as usize as u64)
}

/// Approximates what the taskbar shows.
///
/// A shell-cloaked window is kept, because that is also how a window on another
/// virtual desktop or a hidden komorebi workspace presents.
fn is_taskbar_window(window: HWND) -> bool {
    // SAFETY: handles a null or stale handle by reporting not-visible.
    if !unsafe { IsWindowVisible(window) }.as_bool() {
        return false;
    }
    if !is_top_level(window) {
        return false;
    }
    if is_shell_class(window) || !has_title(window) || !has_app_window_style(window) {
        return false;
    }

    // a cloaked packaged-app host is suspended, not merely off-desktop
    !(is_uwp_host(window) && is_cloaked(window))
}

/// Only top-level windows are taskbar entries.
///
/// The startup sweep only ever sees top-level windows, but window events also fire for
/// children. Chromium and Electron apps (Chrome, Kiro, VS Code, Obsidian) each host a
/// visible, titled child, "Chrome Legacy Window", which passes every other check because
/// a child has a parent rather than an owner. Without this it joined the app's group as a
/// second window.
///
/// The `WS_CHILD` style is not used, because some genuinely top-level windows carry it.
/// Being its own root is what top-level actually means.
fn is_top_level(window: HWND) -> bool {
    // SAFETY: returns a borrowed handle or null, no pointers involved.
    let root = unsafe { GetAncestor(window, GA_ROOT) };
    root == window
}

fn has_app_window_style(window: HWND) -> bool {
    // SAFETY: reads a style word; a bad handle yields zero.
    let extended = unsafe { GetWindowLongPtrW(window, GWL_EXSTYLE) } as u32;

    if extended & WS_EX_TOOLWINDOW.0 != 0 {
        return false;
    }
    if extended & WS_EX_APPWINDOW.0 != 0 {
        return true;
    }
    owner(window).is_none()
}

fn owner(window: HWND) -> Option<HWND> {
    // SAFETY: returns a borrowed handle or an error when there is no owner.
    let owner = unsafe { GetWindow(window, GW_OWNER) }.ok()?;
    (!owner.is_invalid()).then_some(owner)
}

fn is_cloaked(window: HWND) -> bool {
    let mut cloaked: u32 = 0;

    // SAFETY: the out-pointer and its size describe the same live u32.
    let read = unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_CLOAKED,
            (&raw mut cloaked).cast::<c_void>(),
            size_of::<u32>() as u32,
        )
    };

    read.is_ok() && cloaked != 0
}

fn app_key(window: HWND, executable: Option<&Path>) -> Option<AppKey> {
    if let Some(id) = user_model_id(window) {
        return Some(AppKey::Id(id));
    }
    executable.map(|path| AppKey::Executable(path.to_owned()))
}

fn user_model_id(window: HWND) -> Option<String> {
    // SAFETY: the window handle is live and the interface is released on drop.
    let store: IPropertyStore = unsafe { SHGetPropertyStoreForWindow(window) }.ok()?;

    // SAFETY: the property key is a static constant owned by the shell.
    let value = unsafe { store.GetValue(&PKEY_AppUserModel_ID) }.ok()?;

    let id = value.to_string();
    (!id.is_empty()).then_some(id)
}

fn enumerate_windows() -> windows::core::Result<Vec<HWND>> {
    let mut found: Vec<HWND> = Vec::new();

    // SAFETY: `found` outlives the enumeration, which is synchronous.
    unsafe { EnumWindows(Some(collect_window), LPARAM((&raw mut found) as isize)) }?;
    Ok(found)
}

unsafe extern "system" fn collect_window(window: HWND, param: LPARAM) -> BOOL {
    // SAFETY: param is the &mut Vec handed over by enumerate_windows.
    let found = unsafe { &mut *(param.0 as *mut Vec<HWND>) };
    found.push(window);
    TRUE
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GW_CHILD};
    use windows::core::w;

    use super::*;

    /// Every window the startup sweep returns is top-level, so the new check must not
    /// throw any of them away.
    #[test]
    fn every_enumerated_window_counts_as_top_level() {
        let windows = enumerate_windows().expect("windows can be enumerated");

        assert!(!windows.is_empty());
        assert!(windows.iter().all(|window| is_top_level(*window)));
    }

    /// The regression: a child window, like Chromium's "Chrome Legacy Window", must never
    /// become a taskbar entry, whatever its title and visibility say. The taskbar's own
    /// children stand in for it, since they are present on any desktop.
    #[test]
    fn a_child_window_is_never_a_taskbar_window() {
        // SAFETY: looks a window up by class; null when there is no taskbar.
        let Ok(taskbar) = (unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }) else {
            return;
        };
        // SAFETY: returns a borrowed handle or an error when there are no children.
        let Ok(child) = (unsafe { GetWindow(taskbar, GW_CHILD) }) else {
            return;
        };

        assert!(!is_top_level(child));
        assert!(!is_taskbar_window(child));
    }

    #[test]
    fn a_packaged_app_launches_through_the_apps_folder() {
        let key = AppKey::Id("Some.App_abc!App".to_owned());

        assert_eq!(launch_target(&key), r"shell:AppsFolder\Some.App_abc!App");
    }

    #[test]
    fn a_window_id_round_trips_through_its_handle() {
        let id = WindowId(0x1234);

        assert_eq!(id_of(handle(id)), id);
    }
}
