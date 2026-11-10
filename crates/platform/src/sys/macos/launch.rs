//! Everything goes through `open`, which is what Finder and the Dock use.

use super::super::unix::process::single_quoted;
use crate::launch::Launch;
use crate::paths::expand;
use crate::process::Command;

const OPEN: &str = "open";
const TERMINAL_APP: &str = "Terminal";

/// A path opens as an app; anything else is taken as a bundle id.
pub fn app(target: &str) -> Launch {
    let flag = match target.contains('/') {
        true => "-a",
        false => "-b",
    };
    Launch::Program(Command::new(OPEN).with_arguments(Some(format!("{flag} {}", single_quoted(target)))))
}

/// Profiles and distros are Windows Terminal ideas, so only the start folder applies.
pub fn terminal(_profile: Option<&str>, _distro: Option<&str>, cwd: Option<&str>) -> Launch {
    let arguments = match cwd {
        Some(cwd) => format!("-a {TERMINAL_APP} {}", single_quoted(&expand(cwd))),
        None => format!("-a {TERMINAL_APP}"),
    };
    Launch::Program(Command::new(OPEN).with_arguments(Some(arguments)))
}
