//! Single-instance guard, and the hand-over from a second launch.

use std::sync::OnceLock;

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, FindWindowExW, HWND_MESSAGE, PostMessageW,
    RegisterClassExW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WNDCLASSEXW,
};
use windows::core::{HSTRING, PCWSTR};

use crate::event_loop::{Notice, Waker};
use crate::instance::{Claim, InstanceError};

/// Session-local, so the claim is per logged-on user rather than machine wide.
const NAMESPACE: &str = "Local";

/// The message-only window a second launch posts to.
const LISTENER_CLASS_SUFFIX: &str = ".instance";

/// Asks the running instance to open its wheel.
const WM_OPEN_WHEEL: u32 = WM_APP + 2;

/// The window procedure has no user data, so the waker it forwards to is global. There
/// is only ever one listener per process.
static LISTENER_WAKER: OnceLock<Waker> = OnceLock::new();

/// Holds the claim, and the listener window once there is one, for the process lifetime.
#[derive(Debug)]
pub struct Guard {
    mutex: HANDLE,
    app_name: String,
    listener: Option<HWND>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        // SAFETY: both handles are ours and Drop runs exactly once.
        unsafe {
            if let Some(listener) = self.listener {
                let _ = DestroyWindow(listener);
            }
            let _ = CloseHandle(self.mutex);
        }
    }
}

impl Guard {
    pub fn listen(&mut self, waker: Waker) -> Result<(), InstanceError> {
        let _ = LISTENER_WAKER.set(waker);
        let listener = create_listener(&listener_class(&self.app_name)).map_err(|error| {
            InstanceError(format!(
                "could not create the window a second launch hands over to: {error}"
            ))
        })?;
        self.listener = Some(listener);
        Ok(())
    }
}

pub fn claim(app_name: &str) -> Result<Claim, InstanceError> {
    let name = HSTRING::from(format!("{NAMESPACE}\\{app_name}"));

    // SAFETY: `name` outlives the call and the default security descriptor is wanted.
    let mutex = unsafe { CreateMutexW(None, true, &name) }.map_err(|error| {
        InstanceError(format!("could not create the single-instance mutex: {error}"))
    })?;

    // SAFETY: reads this thread's last-error slot, which CreateMutexW just set.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        // SAFETY: the handle is ours and is not touched again.
        let _ = unsafe { CloseHandle(mutex) };
        return Ok(Claim::AlreadyRunning);
    }

    Ok(Claim::Acquired(crate::instance::InstanceGuard::from_sys(
        Guard {
            mutex,
            app_name: app_name.to_owned(),
            listener: None,
        },
    )))
}

pub fn signal_existing(app_name: &str) -> bool {
    let class = HSTRING::from(listener_class(app_name));

    // SAFETY: the class name stays live for the duration of the call; message-only
    // windows are only found by searching under HWND_MESSAGE.
    let found = unsafe {
        FindWindowExW(
            Some(HWND_MESSAGE),
            None,
            PCWSTR(class.as_ptr()),
            PCWSTR::null(),
        )
    };
    let Ok(existing) = found else {
        return false;
    };

    // SAFETY: a parameterless message carries no pointers across the process boundary.
    unsafe { PostMessageW(Some(existing), WM_OPEN_WHEEL, WPARAM(0), LPARAM(0)) }.is_ok()
}

fn listener_class(app_name: &str) -> String {
    format!("{app_name}{LISTENER_CLASS_SUFFIX}")
}

fn create_listener(class_name: &str) -> windows::core::Result<HWND> {
    let class = HSTRING::from(class_name);

    // SAFETY: passing None asks for this executable's own module handle.
    let instance = unsafe { GetModuleHandleW(None) }?;

    let descriptor = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(listener_proc),
        hInstance: instance.into(),
        lpszClassName: PCWSTR(class.as_ptr()),
        ..Default::default()
    };

    // SAFETY: the descriptor and its strings stay live for the duration of the call.
    if unsafe { RegisterClassExW(&descriptor) } == 0 {
        return Err(windows::core::Error::from_thread());
    }

    // SAFETY: the class is registered, the strings stay live for the call, and a
    // message-only window is never shown.
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class.as_ptr()),
            PCWSTR(class.as_ptr()),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance.into()),
            None,
        )
    }
}

unsafe extern "system" fn listener_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_OPEN_WHEEL {
        if let Some(waker) = LISTENER_WAKER.get() {
            waker.send(Notice::OpenWheel);
        }
        return LRESULT(0);
    }

    // SAFETY: forwards the values Windows gave us to the default handler.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}
