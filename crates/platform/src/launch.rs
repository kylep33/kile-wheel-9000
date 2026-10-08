//! Turning a configured item into something the shell can run.

use kile_wheel_core::item::{ItemKind, expand_exec};
use thiserror::Error;

use crate::paths::expand;
use crate::shell::{self, Command, ShellError};

const TERMINAL_EXE: &str = "wt.exe";
const WSL_EXE: &str = "wsl.exe";

#[derive(Debug, Error)]
pub enum LaunchError {
    #[error(transparent)]
    Shell(#[from] ShellError),

    #[error(
        "the exec template for \"{exec}\" names no program; it should start with something runnable, as in: wt.exe -- bash \"{{path}}\""
    )]
    EmptyExec { exec: String },

    #[error("a snippet is typed into the focused window rather than launched")]
    NotLaunchable,
}

/// How a launch reaches the shell.
///
/// Anything with arguments has to be started as a program. Anything that is just a
/// target is handed to Explorer, which resolves urls, folders, shortcuts and packaged
/// apps the same way a double-click does.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Launch {
    Program(Command),
    ShellItem(String),
}

/// Runs everything except an app item that is already open, which the wheel handles
/// itself because it needs the window list.
pub fn run(kind: &ItemKind) -> Result<(), LaunchError> {
    match plan(kind)? {
        Launch::Program(command) => shell::execute(&command).map_err(LaunchError::from),
        Launch::ShellItem(target) => shell::open_shell_item(&target).map_err(LaunchError::from),
    }
}

fn plan(kind: &ItemKind) -> Result<Launch, LaunchError> {
    match kind {
        ItemKind::App { target } => Ok(Launch::ShellItem(expand(target))),
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
        ItemKind::Folder { path } => Ok(Launch::ShellItem(expand(path))),
        ItemKind::Terminal {
            profile,
            distro,
            cwd,
        } => Ok(terminal(
            profile.as_deref(),
            distro.as_deref(),
            cwd.as_deref(),
        )),
        ItemKind::Snippet { .. } => Err(LaunchError::NotLaunchable),
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

/// With no browser named the url goes to Explorer, which opens the default one.
fn url_launch(url: &str, browser: Option<&str>, browser_args: Option<&str>) -> Launch {
    let Some(browser) = browser.filter(|name| !name.trim().is_empty()) else {
        return Launch::ShellItem(url.to_owned());
    };

    let arguments = match browser_args.filter(|args| !args.trim().is_empty()) {
        Some(args) => format!("{args} {url}"),
        None => url.to_owned(),
    };
    Launch::Program(Command::new(expand(browser)).with_arguments(Some(arguments)))
}

fn terminal(profile: Option<&str>, distro: Option<&str>, cwd: Option<&str>) -> Launch {
    let mut arguments = Vec::new();

    if let Some(profile) = profile.filter(|name| !name.trim().is_empty()) {
        arguments.push("-p".to_owned());
        arguments.push(quoted(profile));
    }

    match distro.filter(|name| !name.trim().is_empty()) {
        // a wsl start directory is a linux path, so wsl takes it rather than wt
        Some(distro) => {
            arguments.push(WSL_EXE.to_owned());
            arguments.push("-d".to_owned());
            arguments.push(quoted(distro));
            if let Some(cwd) = cwd.filter(|path| !path.trim().is_empty()) {
                arguments.push("--cd".to_owned());
                arguments.push(quoted(cwd));
            }
        }
        None => {
            if let Some(cwd) = cwd.filter(|path| !path.trim().is_empty()) {
                arguments.push("-d".to_owned());
                arguments.push(quoted(&expand(cwd)));
            }
        }
    }

    Launch::Program(Command::new(TERMINAL_EXE).with_arguments(Some(arguments.join(" "))))
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

fn quoted(text: &str) -> String {
    format!("\"{text}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program_of(launch: &Launch) -> &str {
        match launch {
            Launch::Program(command) => &command.program,
            Launch::ShellItem(target) => target,
        }
    }

    fn arguments_of(launch: &Launch) -> Option<&str> {
        match launch {
            Launch::Program(command) => command.arguments.as_deref(),
            Launch::ShellItem(_) => None,
        }
    }

    #[test]
    fn a_bare_program_splits_with_no_arguments() {
        assert_eq!(
            split_program("wt.exe"),
            Some(("wt.exe".to_owned(), String::new()))
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

    /// An app, a folder and a bare url all go to Explorer, because it already knows how
    /// to resolve every one of them.
    #[test]
    fn targets_without_arguments_go_to_explorer() {
        let app = plan(&ItemKind::App {
            target: r"C:\Apps\Code.exe".to_owned(),
        })
        .expect("plans");
        let folder = plan(&ItemKind::Folder {
            path: r"C:\projects".to_owned(),
        })
        .expect("plans");

        assert_eq!(app, Launch::ShellItem(r"C:\Apps\Code.exe".to_owned()));
        assert_eq!(folder, Launch::ShellItem(r"C:\projects".to_owned()));
    }

    #[test]
    fn a_packaged_app_target_passes_through_untouched() {
        let plan = plan(&ItemKind::App {
            target: r"shell:AppsFolder\Some.App_abc!App".to_owned(),
        })
        .expect("plans");

        assert_eq!(
            plan,
            Launch::ShellItem(r"shell:AppsFolder\Some.App_abc!App".to_owned())
        );
    }

    #[test]
    fn a_script_substitutes_its_path_into_the_template() {
        let plan = script(r"C:\s\go.sh", r#"wt.exe -- bash "{path}""#, None, false).expect("plans");

        assert_eq!(program_of(&plan), "wt.exe");
        assert_eq!(arguments_of(&plan), Some(r#"-- bash "C:\s\go.sh""#));
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
    fn a_url_without_a_browser_goes_to_explorer() {
        let plan = url_launch("https://example.test", None, None);

        assert_eq!(plan, Launch::ShellItem("https://example.test".to_owned()));
    }

    #[test]
    fn a_url_with_a_browser_passes_the_profile_args_first() {
        let plan = url_launch(
            "https://example.test",
            Some("chrome.exe"),
            Some("--profile-directory=Work"),
        );

        assert_eq!(program_of(&plan), "chrome.exe");
        assert_eq!(
            arguments_of(&plan),
            Some("--profile-directory=Work https://example.test")
        );
    }

    #[test]
    fn a_blank_browser_is_treated_as_unset() {
        let plan = url_launch("https://example.test", Some("  "), None);

        assert_eq!(plan, Launch::ShellItem("https://example.test".to_owned()));
    }

    #[test]
    fn a_windows_terminal_profile_gets_a_windows_start_directory() {
        let plan = terminal(Some("PowerShell"), None, Some(r"C:\work"));

        assert_eq!(program_of(&plan), TERMINAL_EXE);
        assert_eq!(arguments_of(&plan), Some(r#"-p "PowerShell" -d "C:\work""#));
    }

    #[test]
    fn a_wsl_profile_hands_the_start_directory_to_wsl() {
        let plan = terminal(Some("Ubuntu"), Some("Ubuntu"), Some("~/ws"));

        assert_eq!(
            arguments_of(&plan),
            Some(r#"-p "Ubuntu" wsl.exe -d "Ubuntu" --cd "~/ws""#)
        );
    }

    #[test]
    fn a_terminal_with_nothing_set_still_opens() {
        let plan = terminal(None, None, None);

        assert_eq!(program_of(&plan), TERMINAL_EXE);
        assert_eq!(arguments_of(&plan), None);
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
