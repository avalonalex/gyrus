//! A centered popup for text that does not belong in a panel.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap};

use crate::layout::centered;
use crate::theme::Theme;

/// A bordered popup showing a block of text: a result, an explanation, an error.
pub struct Overlay<'a> {
    title: &'a str,
    body: &'a str,
    footer: &'a str,
    theme: &'a Theme,
    accent: Option<Color>,
    size: (u16, u16),
    scroll: usize,
    wrap: bool,
}

impl<'a> Overlay<'a> {
    /// A popup titled `title` showing `body`.
    pub fn new(title: &'a str, body: &'a str, theme: &'a Theme) -> Self {
        Self {
            title,
            body,
            footer: "",
            theme,
            accent: None,
            size: (70, 60),
            scroll: 0,
            wrap: false,
        }
    }

    /// A hint drawn beside the title, usually the key that dismisses it.
    pub fn footer(mut self, footer: &'a str) -> Self {
        self.footer = footer;
        self
    }

    /// Border and title color. Defaults to the theme's focused border.
    pub fn accent(mut self, accent: Color) -> Self {
        self.accent = Some(accent);
        self
    }

    /// The most room the popup may take, as a percentage of the screen.
    ///
    /// It takes only as many rows as its text needs, so a six-line result does
    /// not open a box that covers the tape the user wants to look at.
    pub fn size(mut self, percent_x: u16, percent_y: u16) -> Self {
        self.size = (percent_x, percent_y);
        self
    }

    /// First visible line, for text longer than the popup.
    pub fn scroll(mut self, scroll: usize) -> Self {
        self.scroll = scroll;
        self
    }

    /// Wrap long lines instead of clipping them.
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }
}

impl Widget for Overlay<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let width = area.width * self.size.0 / 100;
        let inner = usize::from(width.saturating_sub(2)).max(1);
        // Wrapped text needs more rows than it has lines, and a popup sized for
        // the line count would clip exactly the hint the user opened it for.
        let content: usize = self
            .body
            .lines()
            .skip(self.scroll)
            .map(|line| line.chars().count().max(1).div_ceil(inner))
            .sum();
        let wanted = u16::try_from(content + 2).unwrap_or(u16::MAX);
        let popup = centered(
            area,
            width,
            wanted.min(area.height * self.size.1 / 100).max(3),
        );
        Clear.render(popup, buf);

        let accent = self.accent.unwrap_or(self.theme.border_focused);
        let lines: Vec<Line> = self
            .body
            .lines()
            .skip(self.scroll)
            .map(|line| {
                Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(self.theme.title),
                ))
            })
            .collect();

        let mut title = vec![
            Span::styled(" ", self.theme.dim_style()),
            Span::styled(
                self.title,
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
        ];
        if !self.footer.is_empty() {
            title.push(Span::styled(
                format!("  {} ", self.footer),
                self.theme.dim_style(),
            ));
        } else {
            title.push(Span::styled(" ", self.theme.dim_style()));
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(accent))
            .title(Line::from(title));

        let paragraph = Paragraph::new(lines).block(block);
        if self.wrap {
            paragraph.wrap(Wrap { trim: false }).render(popup, buf);
        } else {
            paragraph.render(popup, buf);
        }
    }
}
