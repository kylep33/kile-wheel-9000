//! The set of windows the taskbar would show, grouped by app.
//!
//! Built once at startup and kept current by WinEvent updates, so opening the wheel
//! never enumerates.

use std::collections::HashMap;
use std::ffi::c_void;
use std::path::PathBuf;

use windows::Win32::Foundation::{HWND, LPARAM, TRUE};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GWL_EXSTYLE, GW_OWNER, GetForegroundWindow, GetWindow, GetWindowLongPtrW,
    IsWindowVisible, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, Result};

use crate::window_class::{is_shell_class, is_uwp_host};
use crate::window_text::{executable_path, has_title, title};

/// How windows are grouped into one wheel slot, matching the taskbar.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AppKey {
    /// AppUserModelID, which is how packaged apps are grouped.
    UserModelId(String),
    /// Executable path, for everything else.
    Executable(PathBuf),
}

/// One taskbar-visible window.
#[derive(Debug, Clone, Copy)]
pub struct WindowRef<'a> {
    pub handle: HWND,
    pub title: &'a str,
}

/// An app and its open windows, oldest first.
#[derive(Debug, Clone)]
pub struct AppGroup<'a> {
    pub key: &'a AppKey,
    pub windows: Vec<WindowRef<'a>>,
}

#[derive(Debug)]
struct TrackedWindow {
    handle: HWND,
    key: AppKey,
    title: String,
    /// Discovery order, which is what keeps group and window order stable.
    sequence: u64,
}

/// Taskbar-visible windows and which one has focus.
#[derive(Debug, Default)]
pub struct WindowModel {
    tracked: HashMap<isize, TrackedWindow>,
    focused: Option<isize>,
    next_sequence: u64,
}

impl WindowModel {
    /// One full sweep. Called at startup only.
    pub fn rebuild(&mut self) -> Result<()> {
        self.tracked.clear();
        for window in enumerate_windows()? {
            self.add(window);
        }

        // SAFETY: returns a borrowed handle or null, no pointers involved.
        self.set_focused(unsafe { GetForegroundWindow() });
        Ok(())
    }

    /// Adds the window if the taskbar would show it.
    pub fn add(&mut self, window: HWND) {
        if !is_taskbar_window(window) {
            return;
        }
        let Some(key) = app_key(window) else {
            return;
        };

        let sequence = self.next_sequence;
        self.next_sequence += 1;
        self.tracked.insert(
            handle_key(window),
            TrackedWindow {
                handle: window,
                key,
                title: title(window),
                sequence,
            },
        );
    }

    pub fn remove(&mut self, window: HWND) {
        let key = handle_key(window);
        self.tracked.remove(&key);
        if self.focused == Some(key) {
            self.focused = None;
        }
    }

    /// Re-checks the window, since one can become taskbar-worthy after it is created,
    /// which is how packaged apps appear.
    pub fn refresh(&mut self, window: HWND) {
        if !is_taskbar_window(window) {
            self.remove(window);
            return;
        }

        match self.tracked.get_mut(&handle_key(window)) {
            Some(tracked) => tracked.title = title(window),
            None => self.add(window),
        }
    }

    pub fn set_focused(&mut self, window: HWND) {
        self.refresh(window);

        let key = handle_key(window);
        self.focused = self.tracked.contains_key(&key).then_some(key);
    }

    /// Groups ordered by when each app's first window appeared, so a close elsewhere
    /// never reorders them.
    pub fn groups(&self) -> Vec<AppGroup<'_>> {
        let mut by_key: HashMap<&AppKey, Vec<&TrackedWindow>> = HashMap::new();
        for tracked in self.tracked.values() {
            by_key.entry(&tracked.key).or_default().push(tracked);
        }

        let mut ordered: Vec<(u64, AppGroup<'_>)> = by_key
            .into_iter()
            .map(|(key, windows)| build_group(key, windows))
            .collect();

        ordered.sort_by_key(|(first_seen, _)| *first_seen);
        ordered.into_iter().map(|(_, group)| group).collect()
    }

    pub fn focused_key(&self) -> Option<&AppKey> {
        let focused = self.focused?;
        self.tracked.get(&focused).map(|tracked| &tracked.key)
    }

    pub fn focused_window(&self) -> Option<HWND> {
        let focused = self.focused?;
        self.tracked.get(&focused).map(|tracked| tracked.handle)
    }
}

fn build_group<'a>(key: &'a AppKey, mut windows: Vec<&'a TrackedWindow>) -> (u64, AppGroup<'a>) {
    windows.sort_by_key(|tracked| tracked.sequence);

    let first_seen = windows
        .first()
        .expect("a group is only built from at least one window")
        .sequence;

    let group = AppGroup {
        key,
        windows: windows
            .into_iter()
            .map(|tracked| WindowRef {
                handle: tracked.handle,
                title: &tracked.title,
            })
            .collect(),
    };
    (first_seen, group)
}

/// Approximates what the taskbar shows.
///
/// A shell-cloaked window is kept, because that is also how a window on another
/// virtual desktop or a hidden komorebi workspace presents.
pub fn is_taskbar_window(window: HWND) -> bool {
    // SAFETY: handles a null or stale handle by reporting not-visible.
    if !unsafe { IsWindowVisible(window) }.as_bool() {
        return false;
    }
    if is_shell_class(window) || !has_title(window) || !has_app_window_style(window) {
        return false;
    }

    // a cloaked packaged-app host is suspended, not merely off-desktop
    !(is_uwp_host(window) && is_cloaked(window))
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

fn app_key(window: HWND) -> Option<AppKey> {
    if let Some(id) = user_model_id(window) {
        return Some(AppKey::UserModelId(id));
    }
    executable_path(window).map(AppKey::Executable)
}

fn user_model_id(window: HWND) -> Option<String> {
    // SAFETY: the window handle is live and the interface is released on drop.
    let store: IPropertyStore = unsafe { SHGetPropertyStoreForWindow(window) }.ok()?;

    // SAFETY: the property key is a static constant owned by the shell.
    let value = unsafe { store.GetValue(&PKEY_AppUserModel_ID) }.ok()?;

    let id = value.to_string();
    (!id.is_empty()).then_some(id)
}

fn handle_key(window: HWND) -> isize {
    window.0 as isize
}

fn enumerate_windows() -> Result<Vec<HWND>> {
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
