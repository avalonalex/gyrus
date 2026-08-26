//! Header, status line, and the key hints along the bottom.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::theme::Theme;

/// A `label: value` pair on the status line.
pub type Field<'a> = (&'a str, String);

/// A `key → action` pair in the hint row.
pub type Hint<'a> = (&'a str, &'a str);

/// The one-line header across the top of the screen.
pub struct Header<'a> {
    title: &'a str,
    subject: String,
    state: String,
    state_color: ratatui::style::Color,
    theme: &'a Theme,
}

impl<'a> Header<'a> {
    /// A header reading `title · subject · state`.
    pub fn new(title: &'a str, theme: &'a Theme) -> Self {
        Self {
            title,
            subject: String::new(),
            state: String::new(),
            state_color: theme.accent,
            theme,
        }
    }

    /// What is being worked on: a file name, a lesson title.
    pub fn subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = subject.into();
        self
    }

    /// The current state, drawn in `color`.
    pub fn state(mut self, state: impl Into<String>, color: ratatui::style::Color) -> Self {
        self.state = state.into();
        self.state_color = color;
        self
    }
}

impl Widget for Header<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut spans = vec![Span::styled(
            format!(" {} ", self.title),
            Style::default()
                .fg(self.theme.accent)
                .add_modifier(Modifier::BOLD),
        )];
        if !self.subject.is_empty() {
            spans.push(Span::styled("│ ", self.theme.dim_style()));
            spans.push(Span::styled(
                format!("{} ", self.subject),
                Style::default().fg(self.theme.title),
            ));
        }
        if !self.state.is_empty() {
            spans.push(Span::styled("│ ", self.theme.dim_style()));
            spans.push(Span::styled(
                format!("{} ", self.state),
                Style::default()
                    .fg(self.state_color)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        Paragraph::new(Line::from(spans)).render(area, buf);
    }
}

/// The two rows along the bottom: status fields, then key hints.
pub struct StatusBar<'a> {
    fields: &'a [Field<'a>],
    hints: &'a [Hint<'a>],
    message: Option<(&'a str, ratatui::style::Color)>,
    theme: &'a Theme,
}

impl<'a> StatusBar<'a> {
    /// A status bar showing `fields` above `hints`.
    pub fn new(fields: &'a [Field<'a>], hints: &'a [Hint<'a>], theme: &'a Theme) -> Self {
        Self {
            fields,
            hints,
            message: None,
            theme,
        }
    }

    /// Replace the field row with a one-off message, such as an error.
    pub fn message(mut self, message: Option<(&'a str, ratatui::style::Color)>) -> Self {
        self.message = message;
        self
    }
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let status = match self.message {
            Some((text, color)) => Line::from(Span::styled(
                format!(" {text}"),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )),
            None => {
                let mut spans = Vec::new();
                for (label, value) in self.fields {
                    spans.push(Span::styled(format!(" {label} "), self.theme.dim_style()));
                    spans.push(Span::styled(
                        value.clone(),
                        Style::default().fg(self.theme.title),
                    ));
                    spans.push(Span::styled("  ", self.theme.dim_style()));
                }
                Line::from(spans)
            }
        };

        let mut hint_spans = Vec::new();
        for (key, action) in self.hints {
            hint_spans.push(Span::styled(
                format!(" {key}"),
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
            hint_spans.push(Span::styled(format!(" {action} "), self.theme.dim_style()));
        }

        Paragraph::new(vec![status, Line::from(hint_spans)]).render(area, buf);
    }
}
