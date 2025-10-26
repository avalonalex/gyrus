//! Syntax highlighting for BrainFuck code.
//!
//! This module provides reusable syntax highlighting functionality for BrainFuck programs.
//! It can be used by various tools (CLI, TUI, debugger) to display code with colors and
//! visual indicators.
//!
//! # Features
//!
//! - Highlight different instruction categories (movement, arithmetic, I/O, loops, comments)
//! - Multiple output formats (ANSI terminal, plain text)
//! - Optional line numbers
//! - Loop nesting visualization
//!
//! # Example
//!
//! ```
//! use ferrous_cortex::syntax::SyntaxHighlighter;
//!
//! let highlighter = SyntaxHighlighter::new();
//! let highlighted = highlighter.highlight("+++++[>+<-]");
//! let ansi_output = highlighted.to_ansi();
//! println!("{}", ansi_output);
//! ```

use std::io::Write;
use termcolor::Color;

/// Syntax highlighter for BrainFuck code.
#[derive(Debug, Clone)]
pub struct SyntaxHighlighter {
    theme: ColorTheme,
    show_line_numbers: bool,
}

/// Color theme for syntax highlighting.
#[derive(Debug, Clone)]
pub struct ColorTheme {
    pub movement: Color,     // <> commands
    pub arithmetic: Color,   // +- commands
    pub io: Color,           // ,. commands
    pub loops: Color,        // [] commands
    pub comments: Color,     // comments
    pub line_numbers: Color, // line number prefix
}

impl Default for ColorTheme {
    fn default() -> Self {
        Self {
            movement: Color::Blue,
            arithmetic: Color::Green,
            io: Color::Yellow,
            loops: Color::Magenta,
            comments: Color::Rgb(128, 128, 128),     // Gray
            line_numbers: Color::Rgb(100, 100, 100), // Dim gray
        }
    }
}

impl ColorTheme {
    /// Light theme (for light backgrounds).
    pub fn light() -> Self {
        Self {
            movement: Color::Blue,
            arithmetic: Color::Green,
            io: Color::Rgb(180, 180, 0), // Dark yellow
            loops: Color::Magenta,
            comments: Color::Rgb(100, 100, 100),     // Dark gray
            line_numbers: Color::Rgb(150, 150, 150), // Medium gray
        }
    }

    /// Dark theme (for dark backgrounds) - same as default.
    pub fn dark() -> Self {
        Self::default()
    }
}

/// Highlighted code ready for display.
#[derive(Debug)]
pub struct HighlightedCode {
    lines: Vec<HighlightedLine>,
}

/// A single highlighted line.
#[derive(Debug)]
pub struct HighlightedLine {
    number: Option<usize>,
    content: String,
    spans: Vec<HighlightedSpan>,
}

/// A span of text with associated style.
#[derive(Debug, Clone)]
pub struct HighlightedSpan {
    pub text: String,
    pub style: SpanStyle,
    pub start: usize, // Start position in line
    pub end: usize,   // End position in line
}

/// Style category for a span of text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanStyle {
    Movement,
    Arithmetic,
    Io,
    LoopStart,
    LoopEnd,
    Comment,
    Whitespace,
}

impl SyntaxHighlighter {
    /// Create a new syntax highlighter with default theme.
    pub fn new() -> Self {
        Self {
            theme: ColorTheme::default(),
            show_line_numbers: false,
        }
    }

    /// Create a syntax highlighter with a custom theme.
    pub fn with_theme(theme: ColorTheme) -> Self {
        Self {
            theme,
            show_line_numbers: false,
        }
    }

    /// Enable or disable line numbers.
    pub fn show_line_numbers(mut self, show: bool) -> Self {
        self.show_line_numbers = show;
        self
    }

    /// Highlight source code.
    pub fn highlight(&self, source: &str) -> HighlightedCode {
        let mut lines = Vec::new();

        for (line_num, line) in source.lines().enumerate() {
            let mut spans = Vec::new();
            let mut current_pos = 0;

            // Line comment state for this line
            let mut in_line_comment = false;

            for ch in line.chars() {
                let char_len = ch.len_utf8();
                let style = if in_line_comment {
                    SpanStyle::Comment
                } else {
                    match ch {
                        '>' | '<' => SpanStyle::Movement,
                        '+' | '-' => SpanStyle::Arithmetic,
                        ',' | '.' => SpanStyle::Io,
                        '[' => SpanStyle::LoopStart,
                        ']' => SpanStyle::LoopEnd,
                        '*' => {
                            in_line_comment = true;
                            SpanStyle::Comment
                        }
                        _ if ch.is_whitespace() => SpanStyle::Whitespace,
                        _ => SpanStyle::Comment, // Non-BF chars are implicit comments
                    }
                };

                spans.push(HighlightedSpan {
                    text: ch.to_string(),
                    style,
                    start: current_pos,
                    end: current_pos + char_len,
                });

                current_pos += char_len;
            }

            let line_number = if self.show_line_numbers {
                Some(line_num + 1)
            } else {
                None
            };

            lines.push(HighlightedLine {
                number: line_number,
                content: line.to_string(),
                spans,
            });
        }

        HighlightedCode { lines }
    }

    /// Get the theme being used.
    pub fn theme(&self) -> &ColorTheme {
        &self.theme
    }
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl HighlightedCode {
    /// Convert to ANSI-colored string for terminal output.
    pub fn to_ansi(&self) -> String {
        let mut buffer = Vec::new();
        self.write_ansi(&mut buffer).unwrap();
        String::from_utf8(buffer).unwrap()
    }

    /// Write ANSI-colored output to a writer.
    pub fn write_ansi<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let theme = ColorTheme::default();

        for line in &self.lines {
            // Write line number if present
            if let Some(num) = line.number {
                write!(writer, "\x1b[38;2;100;100;100m{:4} │\x1b[0m ", num)?;
            }

            // Write spans with colors
            for span in &line.spans {
                let color = match span.style {
                    SpanStyle::Movement => Some(theme.movement),
                    SpanStyle::Arithmetic => Some(theme.arithmetic),
                    SpanStyle::Io => Some(theme.io),
                    SpanStyle::LoopStart | SpanStyle::LoopEnd => Some(theme.loops),
                    SpanStyle::Comment => Some(theme.comments),
                    SpanStyle::Whitespace => None,
                };

                if let Some(color) = color {
                    let (r, g, b) = match color {
                        Color::Black => (0, 0, 0),
                        Color::Blue => (0, 150, 255),
                        Color::Green => (0, 200, 0),
                        Color::Red => (255, 0, 0),
                        Color::Cyan => (0, 255, 255),
                        Color::Magenta => (255, 0, 255),
                        Color::Yellow => (255, 255, 0),
                        Color::White => (255, 255, 255),
                        Color::Rgb(r, g, b) => (r, g, b),
                        _ => (200, 200, 200),
                    };

                    // Special styling for loops (bold)
                    if matches!(span.style, SpanStyle::LoopStart | SpanStyle::LoopEnd) {
                        write!(writer, "\x1b[1;38;2;{};{};{}m", r, g, b)?;
                    } else {
                        write!(writer, "\x1b[38;2;{};{};{}m", r, g, b)?;
                    }
                    write!(writer, "{}", span.text)?;
                    write!(writer, "\x1b[0m")?;
                } else {
                    write!(writer, "{}", span.text)?;
                }
            }

            writeln!(writer)?;
        }

        Ok(())
    }

    /// Convert to plain text (no colors).
    pub fn to_plain(&self) -> String {
        let mut result = String::new();

        for line in &self.lines {
            // Add line number if present
            if let Some(num) = line.number {
                result.push_str(&format!("{:4} │ ", num));
            }

            // Add line content
            result.push_str(&line.content);
            result.push('\n');
        }

        result
    }

    /// Get the lines.
    pub fn lines(&self) -> &[HighlightedLine] {
        &self.lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_basic_commands() {
        let highlighter = SyntaxHighlighter::new();
        let code = highlighter.highlight("+-<>.,[]");

        assert_eq!(code.lines.len(), 1);
        let spans = &code.lines[0].spans;

        assert_eq!(spans[0].style, SpanStyle::Arithmetic); // +
        assert_eq!(spans[1].style, SpanStyle::Arithmetic); // -
        assert_eq!(spans[2].style, SpanStyle::Movement); // <
        assert_eq!(spans[3].style, SpanStyle::Movement); // >
        assert_eq!(spans[4].style, SpanStyle::Io); // .
        assert_eq!(spans[5].style, SpanStyle::Io); // ,
        assert_eq!(spans[6].style, SpanStyle::LoopStart); // [
        assert_eq!(spans[7].style, SpanStyle::LoopEnd); // ]
    }

    #[test]
    fn test_highlight_comments() {
        let highlighter = SyntaxHighlighter::new();
        let code = highlighter.highlight("+++  * this is a comment");

        let spans = &code.lines[0].spans;
        assert_eq!(spans[0].style, SpanStyle::Arithmetic); // +
        assert_eq!(spans[1].style, SpanStyle::Arithmetic); // +
        assert_eq!(spans[2].style, SpanStyle::Arithmetic); // +
        assert_eq!(spans[3].style, SpanStyle::Whitespace); // space
        assert_eq!(spans[4].style, SpanStyle::Whitespace); // space
        assert_eq!(spans[5].style, SpanStyle::Comment); // *
        // Everything after * is comment
        for span in &spans[6..] {
            assert_eq!(span.style, SpanStyle::Comment);
        }
    }

    #[test]
    fn test_highlight_implicit_comments() {
        let highlighter = SyntaxHighlighter::new();
        let code = highlighter.highlight("+hello-");

        let spans = &code.lines[0].spans;
        assert_eq!(spans[0].style, SpanStyle::Arithmetic); // +
        // "hello" are all implicit comments (non-BF characters)
        assert_eq!(spans[1].style, SpanStyle::Comment); // h
        assert_eq!(spans[2].style, SpanStyle::Comment); // e
        assert_eq!(spans[3].style, SpanStyle::Comment); // l
        assert_eq!(spans[4].style, SpanStyle::Comment); // l
        assert_eq!(spans[5].style, SpanStyle::Comment); // o
        assert_eq!(spans[6].style, SpanStyle::Arithmetic); // -
    }

    #[test]
    fn test_line_numbers() {
        let highlighter = SyntaxHighlighter::new().show_line_numbers(true);
        let code = highlighter.highlight("+++\n---");

        assert_eq!(code.lines[0].number, Some(1));
        assert_eq!(code.lines[1].number, Some(2));
    }

    #[test]
    fn test_plain_output() {
        let highlighter = SyntaxHighlighter::new().show_line_numbers(true);
        let code = highlighter.highlight("+++\n---");
        let plain = code.to_plain();

        assert!(plain.contains("   1 │ +++"));
        assert!(plain.contains("   2 │ ---"));
    }

    #[test]
    fn test_ansi_output_contains_escape_codes() {
        let highlighter = SyntaxHighlighter::new();
        let code = highlighter.highlight("+-");
        let ansi = code.to_ansi();

        // Should contain ANSI escape sequences
        assert!(ansi.contains("\x1b["));
        assert!(ansi.contains("m"));
    }

    #[test]
    fn test_multiline() {
        let highlighter = SyntaxHighlighter::new();
        let source = "+++\n---\n<<<";
        let code = highlighter.highlight(source);

        assert_eq!(code.lines.len(), 3);
        assert_eq!(code.lines[0].spans.len(), 3);
        assert_eq!(code.lines[1].spans.len(), 3);
        assert_eq!(code.lines[2].spans.len(), 3);
    }
}
