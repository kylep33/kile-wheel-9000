//! Picking an item's icon from the pictures in the config's icons folder, by looking at
//! them rather than by typing a numbered file name.
//!
//! The folder is scanned when the picker opens, and every icon is rendered through the
//! shell, the same way the wheel renders it, so what you pick is what the wheel shows.
//! Rendering is spread over a few frames so opening a folder of hundreds never stalls.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use kile_wheel_platform::icons::{IconCache, IconImage};

/// Picture types the wheel can show, matching the Browse dialog's filter.
const IMAGE_EXTENSIONS: &[&str] = &["ico", "png", "jpg", "jpeg", "bmp", "gif", "webp"];

/// Rendered at the size the wheel uses on a typical display, so a pick looks the same.
const RENDER_PX: u32 = 48;
const TILE_PX: f32 = 40.0;

/// How many icons are rendered per frame. Enough to fill the grid in well under a second
/// without a frame long enough to notice.
const RENDERS_PER_FRAME: usize = 24;

/// The icons found in the folder and whatever has been rendered so far.
#[derive(Default)]
pub struct IconPicker {
    open: bool,
    /// Which field asked, so a page with several icon fields only fills the right one.
    owner: String,
    filter: String,
    files: Vec<PathBuf>,
    textures: HashMap<PathBuf, Option<egui::TextureHandle>>,
    shell: IconCache,
}

impl std::fmt::Debug for IconPicker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("IconPicker")
            .field("open", &self.open)
            .field("files", &self.files.len())
            .field("rendered", &self.textures.len())
            .finish_non_exhaustive()
    }
}

impl IconPicker {
    /// Rescans on every open, so icons dropped into the folder while the window is up show
    /// without a restart.
    pub fn open(&mut self, icons_dir: &Path, owner: &str) {
        self.files = scan(icons_dir);
        self.filter.clear();
        self.owner = owner.to_owned();
        self.open = true;
    }

    /// Draws the picker if `owner` opened it, and reports what was chosen.
    pub fn show(&mut self, ctx: &egui::Context, config_root: &Path, owner: &str) -> Choice {
        if !self.open || self.owner != owner {
            return Choice::Nothing;
        }

        self.render_some(ctx);

        let mut choice = Choice::Nothing;
        let modal = egui::Modal::new(egui::Id::new("icon_picker")).show(ctx, |ui| {
            ui.set_width(560.0);
            ui.heading("Choose an icon");
            ui.horizontal(|ui| {
                ui.label("Filter");
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text("file name or folder")
                        .desired_width(200.0),
                );
                ui.weak(format!("{} icons", self.files.len()));
            });
            ui.add_space(6.0);

            if self.files.is_empty() {
                ui.label("No pictures in the icons folder yet.");
                ui.weak("Put .ico or .png files in it, then open this again.");
            }

            egui::ScrollArea::vertical()
                .max_height(380.0)
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                        for file in self.visible_files() {
                            if self.tile(ui, file, config_root).clicked() {
                                choice = Choice::Picked(relative_to(file, config_root));
                            }
                        }
                    });
                });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Use the app's own icon").clicked() {
                    choice = Choice::Cleared;
                }
                if ui.button("Cancel").clicked() {
                    choice = Choice::Cancelled;
                }
            });
        });

        if modal.should_close() && choice == Choice::Nothing {
            choice = Choice::Cancelled;
        }
        if choice != Choice::Nothing {
            self.open = false;
        }
        choice
    }

    fn visible_files(&self) -> Vec<&PathBuf> {
        let filter = self.filter.trim().to_lowercase();
        self.files
            .iter()
            .filter(|file| {
                filter.is_empty() || file.to_string_lossy().to_lowercase().contains(&filter)
            })
            .collect()
    }

    fn tile(&self, ui: &mut egui::Ui, file: &Path, config_root: &Path) -> egui::Response {
        let hover = relative_to(file, config_root);
        let size = egui::Vec2::splat(TILE_PX);

        let response = match self.textures.get(file) {
            Some(Some(texture)) => ui.add(
                egui::Button::image(egui::Image::from_texture(texture).fit_to_exact_size(size))
                    .min_size(size + egui::Vec2::splat(8.0)),
            ),
            // not rendered yet, or the shell could not draw it
            Some(None) => ui.add(egui::Button::new("?").min_size(size + egui::Vec2::splat(8.0))),
            None => ui.add(egui::Button::new("…").min_size(size + egui::Vec2::splat(8.0))),
        };
        response.on_hover_text(hover)
    }

    /// Renders the next few icons that do not have a texture yet, and asks for another frame
    /// until they all do.
    fn render_some(&mut self, ctx: &egui::Context) {
        let pending: Vec<PathBuf> = self
            .files
            .iter()
            .filter(|file| !self.textures.contains_key(*file))
            .take(RENDERS_PER_FRAME)
            .cloned()
            .collect();

        for file in &pending {
            let texture = self
                .shell
                .get(&file.to_string_lossy(), RENDER_PX)
                .map(to_colour_image)
                .map(|image| {
                    ctx.load_texture(file.to_string_lossy(), image, egui::TextureOptions::LINEAR)
                });
            // the cache only needs to live long enough to hand the pixels over
            self.shell.forget(&file.to_string_lossy());
            self.textures.insert(file.clone(), texture);
        }

        if !pending.is_empty() {
            ctx.request_repaint();
        }
    }

    /// A small preview of the configured icon, for beside the field.
    pub fn preview(
        &mut self,
        ctx: &egui::Context,
        icon: &str,
        config_root: &Path,
    ) -> Option<egui::TextureHandle> {
        if icon.trim().is_empty() {
            return None;
        }
        let path = resolve(icon, config_root);
        if !self.textures.contains_key(&path) {
            let texture = self
                .shell
                .get(&path.to_string_lossy(), RENDER_PX)
                .map(to_colour_image)
                .map(|image| {
                    ctx.load_texture(path.to_string_lossy(), image, egui::TextureOptions::LINEAR)
                });
            self.shell.forget(&path.to_string_lossy());
            self.textures.insert(path.clone(), texture);
        }
        self.textures.get(&path).cloned().flatten()
    }
}

/// What the picker decided this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    Nothing,
    Cancelled,
    /// Back to the icon Windows has for the target.
    Cleared,
    /// Relative to the config folder when inside it, so the folder stays portable.
    Picked(String),
}

/// Every picture in the folder and its subfolders, sorted so numbered packs read in order.
fn scan(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    collect(dir, &mut found);
    found.sort_by_key(|left| natural_key(left));
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

fn is_picture(path: &Path) -> bool {
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

fn resolve(icon: &str, config_root: &Path) -> PathBuf {
    let path = Path::new(icon);
    match path.is_absolute() {
        true => path.to_path_buf(),
        false => config_root.join(path),
    }
}

/// The shell gives premultiplied BGRA; egui wants premultiplied RGBA.
fn to_colour_image(image: &IconImage) -> egui::ColorImage {
    let rgba: Vec<u8> = image
        .bgra
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|pixel| [pixel[2], pixel[1], pixel[0], pixel[3]])
        .collect();
    egui::ColorImage::from_rgba_premultiplied([image.width as usize, image.height as usize], &rgba)
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
        files.sort_by_key(|left| natural_key(left));

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

    /// The config is meant to move between machines, so a pick inside its folder is stored
    /// relative to it.
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
    fn the_pixel_order_is_swapped_to_rgba() {
        let image = IconImage {
            width: 1,
            height: 1,
            bgra: vec![1, 2, 3, 4],
        };

        let colour = to_colour_image(&image);

        assert_eq!(colour.pixels[0].to_array(), [3, 2, 1, 4]);
    }

    /// Scanning reads the real folder; whatever is there, it must not panic.
    #[test]
    fn scanning_a_missing_folder_finds_nothing() {
        assert!(scan(Path::new(r"C:\definitely\not\a\folder")).is_empty());
    }
}
