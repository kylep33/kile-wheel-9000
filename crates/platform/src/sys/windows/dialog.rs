//! Modal prompts, for the few choices that must not happen by accident.

use windows::Win32::UI::WindowsAndMessaging::{
    IDYES, MB_ICONQUESTION, MB_SETFOREGROUND, MB_TOPMOST, MB_YESNO, MessageBoxW,
};
use windows::core::HSTRING;

/// Yes or no, taking no for an answer if the dialog could not be shown.
pub fn confirm(title: &str, message: &str) -> bool {
    let title = HSTRING::from(title);
    let message = HSTRING::from(message);

    // SAFETY: both strings stay live for the duration of the call, and no owner window is
    // given because the only caller's windows are all non-activating.
    let answer = unsafe {
        MessageBoxW(
            None,
            &message,
            &title,
            MB_YESNO | MB_ICONQUESTION | MB_SETFOREGROUND | MB_TOPMOST,
        )
    };

    answer == IDYES
}
