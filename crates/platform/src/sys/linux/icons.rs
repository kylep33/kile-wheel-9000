//! Icons from the freedesktop icon theme, the way the dash finds them.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use resvg::tiny_skia::{IntSize, Pixmap, Transform};
use resvg::usvg;

use super::desktop;
use crate::icons::{IconImage, decode_picture};

/// Themes to look in after the user's own, ending with the spec's mandatory fallback.
const FALLBACK_THEMES: &[&str] = &["Yaru", "Adwaita", "hicolor"];

/// Theme subfolders an app, folder or file icon can be in.
const CATEGORIES: &[&str] = &["apps", "places", "mimetypes", "devices", "legacy"];

/// Unsized icons, which may also sit loose in the pixmaps folder.
const PIXMAPS_DIR: &str = "/usr/share/pixmaps";

/// A size folder named this holds vectors, good at any size.
const SCALABLE: &str = "scalable";

const FOLDER_ICON: &str = "folder";
const SCRIPT_ICON: &str = "text-x-script";
const PROGRAM_ICON: &str = "application-x-executable";

pub fn extract(source: &str, size_px: u32) -> Option<IconImage> {
    let name = icon_name(source)?;
    let path = match Path::new(&name).is_absolute() {
        true => PathBuf::from(&name),
        false => find_in_themes(&name, size_px)?,
    };
    render(&path, size_px)
}

/// What to look up for a target: its entry's icon, or a generic one for its kind.
fn icon_name(source: &str) -> Option<String> {
    if desktop::is_desktop_target(source) {
        return desktop::find(source)?.icon;
    }
    if super::apps::is_untracked(source) {
        return None;
    }

    let path = Path::new(source);
    if path.is_dir() {
        return Some(FOLDER_ICON.to_owned());
    }
    let generic = match path.extension().and_then(|extension| extension.to_str()) {
        Some("sh" | "py" | "pl" | "rb") => SCRIPT_ICON,
        _ => PROGRAM_ICON,
    };
    Some(generic.to_owned())
}

/// The smallest icon at least as big as asked for, else the biggest there is.
fn find_in_themes(name: &str, size_px: u32) -> Option<PathBuf> {
    let roots: Vec<PathBuf> = desktop::data_dirs()
        .into_iter()
        .map(|dir| dir.join("icons"))
        .collect();

    for theme in themes() {
        let mut found: Vec<(u32, PathBuf)> = Vec::new();
        for root in &roots {
            collect_sizes(&root.join(theme), name, &mut found);
        }
        if let Some(best) = pick(found, size_px) {
            return Some(best);
        }
    }

    ["png", "svg"]
        .iter()
        .map(|extension| Path::new(PIXMAPS_DIR).join(format!("{name}.{extension}")))
        .find(|candidate| candidate.is_file())
}

fn collect_sizes(theme_dir: &Path, name: &str, found: &mut Vec<(u32, PathBuf)>) {
    let Ok(sizes) = std::fs::read_dir(theme_dir) else {
        return;
    };
    for size_dir in sizes.filter_map(Result::ok).map(|entry| entry.path()) {
        let Some(size) = folder_size(&size_dir) else {
            continue;
        };
        for category in CATEGORIES {
            for extension in ["png", "svg"] {
                let candidate = size_dir.join(category).join(format!("{name}.{extension}"));
                if candidate.is_file() {
                    found.push((size, candidate));
                }
            }
        }
    }
}

/// `48x48`, `256x256@2`, or `scalable`, as a pixel size.
fn folder_size(dir: &Path) -> Option<u32> {
    let name = dir.file_name()?.to_str()?;
    if name == SCALABLE {
        return Some(u32::MAX);
    }
    let (dimensions, multiplier) = match name.split_once('@') {
        Some((dimensions, scale)) => (dimensions, scale.parse::<u32>().ok()?),
        None => (name, 1),
    };
    let (width, height) = dimensions.split_once('x')?;
    let width = width.parse::<u32>().ok()?;
    (width == height.parse::<u32>().ok()?).then_some(width * multiplier)
}

fn pick(mut found: Vec<(u32, PathBuf)>, size_px: u32) -> Option<PathBuf> {
    found.sort_by_key(|(size, _)| *size);
    let fitting = found.iter().find(|(size, _)| *size >= size_px).cloned();
    fitting.or_else(|| found.pop()).map(|(_, path)| path)
}

/// The desktop's icon theme first, read once.
fn themes() -> Vec<&'static str> {
    static CURRENT: OnceLock<Option<String>> = OnceLock::new();
    let current = CURRENT.get_or_init(|| {
        let output = std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "icon-theme"])
            .output()
            .ok()?;
        let name = String::from_utf8_lossy(&output.stdout).trim().trim_matches('\'').to_owned();
        (!name.is_empty()).then_some(name)
    });

    let mut themes: Vec<&'static str> = current.as_deref().into_iter().collect();
    themes.extend(FALLBACK_THEMES.iter().filter(|theme| current.as_deref() != Some(**theme)));
    themes
}

fn render(path: &Path, size_px: u32) -> Option<IconImage> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("svg") => render_svg(path, size_px),
        _ => decode_picture(path, size_px),
    }
}

fn render_svg(path: &Path, size_px: u32) -> Option<IconImage> {
    let data = std::fs::read(path).ok()?;
    let tree = usvg::Tree::from_data(&data, &usvg::Options::default()).ok()?;

    let side = size_px.max(1);
    let mut pixmap = Pixmap::new(side, side)?;
    let natural = tree.size();
    let scale = side as f32 / natural.width().max(natural.height());
    resvg::render(&tree, Transform::from_scale(scale, scale), &mut pixmap.as_mut());

    let size = IntSize::from_wh(side, side)?;
    Some(IconImage {
        width: size.width(),
        height: size.height(),
        rgba: pixmap.take(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_folders_read_as_pixels() {
        assert_eq!(folder_size(Path::new("/i/48x48")), Some(48));
        assert_eq!(folder_size(Path::new("/i/256x256@2")), Some(512));
        assert_eq!(folder_size(Path::new("/i/scalable")), Some(u32::MAX));
        assert_eq!(folder_size(Path::new("/i/symbolic")), None);
    }

    #[test]
    fn the_smallest_big_enough_icon_wins() {
        let found = vec![
            (16, PathBuf::from("a")),
            (64, PathBuf::from("b")),
            (32, PathBuf::from("c")),
        ];

        assert_eq!(pick(found.clone(), 30), Some(PathBuf::from("c")));
        assert_eq!(pick(found, 128), Some(PathBuf::from("b")));
    }

    #[test]
    fn a_folder_gets_the_folder_icon() {
        assert_eq!(icon_name("/tmp").as_deref(), Some(FOLDER_ICON));
        assert_eq!(icon_name("window:xterm"), None);
    }
}
