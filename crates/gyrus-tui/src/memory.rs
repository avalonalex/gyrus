//! Memory panel: the tape as a hex dump, with the pointer and recent writes marked.

use std::collections::HashSet;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::cells::{cell_under, points_at, printable};
use crate::theme::Theme;

/// How cell values are written out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellDisplay {
    /// Two hex digits: `2A`
    #[default]
    Hex,
    /// Three decimal digits: ` 42`
    Decimal,
    /// The character itself, `.` when unprintable
    Ascii,
}

impl CellDisplay {
    /// The next mode, for a key that cycles through them.
    pub fn next(self) -> Self {
        match self {
            CellDisplay::Hex => CellDisplay::Decimal,
            CellDisplay::Decimal => CellDisplay::Ascii,
            CellDisplay::Ascii => CellDisplay::Hex,
        }
    }

    /// Name shown in the panel title.
    pub fn name(self) -> &'static str {
        match self {
            CellDisplay::Hex => "hex",
            CellDisplay::Decimal => "dec",
            CellDisplay::Ascii => "ascii",
        }
    }

    fn width(self) -> usize {
        match self {
            CellDisplay::Hex => 2,
            CellDisplay::Decimal => 3,
            CellDisplay::Ascii => 1,
        }
    }

    fn format(self, byte: u8) -> String {
        match self {
            CellDisplay::Hex => format!("{byte:02X}"),
            CellDisplay::Decimal => format!("{byte:>3}"),
            CellDisplay::Ascii => printable(byte, '.').to_string(),
        }
    }
}

/// Width of the address gutter, `"  0000 │ "`.
const GUTTER: usize = 9;

/// The memory panel.
pub struct MemoryView<'a> {
    memory: &'a [u8],
    pointer: isize,
    theme: &'a Theme,
    modified: Option<&'a HashSet<usize>>,
    display: CellDisplay,
    scroll: usize,
    focused: bool,
    following: bool,
}

impl<'a> MemoryView<'a> {
    /// A panel showing `memory` with the cursor at `pointer`.
    ///
    /// `pointer` is signed because the tape contract lets the cursor move off
    /// the tape; only reading or writing there is an error. An off-tape pointer
    /// is reported rather than clamped.
    pub fn new(memory: &'a [u8], pointer: isize, theme: &'a Theme) -> Self {
        Self {
            memory,
            pointer,
            theme,
            modified: None,
            display: CellDisplay::default(),
            scroll: 0,
            focused: false,
            following: true,
        }
    }

    /// Cells written since the last pause, highlighted so changes are visible.
    pub fn modified(mut self, modified: &'a HashSet<usize>) -> Self {
        self.modified = Some(modified);
        self
    }

    /// How to write cell values out.
    pub fn display(mut self, display: CellDisplay) -> Self {
        self.display = display;
        self
    }

    /// First visible row, 0-indexed.
    pub fn scroll(mut self, scroll: usize) -> Self {
        self.scroll = scroll;
        self
    }

    /// Draw the border brighter, to show this panel has keyboard focus.
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Whether the view auto-scrolls to keep the pointer visible.
    pub fn following(mut self, following: bool) -> Self {
        self.following = following;
        self
    }

    /// How many cells fit on one row of `area`, and whether the ASCII sidebar
    /// fits beside them.
    ///
    /// One function because the two answers are one inequality: the sidebar
    /// fits exactly when the row was sized with room for it. Computing them
    /// apart meant the gutter width and the separator width appeared twice, in
    /// forms that did not look alike -- change one and the dump and the sidebar
    /// disagree about how much room they have, silently.
    ///
    /// The count is snapped down to a multiple of 8 so addresses stay round —
    /// but only once eight fit. Below that every cell that fits is shown, on
    /// the grounds that a narrow panel needs the tape more than round
    /// addresses, and every row prints its own address anyway.
    pub fn layout(area: Rect, display: CellDisplay) -> (usize, bool) {
        let inner = area.width.saturating_sub(2) as usize;
        let available = inner.saturating_sub(GUTTER);
        let per_cell = display.width() + 1;
        // The ASCII sidebar costs one column per cell plus a " │ " separator.
        // Prefer a layout wide enough for both; drop the sidebar rather than
        // squeeze the dump below eight cells a row.
        let with_ascii = available.saturating_sub(3) / (per_cell + 1);
        let ascii = with_ascii >= 8;
        let count = if ascii {
            with_ascii
        } else {
            available / per_cell
        };
        let count = if count >= 8 {
            count / 8 * 8
        } else {
            count.max(1)
        };
        (count, ascii)
    }

    /// How many cells fit on one row of `area` in `display` mode.
    pub fn columns(area: Rect, display: CellDisplay) -> usize {
        Self::layout(area, display).0
    }

    /// How many memory rows fit in `area`, after the info line and header.
    pub fn visible_rows(area: Rect) -> usize {
        area.height.saturating_sub(4) as usize
    }

    fn cell_style(&self, address: usize, byte: u8) -> Style {
        if points_at(self.pointer, address) {
            return Style::default()
                .fg(self.theme.pointer)
                .add_modifier(Modifier::REVERSED | Modifier::BOLD);
        }
        if self.modified.is_some_and(|m| m.contains(&address)) {
            return Style::default()
                .fg(self.theme.modified)
                .add_modifier(Modifier::BOLD);
        }
        if byte == 0 {
            self.theme.dim_style()
        } else {
            Style::default().fg(self.theme.title)
        }
    }

    /// The line above the dump: where the pointer is, and what is under it.
    ///
    /// Assembled in priority order and cut off at whole pieces, because the
    /// last piece is a cell count and a clipped number does not read as
    /// clipped. At 80 columns "30000 cells" used to render as "3000".
    fn info_line(&self, width: usize) -> Line<'static> {
        let pointer = self.pointer.to_string();
        let mut spans = vec![
            Span::styled("ptr ", self.theme.dim_style()),
            Span::styled(
                pointer.clone(),
                Style::default()
                    .fg(self.theme.pointer)
                    .add_modifier(Modifier::BOLD),
            ),
        ];
        let mut used = 4 + pointer.chars().count();

        match cell_under(self.memory, self.pointer) {
            Some(byte) => {
                let value = byte.to_string();
                let detail = format!("  0x{byte:02X}  '{}'", printable(byte, '.'));
                if used + 8 + value.chars().count() <= width {
                    spans.push(Span::styled("   cell ", self.theme.dim_style()));
                    spans.push(Span::styled(
                        value.clone(),
                        Style::default().fg(self.theme.title),
                    ));
                    used += 8 + value.chars().count();
                    if used + detail.chars().count() <= width {
                        used += detail.chars().count();
                        spans.push(Span::styled(detail, self.theme.dim_style()));
                    }
                }
            }
            None => {
                // Two spellings, so the narrow one still says the important part.
                let long = "   off tape (reading here would error)";
                let short = "   off tape";
                let text = if used + long.len() <= width {
                    long
                } else {
                    short
                };
                if used + text.len() <= width {
                    used += text.len();
                    spans.push(Span::styled(text, Style::default().fg(self.theme.error)));
                }
            }
        }

        let size = format!("   {} cells", self.memory.len());
        if used + size.chars().count() <= width {
            spans.push(Span::styled(size, self.theme.dim_style()));
        }
        Line::from(spans)
    }

    fn header_line(&self, columns: usize) -> Line<'static> {
        let mut spans = vec![Span::styled(
            format!("{:>6} │ ", "addr"),
            self.theme.dim_style(),
        )];
        for column in 0..columns {
            spans.push(Span::styled(
                format!("{:>width$} ", column % 100, width = self.display.width()),
                self.theme.dim_style(),
            ));
        }
        Line::from(spans)
    }

    fn row_line(&self, base: usize, columns: usize, ascii: bool) -> Line<'static> {
        let mut spans = vec![Span::styled(
            format!("{base:>6} │ "),
            self.theme.dim_style(),
        )];
        for column in 0..columns {
            let address = base + column;
            match self.memory.get(address) {
                Some(&byte) => {
                    spans.push(Span::styled(
                        self.display.format(byte),
                        self.cell_style(address, byte),
                    ));
                    spans.push(Span::raw(" "));
                }
                None => spans.push(Span::raw(" ".repeat(self.display.width() + 1))),
            }
        }
        if ascii {
            spans.push(Span::styled(" │ ", self.theme.dim_style()));
            for column in 0..columns {
                let address = base + column;
                match self.memory.get(address) {
                    Some(&byte) => spans.push(Span::styled(
                        printable(byte, '.').to_string(),
                        self.cell_style(address, byte),
                    )),
                    None => spans.push(Span::raw(" ")),
                }
            }
        }
        Line::from(spans)
    }
}

impl Widget for MemoryView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (columns, ascii) = Self::layout(area, self.display);
        let rows = Self::visible_rows(area);

        let inner = area.width.saturating_sub(2) as usize;
        let mut lines = vec![
            self.info_line(inner),
            Line::raw(""),
            self.header_line(columns),
        ];
        for row in 0..rows {
            let base = (self.scroll + row) * columns;
            if base >= self.memory.len() {
                break;
            }
            lines.push(self.row_line(base, columns, ascii));
        }

        let title = Line::from(vec![
            Span::styled(" ", self.theme.dim_style()),
            Span::styled("Memory", self.theme.title_style()),
            Span::styled(
                format!(
                    "  {}{} ",
                    self.display.name(),
                    if self.following { " · follow" } else { "" }
                ),
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

/// Scroll just enough to keep the row holding `pointer` on screen.
pub fn follow_pointer(scroll: usize, pointer: isize, columns: usize, rows: usize) -> usize {
    if columns == 0 || rows == 0 || pointer < 0 {
        return scroll;
    }
    let row = pointer as usize / columns;
    if row < scroll {
        row
    } else if row >= scroll + rows {
        row + 1 - rows
    } else {
        scroll
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::render;
    use crate::theme::Theme;

    fn area(width: u16, height: u16) -> Rect {
        Rect::new(0, 0, width, height)
    }

    #[test]
    fn column_counts_are_multiples_of_eight() {
        for width in 20..200u16 {
            let columns = MemoryView::columns(area(width, 20), CellDisplay::Hex);
            assert!(columns > 0, "width {width} produced no columns");
            if columns >= 8 {
                assert_eq!(columns % 8, 0, "width {width} produced {columns} columns");
            }
        }
    }

    #[test]
    fn narrower_cells_fit_more_per_row() {
        let wide = area(120, 20);
        assert!(
            MemoryView::columns(wide, CellDisplay::Ascii)
                > MemoryView::columns(wide, CellDisplay::Decimal)
        );
    }

    #[test]
    fn following_keeps_the_pointer_row_on_screen() {
        assert_eq!(follow_pointer(0, 200, 8, 10), 16);
        assert_eq!(follow_pointer(20, 8, 8, 10), 1);
        assert_eq!(follow_pointer(5, 48, 8, 10), 5);
    }

    #[test]
    fn an_off_tape_pointer_never_scrolls_the_view() {
        assert_eq!(follow_pointer(4, -1, 8, 10), 4);
    }

    #[test]
    fn the_info_line_reports_the_cell_under_the_pointer() {
        let theme = Theme::default();
        let mut memory = vec![0u8; 32];
        memory[5] = b'*';
        let lines = render(MemoryView::new(&memory, 5, &theme), 60, 8);
        assert!(lines[1].contains("ptr 5"), "{lines:?}");
        assert!(lines[1].contains("cell 42"), "{lines:?}");
        assert!(lines[1].contains("0x2A"), "{lines:?}");
        assert!(lines[1].contains("'*'"), "{lines:?}");
    }

    #[test]
    fn an_off_tape_pointer_says_so_instead_of_showing_a_value() {
        let theme = Theme::default();
        let memory = vec![0u8; 32];
        let lines = render(MemoryView::new(&memory, -1, &theme), 60, 8);
        assert!(lines[1].contains("ptr -1"), "{lines:?}");
        assert!(lines[1].contains("off tape"), "{lines:?}");
    }

    #[test]
    fn the_ascii_sidebar_shows_printable_bytes() {
        let theme = Theme::default();
        let mut memory = vec![0u8; 16];
        memory[..2].copy_from_slice(b"Hi");
        let lines = render(MemoryView::new(&memory, 0, &theme), 60, 8);
        assert!(lines[4].contains("48 69"), "{lines:?}");
        assert!(lines[4].contains("Hi.."), "{lines:?}");
    }
}

#[cfg(test)]
mod info_line_tests {
    use super::*;
    use crate::test_utils::render;
    use crate::theme::Theme;

    #[test]
    fn the_tape_size_is_dropped_rather_than_clipped() {
        // At 80 columns the memory panel is 32 wide, and "30000 cells" used to
        // render as "3000" -- a complete, plausible, wrong number.
        let theme = Theme::default();
        let memory = vec![0u8; 30000];
        let narrow = render(MemoryView::new(&memory, 0, &theme), 32, 8);
        assert!(narrow[1].contains("ptr 0"), "{narrow:?}");
        assert!(
            !narrow[1].contains("3000"),
            "a partial cell count is worse than none: {narrow:?}"
        );

        let wide = render(MemoryView::new(&memory, 0, &theme), 60, 8);
        assert!(wide[1].contains("30000 cells"), "{wide:?}");
    }

    #[test]
    fn an_off_tape_pointer_still_says_so_when_there_is_no_room_to_explain() {
        let theme = Theme::default();
        let memory = vec![0u8; 32];
        let narrow = render(MemoryView::new(&memory, 99, &theme), 26, 8);
        assert!(narrow[1].contains("off tape"), "{narrow:?}");
    }
}
