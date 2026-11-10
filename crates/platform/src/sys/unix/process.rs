//! Launching through `sh`, so a command line in the config is read the way a terminal
//! would read it, quotes and all.

use std::os::unix::process::CommandExt;
use std::process::Stdio;

use crate::process::{Command, ProcessError};

const SHELL: &str = "/bin/sh";

/// Hands a path or url to the desktop's default handler.
#[cfg(target_os = "linux")]
const OPENER: &str = "xdg-open";
#[cfg(target_os = "macos")]
const OPENER: &str = "open";

/// Its own process group, so the child outlives the wheel and never shares its signals.
const NEW_PROCESS_GROUP: i32 = 0;

pub fn execute(command: &Command) -> Result<(), ProcessError> {
    let mut child = std::process::Command::new(SHELL);
    child
        .arg("-c")
        .arg(command.line())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(NEW_PROCESS_GROUP);
    if let Some(directory) = &command.working_dir {
        child.current_dir(directory);
    }

    let mut started = child.spawn().map_err(|source| {
        ProcessError(format!(
            "could not start \"{}\": {source}; check the program exists and the start folder is real",
            command.program
        ))
    })?;

    // reaped on a thread of its own, so a finished launch never lingers as a zombie
    std::thread::spawn(move || {
        let _ = started.wait();
    });
    Ok(())
}

pub fn open(target: &str) -> Result<(), ProcessError> {
    execute(&Command::new(OPENER).with_arguments(Some(single_quoted(target))))
}

/// Quotes for `sh`, where nothing inside single quotes is special except the quote.
pub(crate) fn single_quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quote_inside_survives_single_quoting() {
        assert_eq!(single_quoted("it's"), r"'it'\''s'");
    }

    #[test]
    fn spaces_and_dollars_stay_literal() {
        assert_eq!(single_quoted("/a b/$HOME"), "'/a b/$HOME'");
    }

    #[test]
    fn a_launch_runs_through_the_shell() {
        let marker = std::env::temp_dir().join(format!("kile-launch-{}", std::process::id()));
        let command = Command::new("touch").with_arguments(Some(single_quoted(&marker.to_string_lossy())));

        execute(&command).expect("sh starts");
        for _ in 0..50 {
            if marker.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(marker.exists());
        let _ = std::fs::remove_file(marker);
    }
}
