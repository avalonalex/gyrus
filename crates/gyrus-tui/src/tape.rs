//! A small, labelled view of the tape, for explaining rather than inspecting.
//!
//! [`MemoryView`](crate::MemoryView) is a hex dump: the right shape for 30,000
//! cells and the wrong one for a beginner watching `++[>+<-]` move a 2 from one
//! cell to the next. This shows a handful of cells with their addresses above
//! them and an arrow underneath the pointer.

use std::collections::HashSet;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::cells::{points_at, printable};
use crate::theme::Theme;

/// Characters each cell column takes, including its trailing space.
const CELL: usize = 4;

/// A labelled strip of tape cells.
pub struct TapeStrip<'a> {
    memory: &'a [u8],
    pointer: isize,
    theme: &'a Theme,
    changed: Option<&'a HashSet<usize>>,
    title: String,
    offset: usize,
}

impl<'a> TapeStrip<'a> {
    /// A strip showing `memory` with the cursor at `pointer`.
    pub fn new(memory: &'a [u8], pointer: isize, theme: &'a Theme) -> Self {
        Self {
            memory,
            pointer,
            theme,
            changed: None,
            title: "Tape".to_string(),
            offset: 0,
        }
    }

    /// Panel title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Cells written by the step being shown.
    pub fn changed(mut self, changed: &'a HashSet<usize>) -> Self {
        self.changed = Some(changed);
        self
    }

    /// First visible cell.
    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }

    /// How many cells fit in `area`, after the row labels.
    pub fn capacity(area: Rect) -> usize {
        (area.width.saturating_sub(2) as usize).saturating_sub(7) / CELL
    }

    /// Scroll just enough to keep the pointer's cell visible.
    ///
    /// A strip is a hex dump one cell wide, so this is
    /// [`follow_pointer`](crate::follow_pointer) with a single column.
    pub fn follow(offset: usize, pointer: isize, capacity: usize) -> usize {
        crate::memory::follow_pointer(offset, pointer, 1, capacity)
    }

    fn cell_style(&self, address: usize) -> Style {
        if points_at(self.pointer, address) {
            Style::default()
                .fg(self.theme.pointer)
                .add_modifier(Modifier::BOLD)
        } else if self.changed.is_some_and(|set| set.contains(&address)) {
            Style::default()
                .fg(self.theme.modified)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.theme.title)
        }
    }
}

impl Widget for TapeStrip<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let capacity = Self::capacity(area).min(self.memory.len().saturating_sub(self.offset));
        let cells = self.offset..self.offset + capacity;

        let mut addresses = vec![Span::styled("cell  ", self.theme.dim_style())];
        let mut values = vec![Span::styled("value ", self.theme.dim_style())];
        let mut chars = vec![Span::styled("char  ", self.theme.dim_style())];
        let mut caret = vec![Span::raw("      ")];

        for address in cells {
            let byte = self.memory[address];
            addresses.push(Span::styled(
                format!("{:>3} ", address % 1000),
                self.theme.dim_style(),
            ));
            values.push(Span::styled(
                format!("{byte:>3} "),
                self.cell_style(address),
            ));
            chars.push(Span::styled(
                format!("{:>3} ", printable(byte, '·')),
                if byte == 0 {
                    self.theme.dim_style()
                } else {
                    self.cell_style(address)
                },
            ));
            caret.push(Span::styled(
                if points_at(self.pointer, address) {
                    "  ▲ "
                } else {
                    "    "
                },
                Style::default().fg(self.theme.pointer),
            ));
        }

        let lines = vec![
            Line::from(addresses),
            Line::from(values),
            Line::from(chars),
            Line::from(caret),
        ];

        // Where the pointer is, when it has walked off the strip.
        let mut lines = lines;
        if crate::cells::cell_under(self.memory, self.pointer).is_none() {
            lines.push(Line::from(Span::styled(
                format!("pointer at {} — off the tape", self.pointer),
                Style::default().fg(self.theme.error),
            )));
        }

        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(self.theme.border_style(false))
                    .title(Line::from(vec![
                        Span::styled(" ", self.theme.dim_style()),
                        Span::styled(self.title.clone(), self.theme.title_style()),
                        Span::styled(" ", self.theme.dim_style()),
                    ])),
            )
            .render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::render;

    #[test]
    fn the_caret_sits_under_the_pointers_cell() {
        let theme = Theme::default();
        let memory = [1u8, 2, 3, 4];
        let lines = render(TapeStrip::new(&memory, 2, &theme), 40, 7);
        let value_column = lines[2].find("  3").expect("cell 2 holds 3");
        assert_eq!(lines[4].find('▲'), Some(value_column + 2), "{lines:?}");
    }

    #[test]
    fn an_off_tape_pointer_is_called_out() {
        let theme = Theme::default();
        let memory = [0u8; 4];
        let lines = render(TapeStrip::new(&memory, 9, &theme), 40, 8);
        assert!(
            lines.iter().any(|line| line.contains("off the tape")),
            "{lines:?}"
        );
    }

    #[test]
    fn following_keeps_the_pointer_in_view() {
        assert_eq!(TapeStrip::follow(0, 20, 8), 13);
        assert_eq!(TapeStrip::follow(13, 2, 8), 2);
        assert_eq!(TapeStrip::follow(4, 6, 8), 4);
    }
}
