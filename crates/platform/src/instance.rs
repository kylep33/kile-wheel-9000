//! Single-instance guard.

use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::{HSTRING, Result};

/// Session-local, so the claim is per logged-on user rather than machine wide.
const NAMESPACE: &str = "Local";

/// Holds the claim for the process lifetime.
#[derive(Debug)]
pub struct InstanceGuard {
    handle: HANDLE,
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        // SAFETY: the handle came from CreateMutexW and Drop runs exactly once.
        let _ = unsafe { CloseHandle(self.handle) };
    }
}

/// Outcome of trying to become the one running instance.
#[derive(Debug)]
pub enum Claim {
    Acquired(InstanceGuard),
    AlreadyRunning,
}

/// Claims the named mutex that marks the running instance.
pub fn claim(app_name: &str) -> Result<Claim> {
    let name = HSTRING::from(format!("{NAMESPACE}\\{app_name}"));

    // SAFETY: `name` outlives the call and the default security descriptor is wanted.
    let handle = unsafe { CreateMutexW(None, true, &name) }?;

    // SAFETY: reads this thread's last-error slot, which CreateMutexW just set.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        // SAFETY: the handle is ours and is not touched again.
        unsafe { CloseHandle(handle) }?;
        return Ok(Claim::AlreadyRunning);
    }

    Ok(Claim::Acquired(InstanceGuard { handle }))
}
