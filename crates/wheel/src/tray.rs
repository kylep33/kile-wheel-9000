//! The notification area icon, which is the only way to reach the app without the hotkey.
//!
//! It owns a window of its own rather than hanging off the overlay. A popup menu needs its
//! owner to be the foreground window to dismiss properly, and the overlay must never take
//! focus, so the two cannot share one.

use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconFromResourceEx, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
    DestroyIcon, DestroyMenu, DestroyWindow, GetCursorPos, GetSystemMetrics, HICON, HMENU,
    LR_DEFAULTCOLOR, MF_GRAYED, MF_SEPARATOR, MF_STRING, PostMessageW, RegisterClassExW,
    SM_CXSMICON, SM_CYSMICON, SetForegroundWindow, TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    TrackPopupMenu, WM_APP, WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::{HSTRING, PCWSTR};

const WINDOW_CLASS: &str = "kile-wheel-9000.tray";

/// Added to the tooltip while the wheel is off, so hovering says which it is.
const OFF_SUFFIX: &str = " (off)";

/// The icon the notification area shows. Drawn once rather than compiled in as a win32
/// resource, which would pull rc.exe into the build.
const ICON_PNG: &[u8] = include_bytes!("../assets/tray.png");

/// `CreateIconFromResourceEx` wants the version of the icon format, not of Windows.
const ICON_FORMAT_VERSION: u32 = 0x0003_0000;

/// Identifies our one icon within this process.
const ICON_ID: u32 = 1;

/// Sent by the shell when the user does something to the icon.
const WM_TRAY_ICON: u32 = WM_APP + 4;

/// Posted to the overlay window with a [`Command`] in its `wParam`.
pub const WM_TRAY_COMMAND: u32 = WM_APP + 5;

/// What the menu asks the app to do. The values are the menu item ids, so zero is kept
/// clear: `TrackPopupMenu` returns zero when the menu is dismissed without a choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    OpenWheel = 1,
    Settings = 2,
    ReloadConfig = 3,
    Quit = 4,
    ToggleEnabled = 5,
}

impl Command {
    fn label(self, enabled: bool) -> &'static str {
        match self {
            Self::OpenWheel => "Open wheel",
            Self::Settings => "Settings",
            Self::ReloadConfig => "Reload config",
            Self::Quit => "Quit",
            Self::ToggleEnabled if enabled => "Disable",
            Self::ToggleEnabled => "Enable",
        }
    }

    /// Greyed out while the wheel is off, rather than clicking through to nothing.
    fn needs_enabled(self) -> bool {
        matches!(self, Self::OpenWheel)
    }

    /// `None` for anything that is not one of ours, which includes the zero that comes back
    /// when the menu was dismissed.
    pub fn from_id(id: usize) -> Option<Self> {
        [
            Self::OpenWheel,
            Self::Settings,
            Self::ReloadConfig,
            Self::Quit,
            Self::ToggleEnabled,
        ]
        .into_iter()
        .find(|command| *command as usize == id)
    }
}

/// Where menu choices are posted. A static is how the window procedure reaches it, and the
/// app is a single instance, so there is only ever one value to hold.
static COMMAND_TARGET: AtomicIsize = AtomicIsize::new(0);

/// Whether the wheel is on. Read while building the menu, which happens in the window
/// procedure and so cannot reach the app.
static ENABLED: AtomicBool = AtomicBool::new(true);

#[derive(Debug)]
pub struct Tray {
    window: HWND,
    icon: HICON,
    /// Kept so the tooltip can be rebuilt when the wheel is turned on or off.
    name: String,
}

impl Tray {
    /// `target` receives [`WM_TRAY_COMMAND`]; it is the overlay window, because that is
    /// what the message loop pumps.
    pub fn create(target: HWND, tooltip: &str) -> Result<Self> {
        COMMAND_TARGET.store(target.0 as isize, Ordering::Relaxed);

        let window = create_window().context("could not create the tray window")?;
        let icon = load_icon().context("could not build the tray icon")?;

        let mut data = icon_data(window);
        data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        data.uCallbackMessage = WM_TRAY_ICON;
        data.hIcon = icon;
        write_tooltip(&mut data.szTip, tooltip);

        // SAFETY: the data outlives the call, and the window and icon are both live.
        let added = unsafe { Shell_NotifyIconW(NIM_ADD, &data) }.as_bool();
        if !added {
            // SAFETY: nothing else owns either handle yet.
            unsafe {
                let _ = DestroyIcon(icon);
                let _ = DestroyWindow(window);
            }
            anyhow::bail!("the shell refused the tray icon");
        }

        Ok(Self {
            window,
            icon,
            name: tooltip.to_owned(),
        })
    }

    /// Tells the icon what the app's state is, for the tooltip and the menu's toggle.
    pub fn set_enabled(&self, enabled: bool) {
        ENABLED.store(enabled, Ordering::Relaxed);

        let mut data = icon_data(self.window);
        data.uFlags = NIF_TIP;
        write_tooltip(&mut data.szTip, &self.tooltip(enabled));

        // SAFETY: the data outlives the call, and the icon is still registered.
        let _ = unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
    }

    fn tooltip(&self, enabled: bool) -> String {
        match enabled {
            true => self.name.clone(),
            false => format!("{}{OFF_SUFFIX}", self.name),
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        let data = icon_data(self.window);

        // SAFETY: removing an icon that is already gone just fails, and the handles below
        // are ours and are not used again.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &data);
            let _ = DestroyIcon(self.icon);
            let _ = DestroyWindow(self.window);
        }
    }
}

fn icon_data(window: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: window,
        uID: ICON_ID,
        ..Default::default()
    }
}

/// Truncates rather than failing: a clipped tooltip is better than no icon.
fn write_tooltip(field: &mut [u16], text: &str) {
    let room = field.len() - 1;

    for (slot, unit) in field.iter_mut().zip(text.encode_utf16().take(room)) {
        *slot = unit;
    }
}

fn load_icon() -> Result<HICON> {
    // SAFETY: reports the notification area's icon size; no pointers involved.
    let width_px = unsafe { GetSystemMetrics(SM_CXSMICON) };
    // SAFETY: as above.
    let height_px = unsafe { GetSystemMetrics(SM_CYSMICON) };

    // SAFETY: the bytes are a png, which this call accepts as an icon image, and they are
    // static so they outlive it.
    let icon = unsafe {
        CreateIconFromResourceEx(
            ICON_PNG,
            true,
            ICON_FORMAT_VERSION,
            width_px,
            height_px,
            LR_DEFAULTCOLOR,
        )
    };
    icon.context("the tray icon image was rejected")
}

fn create_window() -> windows::core::Result<HWND> {
    let class = HSTRING::from(WINDOW_CLASS);

    // SAFETY: passing None asks for this executable's own module handle.
    let instance = unsafe { GetModuleHandleW(None) }?;

    let descriptor = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(window_proc),
        hInstance: instance.into(),
        lpszClassName: PCWSTR(class.as_ptr()),
        ..Default::default()
    };

    // SAFETY: the descriptor and its strings stay live for the duration of the call.
    if unsafe { RegisterClassExW(&descriptor) } == 0 {
        return Err(windows::core::Error::from_thread());
    }

    // SAFETY: the class is registered and the strings stay live for the call. The window is
    // never shown, so it only ever receives messages.
    unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            PCWSTR(class.as_ptr()),
            PCWSTR(class.as_ptr()),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )
    }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_TRAY_ICON {
        // the shell packs the mouse message into the low word of lparam
        match lparam.0 as u32 & 0xffff {
            WM_LBUTTONUP => post(Command::Settings),
            WM_RBUTTONUP => show_menu(window),
            _ => {}
        }
        return LRESULT(0);
    }

    // SAFETY: forwards the values Windows gave us to the default handler.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

/// Builds the menu fresh each time, so there is no handle to keep alive between opens.
fn show_menu(window: HWND) {
    // SAFETY: the menu is destroyed on every path below.
    let menu = unsafe { CreatePopupMenu() };
    let Ok(menu) = menu else {
        return;
    };

    let items = [
        Some(Command::OpenWheel),
        Some(Command::Settings),
        None,
        Some(Command::ToggleEnabled),
        Some(Command::ReloadConfig),
        None,
        Some(Command::Quit),
    ];

    let enabled = ENABLED.load(Ordering::Relaxed);
    for item in items {
        append(menu, item, enabled);
    }

    // SAFETY: taking the foreground is what lets the menu close on a click elsewhere. This
    // window is never visible, so nothing appears to steal focus.
    let _ = unsafe { SetForegroundWindow(window) };

    let mut at = POINT::default();
    // SAFETY: writes into a local that outlives the call.
    if unsafe { GetCursorPos(&mut at) }.is_ok() {
        track(menu, window, at);
    }

    // SAFETY: matches the CreatePopupMenu above.
    let _ = unsafe { DestroyMenu(menu) };
}

fn append(menu: HMENU, item: Option<Command>, enabled: bool) {
    let Some(command) = item else {
        // SAFETY: a separator takes neither an id nor a string.
        let _ = unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, None) };
        return;
    };

    let label = HSTRING::from(command.label(enabled));
    let flags = match command.needs_enabled() && !enabled {
        true => MF_STRING | MF_GRAYED,
        false => MF_STRING,
    };

    // SAFETY: the label stays live for the duration of the call.
    let _ = unsafe { AppendMenuW(menu, flags, command as usize, &label) };
}

fn track(menu: HMENU, window: HWND, at: POINT) {
    // SAFETY: returncmd makes this return the choice rather than post it, so no WM_COMMAND
    // handler is needed on a window that is never visible.
    let chosen = unsafe {
        TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_BOTTOMALIGN,
            at.x,
            at.y,
            None,
            window,
            None,
        )
    };

    if let Some(command) = Command::from_id(chosen.0 as usize) {
        post(command);
    }
}

/// Posts rather than calling into the app directly: the app owns its state on the main
/// thread, and this runs inside a nested menu message loop.
fn post(command: Command) {
    let target = COMMAND_TARGET.load(Ordering::Relaxed);
    if target == 0 {
        return;
    }

    // SAFETY: the message carries an integer, so nothing has to stay alive for it.
    let _ = unsafe {
        PostMessageW(
            Some(HWND(target as *mut std::ffi::c_void)),
            WM_TRAY_COMMAND,
            WPARAM(command as usize),
            LPARAM(0),
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_round_trips_through_its_menu_id() {
        for command in [
            Command::OpenWheel,
            Command::Settings,
            Command::ReloadConfig,
            Command::Quit,
            Command::ToggleEnabled,
        ] {
            assert_eq!(Command::from_id(command as usize), Some(command));
        }
    }

    #[test]
    fn the_toggle_says_what_it_will_do() {
        assert_eq!(Command::ToggleEnabled.label(true), "Disable");
        assert_eq!(Command::ToggleEnabled.label(false), "Enable");
    }

    #[test]
    fn a_dismissed_menu_returns_no_command() {
        assert_eq!(Command::from_id(0), None);
    }

    #[test]
    fn a_tooltip_longer_than_the_field_is_truncated_and_stays_terminated() {
        let mut field = [1u16; 8];
        write_tooltip(&mut field, "abcdefghijkl");

        assert_eq!(field[0], u16::from(b'a'));
        assert_eq!(field[6], u16::from(b'g'));
        // the last slot is left alone, so a zeroed field keeps its terminator
        assert_eq!(field[7], 1);
    }

    #[test]
    fn a_short_tooltip_leaves_the_rest_of_the_field_alone() {
        let mut field = [0u16; 8];
        write_tooltip(&mut field, "ab");

        assert_eq!(field[0], u16::from(b'a'));
        assert_eq!(field[1], u16::from(b'b'));
        assert_eq!(field[2], 0);
    }
}
