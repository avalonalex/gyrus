//! The screen splits both binaries use.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// The regions of a debugger-style screen.
#[derive(Debug, Clone, Copy)]
pub struct Panes {
    /// One-line header across the top.
    pub header: Rect,
    /// Left column: source code.
    pub left: Rect,
    /// Right column, upper: memory.
    pub right_top: Rect,
    /// Right column, lower: watch expressions.
    pub right_bottom: Rect,
    /// Full-width strip above the status bar: program output.
    pub output: Rect,
    /// Two-line status bar.
    pub status: Rect,
}

/// Split `area` into the standard layout: header, two columns, output, status.
///
/// `left_percent` is how much width the source column takes. The right column
/// is split so the watch list gets `watch_height` rows and memory takes the
/// rest; on a short terminal the watch list is dropped rather than squeezed.
pub fn panes(area: Rect, left_percent: u16, output_height: u16, watch_height: u16) -> Panes {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(6),
            Constraint::Length(output_height),
            Constraint::Length(2),
        ])
        .split(area);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left_percent),
            Constraint::Percentage(100 - left_percent),
        ])
        .split(rows[1]);

    let show_watch = columns[1].height > watch_height + 6;
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),
            Constraint::Length(if show_watch { watch_height } else { 0 }),
        ])
        .split(columns[1]);

    Panes {
        header: rows[0],
        left: columns[0],
        right_top: right[0],
        right_bottom: right[1],
        output: rows[2],
        status: rows[3],
    }
}

/// The regions of a lesson screen: prose on the left, work on the right.
#[derive(Debug, Clone, Copy)]
pub struct LessonPanes {
    /// One-line header across the top.
    pub header: Rect,
    /// Left column: the lesson text.
    pub lesson: Rect,
    /// Right column, top: the learner's program.
    pub code: Rect,
    /// Right column, middle: the tape.
    pub tape: Rect,
    /// Right column, bottom: the program's output.
    pub output: Rect,
    /// Two-line status bar.
    pub status: Rect,
}

/// Split `area` for a lesson: header, two columns, status.
///
/// The left column takes `left_percent` of the width. On the right, the editor
/// and the tape get fixed heights and the output takes whatever is left — a
/// lesson program is a line or two, and a runaway one prints thousands.
pub fn lesson_panes(
    area: Rect,
    left_percent: u16,
    code_height: u16,
    tape_height: u16,
) -> LessonPanes {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
        .split(area);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left_percent),
            Constraint::Percentage(100 - left_percent),
        ])
        .split(rows[1]);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(code_height),
            Constraint::Length(tape_height),
            Constraint::Min(4),
        ])
        .split(columns[1]);

    LessonPanes {
        header: rows[0],
        lesson: columns[0],
        code: right[0],
        tape: right[1],
        output: right[2],
        status: rows[2],
    }
}

/// A rectangle of exactly `width` by `height` cells, centered in `area`.
///
/// Both dimensions are clamped to `area`, so a popup asking for more room than
/// the terminal has gets the terminal instead of falling off the edge.
pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

/// A rectangle covering `percent_x` by `percent_y` of `area`, centered.
pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}
