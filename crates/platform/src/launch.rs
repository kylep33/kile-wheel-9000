//! Turning a configured item into something the shell can run.

use kile_wheel_core::item::{ItemKind, expand_exec};
use thiserror::Error;

use crate::paths::expand;
use crate::shell::{self, Command, ShellError};

const TERMINAL_EXE: &str = "wt.exe";
const EXPLORER_EXE: &str = "explorer.exe";
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

/// Runs everything except an app item, which the wheel handles itself because it needs
/// the window list to decide between launching, focusing and minimizing.
pub fn run(kind: &ItemKind) -> Result<(), LaunchError> {
    let command = match kind {
        ItemKind::App { target } => Command::new(expand(target)),
        ItemKind::Script {
            path,
            exec,
            cwd,
            hidden,
        } => script_command(path, exec, cwd.as_deref(), *hidden)?,
        ItemKind::Url {
            url,
            browser,
            browser_args,
        } => url_command(url, browser.as_deref(), browser_args.as_deref()),
        ItemKind::Folder { path } => folder_command(path),
        ItemKind::Terminal {
            profile,
            distro,
            cwd,
        } => terminal_command(profile.as_deref(), distro.as_deref(), cwd.as_deref()),
        ItemKind::Snippet { .. } => return Err(LaunchError::NotLaunchable),
    };

    shell::execute(&command).map_err(LaunchError::from)
}

/// The exec template is a whole command line, so the program is split off the front for
/// the shell and the rest is passed through as arguments.
fn script_command(
    path: &str,
    exec: &str,
    cwd: Option<&str>,
    hidden: bool,
) -> Result<Command, LaunchError> {
    let line = expand_exec(exec, &expand(path));
    let (program, arguments) = split_program(&line).ok_or_else(|| LaunchError::EmptyExec {
        exec: exec.to_owned(),
    })?;

    Ok(Command::new(program)
        .with_arguments(Some(arguments))
        .in_directory(cwd.map(expand))
        .hidden(hidden))
}

/// With no browser named the url goes to the shell, which opens the default one.
fn url_command(url: &str, browser: Option<&str>, browser_args: Option<&str>) -> Command {
    let Some(browser) = browser.filter(|name| !name.trim().is_empty()) else {
        return Command::new(url);
    };

    let arguments = match browser_args.filter(|args| !args.trim().is_empty()) {
        Some(args) => format!("{args} {url}"),
        None => url.to_owned(),
    };
    Command::new(expand(browser)).with_arguments(Some(arguments))
}

/// Explorer is named rather than opening the folder directly, so a folder that happens to
/// have a shell handler still lands in a file window.
fn folder_command(path: &str) -> Command {
    Command::new(EXPLORER_EXE).with_arguments(Some(quoted(&expand(path))))
}

fn terminal_command(profile: Option<&str>, distro: Option<&str>, cwd: Option<&str>) -> Command {
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

    Command::new(TERMINAL_EXE).with_arguments(Some(arguments.join(" ")))
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

    #[test]
    fn a_script_substitutes_its_path_into_the_template() {
        let command = script_command(r"C:\s\go.sh", r#"wt.exe -- bash "{path}""#, None, false)
            .expect("builds");

        assert_eq!(command.file, "wt.exe");
        assert_eq!(
            command.arguments.as_deref(),
            Some(r#"-- bash "C:\s\go.sh""#)
        );
        assert!(!command.hidden);
    }

    #[test]
    fn a_hidden_script_keeps_the_flag() {
        let command = script_command("go.sh", r#"bash "{path}""#, None, true).expect("builds");

        assert!(command.hidden);
    }

    #[test]
    fn a_url_without_a_browser_goes_to_the_shell() {
        let command = url_command("https://example.test", None, None);

        assert_eq!(command.file, "https://example.test");
        assert_eq!(command.arguments, None);
    }

    #[test]
    fn a_url_with_a_browser_passes_the_profile_args_first() {
        let command = url_command(
            "https://example.test",
            Some("chrome.exe"),
            Some("--profile-directory=Work"),
        );

        assert_eq!(command.file, "chrome.exe");
        assert_eq!(
            command.arguments.as_deref(),
            Some("--profile-directory=Work https://example.test")
        );
    }

    #[test]
    fn a_blank_browser_is_treated_as_unset() {
        let command = url_command("https://example.test", Some("  "), None);

        assert_eq!(command.file, "https://example.test");
    }

    #[test]
    fn a_windows_terminal_profile_gets_a_windows_start_directory() {
        let command = terminal_command(Some("PowerShell"), None, Some(r"C:\work"));

        assert_eq!(command.file, TERMINAL_EXE);
        assert_eq!(
            command.arguments.as_deref(),
            Some(r#"-p "PowerShell" -d "C:\work""#)
        );
    }

    #[test]
    fn a_wsl_profile_hands_the_start_directory_to_wsl() {
        let command = terminal_command(Some("Ubuntu"), Some("Ubuntu"), Some("~/ws"));

        assert_eq!(
            command.arguments.as_deref(),
            Some(r#"-p "Ubuntu" wsl.exe -d "Ubuntu" --cd "~/ws""#)
        );
    }

    #[test]
    fn a_terminal_with_nothing_set_still_opens() {
        let command = terminal_command(None, None, None);

        assert_eq!(command.file, TERMINAL_EXE);
        assert_eq!(command.arguments, None);
    }

    #[test]
    fn a_folder_is_opened_through_explorer() {
        let command = folder_command(r"C:\projects");

        assert_eq!(command.file, EXPLORER_EXE);
        assert_eq!(command.arguments.as_deref(), Some(r#""C:\projects""#));
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
