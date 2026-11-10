//! Turning a configured item into something the OS can run.

use kile_wheel_core::item::{ItemKind, expand_exec};
use thiserror::Error;

use crate::paths::expand;
use crate::process::{self, Command, ProcessError};
use crate::sys;

#[derive(Debug, Error)]
pub enum LaunchError {
    #[error(transparent)]
    Process(#[from] ProcessError),

    #[error(
        "the exec template for \"{exec}\" names no program; it should start with something runnable, followed by \"{{path}}\""
    )]
    EmptyExec { exec: String },

    #[error("a snippet is typed into the focused window rather than launched")]
    NotLaunchable,

    #[error("an unpinned app slot holds whatever is running, so there is nothing to launch")]
    NothingToLaunch,
}

/// How a launch reaches the OS.
///
/// Anything with arguments has to be started as a program. Anything that is just a
/// target is handed to the desktop, which resolves urls, folders and apps the same way a
/// double-click does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Launch {
    Program(Command),
    Open(String),
}

/// Runs everything except an app item that is already open, which the wheel handles
/// itself because it needs the window list.
pub fn run(kind: &ItemKind) -> Result<(), LaunchError> {
    match plan(kind)? {
        Launch::Program(command) => process::execute(&command).map_err(LaunchError::from),
        Launch::Open(target) => process::open(&target).map_err(LaunchError::from),
    }
}

fn plan(kind: &ItemKind) -> Result<Launch, LaunchError> {
    match kind {
        ItemKind::App { target } => Ok(sys::launch::app(&expand(target))),
        ItemKind::Script {
            path,
            exec,
            cwd,
            hidden,
        } => script(path, exec, cwd.as_deref(), *hidden),
        ItemKind::Url {
            url,
            browser,
            browser_args,
        } => Ok(url_launch(url, browser.as_deref(), browser_args.as_deref())),
        ItemKind::Folder { path } => Ok(Launch::Open(expand(path))),
        ItemKind::Terminal {
            profile,
            distro,
            cwd,
        } => Ok(sys::launch::terminal(
            non_blank(profile.as_deref()),
            non_blank(distro.as_deref()),
            non_blank(cwd.as_deref()),
        )),
        ItemKind::Snippet { .. } => Err(LaunchError::NotLaunchable),
        ItemKind::UnpinnedApp => Err(LaunchError::NothingToLaunch),
    }
}

/// The exec template is a whole command line, so the program is split off the front and
/// the rest is passed through as arguments.
fn script(path: &str, exec: &str, cwd: Option<&str>, hidden: bool) -> Result<Launch, LaunchError> {
    let line = expand_exec(exec, &expand(path));
    let (program, arguments) = split_program(&line).ok_or_else(|| LaunchError::EmptyExec {
        exec: exec.to_owned(),
    })?;

    Ok(Launch::Program(
        Command::new(program)
            .with_arguments(Some(arguments))
            .in_directory(cwd.map(expand))
            .hidden(hidden),
    ))
}

/// With no browser named the url goes to the desktop, which opens the default one.
fn url_launch(url: &str, browser: Option<&str>, browser_args: Option<&str>) -> Launch {
    let Some(browser) = non_blank(browser) else {
        return Launch::Open(url.to_owned());
    };

    let arguments = match non_blank(browser_args) {
        Some(args) => format!("{args} {url}"),
        None => url.to_owned(),
    };
    Launch::Program(Command::new(expand(browser)).with_arguments(Some(arguments)))
}

fn non_blank(text: Option<&str>) -> Option<&str> {
    text.filter(|text| !text.trim().is_empty())
}

/// Splits the leading program off a command line, honouring quotes around it.
fn split_program(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    if let Some(rest) = line.strip_prefix('"') {
        let (program, arguments) = rest.split_once('"')?;
        return Some((program.to_owned(), arguments.trim_start().to_owned()));
    }

    match line.split_once(char::is_whitespace) {
        Some((program, arguments)) => Some((program.to_owned(), arguments.trim_start().to_owned())),
        None => Some((line.to_owned(), String::new())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program_of(launch: &Launch) -> &str {
        match launch {
            Launch::Program(command) => &command.program,
            Launch::Open(target) => target,
        }
    }

    fn arguments_of(launch: &Launch) -> Option<&str> {
        match launch {
            Launch::Program(command) => command.arguments.as_deref(),
            Launch::Open(_) => None,
        }
    }

    #[test]
    fn a_bare_program_splits_with_no_arguments() {
        assert_eq!(
            split_program("bash"),
            Some(("bash".to_owned(), String::new()))
        );
    }

    #[test]
    fn a_quoted_program_keeps_its_spaces() {
        let line = r#""C:\Program Files\Git\bin\bash.exe" -lc "echo hi""#;

        let (program, arguments) = split_program(line).expect("splits");
        assert_eq!(program, r"C:\Program Files\Git\bin\bash.exe");
        assert_eq!(arguments, r#"-lc "echo hi""#);
    }

    #[test]
    fn an_unquoted_program_splits_at_the_first_space() {
        let (program, arguments) = split_program("wt.exe -- bash script.sh").expect("splits");

        assert_eq!(program, "wt.exe");
        assert_eq!(arguments, "-- bash script.sh");
    }

    #[test]
    fn an_empty_exec_template_is_rejected() {
        assert_eq!(split_program("   "), None);
    }

    #[test]
    fn a_folder_goes_to_the_desktop() {
        let folder = plan(&ItemKind::Folder {
            path: "/projects".to_owned(),
        })
        .expect("plans");

        assert_eq!(folder, Launch::Open("/projects".to_owned()));
    }

    #[test]
    fn a_script_substitutes_its_path_into_the_template() {
        let plan = script("/s/go.sh", r#"bash "{path}""#, None, false).expect("plans");

        assert_eq!(program_of(&plan), "bash");
        assert_eq!(arguments_of(&plan), Some(r#""/s/go.sh""#));
    }

    #[test]
    fn a_hidden_script_keeps_the_flag() {
        let plan = script("go.sh", r#"bash "{path}""#, None, true).expect("plans");

        let Launch::Program(command) = plan else {
            panic!("a script runs as a program");
        };
        assert!(command.hidden);
    }

    #[test]
    fn a_url_without_a_browser_goes_to_the_desktop() {
        let plan = url_launch("https://example.test", None, None);

        assert_eq!(plan, Launch::Open("https://example.test".to_owned()));
    }

    #[test]
    fn a_url_with_a_browser_passes_the_profile_args_first() {
        let plan = url_launch(
            "https://example.test",
            Some("chrome"),
            Some("--profile-directory=Work"),
        );

        assert_eq!(program_of(&plan), "chrome");
        assert_eq!(
            arguments_of(&plan),
            Some("--profile-directory=Work https://example.test")
        );
    }

    #[test]
    fn a_blank_browser_is_treated_as_unset() {
        let plan = url_launch("https://example.test", Some("  "), None);

        assert_eq!(plan, Launch::Open("https://example.test".to_owned()));
    }

    #[test]
    fn a_snippet_is_not_launchable() {
        let kind = ItemKind::Snippet {
            text: "hi".to_owned(),
            mode: Default::default(),
        };

        assert!(matches!(run(&kind), Err(LaunchError::NotLaunchable)));
    }
}
