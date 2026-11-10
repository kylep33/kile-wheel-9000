//! Windows Terminal, WSL, and Explorer as the way into everything else.

use crate::launch::Launch;
use crate::paths::expand;
use crate::process::Command;

const TERMINAL_EXE: &str = "wt.exe";
const WSL_EXE: &str = "wsl.exe";

fn quoted(text: &str) -> String {
    format!("\"{text}\"")
}

/// Explorer opens an exe, a shortcut or `shell:AppsFolder\<id>` as a double-click would.
pub fn app(target: &str) -> Launch {
    Launch::Open(target.to_owned())
}

pub fn terminal(profile: Option<&str>, distro: Option<&str>, cwd: Option<&str>) -> Launch {
    let mut arguments = Vec::new();

    if let Some(profile) = profile {
        arguments.push("-p".to_owned());
        arguments.push(quoted(profile));
    }

    match distro {
        // a wsl start directory is a linux path, so wsl takes it rather than wt
        Some(distro) => {
            arguments.push(WSL_EXE.to_owned());
            arguments.push("-d".to_owned());
            arguments.push(quoted(distro));
            if let Some(cwd) = cwd {
                arguments.push("--cd".to_owned());
                arguments.push(quoted(cwd));
            }
        }
        None => {
            if let Some(cwd) = cwd {
                arguments.push("-d".to_owned());
                arguments.push(quoted(&expand(cwd)));
            }
        }
    }

    Launch::Program(Command::new(TERMINAL_EXE).with_arguments(Some(arguments.join(" "))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments_of(launch: &Launch) -> Option<&str> {
        match launch {
            Launch::Program(command) => command.arguments.as_deref(),
            Launch::Open(_) => None,
        }
    }

    #[test]
    fn a_packaged_app_target_passes_through_untouched() {
        assert_eq!(
            app(r"shell:AppsFolder\Some.App_abc!App"),
            Launch::Open(r"shell:AppsFolder\Some.App_abc!App".to_owned())
        );
    }

    #[test]
    fn a_windows_terminal_profile_gets_a_windows_start_directory() {
        let plan = terminal(Some("PowerShell"), None, Some(r"C:\work"));

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

        let Launch::Program(command) = &plan else {
            panic!("a terminal runs as a program");
        };
        assert_eq!(command.program, TERMINAL_EXE);
        assert_eq!(command.arguments, None);
    }
}
