//! Test helpers. `#[cfg(test)]` and private, so they are not API.
//!
//! Widget tests all want the same thing: draw one widget into a fixed-size
//! buffer and read it back as text. Asserting on the text catches what actually
//! goes wrong -- a marker in the wrong column, a title that no longer fits --
//! and ignores styling, which no assertion should be pinned to.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::widgets::Widget;

/// Render `widget` into a `width` x `height` buffer and return its rows.
pub fn render<W: Widget>(widget: W, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend");
    terminal
        .draw(|frame| frame.render_widget(widget, frame.area()))
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
