//! Window titles and the executable behind a window.

use std::path::PathBuf;

use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
};
use windows::core::PWSTR;

/// Long enough for any real title; the wheel truncates for display anyway.
const MAX_TITLE_LEN: usize = 512;

/// An over-long path makes the query fail outright rather than truncate silently.
const MAX_IMAGE_PATH_LEN: usize = 1024;

pub fn has_title(window: HWND) -> bool {
    // SAFETY: reads a length only, no pointers involved.
    (unsafe { GetWindowTextLengthW(window) }) > 0
}

pub fn title(window: HWND) -> String {
    let mut buffer = [0u16; MAX_TITLE_LEN];

    // SAFETY: the slice is live for the call and its length bounds the write.
    let written = unsafe { GetWindowTextW(window, &mut buffer) };
    if written <= 0 {
        return String::new();
    }

    String::from_utf16_lossy(&buffer[..written as usize])
}

/// Full path of the executable that owns the window.
pub fn executable_path(window: HWND) -> Option<PathBuf> {
    let process_id = process_id(window)?;

    // SAFETY: the handle is closed before every return below.
    let process =
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }.ok()?;

    let mut buffer = [0u16; MAX_IMAGE_PATH_LEN];
    let mut length = buffer.len() as u32;

    // SAFETY: `length` matches the buffer and both stay live for the call.
    let queried = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    };

    // SAFETY: the handle came from OpenProcess and is not used again.
    let _ = unsafe { CloseHandle(process) };

    queried.ok()?;
    Some(PathBuf::from(String::from_utf16_lossy(
        &buffer[..length as usize],
    )))
}

fn process_id(window: HWND) -> Option<u32> {
    let mut process_id = 0;

    // SAFETY: the pointer targets a live local for the duration of the call.
    unsafe { GetWindowThreadProcessId(window, Some(&mut process_id)) };
    (process_id != 0).then_some(process_id)
}
