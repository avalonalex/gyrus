//! Output panel: what the program has written so far.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::theme::Theme;

/// The program-output panel.
pub struct OutputView<'a> {
    bytes: &'a [u8],
    theme: &'a Theme,
    title: &'static str,
    focused: bool,
    scroll: Option<usize>,
}

impl<'a> OutputView<'a> {
    /// A panel showing everything the program has written.
    pub fn new(bytes: &'a [u8], theme: &'a Theme) -> Self {
        Self {
            bytes,
            theme,
            title: "Output",
            focused: false,
            scroll: None,
        }
    }

    /// Draw the border brighter, to show this panel has keyboard focus.
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// First visible line. `None` pins the view to the newest output.
    pub fn scroll(mut self, scroll: Option<usize>) -> Self {
        self.scroll = scroll;
        self
    }

    /// How many output lines fit in `area`.
    pub fn visible_lines(area: Rect) -> usize {
        area.height.saturating_sub(2) as usize
    }

    /// How many display lines `bytes` occupies.
    ///
    /// Counts rather than builds, because a panel showing seven lines should
    /// not pay to format a megabyte of output that scrolled past long ago.
    pub fn line_count(bytes: &[u8]) -> usize {
        bytes.iter().filter(|byte| **byte == b'\n').count() + 1
    }

    /// Byte offset where display line `index` starts.
    fn line_offset(bytes: &[u8], index: usize) -> usize {
        if index == 0 {
            return 0;
        }
        let mut seen = 0;
        for (offset, byte) in bytes.iter().enumerate() {
            if *byte == b'\n' {
                seen += 1;
                if seen == index {
                    return offset + 1;
                }
            }
        }
        bytes.len()
    }

    /// Split output into display lines, rendering control bytes visibly.
    ///
    /// A BrainFuck program can write any byte, and a raw `\r` or `\x07` sent
    /// straight to the terminal would corrupt the interface drawn around it.
    pub fn display_lines(bytes: &[u8]) -> Vec<String> {
        let mut lines = vec![String::new()];
        for &byte in bytes {
            match byte {
                b'\n' => lines.push(String::new()),
                b'\t' => lines.last_mut().expect("never empty").push_str("    "),
                0x20..=0x7e => lines.last_mut().expect("never empty").push(byte as char),
                _ => lines.last_mut().expect("never empty").push('·'),
            }
        }
        lines
    }
}

impl Widget for OutputView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let height = Self::visible_lines(area);
        let total = Self::line_count(self.bytes);
        let start = match self.scroll {
            Some(scroll) => scroll.min(total.saturating_sub(1)),
            None => total.saturating_sub(height),
        };

        // Format only the lines that will be drawn. A program left running
        // writes output faster than anyone reads it, and the panel redraws
        // sixteen times a second regardless of how much has piled up.
        let offset = Self::line_offset(self.bytes, start);
        let lines: Vec<Line> = Self::display_lines(&self.bytes[offset..])
            .into_iter()
            .take(height)
            .map(|line| Line::from(Span::styled(line, Style::default().fg(self.theme.title))))
            .collect();

        let title = Line::from(vec![
            Span::styled(" ", self.theme.dim_style()),
            Span::styled(self.title, self.theme.title_style()),
            Span::styled(
                format!("  {} bytes ", self.bytes.len()),
                self.theme.dim_style(),
            ),
        ]);

        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(self.theme.border_style(self.focused))
                    .title(title),
            )
            .render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newlines_split_lines() {
        assert_eq!(
            OutputView::display_lines(b"ab\ncd"),
            vec!["ab".to_string(), "cd".to_string()]
        );
    }

    #[test]
    fn control_bytes_are_shown_rather_than_sent_to_the_terminal() {
        // A program is free to write \r or an escape byte. Passing those
        // through would move the cursor or recolor the interface drawn around
        // this panel, so they become one visible character each.
        assert_eq!(OutputView::display_lines(b"a\x1b\x07b"), vec!["a··b"]);
        assert_eq!(OutputView::display_lines(b"a\rb"), vec!["a·b"]);
    }

    #[test]
    fn tabs_become_spaces() {
        assert_eq!(OutputView::display_lines(b"a\tb"), vec!["a    b"]);
    }

    #[test]
    fn no_output_is_still_one_empty_line() {
        assert_eq!(OutputView::display_lines(b""), vec![String::new()]);
        assert_eq!(OutputView::line_count(b""), 1);
    }

    #[test]
    fn counting_lines_agrees_with_building_them() {
        for bytes in [&b""[..], b"a", b"a\n", b"a\nb", b"\n\n\n", b"a\nb\nc\n"] {
            assert_eq!(
                OutputView::line_count(bytes),
                OutputView::display_lines(bytes).len(),
                "{bytes:?}"
            );
        }
    }

    #[test]
    fn a_line_offset_lands_on_the_start_of_that_line() {
        let bytes = b"one\ntwo\nthree";
        assert_eq!(OutputView::line_offset(bytes, 0), 0);
        assert_eq!(&bytes[OutputView::line_offset(bytes, 1)..], b"two\nthree");
        assert_eq!(&bytes[OutputView::line_offset(bytes, 2)..], b"three");
    }
}
