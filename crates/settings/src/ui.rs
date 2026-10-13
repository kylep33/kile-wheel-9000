//! The window shell: the page list, the save bar, and the keys that navigate.

use anyhow::Result;
use kile_wheel_core::names::ConfigPaths;
use kile_wheel_platform::sources;

use crate::draft::Draft;
use crate::pages::{self, Page};

const SIDEBAR_WIDTH_PX: f32 = 170.0;
const GAP_PX: f32 = 8.0;

/// Everything the window is holding while it is open.
#[derive(Debug)]
pub struct Settings {
    pub draft: Draft,
    page: Page,
    /// Where Backspace goes back to.
    history: Vec<Page>,
    pub status: Option<String>,
    /// Enumerated once, because neither list changes while the window is open.
    pub terminal_profiles: Vec<String>,
    pub wsl_distros: Vec<String>,
    pub editor: Option<pages::ItemEditor>,
    /// Which view the scroll offset belongs to.
    scrolled_view: View,
}

/// A page, plus whether the item editor is over it.
type View = (Page, bool);

impl Settings {
    pub fn load(paths: ConfigPaths) -> Result<Self> {
        Ok(Self {
            draft: Draft::load(paths)?,
            page: Page::General,
            history: Vec::new(),
            status: None,
            terminal_profiles: sources::terminal_profiles(),
            wsl_distros: sources::wsl_distros(),
            editor: None,
            scrolled_view: (Page::General, false),
        })
    }

    pub fn page(&self) -> Page {
        self.page
    }

    fn go_to(&mut self, page: Page) {
        if page == self.page {
            return;
        }

        self.history.push(self.page);
        self.page = page;
        self.editor = None;
    }

    /// Backspace: out of an item editor first, then back a page.
    fn go_back(&mut self) {
        if self.editor.take().is_some() {
            return;
        }
        if let Some(page) = self.history.pop() {
            self.page = page;
        }
    }

    fn save(&mut self) {
        self.status = Some(match self.draft.save() {
            Ok(()) => "Saved.".to_owned(),
            Err(error) => format!("{error:#}"),
        });
    }
}

impl eframe::App for Settings {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_keys(ui.ctx());

        egui::Panel::left("pages")
            .exact_size(SIDEBAR_WIDTH_PX)
            .resizable(false)
            .show(ui, |ui| self.draw_sidebar(ui));

        egui::Panel::bottom("save").show(ui, |ui| self.draw_save_bar(ui));

        egui::CentralPanel::default_margins().show(ui, |ui| self.draw_page(ui));
    }
}

impl Settings {
    fn handle_keys(&mut self, ctx: &egui::Context) {
        let (escape, backspace) = ctx.input(|input| {
            (
                input.key_pressed(egui::Key::Escape),
                input.key_pressed(egui::Key::Backspace),
            )
        });

        if escape {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        // only when nothing has keyboard focus, or Backspace could not delete characters
        let typing = ctx.memory(|memory| memory.focused().is_some());
        if backspace && !typing {
            self.go_back();
        }
    }

    /// A new view starts at the top, or a scrolled page leaves the next one off screen.
    fn draw_page(&mut self, ui: &mut egui::Ui) {
        let view = (self.page, self.editor.is_some());
        let mut area = egui::ScrollArea::vertical();

        if view != self.scrolled_view {
            area = area.vertical_scroll_offset(0.0);
        }
        self.scrolled_view = view;

        area.show(ui, |ui| pages::draw(self, ui));
    }

    fn draw_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(GAP_PX);
        for page in Page::ALL {
            if ui
                .selectable_label(self.page == page, page.title())
                .clicked()
            {
                self.go_to(page);
            }
        }
    }

    fn draw_save_bar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(GAP_PX / 2.0);
        ui.horizontal(|ui| {
            let problem = self.draft.problem();
            let dirty = self.draft.is_dirty();

            if ui
                .add_enabled(dirty && problem.is_none(), egui::Button::new("Save"))
                .clicked()
            {
                self.save();
            }
            if ui.add_enabled(dirty, egui::Button::new("Revert")).clicked() {
                self.draft.revert();
                self.status = Some("Reverted to the saved config.".to_owned());
            }

            // the reason Save is unavailable has to be visible, not guessed at
            if let Some(problem) = problem {
                ui.colored_label(ui.visuals().error_fg_color, problem);
                return;
            }
            if let Some(status) = &self.status {
                ui.label(status);
                return;
            }
            if dirty {
                ui.label("Unsaved changes.");
            }
        });
        ui.add_space(GAP_PX / 2.0);
    }
}
