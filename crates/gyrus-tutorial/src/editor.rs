//! A very small text editor: enough to type a BrainFuck snippet into.
//!
//! Lesson programs are one or two short lines. This handles insertion,
//! deletion, and moving a caret around, and deliberately handles nothing else —
//! no selection, no undo, no clipboard. Anything more and it would need tests
//! about editing rather than about BrainFuck.

use gyrus_tui::Position;

/// A buffer of text with a caret in it.
#[derive(Debug, Clone)]
pub struct Editor {
    lines: Vec<String>,
    /// Caret line, 0-indexed.
    line: usize,
    /// Caret column in characters, 0-indexed. May sit one past the last
    /// character, which is where typing appends.
    column: usize,
}

impl Editor {
    /// An editor holding `text`, caret at the end.
    pub fn new(text: &str) -> Self {
        let lines: Vec<String> = if text.is_empty() {
            vec![String::new()]
        } else {
            text.lines().map(str::to_owned).collect()
        };
        let line = lines.len() - 1;
        let column = lines[line].chars().count();
        Self {
            lines,
            line,
            column,
        }
    }

    /// The text, with `\n` between lines.
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// The caret as a 1-indexed line and column, for the source panel.
    pub fn cursor(&self) -> Position {
        (self.line + 1, self.column + 1)
    }

    fn width(&self, line: usize) -> usize {
        self.lines[line].chars().count()
    }

    fn byte_offset(&self, line: usize, column: usize) -> usize {
        self.lines[line]
            .char_indices()
            .nth(column)
            .map_or(self.lines[line].len(), |(offset, _)| offset)
    }

    /// Insert a character at the caret.
    pub fn insert(&mut self, ch: char) {
        let offset = self.byte_offset(self.line, self.column);
        self.lines[self.line].insert(offset, ch);
        self.column += 1;
    }

    /// Split the current line at the caret.
    pub fn newline(&mut self) {
        let offset = self.byte_offset(self.line, self.column);
        let rest = self.lines[self.line].split_off(offset);
        self.lines.insert(self.line + 1, rest);
        self.line += 1;
        self.column = 0;
    }

    /// Delete the character before the caret, joining lines at a line start.
    pub fn backspace(&mut self) {
        if self.column > 0 {
            let offset = self.byte_offset(self.line, self.column - 1);
            self.lines[self.line].remove(offset);
            self.column -= 1;
        } else if self.line > 0 {
            let tail = self.lines.remove(self.line);
            self.line -= 1;
            self.column = self.width(self.line);
            self.lines[self.line].push_str(&tail);
        }
    }

    /// Delete the character under the caret.
    pub fn delete(&mut self) {
        if self.column < self.width(self.line) {
            let offset = self.byte_offset(self.line, self.column);
            self.lines[self.line].remove(offset);
        } else if self.line + 1 < self.lines.len() {
            let tail = self.lines.remove(self.line + 1);
            self.lines[self.line].push_str(&tail);
        }
    }

    /// Move the caret one character left, wrapping to the previous line.
    pub fn left(&mut self) {
        if self.column > 0 {
            self.column -= 1;
        } else if self.line > 0 {
            self.line -= 1;
            self.column = self.width(self.line);
        }
    }

    /// Move the caret one character right, wrapping to the next line.
    pub fn right(&mut self) {
        if self.column < self.width(self.line) {
            self.column += 1;
        } else if self.line + 1 < self.lines.len() {
            self.line += 1;
            self.column = 0;
        }
    }

    /// Move the caret up a line, keeping it on the line.
    pub fn up(&mut self) {
        if self.line > 0 {
            self.line -= 1;
            self.column = self.column.min(self.width(self.line));
        }
    }

    /// Move the caret down a line, keeping it on the line.
    pub fn down(&mut self) {
        if self.line + 1 < self.lines.len() {
            self.line += 1;
            self.column = self.column.min(self.width(self.line));
        }
    }

    /// Move the caret to the start of the line.
    pub fn home(&mut self) {
        self.column = 0;
    }

    /// Move the caret to the end of the line.
    pub fn end(&mut self) {
        self.column = self.width(self.line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_editor_puts_the_caret_after_the_text() {
        let editor = Editor::new("+++");
        assert_eq!(editor.cursor(), (1, 4));
        assert_eq!(editor.text(), "+++");
    }

    #[test]
    fn an_empty_editor_has_one_line() {
        let editor = Editor::new("");
        assert_eq!(editor.cursor(), (1, 1));
        assert_eq!(editor.text(), "");
    }

    #[test]
    fn typing_inserts_at_the_caret() {
        let mut editor = Editor::new("++");
        editor.left();
        editor.insert('-');
        assert_eq!(editor.text(), "+-+");
        assert_eq!(editor.cursor(), (1, 3));
    }

    #[test]
    fn backspace_at_a_line_start_joins_the_lines() {
        let mut editor = Editor::new("ab\ncd");
        editor.home();
        editor.backspace();
        assert_eq!(editor.text(), "abcd");
        assert_eq!(editor.cursor(), (1, 3));
    }

    #[test]
    fn backspace_at_the_very_start_does_nothing() {
        let mut editor = Editor::new("ab");
        editor.home();
        editor.backspace();
        assert_eq!(editor.text(), "ab");
    }

    #[test]
    fn enter_splits_the_line() {
        let mut editor = Editor::new("abcd");
        editor.home();
        editor.right();
        editor.right();
        editor.newline();
        assert_eq!(editor.text(), "ab\ncd");
        assert_eq!(editor.cursor(), (2, 1));
    }

    #[test]
    fn delete_at_the_end_of_a_line_pulls_the_next_one_up() {
        let mut editor = Editor::new("ab\ncd");
        editor.up();
        editor.end();
        editor.delete();
        assert_eq!(editor.text(), "abcd");
    }

    #[test]
    fn moving_up_clamps_to_the_shorter_line() {
        let mut editor = Editor::new("ab\ncdef");
        editor.end();
        assert_eq!(editor.cursor(), (2, 5));
        editor.up();
        assert_eq!(editor.cursor(), (1, 3));
    }

    #[test]
    fn arrows_wrap_between_lines() {
        let mut editor = Editor::new("ab\ncd");
        editor.up();
        editor.end();
        editor.right();
        assert_eq!(editor.cursor(), (2, 1));
        editor.left();
        assert_eq!(editor.cursor(), (1, 3));
    }
}
