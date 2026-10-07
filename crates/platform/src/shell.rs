//! Handing work to Explorer, so launched programs do not inherit our admin token.
//!
//! This process runs elevated so UIPI cannot block the input hooks, which means a plain
//! `ShellExecute` from here would give every launched app an admin token. Asking the
//! running, unelevated Explorer to do it keeps children at the user's normal level.

use thiserror::Error;
use windows::Win32::System::Com::{CLSCTX_LOCAL_SERVER, CoCreateInstance, IServiceProvider};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Shell::{
    IShellBrowser, IShellDispatch2, IShellFolderViewDual, IShellView, IShellWindows,
    SID_STopLevelBrowser, SVGIO_BACKGROUND, SWC_DESKTOP, SWFO_NEEDDISPATCH, ShellWindows,
};
use windows::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWNORMAL};
use windows::core::{BSTR, Interface};

/// Win32 `CSIDL_DESKTOP`, the shell folder whose view hosts the Application object.
const CSIDL_DESKTOP: i32 = 0;

/// What to run.
#[derive(Debug, Clone, Default)]
pub struct Command {
    pub file: String,
    pub arguments: Option<String>,
    pub working_dir: Option<String>,
    /// Starts with no window, for a script that only needs to do its work.
    pub hidden: bool,
}

impl Command {
    pub fn new(file: impl Into<String>) -> Self {
        Self {
            file: file.into(),
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
}

#[derive(Debug, Error)]
pub enum ShellError {
    #[error(
        "could not reach Explorer to run the launch: {source}; without it a launched app would inherit this process's admin token, so nothing was started"
    )]
    Unreachable {
        #[source]
        source: windows::core::Error,
    },

    #[error("could not start \"{file}\": {source}; check the path exists and is runnable")]
    Failed {
        file: String,
        #[source]
        source: windows::core::Error,
    },
}

/// Runs the command through Explorer.
pub fn execute(command: &Command) -> Result<(), ShellError> {
    let shell = dispatch().map_err(|source| ShellError::Unreachable { source })?;
    let show = match command.hidden {
        true => SW_HIDE.0,
        false => SW_SHOWNORMAL.0,
    };

    // SAFETY: every argument stays live for the duration of the call.
    unsafe {
        shell.ShellExecute(
            &BSTR::from(command.file.as_str()),
            &optional_variant(command.arguments.as_deref()),
            &optional_variant(command.working_dir.as_deref()),
            &VARIANT::default(),
            &VARIANT::from(show),
        )
    }
    .map_err(|source| ShellError::Failed {
        file: command.file.clone(),
        source,
    })
}

/// Walks from the Explorer desktop view to the shell Application object it hosts.
fn dispatch() -> windows::core::Result<IShellDispatch2> {
    // SAFETY: CLSCTX_LOCAL_SERVER binds to the already running Explorer rather than
    // starting a new, elevated server inside this process.
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

    // SAFETY: the Application object it returns is the shell's own, which is what keeps
    // the launch unelevated.
    let application = unsafe { folder_view.Application() }?;
    application.cast()
}

fn optional_variant(text: Option<&str>) -> VARIANT {
    match text {
        Some(text) => VARIANT::from(BSTR::from(text)),
        None => VARIANT::default(),
    }
}
