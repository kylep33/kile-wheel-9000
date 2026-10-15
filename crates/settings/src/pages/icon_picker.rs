//! Picking an item's icon from the pictures in the config's icons folder, by looking at
//! them rather than by typing a numbered file name.
//!
//! The folder is scanned when the picker opens, and every icon is rendered through the
//! shell, the same way the wheel renders it, so what you pick is what the wheel shows.
//! Rendering is spread over a few frames so opening a folder of hundreds never stalls.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use kile_wheel_platform::icon_files::{IMAGE_EXTENSIONS, is_picture, relative_to, resolve, scan};
use kile_wheel_platform::icons::{IconCache, IconImage};

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
    /// The path box at the top, seeded with the field's current value.
    path: String,
    /// The folder the grid and Browse start in.
    icons_dir: PathBuf,
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
    pub fn open(&mut self, icons_dir: &Path, owner: &str, current: &str) {
        self.files = scan(icons_dir);
        self.icons_dir = icons_dir.to_path_buf();
        self.filter.clear();
        self.path = current.to_owned();
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
            ui.heading("Icon");

            // a typed path or Browse, applied with Use or Enter
            ui.label("Path, or select below");
            ui.horizontal(|ui| {
                let field = ui.add(
                    egui::TextEdit::singleline(&mut self.path)
                        .hint_text(r"icons\something.ico, or a full path")
                        .desired_width(330.0),
                );
                let entered =
                    field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));

                if ui.button("Browse").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .set_directory(&self.icons_dir)
                        .add_filter("Images", IMAGE_EXTENSIONS)
                        .pick_file()
                {
                    self.path = relative_to(&path, config_root);
                }

                let typed = self.path.trim();
                if ui
                    .add_enabled(!typed.is_empty(), egui::Button::new("Use"))
                    .clicked()
                    || (entered && !typed.is_empty())
                {
                    choice = Choice::Picked(typed.to_owned());
                }
            });
            if let Some(problem) = path_problem(&self.path, config_root) {
                ui.colored_label(ui.visuals().warn_fg_color, problem);
            }

            ui.add_space(6.0);
            ui.separator();
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
            None => ui.add(egui::Button::new("â€¦").min_size(size + egui::Vec2::splat(8.0))),
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

/// Says when a typed path points at nothing, so a typo is caught before Use rather than
/// showing up later as a blank icon on the wheel. Blank is not a problem: it is how the
/// box starts.
fn path_problem(path: &str, config_root: &Path) -> Option<&'static str> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }

    let resolved = resolve(path, config_root);
    if !resolved.is_file() {
        return Some("No file there yet. Use it anyway, or pick one below.");
    }
    if !is_picture(&resolved) {
        return Some("That is not a picture the wheel can show.");
    }
    None
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
    fn the_pixel_order_is_swapped_to_rgba() {
        let image = IconImage {
            width: 1,
            height: 1,
            bgra: vec![1, 2, 3, 4],
        };

        let colour = to_colour_image(&image);

        assert_eq!(colour.pixels[0].to_array(), [3, 2, 1, 4]);
    }

    #[test]
    fn a_blank_path_is_not_a_problem() {
        assert_eq!(path_problem("   ", Path::new(r"C:\config")), None);
    }

    #[test]
    fn a_path_to_nothing_says_so() {
        let problem = path_problem(r"icons\nope\missing.ico", Path::new(r"C:\definitely\not"));

        assert!(problem.is_some_and(|text| text.starts_with("No file")));
    }

    #[test]
    fn a_real_file_that_is_not_a_picture_says_so() {
        let windows = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_owned());
        let notepad = Path::new(&windows).join("notepad.exe");
        if !notepad.is_file() {
            return;
        }

        let problem = path_problem(&notepad.to_string_lossy(), Path::new(r"C:\config"));

        assert!(problem.is_some_and(|text| text.contains("not a picture")));
    }
}
