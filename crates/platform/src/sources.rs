//! Lists the settings window offers to pick from.

use std::path::PathBuf;
use std::process::Command;

use crate::known_folders;

/// Windows Terminal keeps its settings under its packaged local state.
const TERMINAL_SETTINGS: &str =
    r"Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json";

const WSL_EXE: &str = "wsl.exe";

/// Suppresses the console window `wsl.exe` would otherwise flash up.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Windows Terminal profile names, in the order the file lists them.
///
/// Returns an empty list when Windows Terminal is not installed, because a missing
/// optional app is not an error worth stopping the settings window for.
pub fn terminal_profiles() -> Vec<String> {
    let Some(path) = terminal_settings_path() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    profile_names(&text)
}

/// Installed WSL distributions.
pub fn wsl_distros() -> Vec<String> {
    let Ok(output) = wsl_list() else {
        return Vec::new();
    };

    // wsl.exe writes UTF-16 to a pipe, which is why this is not read as UTF-8
    decode_utf16(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

fn wsl_list() -> std::io::Result<std::process::Output> {
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;

    let mut command = Command::new(WSL_EXE);
    command.args(["-l", "-q"]);

    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    command.output()
}

fn terminal_settings_path() -> Option<PathBuf> {
    let local = known_folders::local_app_data().ok()?;
    let path = local.join(TERMINAL_SETTINGS);

    path.exists().then_some(path)
}

/// Pulls profile names out of Windows Terminal's settings without a JSON dependency.
///
/// The file is JSONC, so a real parser would need comment handling; the names are all
/// that is wanted here, and they are always `"name": "..."` inside the profiles list.
fn profile_names(text: &str) -> Vec<String> {
    const NAME_KEY: &str = "\"name\"";

    let mut names = Vec::new();
    for (_, after) in text
        .match_indices(NAME_KEY)
        .map(|(at, _)| (at, &text[at..]))
    {
        let Some(value) = quoted_value(after.trim_start_matches(NAME_KEY)) else {
            continue;
        };
        if value.is_empty() || names.contains(&value) {
            continue;
        }
        names.push(value);
    }
    names
}

/// Reads the string that follows a `:` separator.
fn quoted_value(after_key: &str) -> Option<String> {
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let opened = after_colon.trim_start().strip_prefix('"')?;
    let (value, _) = opened.split_once('"')?;

    Some(value.to_owned())
}

fn decode_utf16(bytes: &[u8]) -> String {
    let (pairs, _) = bytes.as_chunks::<2>();
    let wide: Vec<u16> = pairs.iter().copied().map(u16::from_le_bytes).collect();

    String::from_utf16_lossy(&wide)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_names_come_out_in_file_order() {
        let text = r#"
        {
            "profiles": {
                "list": [
                    { "name": "Windows PowerShell", "guid": "{a}" },
                    { "name": "Ubuntu", "guid": "{b}" }
                ]
            }
        }
        "#;

        assert_eq!(profile_names(text), ["Windows PowerShell", "Ubuntu"]);
    }

    #[test]
    fn a_repeated_name_is_only_listed_once() {
        let text = r#"{ "name": "Ubuntu", "x": { "name": "Ubuntu" } }"#;

        assert_eq!(profile_names(text), ["Ubuntu"]);
    }

    #[test]
    fn odd_spacing_around_the_colon_still_parses() {
        let text = "{ \"name\"   :    \"Git Bash\" }";

        assert_eq!(profile_names(text), ["Git Bash"]);
    }

    #[test]
    fn a_name_key_without_a_string_value_is_skipped() {
        let text = r#"{ "name": 42, "other": { "name": "Real" } }"#;

        assert_eq!(profile_names(text), ["Real"]);
    }

    #[test]
    fn no_profiles_in_the_text_yields_nothing() {
        assert!(profile_names("{}").is_empty());
    }

    #[test]
    fn utf16_output_decodes_to_the_distro_names() {
        let encoded: Vec<u8> = "Ubuntu\r\nDebian\r\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();

        let decoded = decode_utf16(&encoded);
        let lines: Vec<&str> = decoded.lines().map(str::trim).collect();
        assert_eq!(lines, ["Ubuntu", "Debian"]);
    }
}
