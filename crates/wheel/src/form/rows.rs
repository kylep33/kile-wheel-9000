//! The lines a form is made of: typed fields, toggles, choices and the actions.

use kile_wheel_core::binding::{KeyCode, Modifiers, keys};
use kile_wheel_core::item::{Item, ItemKind, SCRIPT_PRESETS, SnippetMode};

use super::text::TextValue;

const NAME: &str = "Name";
const ICON: &str = "Icon";

const CLIPBOARD_MODE: &str = "clipboard";
const TYPE_MODE: &str = "type";

/// What a line's own key handling decided.
#[derive(Debug, PartialEq, Eq)]
pub enum RowStep {
    Ignored,
    Redraw,
    /// Move to the next line, which is what Enter does on a field.
    Advance,
    /// Put this on the clipboard. Carries the text because only the row knows it.
    Copy(String),
    Save,
    Delete,
    Cancel,
    PickIcon,
}

/// What a line shows to the right of its label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowValue {
    /// Text split at the caret, so the renderer can place the caret by measuring the
    /// first half rather than the whole string twice.
    Text {
        before: String,
        after: String,
    },
    Toggle(bool),
    Choice(String),
}

/// One drawn line of a form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// Owned because an app picker line is named after a shortcut on disk, not after a
    /// constant in this file.
    pub label: String,
    pub value: Option<RowValue>,
    pub focused: bool,
    /// Drawn as a button rather than a field.
    pub action: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Save,
    Delete,
    Cancel,
    PickIcon,
}

impl Action {
    /// The buttons at the foot of every form. Pick icon sits with the icon field instead.
    const ALL: [Self; 3] = [Self::Save, Self::Delete, Self::Cancel];

    fn label(self) -> &'static str {
        match self {
            Self::Save => "Save",
            Self::Delete => "Delete",
            Self::Cancel => "Cancel",
            Self::PickIcon => "Pick icon",
        }
    }

    fn press(self, code: KeyCode) -> RowStep {
        if code != keys::ENTER {
            return RowStep::Ignored;
        }
        match self {
            Self::Save => RowStep::Save,
            Self::Delete => RowStep::Delete,
            Self::Cancel => RowStep::Cancel,
            Self::PickIcon => RowStep::PickIcon,
        }
    }
}

/// One line of a form.
#[derive(Debug)]
pub enum Row {
    /// A typed value. Paths are typed too, so nothing here opens a dialog.
    Text {
        label: &'static str,
        value: TextValue,
    },
    Toggle {
        label: &'static str,
        on: bool,
    },
    /// One of a fixed set, cycled with the horizontal arrows.
    Choice {
        label: &'static str,
        options: Vec<String>,
        chosen: usize,
    },
    Action(Action),
}

impl Row {
    fn text(label: &'static str, value: &str) -> Self {
        Self::Text {
            label,
            value: TextValue::new(value),
        }
    }

    fn optional(label: &'static str, value: Option<&String>) -> Self {
        Self::text(label, value.map(String::as_str).unwrap_or_default())
    }

    /// None for anything that is not a button.
    pub fn action(&self) -> Option<Action> {
        match self {
            Self::Action(action) => Some(*action),
            _ => None,
        }
    }

    pub fn line(&self, focused: bool) -> Line {
        let (label, value, action) = match self {
            Self::Text { label, value } => {
                let (before, after) = value.split_at_caret();
                (*label, Some(RowValue::Text { before, after }), false)
            }
            Self::Toggle { label, on } => (*label, Some(RowValue::Toggle(*on)), false),
            Self::Choice {
                label,
                options,
                chosen,
            } => {
                let chosen = options.get(*chosen).cloned().unwrap_or_default();
                (*label, Some(RowValue::Choice(chosen)), false)
            }
            Self::Action(action) => (action.label(), None, true),
        };

        Line {
            label: label.to_owned(),
            value,
            focused,
            action,
        }
    }

    pub fn press(
        &mut self,
        code: KeyCode,
        modifiers: Modifiers,
        typed: Option<char>,
        clipboard: Option<&str>,
    ) -> RowStep {
        match self {
            Self::Text { value, .. } => press_text(value, code, modifiers, typed, clipboard),
            Self::Toggle { on, .. } => press_toggle(on, code),
            Self::Choice {
                options, chosen, ..
            } => press_choice(options, chosen, code),
            Self::Action(action) => action.press(code),
        }
    }
}

fn press_text(
    value: &mut TextValue,
    code: KeyCode,
    modifiers: Modifiers,
    typed: Option<char>,
    clipboard: Option<&str>,
) -> RowStep {
    if modifiers.ctrl {
        // there is no selection in these fields, so Ctrl+C takes the whole value
        if code == keys::C {
            return RowStep::Copy(value.text());
        }
        if code != keys::V {
            return RowStep::Ignored;
        }
        let Some(text) = clipboard else {
            return RowStep::Ignored;
        };
        value.insert_str(&one_line(text));
        return RowStep::Redraw;
    }

    if code == keys::BACKSPACE {
        value.backspace();
        return RowStep::Redraw;
    }
    if code == keys::DELETE {
        value.delete();
        return RowStep::Redraw;
    }
    if code == keys::ARROW_LEFT {
        value.left();
        return RowStep::Redraw;
    }
    if code == keys::ARROW_RIGHT {
        value.right();
        return RowStep::Redraw;
    }
    if code == keys::HOME {
        value.home();
        return RowStep::Redraw;
    }
    if code == keys::END {
        value.end();
        return RowStep::Redraw;
    }
    if code == keys::ENTER {
        return RowStep::Advance;
    }

    let Some(character) = typed else {
        return RowStep::Ignored;
    };
    value.insert(character);
    RowStep::Redraw
}

/// Flattens a paste onto one line.
///
/// A path copied out of Explorer usually carries a trailing newline, and a CRLF would
/// otherwise land as two spaces.
fn one_line(text: &str) -> String {
    text.split(['\r', '\n'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<&str>>()
        .join(" ")
}

fn press_toggle(on: &mut bool, code: KeyCode) -> RowStep {
    if code == keys::SPACE || code == keys::ENTER {
        *on = !*on;
        return RowStep::Redraw;
    }
    RowStep::Ignored
}

fn press_choice(options: &[String], chosen: &mut usize, code: KeyCode) -> RowStep {
    if options.is_empty() {
        return RowStep::Ignored;
    }

    if code == keys::ARROW_LEFT {
        *chosen = chosen.checked_sub(1).unwrap_or(options.len() - 1);
        return RowStep::Redraw;
    }
    if code == keys::ARROW_RIGHT || code == keys::SPACE {
        *chosen = (*chosen + 1) % options.len();
        return RowStep::Redraw;
    }
    if code == keys::ENTER {
        return RowStep::Advance;
    }
    RowStep::Ignored
}

/// The lines for an item, in the order they are filled.
pub fn build(item: &Item) -> Vec<Row> {
    let mut rows = vec![Row::text(NAME, &item.name)];
    rows.extend(kind_rows(&item.kind));

    if !item.kind.is_unpinned() {
        rows.push(Row::optional(ICON, item.icon.as_ref()));
        rows.push(Row::Action(Action::PickIcon));
    }
    rows.extend(Action::ALL.map(Row::Action));
    rows
}

fn kind_rows(kind: &ItemKind) -> Vec<Row> {
    match kind {
        ItemKind::UnpinnedApp => Vec::new(),
        ItemKind::App { target } => vec![Row::text("Target", target)],
        ItemKind::Script {
            path,
            exec,
            cwd,
            hidden,
        } => vec![
            Row::text("Script", path),
            Row::text("Run with", exec),
            Row::optional("Start in", cwd.as_ref()),
            Row::Toggle {
                label: "No window",
                on: *hidden,
            },
        ],
        ItemKind::Url {
            url,
            browser,
            browser_args,
        } => vec![
            Row::text("URL", url),
            Row::optional("Browser", browser.as_ref()),
            Row::optional("Browser args", browser_args.as_ref()),
        ],
        ItemKind::Snippet { text, mode } => vec![
            Row::text("Text", text),
            Row::Choice {
                label: "Deliver by",
                options: vec![CLIPBOARD_MODE.to_owned(), TYPE_MODE.to_owned()],
                chosen: usize::from(*mode == SnippetMode::Type),
            },
        ],
        ItemKind::Folder { path } => vec![Row::text("Folder", path)],
        ItemKind::Terminal {
            profile,
            distro,
            cwd,
        } => vec![
            Row::optional("Profile", profile.as_ref()),
            Row::optional("WSL distro", distro.as_ref()),
            Row::optional("Start in", cwd.as_ref()),
        ],
    }
}

pub fn icon_of(rows: &[Row]) -> Option<String> {
    non_empty(field(rows, ICON))
}

/// What the grid picked goes into the field, where it can still be edited by hand.
pub fn set_icon(rows: &mut [Row], icon: Option<&str>) {
    for row in rows {
        if let Row::Text { label: ICON, value } = row {
            *value = TextValue::new(icon.unwrap_or_default());
        }
    }
}

/// Reads the filled lines back out as a name and a kind.
///
/// `None` when the name is blank, because a slot with no name has nothing to show.
pub fn collect(kind: &ItemKind, rows: &[Row]) -> Option<(String, ItemKind)> {
    let name = field(rows, NAME)?;
    if name.trim().is_empty() {
        return None;
    }
    Some((name, rebuild(kind, rows)))
}

/// The kind as the lines currently describe it, name or not.
pub fn rebuild(kind: &ItemKind, rows: &[Row]) -> ItemKind {
    match kind {
        ItemKind::UnpinnedApp => ItemKind::UnpinnedApp,
        ItemKind::App { .. } => ItemKind::App {
            target: field(rows, "Target").unwrap_or_default(),
        },
        ItemKind::Script { .. } => ItemKind::Script {
            path: field(rows, "Script").unwrap_or_default(),
            exec: non_empty(field(rows, "Run with"))
                .unwrap_or_else(|| SCRIPT_PRESETS[0].1.to_owned()),
            cwd: non_empty(field(rows, "Start in")),
            hidden: toggle(rows, "No window"),
        },
        ItemKind::Url { .. } => ItemKind::Url {
            url: field(rows, "URL").unwrap_or_default(),
            browser: non_empty(field(rows, "Browser")),
            browser_args: non_empty(field(rows, "Browser args")),
        },
        ItemKind::Snippet { .. } => ItemKind::Snippet {
            text: field(rows, "Text").unwrap_or_default(),
            mode: match choice(rows, "Deliver by").as_deref() {
                Some(TYPE_MODE) => SnippetMode::Type,
                _ => SnippetMode::Clipboard,
            },
        },
        ItemKind::Folder { .. } => ItemKind::Folder {
            path: field(rows, "Folder").unwrap_or_default(),
        },
        ItemKind::Terminal { .. } => ItemKind::Terminal {
            profile: non_empty(field(rows, "Profile")),
            distro: non_empty(field(rows, "WSL distro")),
            cwd: non_empty(field(rows, "Start in")),
        },
    }
}

fn field(rows: &[Row], label: &str) -> Option<String> {
    rows.iter().find_map(|row| match row {
        Row::Text {
            label: found,
            value,
        } if *found == label => Some(value.text()),
        _ => None,
    })
}

fn toggle(rows: &[Row], label: &str) -> bool {
    rows.iter()
        .find_map(|row| match row {
            Row::Toggle { label: found, on } if *found == label => Some(*on),
            _ => None,
        })
        .unwrap_or(false)
}

fn choice(rows: &[Row], label: &str) -> Option<String> {
    rows.iter().find_map(|row| match row {
        Row::Choice {
            label: found,
            options,
            chosen,
        } if *found == label => options.get(*chosen).cloned(),
        _ => None,
    })
}

fn non_empty(text: Option<String>) -> Option<String> {
    text.filter(|text| !text.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: ItemKind) -> Item {
        Item {
            name: "Thing".to_owned(),
            icon: None,
            tab: 1,
            slot: 0,
            kind,
        }
    }

    fn text_of(row: &Row) -> String {
        match row.line(false).value {
            Some(RowValue::Text { before, after }) => format!("{before}{after}"),
            other => panic!("expected a text line, got {other:?}"),
        }
    }

    fn type_into(row: &mut Row, text: &str) {
        for character in text.chars() {
            row.press(KeyCode(0), Modifiers::default(), Some(character), None);
        }
    }

    #[test]
    fn an_unpinned_slot_has_no_fields_of_its_own() {
        assert!(kind_rows(&ItemKind::UnpinnedApp).is_empty());
    }

    #[test]
    fn an_unpinned_slot_is_not_offered_an_icon() {
        let rows = build(&item(ItemKind::UnpinnedApp));

        let labels: Vec<String> = rows.iter().map(|row| row.line(false).label).collect();
        assert!(!labels.iter().any(|label| label == ICON), "{labels:?}");
    }

    #[test]
    fn every_type_round_trips_through_build_and_collect() {
        let kinds = [
            ItemKind::UnpinnedApp,
            ItemKind::App {
                target: "a.exe".to_owned(),
            },
            ItemKind::Script {
                path: "s.sh".to_owned(),
                exec: "bash \"{path}\"".to_owned(),
                cwd: Some(r"C:\w".to_owned()),
                hidden: true,
            },
            ItemKind::Url {
                url: "https://example.test".to_owned(),
                browser: Some("msedge.exe".to_owned()),
                browser_args: Some("--new-window".to_owned()),
            },
            ItemKind::Snippet {
                text: "hi".to_owned(),
                mode: SnippetMode::Type,
            },
            ItemKind::Folder {
                path: r"C:\p".to_owned(),
            },
            ItemKind::Terminal {
                profile: Some("Ubuntu".to_owned()),
                distro: Some("Ubuntu".to_owned()),
                cwd: Some("~".to_owned()),
            },
        ];

        for kind in kinds {
            let rows = build(&item(kind.clone()));
            let (name, collected) = collect(&kind, &rows).expect("a named item collects");

            assert_eq!(name, "Thing");
            assert_eq!(collected, kind, "{} did not round trip", kind.type_name());
        }
    }

    #[test]
    fn a_blank_name_refuses_to_collect() {
        let kind = ItemKind::Folder {
            path: r"C:\p".to_owned(),
        };
        let mut rows = build(&item(kind.clone()));
        let Some(Row::Text { value, .. }) = rows.first_mut() else {
            panic!("the first line is the name");
        };
        for _ in 0..20 {
            value.backspace();
        }

        assert_eq!(collect(&kind, &rows), None);
    }

    #[test]
    fn a_blank_optional_field_collects_as_unset() {
        let kind = ItemKind::Terminal {
            profile: None,
            distro: None,
            cwd: None,
        };
        let rows = build(&item(kind.clone()));

        let (_, collected) = collect(&kind, &rows).expect("collects");
        assert_eq!(collected, kind);
    }

    #[test]
    fn an_empty_run_with_falls_back_to_the_first_preset() {
        let kind = ItemKind::Script {
            path: "s.sh".to_owned(),
            exec: String::new(),
            cwd: None,
            hidden: false,
        };
        let rows = build(&item(kind.clone()));

        let (_, collected) = collect(&kind, &rows).expect("collects");
        let ItemKind::Script { exec, .. } = collected else {
            panic!("a script collects as a script");
        };
        assert_eq!(exec, SCRIPT_PRESETS[0].1);
    }

    #[test]
    fn backspace_edits_the_field_rather_than_leaving_the_form() {
        let mut row = Row::text("Target", "abc");

        let step = row.press(keys::BACKSPACE, Modifiers::default(), None, None);

        assert_eq!(step, RowStep::Redraw);
        assert_eq!(text_of(&row), "ab");
    }

    #[test]
    fn enter_on_a_field_moves_to_the_next_one() {
        let mut row = Row::text("Target", "abc");

        assert_eq!(
            row.press(keys::ENTER, Modifiers::default(), None, None),
            RowStep::Advance
        );
    }

    #[test]
    fn typing_appends_at_the_caret() {
        let mut row = Row::text("Target", "ab");

        type_into(&mut row, "cd");

        assert_eq!(text_of(&row), "abcd");
    }

    #[test]
    fn ctrl_v_pastes_the_clipboard() {
        let mut row = Row::text("Target", "");
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::default()
        };

        row.press(keys::V, ctrl, None, Some(r"C:\apps\thing.exe"));

        assert_eq!(text_of(&row), r"C:\apps\thing.exe");
    }

    #[test]
    fn a_pasted_newline_becomes_a_space_rather_than_breaking_the_line() {
        let mut row = Row::text("Target", "");
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::default()
        };

        row.press(keys::V, ctrl, None, Some("one\r\ntwo"));

        assert_eq!(text_of(&row), "one two");
    }

    /// Explorer copies a path with a trailing newline, which must not arrive as a space.
    #[test]
    fn a_pasted_path_loses_its_trailing_newline() {
        let mut row = Row::text("Target", "");
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::default()
        };

        row.press(keys::V, ctrl, None, Some("C:\\apps\\thing.exe\r\n"));

        assert_eq!(text_of(&row), r"C:\apps\thing.exe");
    }

    #[test]
    fn other_ctrl_combos_are_left_for_the_wheel() {
        let mut row = Row::text("Target", "ab");
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::default()
        };

        let step = row.press(keys::S, ctrl, Some('s'), None);

        assert_eq!(step, RowStep::Ignored);
        assert_eq!(text_of(&row), "ab");
    }

    #[test]
    fn the_caret_split_follows_the_arrows() {
        let mut row = Row::text("Target", "abcd");
        row.press(keys::ARROW_LEFT, Modifiers::default(), None, None);

        assert_eq!(
            row.line(true).value,
            Some(RowValue::Text {
                before: "abc".to_owned(),
                after: "d".to_owned()
            })
        );
    }

    #[test]
    fn space_flips_a_toggle() {
        let mut row = Row::Toggle {
            label: "No window",
            on: false,
        };

        row.press(keys::SPACE, Modifiers::default(), Some(' '), None);

        assert_eq!(row.line(false).value, Some(RowValue::Toggle(true)));
    }

    #[test]
    fn the_arrows_cycle_a_choice_both_ways() {
        let mut row = Row::Choice {
            label: "Deliver by",
            options: vec!["a".to_owned(), "b".to_owned(), "c".to_owned()],
            chosen: 0,
        };

        row.press(keys::ARROW_LEFT, Modifiers::default(), None, None);
        assert_eq!(
            row.line(false).value,
            Some(RowValue::Choice("c".to_owned()))
        );

        row.press(keys::ARROW_RIGHT, Modifiers::default(), None, None);
        assert_eq!(
            row.line(false).value,
            Some(RowValue::Choice("a".to_owned()))
        );
    }

    #[test]
    fn the_actions_only_fire_on_enter() {
        let mut save = Row::Action(Action::Save);

        assert_eq!(
            save.press(keys::SPACE, Modifiers::default(), None, None),
            RowStep::Ignored
        );
        assert_eq!(
            save.press(keys::ENTER, Modifiers::default(), None, None),
            RowStep::Save
        );
    }

    #[test]
    fn every_form_ends_with_save_delete_and_cancel() {
        let rows = build(&item(ItemKind::UnpinnedApp));

        let tail: Vec<String> = rows
            .iter()
            .filter(|row| matches!(row, Row::Action(_)))
            .map(|row| row.line(false).label)
            .collect();
        assert_eq!(tail, ["Save", "Delete", "Cancel"]);
    }

    #[test]
    fn an_icon_is_carried_through() {
        let mut thing = item(ItemKind::Folder {
            path: r"C:\p".to_owned(),
        });
        thing.icon = Some("icons/p.png".to_owned());

        let rows = build(&thing);

        assert_eq!(icon_of(&rows).as_deref(), Some("icons/p.png"));
    }

    #[test]
    fn a_blank_icon_collects_as_unset() {
        let rows = build(&item(ItemKind::Folder {
            path: r"C:\p".to_owned(),
        }));

        assert_eq!(icon_of(&rows), None);
    }
}
