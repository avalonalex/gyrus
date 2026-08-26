//! Colors shared by every gyrus terminal interface.
//!
//! The instruction colors deliberately match [`gyrus::syntax::ColorTheme`], the
//! scheme `gyrus-tool view` and the error messages already use. Someone who has
//! read a parse error should recognize the same `[` in the debugger's source
//! panel without having to re-learn what magenta means.

use gyrus::syntax::CharClass;
use ratatui::style::{Color, Modifier, Style};

/// What a source character is, for coloring purposes.
///
/// A presentation-side view of [`gyrus::syntax::CharClass`], which is where the
/// language's rule about what counts as code actually lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// `>` and `<`
    Movement,
    /// `+` and `-`
    Arithmetic,
    /// `.` and `,`
    Io,
    /// `[` and `]`
    Loop,
    /// Anything else: whitespace, `*` line comments, stray text
    Comment,
}

impl From<CharClass> for Category {
    fn from(class: CharClass) -> Self {
        match class {
            CharClass::Movement => Category::Movement,
            CharClass::Arithmetic => Category::Arithmetic,
            CharClass::Io => Category::Io,
            CharClass::LoopStart | CharClass::LoopEnd => Category::Loop,
            CharClass::Whitespace | CharClass::Comment => Category::Comment,
        }
    }
}

/// Classify every character of one source line, honoring `*` line comments.
///
/// Returns one category per `char` of `line`, so the result lines up with
/// `line.chars()` -- not with byte offsets. The rule itself is the library's:
/// whether a `+` executes is a fact about BrainFuck, not about this panel.
pub fn classify_line(line: &str) -> Vec<Category> {
    gyrus::syntax::classify_line(line)
        .into_iter()
        .map(Category::from)
        .collect()
}

/// The palette every gyrus TUI draws with.
#[derive(Debug, Clone)]
pub struct Theme {
    // Instruction categories (mirrors gyrus::syntax::ColorTheme)
    pub movement: Color,
    pub arithmetic: Color,
    pub io: Color,
    pub loops: Color,
    pub comment: Color,

    // Chrome
    pub border: Color,
    pub border_focused: Color,
    pub title: Color,
    pub dim: Color,
    pub accent: Color,

    // Debugger state
    pub current: Color,
    pub breakpoint: Color,
    pub modified: Color,
    pub pointer: Color,
    pub error: Color,
    pub success: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            movement: Color::Rgb(80, 160, 255),
            arithmetic: Color::Rgb(80, 200, 120),
            io: Color::Rgb(230, 200, 90),
            loops: Color::Rgb(210, 130, 230),
            comment: Color::Rgb(110, 110, 118),

            border: Color::Rgb(80, 80, 90),
            border_focused: Color::Rgb(80, 160, 255),
            title: Color::Rgb(200, 200, 210),
            dim: Color::Rgb(130, 130, 140),
            accent: Color::Rgb(120, 200, 255),

            current: Color::Rgb(120, 240, 160),
            breakpoint: Color::Rgb(240, 90, 90),
            modified: Color::Rgb(240, 190, 90),
            pointer: Color::Rgb(120, 240, 160),
            error: Color::Rgb(240, 90, 90),
            success: Color::Rgb(120, 240, 160),
        }
    }
}

impl Theme {
    /// The style a source character in `category` is drawn with.
    pub fn category(&self, category: Category) -> Style {
        let color = match category {
            Category::Movement => self.movement,
            Category::Arithmetic => self.arithmetic,
            Category::Io => self.io,
            Category::Loop => self.loops,
            Category::Comment => self.comment,
        };
        Style::default().fg(color)
    }

    /// Border style for a panel, brighter when it has keyboard focus.
    pub fn border_style(&self, focused: bool) -> Style {
        if focused {
            Style::default().fg(self.border_focused)
        } else {
            Style::default().fg(self.border)
        }
    }

    /// Panel title style.
    pub fn title_style(&self) -> Style {
        Style::default().fg(self.title).add_modifier(Modifier::BOLD)
    }

    /// Style for de-emphasized text (line numbers, units, inactive hints).
    pub fn dim_style(&self) -> Style {
        Style::default().fg(self.dim)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_each_instruction_group() {
        let categories = classify_line("><+-.,[]");
        assert_eq!(
            categories,
            vec![
                Category::Movement,
                Category::Movement,
                Category::Arithmetic,
                Category::Arithmetic,
                Category::Io,
                Category::Io,
                Category::Loop,
                Category::Loop,
            ]
        );
    }

    #[test]
    fn a_star_starts_a_comment_that_runs_to_the_end_of_the_line() {
        // The `*` itself is part of the comment, and so is the `+` after it --
        // which is the whole point: the parser ignores it, so coloring it green
        // would say it executes.
        let categories = classify_line("+* +");
        assert_eq!(
            categories,
            vec![
                Category::Arithmetic,
                Category::Comment,
                Category::Comment,
                Category::Comment,
            ]
        );
    }

    #[test]
    fn text_between_instructions_is_a_comment() {
        assert_eq!(classify_line("hi"), vec![Category::Comment; 2]);
    }
}
