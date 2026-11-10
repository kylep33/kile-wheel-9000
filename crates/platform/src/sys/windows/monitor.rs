//! Monitor geometry, DPI, the cursor, and the fullscreen check that suppresses hotkeys.

use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY,
    MONITOR_FROM_FLAGS, MONITORINFO, MonitorFromPoint, MonitorFromWindow,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetForegroundWindow, GetWindowRect, SetCursorPos,
};

use super::window_class::is_shell_class;
use crate::geom::{ScreenPoint, ScreenRect};
use crate::monitor::{Monitor, MonitorChoice};

/// Win32 `USER_DEFAULT_SCREEN_DPI`, the 100% scaling baseline.
const USER_DEFAULT_DPI: u32 = 96;

/// The process is per-monitor DPI aware, so window units are physical pixels.
const PHYSICAL_PIXELS: f32 = 1.0;

pub fn monitor_for(choice: MonitorChoice) -> Monitor {
    let origin = POINT { x: 0, y: 0 };
    match choice {
        MonitorChoice::Primary => describe(from_point(origin, MONITOR_DEFAULTTOPRIMARY)),
        MonitorChoice::UnderCursor => match cursor_position() {
            Some(cursor) => describe(from_point(
                POINT {
                    x: cursor.x,
                    y: cursor.y,
                },
                MONITOR_DEFAULTTONEAREST,
            )),
            None => describe(from_point(origin, MONITOR_DEFAULTTOPRIMARY)),
        },
    }
}

pub fn cursor_position() -> Option<ScreenPoint> {
    let mut position = POINT::default();

    // SAFETY: the pointer targets a live local for the duration of the call.
    unsafe { GetCursorPos(&mut position) }.ok()?;
    Some(ScreenPoint {
        x: position.x,
        y: position.y,
    })
}

pub fn move_cursor(to: ScreenPoint) {
    // SAFETY: moves the cursor only. The move it generates is flagged injected, so our
    // own mouse hook ignores it and the selection does not jump.
    let _ = unsafe { SetCursorPos(to.x, to.y) };
}

/// Reports whether the foreground window covers its whole monitor, which is how
/// borderless-fullscreen games present. Desktop and shell windows do not count.
pub fn foreground_is_fullscreen() -> bool {
    // SAFETY: returns a borrowed window handle or null, no pointers involved.
    let window = unsafe { GetForegroundWindow() };
    if window.is_invalid() || is_shell_class(window) {
        return false;
    }

    let mut bounds = RECT::default();
    // SAFETY: the pointer targets a live local for the duration of the call.
    if unsafe { GetWindowRect(window, &mut bounds) }.is_err() {
        return false;
    }

    // SAFETY: the handle is a live window, so the monitor lookup cannot fail.
    let monitor = describe(unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST) });
    screen_rect(bounds).covers(monitor.bounds)
}

fn from_point(point: POINT, flags: MONITOR_FROM_FLAGS) -> HMONITOR {
    // SAFETY: the flags guarantee a monitor is returned even for an off-screen point.
    unsafe { MonitorFromPoint(point, flags) }
}

fn describe(monitor: HMONITOR) -> Monitor {
    let (bounds, work) = monitor_rects(monitor);
    Monitor {
        bounds,
        work,
        ui_scale: monitor_dpi(monitor) as f32 / USER_DEFAULT_DPI as f32,
        pixel_ratio: PHYSICAL_PIXELS,
    }
}

/// The whole monitor and its work area.
fn monitor_rects(monitor: HMONITOR) -> (ScreenRect, ScreenRect) {
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..MONITORINFO::default()
    };

    // SAFETY: cbSize is set and the pointer targets a live local.
    if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return (screen_rect(info.rcMonitor), screen_rect(info.rcWork));
    }
    (ScreenRect::default(), ScreenRect::default())
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

fn screen_rect(rect: RECT) -> ScreenRect {
    ScreenRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}
