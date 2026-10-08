//! A text value with a caret, edited by the keys the input hook hands over.
//!
//! The overlay never takes window focus, so there is no Win32 edit control to lean on.
//! Characters are held as a `Vec<char>` rather than a `String` so the caret is a plain
//! index and can never land between the bytes of one character.

/// Keeps one runaway paste from growing a field without bound.
const MAX_LEN: usize = 4096;

#[derive(Debug, Clone, Default)]
pub struct TextValue {
    chars: Vec<char>,
    caret: usize,
}

impl TextValue {
    pub fn new(text: &str) -> Self {
        let chars: Vec<char> = text.chars().take(MAX_LEN).collect();
        let caret = chars.len();

        Self { chars, caret }
    }

    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn insert(&mut self, character: char) {
        if self.chars.len() >= MAX_LEN {
            return;
        }

        self.chars.insert(self.caret, character);
        self.caret += 1;
    }

    pub fn insert_str(&mut self, text: &str) {
        for character in text.chars() {
            self.insert(character);
        }
    }

    /// Removes the character before the caret.
    pub fn backspace(&mut self) {
        if self.caret == 0 {
            return;
        }

        self.caret -= 1;
        self.chars.remove(self.caret);
    }

    /// Removes the character after the caret.
    pub fn delete(&mut self) {
        if self.caret >= self.chars.len() {
            return;
        }
        self.chars.remove(self.caret);
    }

    pub fn left(&mut self) {
        self.caret = self.caret.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.caret = (self.caret + 1).min(self.chars.len());
    }

    pub fn home(&mut self) {
        self.caret = 0;
    }

    pub fn end(&mut self) {
        self.caret = self.chars.len();
    }

    /// Text split at the caret, so the renderer can place the caret without measuring the
    /// whole string twice.
    pub fn split_at_caret(&self) -> (String, String) {
        let before: String = self.chars[..self.caret].iter().collect();
        let after: String = self.chars[self.caret..].iter().collect();

        (before, after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The caret split is how the renderer finds the caret, so it stands in for reading
    /// the index directly.
    fn caret(value: &TextValue) -> usize {
        value.split_at_caret().0.chars().count()
    }

    fn length(value: &TextValue) -> usize {
        value.text().chars().count()
    }

    #[test]
    fn a_new_value_puts_the_caret_at_the_end() {
        let value = TextValue::new("hello");

        assert_eq!(caret(&value), 5);
        assert_eq!(value.text(), "hello");
    }

    #[test]
    fn typing_inserts_at_the_caret() {
        let mut value = TextValue::new("ac");
        value.left();

        value.insert('b');

        assert_eq!(value.text(), "abc");
        assert_eq!(caret(&value), 2);
    }

    #[test]
    fn backspace_removes_the_character_before_the_caret() {
        let mut value = TextValue::new("abc");

        value.backspace();

        assert_eq!(value.text(), "ab");
        assert_eq!(caret(&value), 2);
    }

    #[test]
    fn backspace_at_the_start_does_nothing() {
        let mut value = TextValue::new("abc");
        value.home();

        value.backspace();

        assert_eq!(value.text(), "abc");
        assert_eq!(caret(&value), 0);
    }

    #[test]
    fn delete_removes_the_character_after_the_caret() {
        let mut value = TextValue::new("abc");
        value.home();

        value.delete();

        assert_eq!(value.text(), "bc");
        assert_eq!(caret(&value), 0);
    }

    #[test]
    fn delete_at_the_end_does_nothing() {
        let mut value = TextValue::new("abc");

        value.delete();

        assert_eq!(value.text(), "abc");
    }

    #[test]
    fn the_caret_stops_at_both_ends() {
        let mut value = TextValue::new("ab");

        value.left();
        value.left();
        value.left();
        assert_eq!(caret(&value), 0);

        value.right();
        value.right();
        value.right();
        assert_eq!(caret(&value), 2);
    }

    #[test]
    fn home_and_end_jump_to_the_ends() {
        let mut value = TextValue::new("abcd");

        value.home();
        assert_eq!(caret(&value), 0);

        value.end();
        assert_eq!(caret(&value), 4);
    }

    #[test]
    fn pasting_inserts_the_whole_run_at_the_caret() {
        let mut value = TextValue::new("ad");
        value.left();

        value.insert_str("bc");

        assert_eq!(value.text(), "abcd");
        assert_eq!(caret(&value), 3);
    }

    /// A path pasted from Explorer is the main thing these fields take.
    #[test]
    fn a_pasted_path_survives_intact() {
        let path = r"C:\Program Files\Git\bin\bash.exe";
        let mut value = TextValue::default();

        value.insert_str(path);

        assert_eq!(value.text(), path);
    }

    /// Caret arithmetic on a Vec<char> is what makes this safe; on bytes it would split
    /// the character.
    #[test]
    fn a_multi_byte_character_counts_as_one_step() {
        let mut value = TextValue::new("aéb");

        value.left();
        value.backspace();

        assert_eq!(value.text(), "ab");
    }

    #[test]
    fn a_runaway_paste_is_capped() {
        let mut value = TextValue::default();

        value.insert_str(&"x".repeat(MAX_LEN * 2));

        assert_eq!(length(&value), MAX_LEN);
    }

    #[test]
    fn splitting_at_the_caret_gives_both_halves() {
        let mut value = TextValue::new("abcd");
        value.left();
        value.left();

        assert_eq!(value.split_at_caret(), ("ab".to_owned(), "cd".to_owned()));
    }
}
