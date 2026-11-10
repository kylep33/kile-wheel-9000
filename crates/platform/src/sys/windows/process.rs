//! Starting programs at the shell's integrity level.
//!
//! This process runs elevated so UIPI cannot block the input hooks. Anything it started
//! directly would inherit that admin token, so launches borrow a primary token from the
//! Explorer that owns the desktop and start the child with that instead.
//!
//! An earlier attempt walked Explorer's desktop view to `IShellDispatch2`. That failed
//! with `E_NOINTERFACE` in practice, because it needs an active shell view on the
//! desktop, which is not something a launcher can rely on.

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{
    DuplicateTokenEx, GetTokenInformation, SECURITY_ATTRIBUTES, SecurityImpersonation,
    TOKEN_ALL_ACCESS, TOKEN_DUPLICATE, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation, TokenPrimary,
};
use windows::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, CreateProcessWithTokenW, GetCurrentProcess,
    OpenProcess, OpenProcessToken, PROCESS_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION,
    STARTF_USESHOWWINDOW, STARTUPINFOW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetShellWindow, GetWindowThreadProcessId, SW_HIDE, SW_SHOWNORMAL,
};
use windows::core::{HSTRING, PCWSTR, PWSTR};

use crate::process::{Command, ProcessError};

/// Explorer resolves anything a double-click would: urls, folders, shortcuts, documents
/// and `shell:AppsFolder\<AppUserModelID>`.
const EXPLORER_EXE: &str = "explorer.exe";

/// Closes the handle it holds, so an early return cannot leak it.
#[derive(Debug)]
struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: the handle came from a call that hands ownership over, and Drop runs
        // exactly once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Runs the command unelevated: with the shell's token from the elevated wheel, or as a
/// plain child from the settings window, which has no token-borrowing privilege.
pub fn execute(command: &Command) -> Result<(), ProcessError> {
    match is_elevated() {
        true => execute_as_shell(command),
        false => execute_directly(command),
    }
}

fn execute_directly(command: &Command) -> Result<(), ProcessError> {
    use std::os::windows::process::CommandExt;

    let mut child = std::process::Command::new(&command.program);
    if let Some(arguments) = &command.arguments {
        child.raw_arg(arguments);
    }
    if let Some(directory) = &command.working_dir {
        child.current_dir(directory);
    }
    if command.hidden {
        child.creation_flags(CREATE_NO_WINDOW.0);
    }

    child.spawn().map(drop).map_err(|source| {
        ProcessError(format!(
            "could not start \"{}\": {source}; check the path exists and is runnable",
            command.program
        ))
    })
}

fn is_elevated() -> bool {
    let mut token = HANDLE::default();

    // SAFETY: the pseudo handle needs no closing, and the token is closed by its guard.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) }.is_err() {
        return false;
    }
    let token = OwnedHandle(token);

    let mut elevation = TOKEN_ELEVATION::default();
    let mut written = 0;

    // SAFETY: the buffer and its size describe the same live struct.
    let read = unsafe {
        GetTokenInformation(
            token.0,
            TokenElevation,
            Some((&raw mut elevation).cast()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &raw mut written,
        )
    };
    read.is_ok() && elevation.TokenIsElevated != 0
}

fn execute_as_shell(command: &Command) -> Result<(), ProcessError> {
    let token = shell_token()?;
    let mut line = wide(&command.line());
    let working_dir = command
        .working_dir
        .as_ref()
        .map(|path| HSTRING::from(path.as_str()));
    let current_dir = working_dir
        .as_ref()
        .map_or(PCWSTR::null(), |path| PCWSTR(path.as_ptr()));

    let startup = STARTUPINFOW {
        cb: size_of::<STARTUPINFOW>() as u32,
        dwFlags: STARTF_USESHOWWINDOW,
        wShowWindow: show_flag(command.hidden),
        ..Default::default()
    };
    let mut process = PROCESS_INFORMATION::default();

    // SAFETY: the command line is a mutable, null-terminated buffer that outlives the
    // call, which is what CreateProcessWithTokenW requires of it.
    let started = unsafe {
        CreateProcessWithTokenW(
            token.0,
            Default::default(),
            PCWSTR::null(),
            Some(PWSTR(line.as_mut_ptr())),
            CREATE_UNICODE_ENVIRONMENT,
            None,
            current_dir,
            &startup,
            &raw mut process,
        )
    };

    // the child runs on its own; we only hold these to avoid leaking them
    close_if_set(process.hProcess);
    close_if_set(process.hThread);

    started.map_err(|source| {
        ProcessError(format!(
            "could not start \"{}\": {source}; check the path exists and is runnable",
            command.program
        ))
    })
}

/// Hands a single path, url or shell item to Explorer to resolve, exactly as a
/// double-click would.
pub fn open(target: &str) -> Result<(), ProcessError> {
    execute(&Command::new(EXPLORER_EXE).with_arguments(Some(format!("\"{target}\""))))
}

/// A primary token copied from the Explorer that owns the desktop.
fn shell_token() -> Result<OwnedHandle, ProcessError> {
    let process_id = shell_process_id().ok_or_else(|| {
        ProcessError(
            "could not find the Explorer that owns the desktop, so there is no unelevated token to launch with; nothing was started"
                .to_owned(),
        )
    })?;

    // SAFETY: the handle is owned and closed when the guard drops.
    let shell = OwnedHandle(
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
            .map_err(no_token)?,
    );

    let mut opened = HANDLE::default();

    // SAFETY: the out-pointer targets a live local.
    unsafe { OpenProcessToken(shell.0, TOKEN_DUPLICATE | TOKEN_QUERY, &raw mut opened) }
        .map_err(no_token)?;
    let opened = OwnedHandle(opened);

    let mut primary = HANDLE::default();

    // SAFETY: a primary token is what CreateProcessWithTokenW needs; an impersonation
    // token would be refused.
    unsafe {
        DuplicateTokenEx(
            opened.0,
            TOKEN_ALL_ACCESS,
            None::<*const SECURITY_ATTRIBUTES>,
            SecurityImpersonation,
            TokenPrimary,
            &raw mut primary,
        )
    }
    .map_err(no_token)?;

    Ok(OwnedHandle(primary))
}

fn no_token(source: windows::core::Error) -> ProcessError {
    ProcessError(format!(
        "could not borrow the shell's token: {source}; without it a launched app would inherit this process's admin token, so nothing was started"
    ))
}

/// The Explorer that owns the desktop, which is the shell running at the user's normal
/// integrity level.
fn shell_process_id() -> Option<u32> {
    // SAFETY: returns a borrowed handle or null, no pointers involved.
    let desktop = unsafe { GetShellWindow() };
    if desktop.is_invalid() {
        return None;
    }

    let mut process_id = 0;

    // SAFETY: the pointer targets a live local for the duration of the call.
    unsafe { GetWindowThreadProcessId(desktop, Some(&raw mut process_id)) };
    (process_id != 0).then_some(process_id)
}

fn show_flag(hidden: bool) -> u16 {
    match hidden {
        true => SW_HIDE.0 as u16,
        false => SW_SHOWNORMAL.0 as u16,
    }
}

fn close_if_set(handle: HANDLE) {
    if handle.is_invalid() {
        return;
    }

    // SAFETY: CreateProcessWithTokenW hands ownership of both handles over.
    let _ = unsafe { CloseHandle(handle) };
}

/// A null-terminated, mutable UTF-16 buffer.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_string_is_null_terminated() {
        assert_eq!(wide("hi"), [u16::from(b'h'), u16::from(b'i'), 0]);
    }
}
