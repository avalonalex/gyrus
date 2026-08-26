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
    always: &'a [Hint<'a>],
    message: Option<(&'a str, ratatui::style::Color)>,
    theme: &'a Theme,
}

impl<'a> StatusBar<'a> {
    /// A status bar showing `fields` above `hints`.
    pub fn new(fields: &'a [Field<'a>], hints: &'a [Hint<'a>], theme: &'a Theme) -> Self {
        Self {
            fields,
            hints,
            always: &[],
            message: None,
            theme,
        }
    }

    /// Hints that must survive a narrow terminal, drawn at the right.
    ///
    /// The row is filled from the left with `hints` and truncated when it runs
    /// out of room, so whatever is last gets dropped first. On an 80-column
    /// terminal that was "q quit" — the one thing someone who cannot work out
    /// how to leave needs to see. These are reserved before the rest are laid
    /// out, so they are never the ones that fall off.
    pub fn always(mut self, always: &'a [Hint<'a>]) -> Self {
        self.always = always;
        self
    }

    /// Replace the field row with a one-off message, such as an error.
    pub fn message(mut self, message: Option<(&'a str, ratatui::style::Color)>) -> Self {
        self.message = message;
        self
    }
}

/// Width one `label value  ` field takes.
fn field_width((label, value): &Field<'_>) -> usize {
    label.chars().count() + value.chars().count() + 4
}

/// Width one ` key action ` hint takes.
fn hint_width((key, action): &Hint<'_>) -> usize {
    key.chars().count() + action.chars().count() + 3
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let width = area.width as usize;

        let status = match self.message {
            Some((text, color)) => Line::from(Span::styled(
                format!(" {text}"),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )),
            None => {
                let mut spans = Vec::new();
                let mut used = 0;
                for field in self.fields {
                    // Drop whole fields rather than letting one clip. A status
                    // row is mostly numbers, and half a number does not look
                    // truncated -- it looks like a smaller number.
                    used += field_width(field);
                    if used > width {
                        break;
                    }
                    let (label, value) = field;
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

        let reserved: usize = self.always.iter().map(hint_width).sum();
        let mut hint_spans = Vec::new();
        let mut used = 0;
        let mut dropped = false;
        for hint in self.hints {
            used += hint_width(hint);
            // Leave room for the reserved hints, and for the "…" that says
            // some were left out.
            if used + reserved + 2 > width {
                dropped = true;
                break;
            }
            push_hint(&mut hint_spans, hint, self.theme);
        }
        if dropped {
            hint_spans.push(Span::styled(" …", self.theme.dim_style()));
        }
        for hint in self.always {
            push_hint(&mut hint_spans, hint, self.theme);
        }

        Paragraph::new(vec![status, Line::from(hint_spans)]).render(area, buf);
    }
}

fn push_hint<'a>(spans: &mut Vec<Span<'a>>, (key, action): &'a Hint<'a>, theme: &Theme) {
    spans.push(Span::styled(
        format!(" {key}"),
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(format!(" {action} "), theme.dim_style()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::render;

    const FIELDS: &[Field<'static>] = &[];

    fn bar<'a>(
        fields: &'a [Field<'a>],
        hints: &'a [Hint<'a>],
        always: &'a [Hint<'a>],
        theme: &'a Theme,
    ) -> StatusBar<'a> {
        StatusBar::new(fields, hints, theme).always(always)
    }

    #[test]
    fn reserved_hints_survive_a_narrow_terminal() {
        // The row wants 87 columns; an 80-column terminal is the common case,
        // and "q quit" being what falls off is the worst possible outcome.
        let theme = Theme::default();
        let hints = [
            ("space", "step"),
            ("n", "over"),
            ("o", "out"),
            ("c", "continue"),
            ("g", "to cursor"),
            ("b", "break"),
            ("r", "restart"),
        ];
        let always = [("?", "help"), ("q", "quit")];
        let lines = render(bar(FIELDS, &hints, &always, &theme), 80, 2);
        assert!(lines[1].contains("q quit"), "{lines:?}");
        assert!(lines[1].contains("? help"), "{lines:?}");
        assert!(lines[1].contains('…'), "dropped hints unmarked: {lines:?}");
    }

    #[test]
    fn everything_fits_when_there_is_room() {
        let theme = Theme::default();
        let hints = [("space", "step")];
        let always = [("q", "quit")];
        let lines = render(bar(FIELDS, &hints, &always, &theme), 80, 2);
        assert!(lines[1].contains("space step"), "{lines:?}");
        assert!(!lines[1].contains('…'), "{lines:?}");
    }

    #[test]
    fn fields_are_dropped_whole_rather_than_clipped() {
        // A clipped number does not read as clipped -- "30000" cut to "3000" is
        // a plausible wrong answer, which is worse than a missing one.
        let theme = Theme::default();
        let fields = [
            ("ran", "905".to_string()),
            ("at", "1:12".to_string()),
            ("next", "#11 of 103".to_string()),
        ];
        let lines = render(bar(&fields, &[], &[], &theme), 24, 2);
        assert!(lines[0].contains("ran 905"), "{lines:?}");
        assert!(
            !lines[0].contains("next"),
            "a field that cannot fit whole must be dropped: {lines:?}"
        );
    }
}
