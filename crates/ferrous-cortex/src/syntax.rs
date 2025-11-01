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
use termcolor::{Ansi, Color, ColorSpec, WriteColor};

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

impl HighlightedLine {
    /// Get the spans in this line
    pub fn spans(&self) -> &[HighlightedSpan] {
        &self.spans
    }
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
    LoopStart(usize), // Nesting depth (0 = outermost)
    LoopEnd(usize),   // Nesting depth (0 = outermost)
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
        let mut nesting_depth = 0;

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
                        '[' => {
                            let depth = nesting_depth;
                            nesting_depth += 1;
                            SpanStyle::LoopStart(depth)
                        }
                        ']' => {
                            // Prevent underflow if brackets are unmatched
                            nesting_depth = nesting_depth.saturating_sub(1);
                            SpanStyle::LoopEnd(nesting_depth)
                        }
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

    /// Write ANSI-colored output to a writer (all lines).
    pub fn write_ansi<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        self.write_ansi_range(writer, 0, self.lines.len())
    }

    /// Get color for a specific nesting depth.
    fn color_for_depth(depth: usize) -> (u8, u8, u8) {
        // Cycle through colors for different nesting levels
        match depth % 6 {
            0 => (255, 0, 255),   // Magenta - depth 0
            1 => (0, 255, 255),   // Cyan - depth 1
            2 => (255, 128, 0),   // Orange - depth 2
            3 => (255, 0, 128),   // Pink - depth 3
            4 => (128, 255, 0),   // Lime - depth 4
            5 => (128, 128, 255), // Light blue - depth 5
            _ => (255, 0, 255),   // Fallback to magenta
        }
    }

    /// Write ANSI-colored output for a range of lines to a writer.
    pub fn write_ansi_range<W: Write>(
        &self,
        writer: &mut W,
        start_line: usize,
        end_line: usize,
    ) -> std::io::Result<()> {
        let theme = ColorTheme::default();
        let mut ansi_writer = Ansi::new(writer);

        for (idx, line) in self.lines.iter().enumerate() {
            if idx < start_line || idx >= end_line {
                continue;
            }
            // Write line number if present
            if let Some(num) = line.number {
                let mut line_num_spec = ColorSpec::new();
                line_num_spec.set_fg(Some(Color::Rgb(100, 100, 100)));
                ansi_writer.set_color(&line_num_spec)?;
                write!(ansi_writer, "{:4} │", num)?;
                ansi_writer.reset()?;
                write!(ansi_writer, " ")?;
            }

            // Write spans with colors
            for span in &line.spans {
                let color = match span.style {
                    SpanStyle::Movement => Some(theme.movement),
                    SpanStyle::Arithmetic => Some(theme.arithmetic),
                    SpanStyle::Io => Some(theme.io),
                    SpanStyle::LoopStart(_) | SpanStyle::LoopEnd(_) => Some(theme.loops),
                    SpanStyle::Comment => Some(theme.comments),
                    SpanStyle::Whitespace => None,
                };

                if let Some(base_color) = color {
                    let final_color = match span.style {
                        // Use depth-based colors for loops
                        SpanStyle::LoopStart(depth) | SpanStyle::LoopEnd(depth) => {
                            let (r, g, b) = Self::color_for_depth(depth);
                            Color::Rgb(r, g, b)
                        }
                        // Use theme colors for other instructions
                        _ => match base_color {
                            Color::Blue => Color::Rgb(0, 150, 255),
                            Color::Green => Color::Rgb(0, 200, 0),
                            other => other,
                        },
                    };

                    let mut color_spec = ColorSpec::new();
                    color_spec.set_fg(Some(final_color));

                    // Special styling for loops (bold)
                    if matches!(span.style, SpanStyle::LoopStart(_) | SpanStyle::LoopEnd(_)) {
                        color_spec.set_bold(true);
                    }

                    ansi_writer.set_color(&color_spec)?;
                    write!(ansi_writer, "{}", span.text)?;
                    ansi_writer.reset()?;
                } else {
                    write!(ansi_writer, "{}", span.text)?;
                }
            }

            writeln!(ansi_writer)?;
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
        assert_eq!(spans[6].style, SpanStyle::LoopStart(0)); // [ - depth 0
        assert_eq!(spans[7].style, SpanStyle::LoopEnd(0)); // ] - depth 0
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

    #[test]
    fn test_loop_nesting_depth() {
        let highlighter = SyntaxHighlighter::new();
        // Test nested loops: depth 0, 1, 2
        let code = highlighter.highlight("[+[>[-]<]>]");

        let spans = &code.lines[0].spans;

        // [ at depth 0
        assert_eq!(spans[0].style, SpanStyle::LoopStart(0));
        // +
        assert_eq!(spans[1].style, SpanStyle::Arithmetic);
        // [ at depth 1
        assert_eq!(spans[2].style, SpanStyle::LoopStart(1));
        // >
        assert_eq!(spans[3].style, SpanStyle::Movement);
        // [ at depth 2
        assert_eq!(spans[4].style, SpanStyle::LoopStart(2));
        // -
        assert_eq!(spans[5].style, SpanStyle::Arithmetic);
        // ] at depth 2 (closes depth 2, returns to depth 2)
        assert_eq!(spans[6].style, SpanStyle::LoopEnd(2));
        // <
        assert_eq!(spans[7].style, SpanStyle::Movement);
        // ] at depth 1 (closes depth 1, returns to depth 1)
        assert_eq!(spans[8].style, SpanStyle::LoopEnd(1));
        // >
        assert_eq!(spans[9].style, SpanStyle::Movement);
        // ] at depth 0 (closes depth 0, returns to depth 0)
        assert_eq!(spans[10].style, SpanStyle::LoopEnd(0));
    }

    #[test]
    fn test_loop_nesting_across_lines() {
        let highlighter = SyntaxHighlighter::new();
        // Test nested loops across multiple lines
        let source = "[\n  [\n    +\n  ]\n]";
        let code = highlighter.highlight(source);

        // Line 0: [
        assert_eq!(code.lines[0].spans[0].style, SpanStyle::LoopStart(0));

        // Line 1: whitespace + [
        assert_eq!(code.lines[1].spans[2].style, SpanStyle::LoopStart(1));

        // Line 2: whitespace + +
        assert_eq!(code.lines[2].spans[4].style, SpanStyle::Arithmetic);

        // Line 3: whitespace + ]
        assert_eq!(code.lines[3].spans[2].style, SpanStyle::LoopEnd(1));

        // Line 4: ]
        assert_eq!(code.lines[4].spans[0].style, SpanStyle::LoopEnd(0));
    }

    #[test]
    fn test_unmatched_brackets_nesting() {
        let highlighter = SyntaxHighlighter::new();
        // Test that unmatched ] doesn't cause underflow
        let code = highlighter.highlight("]][+]");

        let spans = &code.lines[0].spans;

        // First ] at depth 0 (underflow prevented)
        assert_eq!(spans[0].style, SpanStyle::LoopEnd(0));
        // Second ] at depth 0 (still at 0)
        assert_eq!(spans[1].style, SpanStyle::LoopEnd(0));
        // [ at depth 0
        assert_eq!(spans[2].style, SpanStyle::LoopStart(0));
        // +
        assert_eq!(spans[3].style, SpanStyle::Arithmetic);
        // ] at depth 0 (closes the [)
        assert_eq!(spans[4].style, SpanStyle::LoopEnd(0));
    }
}
