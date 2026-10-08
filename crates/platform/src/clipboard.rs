//! Reading text off the clipboard, so Ctrl+V works in the wheel's own fields, and
//! writing it, which is how a snippet reaches the window that had focus.

use thiserror::Error;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;

#[derive(Debug, Error)]
pub enum ClipboardError {
    #[error("another process is holding the clipboard open")]
    Busy(#[source] windows::core::Error),

    #[error("the clipboard would not take the text")]
    Refused(#[source] windows::core::Error),

    #[error("could not reserve {bytes} bytes for the clipboard")]
    OutOfMemory { bytes: usize },
}

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
    let locked = unsafe { GlobalLock(HGLOBAL(handle.0)) };
    if locked.is_null() {
        return None;
    }

    // SAFETY: the block holds a null-terminated wide string, which is what the format
    // guarantees, and the lock is released below.
    let text = unsafe { wide_to_string(locked.cast::<u16>()) };

    // SAFETY: matches the GlobalLock above.
    let _ = unsafe { GlobalUnlock(HGLOBAL(handle.0)) };
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
    assert!(size_of::<HANDLE>() == size_of::<HGLOBAL>());
};

/// Replaces the clipboard with `text`. The block handed to `SetClipboardData` becomes the
/// clipboard's to free, so it is deliberately not freed here once the call succeeds.
pub fn write_text(text: &str) -> Result<(), ClipboardError> {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = std::mem::size_of_val(wide.as_slice());

    // SAFETY: a null owner is allowed, and the clipboard is closed on every path below.
    unsafe { OpenClipboard(None) }.map_err(ClipboardError::Busy)?;

    let result = write_while_open(&wide, bytes);

    // SAFETY: matches the OpenClipboard above.
    let _ = unsafe { CloseClipboard() };
    result
}

fn write_while_open(wide: &[u16], bytes: usize) -> Result<(), ClipboardError> {
    // SAFETY: emptying is required before taking ownership of the clipboard.
    unsafe { EmptyClipboard() }.map_err(ClipboardError::Refused)?;

    // SAFETY: moveable is what CF_UNICODETEXT requires, and the block is freed below on
    // every path that does not hand it to the clipboard.
    let block = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes) }
        .map_err(|_| ClipboardError::OutOfMemory { bytes })?;

    // SAFETY: a moveable block has to be locked before the pointer is usable.
    let locked = unsafe { GlobalLock(block) };
    if locked.is_null() {
        // SAFETY: nothing else holds the block, so this is the only owner.
        let _ = unsafe { GlobalFree(Some(block)) };
        return Err(ClipboardError::OutOfMemory { bytes });
    }

    // SAFETY: the block was allocated at exactly this size, and the lock is held.
    unsafe { std::ptr::copy_nonoverlapping(wide.as_ptr(), locked.cast::<u16>(), wide.len()) };

    // SAFETY: matches the GlobalLock above.
    let _ = unsafe { GlobalUnlock(block) };

    // SAFETY: ownership of the block passes to the clipboard here.
    match unsafe { SetClipboardData(CF_UNICODETEXT.0.into(), Some(HANDLE(block.0))) } {
        Ok(_) => Ok(()),
        Err(error) => {
            // SAFETY: the clipboard refused it, so this is still the only owner.
            let _ = unsafe { GlobalFree(Some(block)) };
            Err(ClipboardError::Refused(error))
        }
    }
}
