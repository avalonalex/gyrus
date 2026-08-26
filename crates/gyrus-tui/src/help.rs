//! The key-binding overlay.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::layout::centered;
use crate::theme::Theme;

/// A group of related bindings: a heading and its `keys → description` rows.
pub type Section<'a> = (&'a str, &'a [(&'a str, &'a str)]);

/// A centered popup listing every key binding.
pub struct HelpOverlay<'a> {
    sections: &'a [Section<'a>],
    theme: &'a Theme,
    title: &'a str,
    dismiss: &'a str,
    scroll: usize,
}

impl<'a> HelpOverlay<'a> {
    /// A popup listing `sections`.
    pub fn new(sections: &'a [Section<'a>], theme: &'a Theme) -> Self {
        Self {
            sections,
            theme,
            title: "Keys",
            dismiss: "esc to close",
            scroll: 0,
        }
    }

    /// Popup title.
    pub fn title(mut self, title: &'a str) -> Self {
        self.title = title;
        self
    }

    /// How to close it, shown beside the title.
    ///
    /// A parameter rather than a constant because the two binaries bind it
    /// differently: `?` toggles help in the debugger, but in the tutorial `?`
    /// is a character you might be typing, so help is on F1. A widget crate
    /// that names a key is asserting something only its caller knows.
    pub fn dismiss(mut self, dismiss: &'a str) -> Self {
        self.dismiss = dismiss;
        self
    }

    /// First visible row, for overlays taller than the terminal.
    pub fn scroll(mut self, scroll: usize) -> Self {
        self.scroll = scroll;
        self
    }

    /// Total rows the overlay wants, so callers can bound their scrolling.
    ///
    /// A lower bound: it assumes no description wraps, which is true on a wide
    /// terminal. Scrolling is clamped to it, so under-counting only means the
    /// last row cannot be scrolled to on a narrow one — and the row that
    /// matters, the "more below" marker, is drawn regardless.
    pub fn height(sections: &[Section<'_>]) -> usize {
        sections
            .iter()
            .map(|(_, rows)| rows.len() + 2)
            .sum::<usize>()
            .saturating_sub(1)
    }

    fn rows(&self, available: usize) -> Vec<Line<'static>> {
        let key_width = self
            .sections
            .iter()
            .flat_map(|(_, rows)| rows.iter())
            .map(|(keys, _)| keys.chars().count())
            .max()
            .unwrap_or(0);
        let indent = key_width + 5;
        let text_width = available.saturating_sub(indent).max(8);

        let mut lines = Vec::new();
        for (index, (heading, rows)) in self.sections.iter().enumerate() {
            if index > 0 {
                lines.push(Line::raw(""));
            }
            lines.push(Line::from(Span::styled(
                format!(" {heading}"),
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            )));
            for (keys, description) in rows.iter() {
                // Wrapped, not clipped. A description cut mid-word reads as a
                // different, shorter instruction: "step over: run the whole
                // loop, if" is not what the key does.
                let mut first = true;
                for chunk in wrap(description, text_width) {
                    let prefix = if first {
                        format!("   {keys:<key_width$}  ")
                    } else {
                        " ".repeat(indent)
                    };
                    lines.push(Line::from(vec![
                        Span::styled(prefix, Style::default().fg(self.theme.title)),
                        Span::styled(chunk, self.theme.dim_style()),
                    ]));
                    first = false;
                }
            }
        }
        lines
    }
}

impl Widget for HelpOverlay<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let width = area.width * 72 / 100;
        let all = self.rows(usize::from(width.saturating_sub(2)));
        let wanted = u16::try_from(all.len().saturating_sub(self.scroll) + 2).unwrap_or(u16::MAX);
        let popup = centered(area, width, wanted.min(area.height).max(3));
        Clear.render(popup, buf);

        let height = popup.height.saturating_sub(2) as usize;
        let start = self.scroll.min(all.len().saturating_sub(1));
        let mut lines: Vec<Line> = all.iter().skip(start).take(height).cloned().collect();

        // Say so when the list does not fit. Without this the overlay ends
        // mid-list on a short terminal and reads as the whole thing, so the
        // keys below the fold may as well not exist.
        let remaining = all.len().saturating_sub(start + height);
        if remaining > 0 && !lines.is_empty() {
            let last = lines.len() - 1;
            lines[last] = Line::from(Span::styled(
                format!("   … {remaining} more — j/k or ↑↓ to scroll"),
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(self.theme.border_focused))
                    .title(Line::from(vec![
                        Span::styled(" ", self.theme.dim_style()),
                        Span::styled(self.title, self.theme.title_style()),
                        Span::styled(format!("  {} ", self.dismiss), self.theme.dim_style()),
                    ])),
            )
            .render(popup, buf);
    }
}

/// Break `text` into lines of at most `width` characters, on spaces.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split(' ') {
        let extra = if current.is_empty() {
            word.chars().count()
        } else {
            word.chars().count() + 1
        };
        if !current.is_empty() && current.chars().count() + extra > width {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_breaks_on_spaces_and_keeps_every_word() {
        let wrapped = wrap("run the whole loop, if this is a bracket", 12);
        assert!(
            wrapped.iter().all(|line| line.chars().count() <= 12),
            "{wrapped:?}"
        );
        assert_eq!(
            wrapped.join(" "),
            "run the whole loop, if this is a bracket"
        );
    }

    #[test]
    fn a_word_longer_than_the_width_still_gets_its_own_line() {
        assert_eq!(
            wrap("supercalifragilistic", 5),
            vec!["supercalifragilistic"]
        );
    }

    #[test]
    fn empty_text_is_one_empty_line() {
        assert_eq!(wrap("", 10), vec![String::new()]);
    }
}
