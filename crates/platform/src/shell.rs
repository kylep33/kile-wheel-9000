//! Starting programs at the shell's integrity level.
//!
//! This process runs elevated so UIPI cannot block the input hooks. Anything it started
//! directly would inherit that admin token, so launches borrow a primary token from the
//! Explorer that owns the desktop and start the child with that instead.
//!
//! An earlier attempt walked Explorer's desktop view to `IShellDispatch2`. That failed
//! with `E_NOINTERFACE` in practice, because it needs an active shell view on the
//! desktop, which is not something a launcher can rely on.

use thiserror::Error;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{
    DuplicateTokenEx, SECURITY_ATTRIBUTES, SecurityImpersonation, TOKEN_ALL_ACCESS,
    TOKEN_DUPLICATE, TOKEN_QUERY, TokenPrimary,
};
use windows::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateProcessWithTokenW, OpenProcess, OpenProcessToken,
    PROCESS_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, STARTF_USESHOWWINDOW, STARTUPINFOW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetShellWindow, GetWindowThreadProcessId, SW_HIDE, SW_SHOWNORMAL,
};
use windows::core::{HSTRING, PCWSTR, PWSTR};

/// Explorer resolves anything a double-click would: urls, folders, shortcuts, documents
/// and `shell:AppsFolder\<AppUserModelID>`.
const EXPLORER_EXE: &str = "explorer.exe";

/// What to run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Command {
    pub program: String,
    pub arguments: Option<String>,
    pub working_dir: Option<String>,
    /// Starts with no window, for a script that only needs to do its work.
    pub hidden: bool,
}

impl Command {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            ..Self::default()
        }
    }

    pub fn with_arguments(mut self, arguments: Option<String>) -> Self {
        self.arguments = arguments.filter(|text| !text.trim().is_empty());
        self
    }

    pub fn in_directory(mut self, working_dir: Option<String>) -> Self {
        self.working_dir = working_dir.filter(|text| !text.trim().is_empty());
        self
    }

    pub fn hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }

    /// The command line Windows parses, with the program quoted so a path containing
    /// spaces stays one argument.
    fn line(&self) -> String {
        match &self.arguments {
            Some(arguments) => format!("\"{}\" {arguments}", self.program),
            None => format!("\"{}\"", self.program),
        }
    }
}

#[derive(Debug, Error)]
pub enum ShellError {
    #[error(
        "could not find the Explorer that owns the desktop, so there is no unelevated token to launch with; nothing was started"
    )]
    NoShell,

    #[error(
        "could not borrow the shell's token: {source}; without it a launched app would inherit this process's admin token, so nothing was started"
    )]
    NoToken {
        #[source]
        source: windows::core::Error,
    },

    #[error("could not start \"{program}\": {source}; check the path exists and is runnable")]
    Failed {
        program: String,
        #[source]
        source: windows::core::Error,
    },
}

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

/// Runs the command with the shell's token, so the child is not elevated.
pub fn execute(command: &Command) -> Result<(), ShellError> {
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

    started.map_err(|source| ShellError::Failed {
        program: command.program.clone(),
        source,
    })
}

/// Hands a single path, url or shell item to Explorer to resolve, exactly as a
/// double-click would.
pub fn open_shell_item(target: &str) -> Result<(), ShellError> {
    execute(&Command::new(EXPLORER_EXE).with_arguments(Some(format!("\"{target}\""))))
}

/// A primary token copied from the Explorer that owns the desktop.
fn shell_token() -> Result<OwnedHandle, ShellError> {
    let process_id = shell_process_id().ok_or(ShellError::NoShell)?;

    // SAFETY: the handle is owned and closed when the guard drops.
    let shell = OwnedHandle(
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
            .map_err(|source| ShellError::NoToken { source })?,
    );

    let mut opened = HANDLE::default();

    // SAFETY: the out-pointer targets a live local.
    unsafe { OpenProcessToken(shell.0, TOKEN_DUPLICATE | TOKEN_QUERY, &raw mut opened) }
        .map_err(|source| ShellError::NoToken { source })?;
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
    .map_err(|source| ShellError::NoToken { source })?;

    Ok(OwnedHandle(primary))
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
    fn a_program_without_arguments_is_quoted_on_its_own() {
        let command = Command::new(r"C:\Program Files\App\app.exe");

        assert_eq!(command.line(), r#""C:\Program Files\App\app.exe""#);
    }

    #[test]
    fn arguments_follow_the_quoted_program() {
        let command = Command::new("wt.exe").with_arguments(Some("-p \"Ubuntu\"".to_owned()));

        assert_eq!(command.line(), r#""wt.exe" -p "Ubuntu""#);
    }

    #[test]
    fn blank_arguments_are_treated_as_absent() {
        let command = Command::new("app.exe").with_arguments(Some("   ".to_owned()));

        assert_eq!(command.arguments, None);
        assert_eq!(command.line(), r#""app.exe""#);
    }

    #[test]
    fn blank_working_directories_are_treated_as_absent() {
        let command = Command::new("app.exe").in_directory(Some(String::new()));

        assert_eq!(command.working_dir, None);
    }

    #[test]
    fn a_wide_string_is_null_terminated() {
        assert_eq!(wide("hi"), [u16::from(b'h'), u16::from(b'i'), 0]);
    }
}
