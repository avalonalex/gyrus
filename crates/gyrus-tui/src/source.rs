//! Source panel: BrainFuck code with line numbers, breakpoints, and a cursor.

use std::collections::BTreeSet;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::theme::{Category, Theme, classify_line};

/// Width of the gutter and line-number prefix, `"▶●  12 │ "`.
const PREFIX: usize = 9;

/// A source position: 1-indexed line and 1-indexed column.
///
/// This is [`gyrus::SourceLocation`] minus the byte offset, which the panel
/// never needs.
pub type Position = (usize, usize);

/// Source text split into lines, with each line's syntax coloring precomputed.
///
/// Rebuilding this every frame would re-classify the whole program sixty times
/// a second; building it once per edit costs nothing.
#[derive(Debug, Clone, Default)]
pub struct SourceDocument {
    lines: Vec<String>,
    categories: Vec<Vec<Category>>,
}

impl SourceDocument {
    /// Split and classify a program's source.
    pub fn new(source: &str) -> Self {
        let lines: Vec<String> = if source.is_empty() {
            vec![String::new()]
        } else {
            source.lines().map(str::to_owned).collect()
        };
        let categories = lines.iter().map(|line| classify_line(line)).collect();
        Self { lines, categories }
    }

    /// Number of lines. Always at least 1.
    pub fn line_count(&self) -> usize {
        self.lines.len().max(1)
    }

    /// The text of a 1-indexed line.
    pub fn line(&self, line: usize) -> Option<&str> {
        self.lines.get(line.checked_sub(1)?).map(String::as_str)
    }

    /// Length of a 1-indexed line, in characters.
    pub fn line_width(&self, line: usize) -> usize {
        self.line(line).map_or(0, |text| text.chars().count())
    }
}

/// The source code panel.
pub struct SourceView<'a> {
    doc: &'a SourceDocument,
    theme: &'a Theme,
    title: String,
    focused: bool,
    current: Option<Position>,
    cursor: Option<Position>,
    show_cursor: bool,
    breakpoints: Option<&'a BTreeSet<Position>>,
    scroll: usize,
    h_scroll: usize,
}

impl<'a> SourceView<'a> {
    /// A panel showing `doc`.
    pub fn new(doc: &'a SourceDocument, theme: &'a Theme) -> Self {
        Self {
            doc,
            theme,
            title: "Source".to_string(),
            focused: false,
            current: None,
            cursor: None,
            show_cursor: false,
            breakpoints: None,
            scroll: 0,
            h_scroll: 0,
        }
    }

    /// Panel title, usually the file name.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Draw the border brighter, to show this panel has keyboard focus.
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// The instruction about to execute.
    pub fn current(mut self, position: Option<Position>) -> Self {
        self.current = position;
        self
    }

    /// The user's cursor, and whether to draw a caret on it.
    ///
    /// The line is marked either way — that is what tells the user which line
    /// `b` would toggle a breakpoint on — but the caret only appears when the
    /// panel has focus, so two panels never look like they both have one.
    pub fn cursor(mut self, position: Option<Position>, show_caret: bool) -> Self {
        self.cursor = position;
        self.show_cursor = show_caret;
        self
    }

    /// Positions carrying a breakpoint.
    pub fn breakpoints(mut self, breakpoints: &'a BTreeSet<Position>) -> Self {
        self.breakpoints = Some(breakpoints);
        self
    }

    /// First visible line, 0-indexed.
    pub fn scroll(mut self, scroll: usize) -> Self {
        self.scroll = scroll;
        self
    }

    /// First visible column, 0-indexed. BrainFuck lines get long.
    pub fn h_scroll(mut self, h_scroll: usize) -> Self {
        self.h_scroll = h_scroll;
        self
    }

    /// How many source lines fit in `area`.
    pub fn visible_lines(area: Rect) -> usize {
        area.height.saturating_sub(2) as usize
    }

    /// How many source columns fit in `area`, after the line-number prefix.
    pub fn visible_columns(area: Rect) -> usize {
        (area.width.saturating_sub(2) as usize).saturating_sub(PREFIX)
    }

    fn gutter(&self, line: usize) -> Span<'static> {
        let is_current = self.current.is_some_and(|(l, _)| l == line);
        let has_breakpoint = self
            .breakpoints
            .is_some_and(|set| set.range((line, 0)..(line + 1, 0)).next().is_some());
        let (text, color) = match (is_current, has_breakpoint) {
            (true, true) => ("▶●", self.theme.breakpoint),
            (true, false) => ("▶ ", self.theme.current),
            (false, true) => (" ●", self.theme.breakpoint),
            (false, false) if self.cursor.is_some_and(|(l, _)| l == line) => {
                ("› ", self.theme.accent)
            }
            (false, false) => ("  ", self.theme.dim),
        };
        Span::styled(text, Style::default().fg(color))
    }

    fn char_style(&self, line: usize, column: usize, category: Category) -> Style {
        if self.current == Some((line, column)) {
            return Style::default()
                .fg(self.theme.current)
                .add_modifier(Modifier::REVERSED | Modifier::BOLD);
        }
        if self
            .breakpoints
            .is_some_and(|set| set.contains(&(line, column)))
        {
            return Style::default()
                .fg(self.theme.breakpoint)
                .add_modifier(Modifier::REVERSED | Modifier::BOLD);
        }
        if self.show_cursor && self.cursor == Some((line, column)) {
            return self
                .theme
                .category(category)
                .add_modifier(Modifier::UNDERLINED | Modifier::BOLD);
        }
        self.theme.category(category)
    }

    fn render_line(&self, number: usize, width: usize) -> Line<'static> {
        let mut spans = vec![
            self.gutter(number),
            Span::styled(format!("{number:>4} "), self.theme.dim_style()),
            Span::styled("│ ", self.theme.dim_style()),
        ];

        let text = self.doc.line(number).unwrap_or("");
        let categories = self
            .doc
            .categories
            .get(number - 1)
            .map(Vec::as_slice)
            .unwrap_or(&[]);

        let mut rendered = 0;
        for (index, ch) in text.chars().enumerate().skip(self.h_scroll) {
            if rendered == width {
                break;
            }
            let category = categories.get(index).copied().unwrap_or(Category::Comment);
            spans.push(Span::styled(
                ch.to_string(),
                self.char_style(number, index + 1, category),
            ));
            rendered += 1;
        }

        // A caret sitting one past the end of a line still needs somewhere to go.
        if self.show_cursor
            && let Some((line, column)) = self.cursor
            && line == number
            && column > text.chars().count()
            && rendered < width
        {
            spans.push(Span::styled(
                " ",
                Style::default().add_modifier(Modifier::REVERSED),
            ));
        }

        Line::from(spans)
    }
}

impl Widget for SourceView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let height = Self::visible_lines(area);
        let width = Self::visible_columns(area);
        let first = self.scroll + 1;
        let last = (self.scroll + height).min(self.doc.line_count());

        let lines: Vec<Line> = (first..=last)
            .map(|number| self.render_line(number, width))
            .collect();

        let mut title = vec![
            Span::styled(" ", self.theme.dim_style()),
            Span::styled(self.title.clone(), self.theme.title_style()),
        ];
        if let Some(count) = self.breakpoints.map(BTreeSet::len).filter(|n| *n > 0) {
            title.push(Span::styled(
                format!("  ● {count}"),
                Style::default().fg(self.theme.breakpoint),
            ));
        }
        // On a one-line program -- which hello_world.bf is, and quine.bf, and
        // most golfed BrainFuck -- the cursor's gutter chevron is hidden by the
        // current-instruction arrow on that same line, leaving one underlined
        // character among a hundred as the only cue. `b` and `g` both act at
        // the cursor, so saying where it is belongs next to the code.
        if let Some((line, column)) = self.cursor.filter(|_| self.show_cursor) {
            title.push(Span::styled(
                format!("  cur {line}:{column}"),
                Style::default().fg(self.theme.accent),
            ));
        }
        if self.h_scroll > 0 {
            title.push(Span::styled(
                format!("  col {}", self.h_scroll + 1),
                self.theme.dim_style(),
            ));
        }
        title.push(Span::styled(" ", self.theme.dim_style()));

        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(self.theme.border_style(self.focused))
                    .title(Line::from(title)),
            )
            .render(area, buf);
    }
}

/// Scroll just enough to keep `target` (1-indexed) on screen with a margin.
///
/// Returns the new 0-indexed first visible line or column.
pub fn follow_scroll(scroll: usize, target: usize, span: usize, margin: usize) -> usize {
    if span == 0 {
        return scroll;
    }
    let target = target.saturating_sub(1);
    let margin = margin.min(span.saturating_sub(1) / 2);
    if target < scroll + margin {
        target.saturating_sub(margin)
    } else if target + margin >= scroll + span {
        target + margin + 1 - span
    } else {
        scroll
    }
}

/// Clamp a scroll offset so the panel never scrolls past the end of the file.
pub fn clamp_scroll(scroll: usize, total: usize, span: usize) -> usize {
    scroll.min(total.saturating_sub(span.max(1)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::render;
    use crate::theme::Theme;

    #[test]
    fn an_empty_program_still_has_one_line() {
        let doc = SourceDocument::new("");
        assert_eq!(doc.line_count(), 1);
        assert_eq!(doc.line(1), Some(""));
    }

    #[test]
    fn lines_are_one_indexed() {
        let doc = SourceDocument::new("a\nbb\nccc");
        assert_eq!(doc.line(1), Some("a"));
        assert_eq!(doc.line(3), Some("ccc"));
        assert_eq!(doc.line(4), None);
        assert_eq!(doc.line_width(2), 2);
    }

    #[test]
    fn following_leaves_a_target_already_on_screen_alone() {
        assert_eq!(follow_scroll(10, 15, 20, 3), 10);
    }

    #[test]
    fn following_scrolls_up_when_the_target_is_above() {
        assert_eq!(follow_scroll(10, 5, 20, 3), 1);
    }

    #[test]
    fn following_scrolls_down_when_the_target_is_below() {
        // Line 40, twenty rows, three of margin: the last visible line is 43.
        assert_eq!(follow_scroll(10, 40, 20, 3), 23);
    }

    #[test]
    fn following_never_scrolls_a_zero_height_panel() {
        assert_eq!(follow_scroll(7, 100, 0, 3), 7);
    }

    #[test]
    fn clamping_stops_at_the_last_screenful() {
        assert_eq!(clamp_scroll(90, 50, 20), 30);
        assert_eq!(clamp_scroll(5, 50, 20), 5);
    }

    #[test]
    fn the_current_instruction_gets_an_arrow_and_the_cursor_a_chevron() {
        let theme = Theme::default();
        let doc = SourceDocument::new("+++\n>>>\n---");
        let breakpoints = BTreeSet::new();
        let lines = render(
            SourceView::new(&doc, &theme)
                .current(Some((2, 1)))
                .cursor(Some((3, 1)), true)
                .breakpoints(&breakpoints),
            30,
            6,
        );
        assert!(lines[1].contains("   1 │ +++"), "{lines:?}");
        assert!(lines[2].starts_with("│▶    2 │ >>>"), "{lines:?}");
        assert!(lines[3].starts_with("│›    3 │ ---"), "{lines:?}");
    }

    #[test]
    fn a_breakpoint_shows_in_the_gutter_and_the_title() {
        let theme = Theme::default();
        let doc = SourceDocument::new("+++\n>>>");
        let breakpoints = BTreeSet::from([(2, 2)]);
        let lines = render(
            SourceView::new(&doc, &theme).breakpoints(&breakpoints),
            30,
            5,
        );
        assert!(lines[0].contains("● 1"), "{lines:?}");
        assert!(lines[2].starts_with("│ ●   2 │ >>>"), "{lines:?}");
    }

    #[test]
    fn long_lines_scroll_sideways() {
        let theme = Theme::default();
        let doc = SourceDocument::new(&"+".repeat(200));
        let lines = render(SourceView::new(&doc, &theme).h_scroll(100), 30, 4);
        // Thirty columns, less two borders and the nine-column prefix.
        assert_eq!(lines[1].matches('+').count(), 19, "{lines:?}");
        assert!(lines[0].contains("col 101"), "{lines:?}");
    }
}
