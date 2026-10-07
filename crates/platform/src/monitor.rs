//! Monitor geometry, DPI, and the fullscreen check that suppresses hotkeys.

use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    HMONITOR, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint,
    MonitorFromWindow,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetForegroundWindow, GetWindowRect};

use crate::window_class::is_shell_class;

/// Which monitor the wheel opens on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorChoice {
    UnderCursor,
    Primary,
}

/// A monitor's pixel bounds and its effective DPI.
#[derive(Debug, Clone, Copy)]
pub struct Monitor {
    pub bounds: RECT,
    pub dpi: u32,
}

impl Monitor {
    pub fn width(self) -> i32 {
        self.bounds.right - self.bounds.left
    }

    pub fn height(self) -> i32 {
        self.bounds.bottom - self.bounds.top
    }

    pub fn center(self) -> POINT {
        POINT {
            x: self.bounds.left + self.width() / 2,
            y: self.bounds.top + self.height() / 2,
        }
    }
}

/// The monitor the wheel should open on, falling back to the primary one when the
/// cursor cannot be read.
pub fn monitor_for(choice: MonitorChoice) -> Monitor {
    let origin = POINT { x: 0, y: 0 };
    match choice {
        MonitorChoice::Primary => describe(from_point(origin, MONITOR_DEFAULTTOPRIMARY)),
        MonitorChoice::UnderCursor => match cursor_position() {
            Some(cursor) => describe(from_point(cursor, MONITOR_DEFAULTTONEAREST)),
            None => describe(from_point(origin, MONITOR_DEFAULTTOPRIMARY)),
        },
    }
}

pub fn cursor_position() -> Option<POINT> {
    let mut position = POINT::default();

    // SAFETY: the pointer targets a live local for the duration of the call.
    unsafe { GetCursorPos(&mut position) }.ok()?;
    Some(position)
}

/// Reports whether the foreground window covers its whole monitor, which is how
/// borderless-fullscreen games present. Desktop and shell windows do not count.
pub fn foreground_is_fullscreen() -> bool {
    // SAFETY: returns a borrowed window handle or null, no pointers involved.
    let window = unsafe { GetForegroundWindow() };
    if window.is_invalid() || is_shell_class(window) {
        return false;
    }

    let Some(bounds) = window_bounds(window) else {
        return false;
    };

    // SAFETY: the handle is a live window, so the monitor lookup cannot fail.
    let monitor = describe(unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST) });
    covers(bounds, monitor.bounds)
}

fn window_bounds(window: HWND) -> Option<RECT> {
    let mut bounds = RECT::default();

    // SAFETY: the pointer targets a live local for the duration of the call.
    unsafe { GetWindowRect(window, &mut bounds) }.ok()?;
    Some(bounds)
}

fn covers(inner: RECT, outer: RECT) -> bool {
    inner.left <= outer.left
        && inner.top <= outer.top
        && inner.right >= outer.right
        && inner.bottom >= outer.bottom
}

fn from_point(point: POINT, flags: windows::Win32::Graphics::Gdi::MONITOR_FROM_FLAGS) -> HMONITOR {
    // SAFETY: the flags guarantee a monitor is returned even for an off-screen point.
    unsafe { MonitorFromPoint(point, flags) }
}

fn describe(monitor: HMONITOR) -> Monitor {
    Monitor {
        bounds: monitor_bounds(monitor),
        dpi: monitor_dpi(monitor),
    }
}

fn monitor_bounds(monitor: HMONITOR) -> RECT {
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..MONITORINFO::default()
    };

    // SAFETY: cbSize is set and the pointer targets a live local.
    let ok = unsafe { windows::Win32::Graphics::Gdi::GetMonitorInfoW(monitor, &mut info) };
    if ok.as_bool() {
        return info.rcMonitor;
    }
    RECT::default()
}

fn monitor_dpi(monitor: HMONITOR) -> u32 {
    let mut horizontal = 0;
    let mut vertical = 0;

    // SAFETY: both pointers target live locals for the duration of the call.
    let result =
        unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut horizontal, &mut vertical) };
    if result.is_ok() {
        return horizontal;
    }
    USER_DEFAULT_DPI
}

/// Win32 `USER_DEFAULT_SCREEN_DPI`, the 100% scaling baseline.
pub const USER_DEFAULT_DPI: u32 = 96;
