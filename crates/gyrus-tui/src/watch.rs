//! Watch panel: a handful of cells, and whether they just changed.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::theme::Theme;

/// One watched cell, resolved against the current tape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchEntry {
    /// Cell address on the tape.
    pub address: usize,
    /// Current value, or `None` when the address is past the end of the tape.
    pub value: Option<u8>,
    /// Whether the value changed since the previous pause.
    pub changed: bool,
}

/// The watch panel.
pub struct WatchList<'a> {
    entries: &'a [WatchEntry],
    theme: &'a Theme,
    selected: Option<usize>,
    focused: bool,
    empty_hint: &'a str,
}

impl<'a> WatchList<'a> {
    /// A panel listing `entries`.
    pub fn new(entries: &'a [WatchEntry], theme: &'a Theme) -> Self {
        Self {
            entries,
            theme,
            selected: None,
            focused: false,
            empty_hint: "nothing watched",
        }
    }

    /// What to say when nothing is being watched.
    ///
    /// The caller supplies it because a useful empty state names the key that
    /// fills it, and only the caller knows which key that is.
    pub fn empty_hint(mut self, hint: &'a str) -> Self {
        self.empty_hint = hint;
        self
    }

    /// Index of the highlighted entry, for keyboard removal.
    pub fn selected(mut self, selected: Option<usize>) -> Self {
        self.selected = selected;
        self
    }

    /// Draw the border brighter, to show this panel has keyboard focus.
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

impl Widget for WatchList<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let lines: Vec<Line> = if self.entries.is_empty() {
            vec![Line::from(Span::styled(
                format!(" {}", self.empty_hint),
                self.theme.dim_style(),
            ))]
        } else {
            self.entries
                .iter()
                .enumerate()
                .map(|(index, entry)| {
                    let marker = if self.selected == Some(index) {
                        "›"
                    } else {
                        " "
                    };
                    let value = match entry.value {
                        Some(byte) => format!("{byte}"),
                        None => "off tape".to_string(),
                    };
                    let value_style = if entry.changed {
                        Style::default()
                            .fg(self.theme.modified)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(self.theme.title)
                    };
                    Line::from(vec![
                        Span::styled(
                            format!("{marker} cell[{}] = ", entry.address),
                            self.theme.dim_style(),
                        ),
                        Span::styled(value, value_style),
                    ])
                })
                .collect()
        };

        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(self.theme.border_style(self.focused))
                    .title(Line::from(vec![
                        Span::styled(" ", self.theme.dim_style()),
                        Span::styled("Watch", self.theme.title_style()),
                        Span::styled(" ", self.theme.dim_style()),
                    ])),
            )
            .render(area, buf);
    }
}
