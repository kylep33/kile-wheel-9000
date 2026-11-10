//! Where the config lives, and path expansion so a config can be carried between machines.
//!
//! Windows configs write `%USERPROFILE%`; Linux and macOS configs write `~` and `$HOME`.

use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
#[error(
    "could not find the per-user config folder; the OS did not report one, so set HOME (or log in normally) and try again"
)]
pub struct NoConfigHome;

/// `%APPDATA%` on Windows, `~/.config` on Linux, `~/Library/Application Support` on macOS.
/// The app's own folder goes under it.
pub fn config_home() -> Result<PathBuf, NoConfigHome> {
    dirs::config_dir().ok_or(NoConfigHome)
}

/// Expands environment variables in the OS's own syntax.
///
/// An unknown variable is left in place, because a half-expanded path that still names
/// its variable is easier to fix than one with a hole in it.
pub fn expand(text: &str) -> String {
    expand_with(text, |name| std::env::var(name).ok())
}

#[cfg(windows)]
fn expand_with(text: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    const MARK: char = '%';

    let mut expanded = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(MARK) {
        let after = &rest[start + 1..];
        let Some(length) = after.find(MARK) else {
            break;
        };
        let name = &after[..length];
        expanded.push_str(&rest[..start]);
        match lookup(name).filter(|_| !name.is_empty()) {
            Some(value) => expanded.push_str(&value),
            None => expanded.push_str(&rest[start..start + length + 2]),
        }
        rest = &after[length + 1..];
    }
    expanded.push_str(rest);
    expanded
}

#[cfg(not(windows))]
fn expand_with(text: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    const HOME: &str = "HOME";

    let mut expanded = String::with_capacity(text.len());
    let mut rest = text;

    // only a leading ~ means home, as in the shell
    if rest == "~" || rest.starts_with("~/") {
        match lookup(HOME) {
            Some(home) => expanded.push_str(&home),
            None => expanded.push('~'),
        }
        rest = &rest[1..];
    }

    while let Some(start) = rest.find('$') {
        expanded.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let (name, consumed) = variable_name(after);
        match lookup(name).filter(|_| !name.is_empty()) {
            Some(value) => expanded.push_str(&value),
            None => expanded.push_str(&rest[start..start + 1 + consumed]),
        }
        rest = &after[consumed..];
    }
    expanded.push_str(rest);
    expanded
}

/// The name after a `$`, as `NAME` or `{NAME}`, and how many bytes it took.
#[cfg(not(windows))]
fn variable_name(after: &str) -> (&str, usize) {
    if let Some(braced) = after.strip_prefix('{') {
        return match braced.find('}') {
            Some(end) => (&braced[..end], end + 2),
            None => ("", 0),
        };
    }

    let end = after
        .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .unwrap_or(after.len());
    (&after[..end], end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(name: &str) -> Option<String> {
        match name {
            "HOME" => Some("/home/me".to_owned()),
            "USERPROFILE" | "Thing" => Some(r"C:\Users\me".to_owned()),
            _ => None,
        }
    }

    #[test]
    fn text_without_a_variable_is_left_alone() {
        assert_eq!(expand_with("/tools/thing", fake), "/tools/thing");
    }

    #[cfg(windows)]
    #[test]
    fn a_known_variable_expands() {
        assert_eq!(
            expand_with(r"%USERPROFILE%\scripts", fake),
            r"C:\Users\me\scripts"
        );
    }

    #[cfg(windows)]
    #[test]
    fn an_unknown_variable_is_left_in_place() {
        assert_eq!(expand_with("%NOPE%\\x", fake), "%NOPE%\\x");
        assert_eq!(expand_with("50% off", fake), "50% off");
    }

    #[cfg(windows)]
    #[test]
    fn the_real_environment_is_read() {
        assert!(!expand(r"%SystemRoot%\system32").contains('%'));
    }

    #[cfg(not(windows))]
    #[test]
    fn a_leading_tilde_is_home() {
        assert_eq!(expand_with("~/scripts", fake), "/home/me/scripts");
        assert_eq!(expand_with("~", fake), "/home/me");
        assert_eq!(expand_with("a~/b", fake), "a~/b");
    }

    #[cfg(not(windows))]
    #[test]
    fn both_dollar_forms_expand() {
        assert_eq!(expand_with("$HOME/x", fake), "/home/me/x");
        assert_eq!(expand_with("${HOME}x", fake), "/home/mex");
    }

    #[cfg(not(windows))]
    #[test]
    fn an_unknown_variable_is_left_in_place() {
        assert_eq!(expand_with("$NOPE/x", fake), "$NOPE/x");
        assert_eq!(expand_with("cost $5", fake), "cost $5");
    }
}
