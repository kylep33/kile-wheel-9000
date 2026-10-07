//! Bringing a window forward, and starting programs.

use thiserror::Error;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_LOCAL_SERVER, CoCreateInstance, IServiceProvider};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::Shell::{
    IShellBrowser, IShellDispatch2, IShellFolderViewDual, IShellView, IShellWindows,
    SID_STopLevelBrowser, SVGIO_BACKGROUND, SWC_DESKTOP, SWFO_NEEDDISPATCH, ShellWindows,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, IsIconic, SW_MINIMIZE,
    SW_RESTORE, SetForegroundWindow, ShowWindow,
};
use windows::core::{BSTR, Interface};

/// Win32 `CSIDL_DESKTOP`, the shell folder whose view hosts the Application object.
const CSIDL_DESKTOP: i32 = 0;

#[derive(Debug, Error)]
pub enum ActivateError {
    #[error(
        "SetForegroundWindow was refused for window {0:#x}, so it stayed where it was; another process is holding the foreground lock"
    )]
    ForegroundRefused(isize),

    #[error(
        "could not reach Explorer to run the launch: {source}; without it a launched app would inherit this process's admin token, so nothing was started"
    )]
    ShellUnreachable {
        #[source]
        source: windows::core::Error,
    },

    #[error("could not start \"{target}\": {source}; check the path exists and is runnable")]
    LaunchFailed {
        target: String,
        #[source]
        source: windows::core::Error,
    },
}

pub fn is_minimized(window: HWND) -> bool {
    // SAFETY: a stale handle reports not-minimized rather than misbehaving.
    unsafe { IsIconic(window) }.as_bool()
}

pub fn is_foreground(window: HWND) -> bool {
    // SAFETY: returns a borrowed handle or null, no pointers involved.
    (unsafe { GetForegroundWindow() }) == window
}

pub fn minimize(window: HWND) {
    // SAFETY: a stale handle makes this a no-op.
    let _ = unsafe { ShowWindow(window, SW_MINIMIZE) };
}

/// Brings the window forward, restoring it first if it is minimized.
///
/// A background process is not normally allowed to take the foreground, so the
/// target's input queue is borrowed for the duration of the call.
pub fn focus(window: HWND) -> Result<(), ActivateError> {
    if is_minimized(window) {
        // SAFETY: a stale handle makes this a no-op.
        let _ = unsafe { ShowWindow(window, SW_RESTORE) };
    }

    let attached = attach_to(window);

    // SAFETY: a stale handle makes these fail rather than misbehave.
    let raised = unsafe {
        let raised = SetForegroundWindow(window).as_bool();
        let _ = BringWindowToTop(window);
        let _ = SetFocus(Some(window));
        raised
    };

    if let Some(target) = attached {
        detach_from(target);
    }

    if raised {
        return Ok(());
    }
    Err(ActivateError::ForegroundRefused(window.0 as isize))
}

/// Starts a program at the shell's integrity level.
///
/// This process runs elevated so UIPI cannot block the input hooks, which means a
/// plain `ShellExecute` from here would hand every launched app an admin token. The
/// launch is handed to Explorer instead, which is running unelevated.
pub fn launch(
    target: &str,
    arguments: Option<&str>,
    working_dir: Option<&str>,
) -> Result<(), ActivateError> {
    let shell = shell_dispatch().map_err(|source| ActivateError::ShellUnreachable { source })?;

    // SAFETY: every argument stays live for the duration of the call.
    unsafe {
        shell.ShellExecute(
            &BSTR::from(target),
            &optional_variant(arguments),
            &optional_variant(working_dir),
            &VARIANT::default(),
            &VARIANT::default(),
        )
    }
    .map_err(|source| ActivateError::LaunchFailed {
        target: target.to_owned(),
        source,
    })
}

/// Walks from the Explorer desktop view to the shell Application object it hosts.
fn shell_dispatch() -> windows::core::Result<IShellDispatch2> {
    // SAFETY: CLSCTX_LOCAL_SERVER binds to the already running Explorer rather than
    // starting a new, elevated server in this process.
    let shell_windows: IShellWindows =
        unsafe { CoCreateInstance(&ShellWindows, None, CLSCTX_LOCAL_SERVER) }?;

    let location = VARIANT::from(CSIDL_DESKTOP);
    let any_root = VARIANT::default();
    let mut desktop_window = 0i32;

    // SAFETY: all three pointers target live locals for the duration of the call.
    let desktop = unsafe {
        shell_windows.FindWindowSW(
            &raw const location,
            &raw const any_root,
            SWC_DESKTOP,
            &mut desktop_window,
            SWFO_NEEDDISPATCH,
        )
    }?;

    let provider: IServiceProvider = desktop.cast()?;

    // SAFETY: the desktop dispatch is live for the rest of this function.
    let browser: IShellBrowser = unsafe { provider.QueryService(&SID_STopLevelBrowser) }?;

    // SAFETY: the browser is live.
    let view: IShellView = unsafe { browser.QueryActiveShellView() }?;

    // SAFETY: the view background item is the folder view that owns Application.
    let folder_view: IShellFolderViewDual = unsafe { view.GetItemObject(SVGIO_BACKGROUND) }?;

    // SAFETY: the dual view is live, and the Application object it returns is the
    // shell's own, which is what keeps the launch unelevated.
    let application = unsafe { folder_view.Application() }?;
    application.cast()
}

fn optional_variant(text: Option<&str>) -> VARIANT {
    match text {
        Some(text) => VARIANT::from(BSTR::from(text)),
        None => VARIANT::default(),
    }
}

/// Borrows the target thread's input queue so the foreground change is allowed.
fn attach_to(window: HWND) -> Option<u32> {
    // SAFETY: reads the owning thread id; passing None skips the process id.
    let target = unsafe { GetWindowThreadProcessId(window, None) };
    if target == 0 {
        return None;
    }

    let ours = current_thread();
    if ours == target {
        return None;
    }

    // SAFETY: detached again by detach_from before focus() returns.
    if !unsafe { AttachThreadInput(ours, target, true) }.as_bool() {
        return None;
    }
    Some(target)
}

fn detach_from(target: u32) {
    // SAFETY: undoes exactly the attach that attach_to made.
    let _ = unsafe { AttachThreadInput(current_thread(), target, false) };
}

fn current_thread() -> u32 {
    // SAFETY: reads this thread's own id, no pointers involved.
    unsafe { GetCurrentThreadId() }
}
