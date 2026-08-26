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

    /// Total rows the overlay wants, so callers can size or scroll it.
    pub fn height(sections: &[Section<'_>]) -> usize {
        sections
            .iter()
            .map(|(_, rows)| rows.len() + 2)
            .sum::<usize>()
            .saturating_sub(1)
    }

    fn rows(&self) -> Vec<Line<'static>> {
        let width = self
            .sections
            .iter()
            .flat_map(|(_, rows)| rows.iter())
            .map(|(keys, _)| keys.chars().count())
            .max()
            .unwrap_or(0);

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
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("   {keys:<width$}  "),
                        Style::default().fg(self.theme.title),
                    ),
                    Span::styled((*description).to_string(), self.theme.dim_style()),
                ]));
            }
        }
        lines
    }
}

impl Widget for HelpOverlay<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let all = self.rows();
        let wanted = u16::try_from(all.len().saturating_sub(self.scroll) + 2).unwrap_or(u16::MAX);
        let popup = centered(area, area.width * 72 / 100, wanted.min(area.height).max(3));
        Clear.render(popup, buf);

        let height = popup.height.saturating_sub(2) as usize;
        let start = self.scroll.min(all.len().saturating_sub(1));
        let lines: Vec<Line> = all.into_iter().skip(start).take(height).collect();

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
