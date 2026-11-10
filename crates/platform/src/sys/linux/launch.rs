//! Desktop entries through `gio launch`, which handles field codes, D-Bus activation and
//! startup notification the way the dash does; plain programs run as themselves.

use super::desktop;
use super::super::unix::process::single_quoted;
use crate::launch::Launch;
use crate::paths::expand;
use crate::process::Command;

const GIO: &str = "gio";

/// The usual names of the default terminal, best first. Ubuntu points
/// `x-terminal-emulator` at whatever it ships.
const TERMINALS: &[&str] = &[
    "x-terminal-emulator",
    "xdg-terminal-exec",
    "ptyxis",
    "gnome-terminal",
    "kgx",
    "konsole",
    "xterm",
];

pub fn app(target: &str) -> Launch {
    if !desktop::is_desktop_target(target) {
        return Launch::Program(Command::new(target));
    }

    // an id that is not installed still goes to gio, whose error says what is missing
    let path = desktop::find(target)
        .map(|entry| entry.path.to_string_lossy().into_owned())
        .unwrap_or_else(|| target.to_owned());
    Launch::Program(Command::new(GIO).with_arguments(Some(format!("launch {}", single_quoted(&path)))))
}

/// Profiles and distros are Windows Terminal ideas, so only the start folder applies.
pub fn terminal(_profile: Option<&str>, _distro: Option<&str>, cwd: Option<&str>) -> Launch {
    let terminal = TERMINALS
        .iter()
        .find(|name| desktop::resolve_program(name).is_some())
        .unwrap_or(&TERMINALS[0]);

    Launch::Program(Command::new(*terminal).in_directory(cwd.map(expand)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_runs_as_itself() {
        assert_eq!(app("/usr/bin/foot"), Launch::Program(Command::new("/usr/bin/foot")));
    }

    #[test]
    fn a_desktop_entry_goes_through_gio() {
        let Launch::Program(command) = app("not-installed.desktop") else {
            panic!("a desktop entry runs as a program");
        };

        assert_eq!(command.program, GIO);
        assert_eq!(command.arguments.as_deref(), Some("launch 'not-installed.desktop'"));
    }

    #[test]
    fn a_terminal_starts_in_its_folder() {
        let Launch::Program(command) = terminal(Some("ignored"), None, Some("/tmp")) else {
            panic!("a terminal runs as a program");
        };

        assert_eq!(command.working_dir.as_deref(), Some("/tmp"));
    }
}
