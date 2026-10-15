//! The pictures in the config's icons folder, shared by the wheel and the settings window
//! so both offer the same list in the same order.

use std::path::{Path, PathBuf};

/// Picture types the shell can render as an icon.
pub const IMAGE_EXTENSIONS: &[&str] = &["ico", "png", "jpg", "jpeg", "bmp", "gif", "webp"];

/// Every picture in the folder and its subfolders, sorted so numbered packs read in order.
/// A missing folder is just an empty list, since having no icons is a normal setup.
pub fn scan(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    collect(dir, &mut found);
    found.sort_by_key(|path| natural_key(path));
    found
}

fn collect(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, found);
        } else if is_picture(&path) {
            found.push(path);
        }
    }
}

pub fn is_picture(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            IMAGE_EXTENSIONS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(extension))
        })
}

/// Sorts `2.ico` before `10.ico`, which plain text sorting gets backwards.
fn natural_key(path: &Path) -> (PathBuf, u64, String) {
    let folder = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let number = stem.parse::<u64>().unwrap_or(u64::MAX);
    (folder, number, stem)
}

/// The config stores icon paths relative to its own folder, so moving the folder to
/// another machine keeps every icon.
pub fn relative_to(path: &Path, config_root: &Path) -> String {
    match path.strip_prefix(config_root) {
        Ok(relative) => relative.to_string_lossy().into_owned(),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

/// The other half of [`relative_to`]: an absolute path passes through untouched.
pub fn resolve(icon: &str, config_root: &Path) -> PathBuf {
    let path = Path::new(icon);
    match path.is_absolute() {
        true => path.to_path_buf(),
        false => config_root.join(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_numbered_pack_sorts_by_number_not_by_text() {
        let mut files = [
            PathBuf::from(r"C:\icons\xp\10.ico"),
            PathBuf::from(r"C:\icons\xp\2.ico"),
            PathBuf::from(r"C:\icons\xp\100.ico"),
        ];
        files.sort_by_key(|path| natural_key(path));

        let names: Vec<_> = files.iter().filter_map(|file| file.file_stem()).collect();
        assert_eq!(names, ["2", "10", "100"]);
    }

    #[test]
    fn named_icons_follow_numbered_ones_in_the_same_folder() {
        assert!(natural_key(Path::new(r"C:\i\5.ico")) < natural_key(Path::new(r"C:\i\kiro.png")));
    }

    #[test]
    fn only_pictures_are_offered() {
        assert!(is_picture(Path::new("a.ICO")));
        assert!(is_picture(Path::new("a.png")));
        assert!(!is_picture(Path::new("desktop.ini")));
        assert!(!is_picture(Path::new("notes")));
    }

    #[test]
    fn a_pick_inside_the_config_folder_is_stored_relative() {
        let root = Path::new(r"C:\Users\me\AppData\Roaming\kile-wheel-9000");
        let pick = root.join(r"icons\xp\233.ico");

        assert_eq!(relative_to(&pick, root), r"icons\xp\233.ico");
        assert_eq!(resolve(r"icons\xp\233.ico", root), pick);
    }

    #[test]
    fn a_pick_outside_the_config_folder_stays_absolute() {
        let root = Path::new(r"C:\config");

        assert_eq!(
            relative_to(Path::new(r"D:\art\a.ico"), root),
            r"D:\art\a.ico"
        );
        assert_eq!(
            resolve(r"D:\art\a.ico", root),
            PathBuf::from(r"D:\art\a.ico")
        );
    }

    #[test]
    fn scanning_a_missing_folder_finds_nothing() {
        assert!(scan(Path::new(r"C:\definitely\not\a\folder")).is_empty());
    }
}
