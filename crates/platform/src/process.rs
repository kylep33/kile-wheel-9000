//! Starting programs and opening things the way a double-click would.

use thiserror::Error;

use crate::sys;

/// What to run. `arguments` is one command line, as the user typed it, so quoting works
/// the way it does in the OS's own shell.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Command {
    pub program: String,
    pub arguments: Option<String>,
    pub working_dir: Option<String>,
    /// Starts with no window, for a script that only needs to do its work.
    pub hidden: bool,
}

impl Command {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            ..Self::default()
        }
    }

    pub fn with_arguments(mut self, arguments: Option<String>) -> Self {
        self.arguments = arguments.filter(|text| !text.trim().is_empty());
        self
    }

    pub fn in_directory(mut self, working_dir: Option<String>) -> Self {
        self.working_dir = working_dir.filter(|text| !text.trim().is_empty());
        self
    }

    pub fn hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
    }

    /// The full command line, with the program quoted so a path containing spaces stays
    /// one argument.
    pub fn line(&self) -> String {
        match &self.arguments {
            Some(arguments) => format!("\"{}\" {arguments}", self.program),
            None => format!("\"{}\"", self.program),
        }
    }
}

#[derive(Debug, Error)]
#[error("{0}")]
pub struct ProcessError(pub(crate) String);

/// Runs the command detached, so it outlives the wheel.
pub fn execute(command: &Command) -> Result<(), ProcessError> {
    sys::process::execute(command)
}

/// Hands a path, url or app to the desktop to open, exactly as a double-click would.
pub fn open(target: &str) -> Result<(), ProcessError> {
    sys::process::open(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_without_arguments_is_quoted_on_its_own() {
        let command = Command::new("/opt/My App/app");

        assert_eq!(command.line(), r#""/opt/My App/app""#);
    }

    #[test]
    fn arguments_follow_the_quoted_program() {
        let command = Command::new("wt.exe").with_arguments(Some("-p \"Ubuntu\"".to_owned()));

        assert_eq!(command.line(), r#""wt.exe" -p "Ubuntu""#);
    }

    #[test]
    fn blank_arguments_are_treated_as_absent() {
        let command = Command::new("app").with_arguments(Some("   ".to_owned()));

        assert_eq!(command.arguments, None);
        assert_eq!(command.line(), r#""app""#);
    }

    #[test]
    fn blank_working_directories_are_treated_as_absent() {
        let command = Command::new("app").in_directory(Some(String::new()));

        assert_eq!(command.working_dir, None);
    }
}
