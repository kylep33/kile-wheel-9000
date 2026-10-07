//! Path expansion, so a config can be carried between machines.

use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::core::HSTRING;

/// Long enough for any expanded path; a longer result is reported rather than truncated.
const MAX_EXPANDED_LEN: usize = 2048;

/// Expands `%USERPROFILE%` and friends.
///
/// Returns the text unchanged when there is nothing to expand, or when the expansion
/// would not fit, because a truncated path is worse than the original.
pub fn expand(text: &str) -> String {
    if !text.contains('%') {
        return text.to_owned();
    }

    let source = HSTRING::from(text);
    let mut buffer = vec![0u16; MAX_EXPANDED_LEN];

    // SAFETY: the source and the buffer both outlive the call.
    let written = unsafe { ExpandEnvironmentStringsW(&source, Some(&mut buffer)) } as usize;

    // the count includes the terminator, and an over-long result reports the size needed
    if written == 0 || written > MAX_EXPANDED_LEN {
        return text.to_owned();
    }

    String::from_utf16_lossy(&buffer[..written - 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_without_a_percent_is_left_alone() {
        assert_eq!(expand(r"C:\tools\thing.exe"), r"C:\tools\thing.exe");
    }

    #[test]
    fn a_known_variable_expands() {
        let expanded = expand(r"%SystemRoot%\system32");

        assert!(
            expanded.to_lowercase().ends_with(r"\system32"),
            "{expanded}"
        );
        assert!(!expanded.contains('%'), "{expanded}");
    }

    #[test]
    fn an_unknown_variable_is_left_in_place() {
        let expanded = expand("%KILE_WHEEL_NOT_A_REAL_VARIABLE%");

        assert_eq!(expanded, "%KILE_WHEEL_NOT_A_REAL_VARIABLE%");
    }
}
