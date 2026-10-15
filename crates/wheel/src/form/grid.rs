//! The icon picker inside the petal: a scrolling grid over the icons folder.
//!
//! The first tile is the app's own icon, which is what a blank icon means, so going back
//! to the default is one pick rather than clearing a field.

use kile_wheel_core::binding::{KeyCode, keys};
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;

use super::Petal;

pub const COLUMNS: usize = 6;
const VISIBLE_ROWS: usize = 4;
pub const VISIBLE_TILES: usize = COLUMNS * VISIBLE_ROWS;

/// Each grid row spans this many form rows, which keeps the tiles close to square.
const FORM_ROWS_PER_GRID_ROW: usize = 2;

/// The grid plus the line under it naming the highlighted icon.
pub const FORM_ROWS: usize = VISIBLE_ROWS * FORM_ROWS_PER_GRID_ROW + 1;

const OWN_ICON_TILE: usize = 0;
const OWN_ICON_CAPTION: &str = "The app's own icon";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tile<'a> {
    OwnIcon,
    /// As stored in the config, so usually relative to its folder.
    File(&'a str),
}

/// What a key or click in the grid decided.
#[derive(Debug, PartialEq, Eq)]
pub enum GridStep {
    Ignored,
    Redraw,
    /// `None` is the app's own icon.
    Picked(Option<String>),
    Back,
}

#[derive(Debug)]
pub struct IconGrid {
    files: Vec<String>,
    highlighted: usize,
    /// The grid row at the top of the view.
    first_row: usize,
}

impl IconGrid {
    /// Opens on the icon already set, so the picker starts where you are.
    pub fn new(files: Vec<String>, current: Option<&str>) -> Self {
        let highlighted = current
            .and_then(|current| {
                files
                    .iter()
                    .position(|file| file.eq_ignore_ascii_case(current))
            })
            .map_or(OWN_ICON_TILE, |index| index + 1);

        let mut grid = Self {
            files,
            highlighted,
            first_row: 0,
        };
        grid.keep_in_view();
        grid
    }

    /// The tiles on the visible page, in reading order.
    pub fn visible(&self) -> impl Iterator<Item = Tile<'_>> {
        let start = self.first_row * COLUMNS;
        (start..start + VISIBLE_TILES).map_while(|index| self.tile(index))
    }

    pub fn highlighted_in_view(&self) -> Option<usize> {
        self.highlighted
            .checked_sub(self.first_row * COLUMNS)
            .filter(|index| *index < VISIBLE_TILES)
    }

    /// Names the highlighted icon and where it sits, since the tiles have no labels.
    pub fn caption(&self) -> String {
        let name = match self.tile(self.highlighted) {
            Some(Tile::File(file)) => file,
            _ => OWN_ICON_CAPTION,
        };
        format!("{name}   {}/{}", self.highlighted + 1, self.len())
    }

    pub fn press(&mut self, code: KeyCode) -> GridStep {
        if code == keys::ENTER {
            return self.pick(self.highlighted);
        }
        if code == keys::BACKSPACE {
            return GridStep::Back;
        }
        let Some(step) = movement(code) else {
            return GridStep::Ignored;
        };

        let Some(target) = self.highlighted.checked_add_signed(step) else {
            return GridStep::Ignored;
        };
        self.highlighted = target.min(self.len() - 1);
        self.keep_in_view();
        GridStep::Redraw
    }

    /// `index` counts from the first visible tile.
    pub fn click(&mut self, index: usize) -> GridStep {
        let index = self.first_row * COLUMNS + index;
        if index >= self.len() {
            return GridStep::Ignored;
        }
        self.highlighted = index;
        self.pick(index)
    }

    /// Moves the view by whole rows, dragging the highlight along so Enter never picks
    /// something off screen.
    pub fn scroll(&mut self, rows: isize) {
        let last_first_row = self.rows().saturating_sub(VISIBLE_ROWS);
        self.first_row = self
            .first_row
            .saturating_add_signed(rows)
            .min(last_first_row);

        let start = self.first_row * COLUMNS;
        let end = (start + VISIBLE_TILES).min(self.len()) - 1;
        self.highlighted = self.highlighted.clamp(start, end);
    }

    fn len(&self) -> usize {
        self.files.len() + 1
    }

    fn rows(&self) -> usize {
        self.len().div_ceil(COLUMNS)
    }

    fn tile(&self, index: usize) -> Option<Tile<'_>> {
        match index {
            OWN_ICON_TILE => Some(Tile::OwnIcon),
            _ => self.files.get(index - 1).map(|file| Tile::File(file)),
        }
    }

    fn pick(&self, index: usize) -> GridStep {
        match self.tile(index) {
            Some(Tile::OwnIcon) => GridStep::Picked(None),
            Some(Tile::File(file)) => GridStep::Picked(Some(file.to_owned())),
            None => GridStep::Ignored,
        }
    }

    fn keep_in_view(&mut self) {
        let row = self.highlighted / COLUMNS;
        if row < self.first_row {
            self.first_row = row;
        }
        if row >= self.first_row + VISIBLE_ROWS {
            self.first_row = row + 1 - VISIBLE_ROWS;
        }
    }
}

/// How far a key moves the highlight, in tiles.
fn movement(code: KeyCode) -> Option<isize> {
    const ROW: isize = COLUMNS as isize;
    const PAGE: isize = VISIBLE_TILES as isize;

    match code {
        _ if code == keys::ARROW_LEFT || code == keys::A => Some(-1),
        _ if code == keys::ARROW_RIGHT || code == keys::D => Some(1),
        _ if code == keys::ARROW_UP || code == keys::W => Some(-ROW),
        _ if code == keys::ARROW_DOWN || code == keys::S => Some(ROW),
        _ if code == keys::PAGE_UP => Some(-PAGE),
        _ if code == keys::PAGE_DOWN => Some(PAGE),
        _ => None,
    }
}

/// The box tile `index` of the visible page sits in.
pub fn tile_bounds(petal: &Petal, index: usize) -> D2D_RECT_F {
    let (row, column) = (index / COLUMNS, index % COLUMNS);
    let first = petal.row_bounds(row * FORM_ROWS_PER_GRID_ROW);
    let last = petal.row_bounds((row + 1) * FORM_ROWS_PER_GRID_ROW - 1);
    let width = (first.right - first.left) / COLUMNS as f32;
    let left = first.left + width * column as f32;

    D2D_RECT_F {
        left,
        top: first.top,
        right: left + width,
        bottom: last.bottom,
    }
}

pub fn caption_bounds(petal: &Petal) -> D2D_RECT_F {
    petal.row_bounds(VISIBLE_ROWS * FORM_ROWS_PER_GRID_ROW)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(count: usize) -> Vec<String> {
        (1..=count)
            .map(|number| format!(r"icons\xp\{number}.ico"))
            .collect()
    }

    #[test]
    fn a_blank_icon_opens_on_the_apps_own() {
        let grid = IconGrid::new(files(3), None);

        assert_eq!(grid.highlighted_in_view(), Some(OWN_ICON_TILE));
        assert_eq!(grid.visible().next(), Some(Tile::OwnIcon));
    }

    #[test]
    fn a_set_icon_opens_on_itself_and_scrolls_to_it() {
        let grid = IconGrid::new(files(100), Some(r"ICONS\xp\60.ico"));

        assert!(grid.highlighted_in_view().is_some());
        assert!(grid.caption().starts_with(r"icons\xp\60.ico"));
    }

    #[test]
    fn enter_on_the_first_tile_goes_back_to_the_default() {
        let mut grid = IconGrid::new(files(3), Some(r"icons\xp\2.ico"));
        grid.press(keys::ARROW_LEFT);
        grid.press(keys::ARROW_LEFT);

        assert_eq!(grid.press(keys::ENTER), GridStep::Picked(None));
    }

    #[test]
    fn enter_picks_the_highlighted_file() {
        let mut grid = IconGrid::new(files(3), None);
        grid.press(keys::ARROW_RIGHT);

        assert_eq!(
            grid.press(keys::ENTER),
            GridStep::Picked(Some(r"icons\xp\1.ico".to_owned()))
        );
    }

    #[test]
    fn moving_down_past_the_page_scrolls() {
        let mut grid = IconGrid::new(files(100), None);
        for _ in 0..VISIBLE_ROWS {
            grid.press(keys::ARROW_DOWN);
        }

        assert_eq!(grid.highlighted_in_view(), Some(VISIBLE_TILES - COLUMNS));
        assert_eq!(grid.visible().next(), Some(Tile::File(r"icons\xp\6.ico")));
    }

    #[test]
    fn up_from_the_top_row_stays_put() {
        let mut grid = IconGrid::new(files(10), None);
        grid.press(keys::ARROW_RIGHT);

        assert_eq!(grid.press(keys::ARROW_UP), GridStep::Ignored);
        assert_eq!(grid.highlighted_in_view(), Some(1));
    }

    #[test]
    fn down_past_the_end_lands_on_the_last_tile() {
        let mut grid = IconGrid::new(files(3), None);
        grid.press(keys::PAGE_DOWN);

        assert!(grid.caption().starts_with(r"icons\xp\3.ico"));
    }

    #[test]
    fn scrolling_drags_the_highlight_into_view() {
        let mut grid = IconGrid::new(files(100), None);
        grid.scroll(2);

        assert_eq!(grid.highlighted_in_view(), Some(0));
    }

    #[test]
    fn scrolling_stops_at_the_last_page() {
        let mut grid = IconGrid::new(files(100), None);
        grid.scroll(isize::MAX / 2);

        assert_eq!(grid.visible().count(), (100 + 1) - grid.first_row * COLUMNS);
        assert!(grid.visible().count() <= VISIBLE_TILES);
    }

    #[test]
    fn a_click_past_the_last_tile_does_nothing() {
        let mut grid = IconGrid::new(files(2), None);

        assert_eq!(grid.click(VISIBLE_TILES - 1), GridStep::Ignored);
    }

    #[test]
    fn a_click_picks_the_tile_under_it() {
        let mut grid = IconGrid::new(files(2), None);

        assert_eq!(
            grid.click(2),
            GridStep::Picked(Some(r"icons\xp\2.ico".to_owned()))
        );
    }
}
