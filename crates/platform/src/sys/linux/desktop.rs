//! Desktop entries: how GNOME names, launches and draws an app.

use std::path::{Path, PathBuf};

const DESKTOP_SUFFIX: &str = ".desktop";
const ENTRY_GROUP: &str = "[Desktop Entry]";

/// The fields the wheel uses.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub name: Option<String>,
    pub icon: Option<String>,
    /// The program the entry runs, without its arguments and field codes.
    pub program: Option<String>,
}

/// Where entries and icons live, user first so a local override wins.
pub fn data_dirs() -> Vec<PathBuf> {
    const SYSTEM_DEFAULT: &str = "/usr/local/share:/usr/share";
    const SNAP_EXPORTS: &str = "/var/lib/snapd/desktop";
    const FLATPAK_SYSTEM: &str = "/var/lib/flatpak/exports/share";

    let mut dirs = Vec::new();
    if let Some(home) = dirs::data_dir() {
        dirs.push(home.join("flatpak/exports/share"));
        dirs.push(home);
    }
    let system = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| SYSTEM_DEFAULT.to_owned());
    dirs.extend(system.split(':').filter(|dir| !dir.is_empty()).map(PathBuf::from));
    dirs.push(PathBuf::from(SNAP_EXPORTS));
    dirs.push(PathBuf::from(FLATPAK_SYSTEM));
    dirs
}

pub fn is_desktop_target(target: &str) -> bool {
    target.ends_with(DESKTOP_SUFFIX)
}

/// The id GNOME tracks an entry by, which is its file name.
pub fn id_of(target: &str) -> &str {
    target.rsplit('/').next().unwrap_or(target)
}

/// Finds an entry by id or path.
pub fn find(target: &str) -> Option<Entry> {
    let path = Path::new(target);
    if path.is_absolute() {
        return load(path);
    }
    data_dirs()
        .into_iter()
        .map(|dir| dir.join("applications").join(id_of(target)))
        .find(|candidate| candidate.is_file())
        .and_then(|found| load(&found))
}

pub fn load(path: &Path) -> Option<Entry> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut entry = parse(&text);
    entry.path = path.to_owned();
    Some(entry)
}

fn parse(text: &str) -> Entry {
    let mut entry = Entry::default();
    let mut in_entry = false;

    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_entry = line == ENTRY_GROUP;
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().to_owned();
        match key.trim() {
            "Name" => entry.name = Some(value),
            "Icon" => entry.icon = Some(value),
            "Exec" => entry.program = program_of(&value),
            _ => {}
        }
    }
    entry
}

/// The first word of an Exec line, unquoted.
fn program_of(exec: &str) -> Option<String> {
    let exec = exec.trim();
    let program = match exec.strip_prefix('"') {
        Some(quoted) => quoted.split('"').next()?,
        None => exec.split_whitespace().next()?,
    };
    (!program.is_empty()).then(|| program.to_owned())
}

/// A bare program name resolved against PATH, as the desktop would run it.
pub fn resolve_program(program: &str) -> Option<PathBuf> {
    let path = Path::new(program);
    if path.is_absolute() {
        return Some(path.to_owned());
    }
    let search = std::env::var_os("PATH")?;
    std::env::split_paths(&search)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIREFOX: &str = "[Desktop Entry]\nName=Firefox\nExec=env BAMF=1 /snap/bin/firefox %u\nIcon=/snap/firefox/current/default256.png\n\n[Desktop Action new-window]\nName=New Window\nExec=/snap/bin/firefox --new-window\n";

    #[test]
    fn only_the_main_group_is_read() {
        let entry = parse(FIREFOX);

        assert_eq!(entry.name.as_deref(), Some("Firefox"));
        assert_eq!(entry.program.as_deref(), Some("env"));
        assert_eq!(
            entry.icon.as_deref(),
            Some("/snap/firefox/current/default256.png")
        );
    }

    #[test]
    fn a_quoted_program_keeps_its_spaces() {
        assert_eq!(
            program_of("\"/opt/My App/app\" --flag").as_deref(),
            Some("/opt/My App/app")
        );
    }

    #[test]
    fn the_id_is_the_file_name() {
        assert_eq!(id_of("/usr/share/applications/org.gnome.Nautilus.desktop"), "org.gnome.Nautilus.desktop");
        assert_eq!(id_of("firefox_firefox.desktop"), "firefox_firefox.desktop");
        assert!(is_desktop_target("x.desktop"));
        assert!(!is_desktop_target("/usr/bin/x"));
    }

    #[test]
    fn sh_is_found_on_the_path() {
        assert!(resolve_program("sh").is_some());
    }
}
