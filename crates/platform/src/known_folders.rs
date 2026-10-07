//! Shell folder lookups.

use std::path::PathBuf;

use windows::Win32::Foundation::E_UNEXPECTED;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{FOLDERID_RoamingAppData, KF_FLAG_DEFAULT, SHGetKnownFolderPath};
use windows::core::{Error, Result};

/// Resolves `%APPDATA%`.
///
/// The shell is asked instead of the environment because a scheduled task can start
/// with an incomplete environment block.
pub fn roaming_app_data() -> Result<PathBuf> {
    // SAFETY: the shell allocates the string, and it is freed before any early return.
    unsafe {
        let wide = SHGetKnownFolderPath(&FOLDERID_RoamingAppData, KF_FLAG_DEFAULT, None)?;
        let text = wide.to_string();
        CoTaskMemFree(Some(wide.as_ptr().cast()));

        let text = text.map_err(|_| Error::from(E_UNEXPECTED))?;
        Ok(PathBuf::from(text))
    }
}
