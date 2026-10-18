/// A single-line text buffer with a movable cursor.
///
/// The cursor is tracked as a character index rather than a byte offset so that
/// multi-byte UTF-8 input behaves correctly.
#[derive(Debug, Clone, Default)]
pub struct InputState {
    value: String,
    cursor: usize,
}

impl InputState {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        let cursor = value.chars().count();
        InputState { value, cursor }
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn len(&self) -> usize {
        self.value.chars().count()
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }

    pub fn insert_char(&mut self, c: char) {
        let index = self.byte_index(self.cursor);
        self.value.insert(index, c);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }

        let start = self.byte_index(self.cursor - 1);
        let end = self.byte_index(self.cursor);
        self.value.replace_range(start..end, "");
        self.cursor -= 1;
    }

    pub fn delete(&mut self) {
        if self.cursor >= self.len() {
            return;
        }

        let start = self.byte_index(self.cursor);
        let end = self.byte_index(self.cursor + 1);
        self.value.replace_range(start..end, "");
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.len() {
            self.cursor += 1;
        }
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.len();
    }

    fn byte_index(&self, char_index: usize) -> usize {
        self.value
            .char_indices()
            .nth(char_index)
            .map(|(index, _)| index)
            .unwrap_or(self.value.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_and_backspaces() {
        let mut input = InputState::new("");
        input.insert_char('a');
        input.insert_char('b');
        input.insert_char('c');

        assert_eq!(input.value(), "abc");
        assert_eq!(input.cursor(), 3);

        input.backspace();
        assert_eq!(input.value(), "ab");
        assert_eq!(input.cursor(), 2);
    }

    #[test]
    fn moves_cursor_before_inserting() {
        let mut input = InputState::new("ac");
        input.move_home();
        input.move_right();
        input.insert_char('b');

        assert_eq!(input.value(), "abc");
        assert_eq!(input.cursor(), 2);
    }

    #[test]
    fn handles_multibyte_characters() {
        let mut input = InputState::new("héllo");
        input.move_end();
        input.backspace();
        input.backspace();

        assert_eq!(input.value(), "hél");
        assert_eq!(input.cursor(), 3);
    }

    #[test]
    fn deletes_character_at_cursor() {
        let mut input = InputState::new("abc");
        input.move_home();
        input.delete();

        assert_eq!(input.value(), "bc");
        assert_eq!(input.cursor(), 0);
    }

    #[test]
    fn cursor_never_leaves_bounds() {
        let mut input = InputState::new("");
        input.move_left();
        input.move_right();
        input.backspace();
        input.delete();

        assert_eq!(input.value(), "");
        assert_eq!(input.cursor(), 0);
    }
}
