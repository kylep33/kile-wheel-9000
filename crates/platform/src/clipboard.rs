//! Reading text off the clipboard, so Ctrl+V works in the wheel's own fields.

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::DataExchange::{
    CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
};
use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;

/// `None` when the clipboard holds no text, or another process has it open.
pub fn read_text() -> Option<String> {
    // SAFETY: reports whether the format is there; no pointers involved.
    if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT.0.into()) }.is_err() {
        return None;
    }

    // SAFETY: a null owner is allowed, and the clipboard is closed on every path below.
    unsafe { OpenClipboard(None) }.ok()?;

    let text = read_while_open();

    // SAFETY: matches the OpenClipboard above.
    let _ = unsafe { CloseClipboard() };
    text
}

fn read_while_open() -> Option<String> {
    // SAFETY: the handle stays owned by the clipboard, so it is read but never freed.
    let handle = unsafe { GetClipboardData(CF_UNICODETEXT.0.into()) }.ok()?;
    if handle.is_invalid() {
        return None;
    }

    // SAFETY: clipboard text is a movable global block, so it has to be locked to read.
    let locked = unsafe { GlobalLock(windows::Win32::Foundation::HGLOBAL(handle.0)) };
    if locked.is_null() {
        return None;
    }

    // SAFETY: the block holds a null-terminated wide string, which is what the format
    // guarantees, and the lock is released below.
    let text = unsafe { wide_to_string(locked.cast::<u16>()) };

    // SAFETY: matches the GlobalLock above.
    let _ = unsafe { GlobalUnlock(windows::Win32::Foundation::HGLOBAL(handle.0)) };
    Some(text)
}

/// # Safety
/// `start` must point at a null-terminated UTF-16 string.
unsafe fn wide_to_string(start: *const u16) -> String {
    let mut length = 0;

    // SAFETY: upheld by this function's contract.
    while unsafe { *start.add(length) } != 0 {
        length += 1;
    }

    // SAFETY: the length was just measured against the terminator.
    let slice = unsafe { std::slice::from_raw_parts(start, length) };
    String::from_utf16_lossy(slice)
}

/// Kept out of a clipboard handle's way: `HANDLE` and `HGLOBAL` are the same value here,
/// which is why the conversions above are plain field reads.
const _: () = {
    assert!(size_of::<HANDLE>() == size_of::<windows::Win32::Foundation::HGLOBAL>());
};
