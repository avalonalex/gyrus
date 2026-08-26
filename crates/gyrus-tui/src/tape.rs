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
    show_chars: bool,
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
            show_chars: true,
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

    /// Whether to show the character each value stands for.
    pub fn show_chars(mut self, show: bool) -> Self {
        self.show_chars = show;
        self
    }

    /// How many cells fit in `area`, after the row labels.
    pub fn capacity(area: Rect) -> usize {
        (area.width.saturating_sub(2) as usize).saturating_sub(7) / CELL
    }

    /// Scroll just enough to keep the pointer's cell visible.
    pub fn follow(offset: usize, pointer: isize, capacity: usize) -> usize {
        if capacity == 0 || pointer < 0 {
            return offset;
        }
        let cell = pointer as usize;
        if cell < offset {
            cell
        } else if cell >= offset + capacity {
            cell + 1 - capacity
        } else {
            offset
        }
    }

    fn cell_style(&self, address: usize) -> Style {
        if self.pointer >= 0 && self.pointer as usize == address {
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
            let glyph = if (0x20..0x7f).contains(&byte) {
                (byte as char).to_string()
            } else {
                "·".to_string()
            };
            chars.push(Span::styled(
                format!("{glyph:>3} "),
                if byte == 0 {
                    self.theme.dim_style()
                } else {
                    self.cell_style(address)
                },
            ));
            let is_pointer = self.pointer >= 0 && self.pointer as usize == address;
            caret.push(Span::styled(
                if is_pointer { "  ▲ " } else { "    " },
                Style::default().fg(self.theme.pointer),
            ));
        }

        let mut lines = vec![Line::from(addresses), Line::from(values)];
        if self.show_chars {
            lines.push(Line::from(chars));
        }
        lines.push(Line::from(caret));

        // Where the pointer is, when it has walked off the strip.
        if self.pointer < 0 || self.pointer as usize >= self.memory.len() {
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
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(view: TapeStrip<'_>, width: u16, height: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend");
        terminal
            .draw(|frame| frame.render_widget(view, frame.area()))
            .expect("draw");
        let buffer = terminal.backend().buffer().clone();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

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
