//! Watch panel: a handful of cells, and whether they just changed.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::theme::Theme;

/// One row of the watch panel, already resolved to text.
///
/// Deliberately not a cell address: what a caller watches is its own business.
/// The debugger watches cells and output bytes, and this panel only needs to
/// know how wide the labels are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchEntry {
    /// What is being watched, e.g. `cell[3]` or `output`.
    pub label: String,
    /// Its current value, e.g. `72` or `any byte`.
    pub value: String,
    /// Whether the value changed since the previous frame.
    pub changed: bool,
    /// Whether reaching this stops execution, as opposed to only being shown.
    pub stops: bool,
}

impl WatchEntry {
    /// A row showing `label` and `value`. Displayed only, until
    /// [`Self::stopping`] says otherwise.
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            changed: false,
            stops: false,
        }
    }

    /// Whether reaching this row's condition stops execution.
    pub fn stopping(mut self, stops: bool) -> Self {
        self.stops = stops;
        self
    }

    /// Mark the value as having just changed.
    pub fn changed(mut self, changed: bool) -> Self {
        self.changed = changed;
        self
    }
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
            let width = self
                .entries
                .iter()
                .map(|entry| entry.label.chars().count())
                .max()
                .unwrap_or(0);
            self.entries
                .iter()
                .enumerate()
                .map(|(index, entry)| {
                    let marker = if self.selected == Some(index) {
                        "›"
                    } else {
                        " "
                    };
                    let value_style = if entry.changed {
                        Style::default()
                            .fg(self.theme.modified)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(self.theme.title)
                    };
                    let mut spans =
                        vec![Span::styled(format!("{marker} "), self.theme.dim_style())];
                    // A dot for the rows that stop execution, so a panel holding
                    // both kinds says which is which without a legend.
                    if entry.stops {
                        spans.push(Span::styled(
                            "● ",
                            Style::default().fg(self.theme.breakpoint),
                        ));
                    } else {
                        spans.push(Span::raw("  "));
                    }
                    spans.push(Span::styled(
                        format!("{:<width$}  ", entry.label),
                        self.theme.dim_style(),
                    ));
                    spans.push(Span::styled(entry.value.clone(), value_style));
                    Line::from(spans)
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
