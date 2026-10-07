//! Window class names, and the classes that must never be treated as apps.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::GetClassNameW;

/// Win32 caps class names at 256 characters including the terminator.
const MAX_CLASS_NAME_LEN: usize = 256;

/// Shell surfaces that are windows but never taskbar entries.
const SHELL_CLASSES: &[&str] = &[
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "Windows.UI.Core.CoreWindow",
];

/// Packaged-app hosts, whose cloaked state means suspended rather than off-desktop.
const UWP_HOST_CLASSES: &[&str] = &["ApplicationFrameWindow"];

pub fn is_shell_class(window: HWND) -> bool {
    class_matches(window, SHELL_CLASSES)
}

pub fn is_uwp_host(window: HWND) -> bool {
    class_matches(window, UWP_HOST_CLASSES)
}

/// For logs only; the predicates above avoid the allocation.
pub fn class_name(window: HWND) -> Option<String> {
    let mut buffer = [0u16; MAX_CLASS_NAME_LEN];
    let written = read_class(window, &mut buffer)?;

    Some(String::from_utf16_lossy(written))
}

fn class_matches(window: HWND, candidates: &[&str]) -> bool {
    let mut buffer = [0u16; MAX_CLASS_NAME_LEN];
    let Some(class) = read_class(window, &mut buffer) else {
        return false;
    };

    candidates
        .iter()
        .any(|candidate| class.iter().copied().eq(candidate.encode_utf16()))
}

fn read_class(window: HWND, buffer: &mut [u16; MAX_CLASS_NAME_LEN]) -> Option<&[u16]> {
    // SAFETY: the slice is live for the call and its length bounds the write.
    let written = unsafe { GetClassNameW(window, buffer) };
    if written <= 0 {
        return None;
    }

    Some(&buffer[..written as usize])
}
