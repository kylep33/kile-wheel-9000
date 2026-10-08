//! The petal form: what grows out of a slice when you edit it.
//!
//! Paths are typed rather than picked. A file dialog launched from here would get no
//! keystrokes, because the hook swallows everything while the wheel is up, and waiting on
//! one would block the message loop with input captured.

mod petal;
mod rows;
mod text;

pub use petal::{PETAL_MAX_REACH_RATIO, Petal};
pub use rows::{Line, RowValue};

use kile_wheel_core::binding::{KeyCode, Modifiers, keys};
use kile_wheel_core::item::{Item, ItemKind};
use kile_wheel_core::slot::{Slot, Tab};

use rows::{Action, Row, RowStep};

use crate::anim::Animation;

/// Name an unpinned app slot is given, since the running app supplies the label anyway.
const UNPINNED_NAME: &str = "Open";

/// What the form wants the caller to do after a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing changed on screen.
    Ignored,
    Redraw,
    /// Write this item to the config.
    Commit(Box<Item>),
    /// Clear the slot entirely.
    Delete,
    /// Put this on the clipboard.
    Copy(String),
    /// Close the form, leaving the slot as it was.
    Cancel,
}

/// Which half of the form is showing.
#[derive(Debug)]
enum Stage {
    /// Choosing what the slot should be.
    PickType { highlighted: usize },
    /// Filling the chosen type in. The kind is held here rather than in a hidden line, so
    /// every line in `rows` is one you can actually reach.
    Fill {
        kind: ItemKind,
        rows: Vec<Row>,
        focused: usize,
    },
}

/// One slot's editor, rooted at the slice it belongs to.
#[derive(Debug)]
pub struct Form {
    tab: Tab,
    slot: Slot,
    stage: Stage,
    /// Whether the slot already held something, which decides if Delete does anything.
    occupied: bool,
    opened: Animation,
}

impl Form {
    /// Opens on the type picker, for a slot with nothing in it.
    pub fn picking(tab: Tab, slot: Slot) -> Self {
        Self {
            tab,
            slot,
            stage: Stage::PickType { highlighted: 0 },
            occupied: false,
            opened: Animation::started(),
        }
    }

    /// Opens straight into the fields of something already configured.
    pub fn editing(tab: Tab, slot: Slot, item: &Item) -> Self {
        Self {
            tab,
            slot,
            stage: Stage::Fill {
                kind: item.kind.clone(),
                rows: rows::build(item),
                focused: 0,
            },
            occupied: true,
            opened: Animation::started(),
        }
    }

    pub fn tab(&self) -> Tab {
        self.tab
    }

    pub fn slot(&self) -> Slot {
        self.slot
    }

    pub fn occupied(&self) -> bool {
        self.occupied
    }

    /// How far the open animation has run, from 0 to 1.
    pub fn progress(&self, duration_ms: u64) -> f32 {
        self.opened.progress(duration_ms)
    }

    /// Whether the animation is still running, so the caller knows to keep drawing.
    pub fn animating(&self, duration_ms: u64) -> bool {
        self.opened.running(duration_ms)
    }

    pub fn title(&self) -> &'static str {
        match &self.stage {
            Stage::PickType { .. } => "What goes here?",
            Stage::Fill { kind, .. } => kind.type_name(),
        }
    }

    /// The lines to draw, with the focused one marked.
    pub fn lines(&self) -> Vec<Line> {
        match &self.stage {
            Stage::PickType { highlighted } => ItemKind::NAMES
                .iter()
                .enumerate()
                .map(|(index, name)| Line {
                    label: name,
                    value: None,
                    focused: index == *highlighted,
                    action: true,
                })
                .collect(),
            Stage::Fill { rows, focused, .. } => rows
                .iter()
                .enumerate()
                .map(|(index, row)| row.line(index == *focused))
                .collect(),
        }
    }

    /// Acts on a click that landed on line `index`.
    ///
    /// A click is decisive rather than just a focus move: on the picker it chooses the
    /// type, and on an action line it runs the action. Clicking a field only focuses it,
    /// which is all a field can do without a caret position to aim at.
    pub fn click(&mut self, index: usize) -> Outcome {
        match &mut self.stage {
            Stage::PickType { highlighted } => match index < ItemKind::NAMES.len() {
                true => {
                    *highlighted = index;
                    self.chose_type(index)
                }
                false => Outcome::Ignored,
            },
            Stage::Fill { rows, focused, .. } => {
                let Some(action) = rows.get(index).map(Row::action) else {
                    return Outcome::Ignored;
                };
                *focused = index;

                match action {
                    Some(action) => self.run_action(action),
                    None => Outcome::Redraw,
                }
            }
        }
    }

    fn run_action(&mut self, action: Action) -> Outcome {
        match action {
            Action::Save => self.save(),
            Action::Delete => Outcome::Delete,
            Action::Cancel => Outcome::Cancel,
        }
    }

    /// Feeds a key press in. Printable characters arrive as `typed`.
    pub fn press(
        &mut self,
        code: KeyCode,
        modifiers: Modifiers,
        typed: Option<char>,
        clipboard: Option<&str>,
    ) -> Outcome {
        let step = match &mut self.stage {
            Stage::PickType { highlighted } => press_picker(highlighted, code),
            Stage::Fill { rows, focused, .. } => {
                press_fill(rows, focused, code, modifiers, typed, clipboard)
            }
        };

        match step {
            Step::Ignored => Outcome::Ignored,
            Step::Redraw => Outcome::Redraw,
            Step::Cancel => Outcome::Cancel,
            Step::Delete => Outcome::Delete,
            Step::Copy(text) => Outcome::Copy(text),
            Step::ChoseType(index) => self.chose_type(index),
            Step::Save => self.save(),
        }
    }

    /// An unpinned app slot needs nothing filling in, so picking it is the whole job.
    fn chose_type(&mut self, index: usize) -> Outcome {
        let Some(kind) = ItemKind::blank(index) else {
            return Outcome::Ignored;
        };

        if kind.is_unpinned() {
            return Outcome::Commit(Box::new(self.item(UNPINNED_NAME.to_owned(), kind, None)));
        }

        let seed = self.item(ItemKind::NAMES[index].to_owned(), kind.clone(), None);
        self.stage = Stage::Fill {
            kind,
            rows: rows::build(&seed),
            focused: 0,
        };
        self.opened.restart();
        Outcome::Redraw
    }

    fn save(&mut self) -> Outcome {
        let Stage::Fill { kind, rows, .. } = &self.stage else {
            return Outcome::Ignored;
        };
        let Some((name, rebuilt)) = rows::collect(kind, rows) else {
            // a blank name has nothing to show on the wheel, so Save does nothing
            return Outcome::Ignored;
        };

        let icon = rows::icon_of(rows);
        Outcome::Commit(Box::new(self.item(name, rebuilt, icon)))
    }

    fn item(&self, name: String, kind: ItemKind, icon: Option<String>) -> Item {
        Item {
            name,
            icon,
            tab: self.tab.number(),
            slot: self.slot.index(),
            kind,
        }
    }
}

/// What a key did, before it is turned into an [`Outcome`].
enum Step {
    Ignored,
    Redraw,
    ChoseType(usize),
    Copy(String),
    Save,
    Delete,
    Cancel,
}

fn press_picker(highlighted: &mut usize, code: KeyCode) -> Step {
    let last = ItemKind::NAMES.len() - 1;

    if code == keys::ARROW_UP || code == keys::W {
        *highlighted = highlighted.saturating_sub(1);
        return Step::Redraw;
    }
    if code == keys::ARROW_DOWN || code == keys::S {
        *highlighted = (*highlighted + 1).min(last);
        return Step::Redraw;
    }
    if code == keys::ENTER {
        return Step::ChoseType(*highlighted);
    }
    Step::Ignored
}

fn press_fill(
    rows: &mut [Row],
    focused: &mut usize,
    code: KeyCode,
    modifiers: Modifiers,
    typed: Option<char>,
    clipboard: Option<&str>,
) -> Step {
    if rows.is_empty() {
        return Step::Ignored;
    }
    let last = rows.len() - 1;

    // Tab and the vertical arrows move between lines; every other key is the line's
    if code == keys::TAB {
        *focused = match modifiers.shift {
            true => focused.checked_sub(1).unwrap_or(last),
            false => (*focused + 1) % rows.len(),
        };
        return Step::Redraw;
    }
    if code == keys::ARROW_UP {
        *focused = focused.saturating_sub(1);
        return Step::Redraw;
    }
    if code == keys::ARROW_DOWN {
        *focused = (*focused + 1).min(last);
        return Step::Redraw;
    }

    let Some(row) = rows.get_mut(*focused) else {
        return Step::Ignored;
    };
    match row.press(code, modifiers, typed, clipboard) {
        RowStep::Ignored => Step::Ignored,
        RowStep::Redraw => Step::Redraw,
        // Enter on a field is the same as Tab, so a form can be filled without reaching
        // for Tab at all
        RowStep::Advance => {
            *focused = (*focused + 1).min(last);
            Step::Redraw
        }
        RowStep::Copy(text) => Step::Copy(text),
        RowStep::Save => Step::Save,
        RowStep::Delete => Step::Delete,
        RowStep::Cancel => Step::Cancel,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot() -> Slot {
        Slot::new(3).expect("slot 3")
    }

    fn picker() -> Form {
        Form::picking(Tab::FIRST, slot())
    }

    fn press(form: &mut Form, code: KeyCode) -> Outcome {
        form.press(code, Modifiers::default(), None, None)
    }

    fn type_text(form: &mut Form, text: &str) {
        for character in text.chars() {
            form.press(KeyCode(0), Modifiers::default(), Some(character), None);
        }
    }

    /// Walks the picker down to a named type and opens it.
    fn open_type(name: &str) -> Form {
        let index = ItemKind::NAMES
            .iter()
            .position(|candidate| *candidate == name)
            .expect("the type is listed");

        let mut form = picker();
        for _ in 0..index {
            press(&mut form, keys::ARROW_DOWN);
        }
        press(&mut form, keys::ENTER);
        form
    }

    fn focus(form: &Form, label: &str) -> usize {
        form.lines()
            .iter()
            .position(|line| line.label == label)
            .expect("the line is there")
    }

    fn go_to(form: &mut Form, label: &str) {
        let target = focus(form, label);
        while form.lines().iter().position(|line| line.focused) != Some(target) {
            press(form, keys::TAB);
        }
    }

    #[test]
    fn the_picker_lists_every_type() {
        let labels: Vec<&str> = picker().lines().iter().map(|line| line.label).collect();

        assert_eq!(labels, ItemKind::NAMES);
    }

    #[test]
    fn the_picker_starts_on_the_first_type() {
        assert!(picker().lines()[0].focused);
    }

    #[test]
    fn the_picker_stops_at_both_ends() {
        let mut form = picker();

        press(&mut form, keys::ARROW_UP);
        assert!(form.lines()[0].focused);

        for _ in 0..20 {
            press(&mut form, keys::ARROW_DOWN);
        }
        assert!(form.lines()[ItemKind::NAMES.len() - 1].focused);
    }

    /// The common case: one keypress and the slot is open for running apps.
    #[test]
    fn picking_an_unpinned_slot_commits_straight_away() {
        let mut form = picker();

        let outcome = press(&mut form, keys::ENTER);

        let Outcome::Commit(item) = outcome else {
            panic!("an unpinned slot needs nothing filling in, got {outcome:?}");
        };
        assert!(item.kind.is_unpinned());
        assert_eq!((item.tab, item.slot), (1, 3));
    }

    #[test]
    fn picking_a_type_with_fields_opens_them() {
        let form = open_type("Pinned app");

        assert_eq!(form.title(), "Pinned app");
        assert_eq!(focus(&form, "Target"), 1);
    }

    #[test]
    fn typing_lands_in_the_focused_field() {
        let mut form = open_type("Folder");
        go_to(&mut form, "Folder");

        type_text(&mut form, r"C:\p");

        let lines = form.lines();
        let line = &lines[focus(&form, "Folder")];
        assert_eq!(
            line.value,
            Some(RowValue::Text {
                before: r"C:\p".to_owned(),
                after: String::new()
            })
        );
    }

    #[test]
    fn tab_moves_down_and_shift_tab_moves_up() {
        let mut form = open_type("Pinned app");

        press(&mut form, keys::TAB);
        assert!(form.lines()[1].focused);

        let shifted = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        form.press(keys::TAB, shifted, None, None);
        assert!(form.lines()[0].focused);
    }

    #[test]
    fn tab_wraps_round_the_end() {
        let mut form = open_type("Pinned app");
        let count = form.lines().len();

        for _ in 0..count {
            press(&mut form, keys::TAB);
        }

        assert!(form.lines()[0].focused);
    }

    #[test]
    fn save_commits_what_was_typed() {
        let mut form = open_type("Folder");
        go_to(&mut form, "Folder");
        type_text(&mut form, r"C:\projects");
        go_to(&mut form, "Save");

        let outcome = press(&mut form, keys::ENTER);

        let Outcome::Commit(item) = outcome else {
            panic!("save should commit, got {outcome:?}");
        };
        assert_eq!(
            item.kind,
            ItemKind::Folder {
                path: r"C:\projects".to_owned()
            }
        );
        assert_eq!(item.name, "Folder");
    }

    #[test]
    fn save_with_a_blank_name_does_nothing() {
        let mut form = open_type("Folder");
        go_to(&mut form, "Name");
        for _ in 0..20 {
            press(&mut form, keys::BACKSPACE);
        }
        go_to(&mut form, "Save");

        assert_eq!(press(&mut form, keys::ENTER), Outcome::Ignored);
    }

    #[test]
    fn cancel_and_delete_report_themselves() {
        let mut form = open_type("Folder");

        go_to(&mut form, "Delete");
        assert_eq!(press(&mut form, keys::ENTER), Outcome::Delete);

        go_to(&mut form, "Cancel");
        assert_eq!(press(&mut form, keys::ENTER), Outcome::Cancel);
    }

    #[test]
    fn a_typed_icon_reaches_the_committed_item() {
        let mut form = open_type("Folder");
        go_to(&mut form, "Icon");
        type_text(&mut form, "icons/p.png");
        go_to(&mut form, "Save");

        let Outcome::Commit(item) = press(&mut form, keys::ENTER) else {
            panic!("save should commit");
        };
        assert_eq!(item.icon.as_deref(), Some("icons/p.png"));
    }

    #[test]
    fn editing_an_existing_item_starts_on_its_fields() {
        let item = Item {
            name: "Projects".to_owned(),
            icon: None,
            tab: 1,
            slot: 3,
            kind: ItemKind::Folder {
                path: r"C:\p".to_owned(),
            },
        };

        let form = Form::editing(Tab::FIRST, slot(), &item);

        assert_eq!(form.title(), "Folder");
        assert!(form.occupied());
    }

    #[test]
    fn a_picker_opened_on_a_blank_slot_is_not_occupied() {
        assert!(!picker().occupied());
    }

    #[test]
    fn the_open_animation_finishes_instantly_when_animation_is_off() {
        assert_eq!(picker().progress(0), 1.0);
    }

    #[test]
    fn the_open_animation_starts_near_zero_and_is_still_running() {
        let form = picker();

        assert!(form.progress(10_000) < 0.5);
        assert!(form.animating(10_000));
    }

    /// Index of "Pinned app" in the type list, which needs filling in rather than
    /// committing straight away.
    const PINNED_APP: usize = 1;

    /// Index of "Unpinned app slot", which commits as soon as it is picked.
    const UNPINNED_SLOT: usize = 0;

    #[test]
    fn clicking_a_type_picks_it_rather_than_just_highlighting_it() {
        let mut form = picker();

        let outcome = form.click(PINNED_APP);

        assert_eq!(outcome, Outcome::Redraw, "it should move on to the fields");
        assert!(
            form.lines().iter().any(|line| line.label == "Name"),
            "the fields should be showing"
        );
    }

    #[test]
    fn clicking_an_unpinned_slot_commits_it_on_the_spot() {
        let mut form = picker();

        assert!(matches!(form.click(UNPINNED_SLOT), Outcome::Commit(_)));
    }

    #[test]
    fn clicking_past_the_last_type_does_nothing() {
        let mut form = picker();

        assert_eq!(form.click(ItemKind::NAMES.len()), Outcome::Ignored);
    }

    #[test]
    fn clicking_a_field_focuses_it() {
        let mut form = picker();
        form.click(PINNED_APP);

        let last = form.lines().len() - 1;
        form.click(last);
        // Cancel is the last line, so step back to a field and check focus moved there
        let field = 0;
        assert_eq!(form.click(field), Outcome::Redraw);
        assert!(
            form.lines()[field].focused,
            "the clicked field should be focused"
        );
    }

    #[test]
    fn clicking_cancel_closes_the_form() {
        let mut form = picker();
        form.click(PINNED_APP);

        let cancel = form
            .lines()
            .iter()
            .position(|line| line.label == "Cancel")
            .expect("a filled form has a Cancel line");

        assert_eq!(form.click(cancel), Outcome::Cancel);
    }

    #[test]
    fn clicking_delete_clears_the_slot() {
        let mut form = picker();
        form.click(PINNED_APP);

        let delete = form
            .lines()
            .iter()
            .position(|line| line.label == "Delete")
            .expect("a filled form has a Delete line");

        assert_eq!(form.click(delete), Outcome::Delete);
    }

    #[test]
    fn clicking_past_the_last_line_does_nothing() {
        let mut form = picker();
        form.click(PINNED_APP);
        let past_end = form.lines().len();

        assert_eq!(form.click(past_end), Outcome::Ignored);
    }

    /// The name field is seeded with the type name, so the copy is whatever the field
    /// actually shows rather than only what was typed.
    #[test]
    fn ctrl_c_copies_the_whole_focused_field() {
        let mut form = picker();
        form.click(PINNED_APP);
        type_text(&mut form, "notepad");

        let showing = match &form.lines()[0].value {
            Some(RowValue::Text { before, after }) => format!("{before}{after}"),
            other => panic!("line 0 should be a text field, got {other:?}"),
        };

        let outcome = form.press(
            keys::C,
            Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
            None,
            None,
        );

        assert_eq!(outcome, Outcome::Copy(showing));
    }
}
