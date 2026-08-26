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
    title: String,
    focused: bool,
    scroll: Option<usize>,
}

impl<'a> OutputView<'a> {
    /// A panel showing everything the program has written.
    pub fn new(bytes: &'a [u8], theme: &'a Theme) -> Self {
        Self {
            bytes,
            theme,
            title: "Output".to_string(),
            focused: false,
            scroll: None,
        }
    }

    /// Panel title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
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
        let all = Self::display_lines(self.bytes);
        let start = match self.scroll {
            Some(scroll) => scroll.min(all.len().saturating_sub(1)),
            None => all.len().saturating_sub(height),
        };

        let lines: Vec<Line> = all[start..]
            .iter()
            .take(height)
            .map(|line| {
                Line::from(Span::styled(
                    line.clone(),
                    Style::default().fg(self.theme.title),
                ))
            })
            .collect();

        let title = Line::from(vec![
            Span::styled(" ", self.theme.dim_style()),
            Span::styled(self.title.clone(), self.theme.title_style()),
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
    }
}
