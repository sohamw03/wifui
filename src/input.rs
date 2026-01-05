#[derive(Debug, Default, Clone)]
pub struct InputState {
    pub value: String,
    pub cursor: usize,
}

impl InputState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }

    pub fn insert(&mut self, c: char) {
        let byte_idx = self
            .value
            .chars()
            .take(self.cursor)
            .map(|c| c.len_utf8())
            .sum();
        self.value.insert(byte_idx, c);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            let byte_idx = self
                .value
                .chars()
                .take(self.cursor - 1)
                .map(|c| c.len_utf8())
                .sum();
            self.value.remove(byte_idx);
            self.cursor -= 1;
        }
    }

    pub fn backspace_word(&mut self) {
        if self.cursor > 0 {
            let chars: Vec<char> = self.value.chars().collect();
            let mut idx = self.cursor;
            // Skip trailing whitespace
            while idx > 0 && idx <= chars.len() && chars[idx - 1].is_whitespace() {
                idx -= 1;
            }
            // Skip word characters
            while idx > 0 && !chars[idx - 1].is_whitespace() {
                idx -= 1;
            }

            let start_byte = chars.iter().take(idx).map(|c| c.len_utf8()).sum::<usize>();
            let end_byte = chars
                .iter()
                .take(self.cursor)
                .map(|c| c.len_utf8())
                .sum::<usize>();

            self.value.replace_range(start_byte..end_byte, "");
            self.cursor = idx;
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_word_left(&mut self) {
        let chars: Vec<char> = self.value.chars().collect();
        let mut idx = self.cursor;
        while idx > 0 && idx <= chars.len() && chars[idx - 1].is_whitespace() {
            idx -= 1;
        }
        while idx > 0 && !chars[idx - 1].is_whitespace() {
            idx -= 1;
        }
        self.cursor = idx;
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.value.chars().count() {
            self.cursor += 1;
        }
    }

    pub fn move_word_right(&mut self) {
        let chars: Vec<char> = self.value.chars().collect();
        let mut idx = self.cursor;
        let len = chars.len();
        while idx < len && !chars[idx].is_whitespace() {
            idx += 1;
        }
        while idx < len && chars[idx].is_whitespace() {
            idx += 1;
        }
        self.cursor = idx;
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.value.chars().count();
    }

    /// Handle common input key events, returns true if the key was handled
    pub fn handle_key(&mut self, key: &crossterm::event::KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyModifiers};

        match key.code {
            KeyCode::Char(c) => {
                self.insert(c);
                true
            }
            KeyCode::Backspace
                if key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.backspace_word();
                true
            }
            KeyCode::Backspace => {
                self.backspace();
                true
            }
            KeyCode::Left
                if key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.move_word_left();
                true
            }
            KeyCode::Left => {
                self.move_left();
                true
            }
            KeyCode::Right
                if key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.move_word_right();
                true
            }
            KeyCode::Right => {
                self.move_right();
                true
            }
            KeyCode::Home => {
                self.move_home();
                true
            }
            KeyCode::End => {
                self.move_end();
                true
            }
            _ => false,
        }
    }
}
