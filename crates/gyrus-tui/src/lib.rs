//! Terminal-UI parts shared by the gyrus debugger and the tutorial.
//!
//! This crate holds the widgets both binaries draw — source, memory, output,
//! watches, status, help — and nothing about what either of them *does*. The
//! debugger's stepping logic lives in `gyrus-debug`; the lessons live in
//! `gyrus-tutorial`. Anything that would have to know about breakpoints or
//! lesson progress does not belong here.
//!
//! The widgets are plain [`ratatui::widgets::Widget`] implementations built
//! with the usual `let view = SourceView::new(..).title(..).focused(true)`
//! chain. They own no state: every one of them takes what it needs to draw one
//! frame and is consumed by the render.
//!
//! # Example
//!
//! ```no_run
//! use gyrus_tui::{SourceDocument, SourceView, Theme, TerminalGuard};
//!
//! # fn main() -> std::io::Result<()> {
//! let theme = Theme::default();
//! let doc = SourceDocument::new("+++[>+<-]");
//! let (_guard, mut terminal) = TerminalGuard::enter()?;
//! terminal.draw(|frame| {
//!     let view = SourceView::new(&doc, &theme).current(Some((1, 4)));
//!     frame.render_widget(view, frame.area());
//! })?;
//! # Ok(())
//! # }
//! ```

pub mod help;
pub mod layout;
pub mod memory;
pub mod output;
pub mod overlay;
pub mod source;
pub mod status;
pub mod tape;
pub mod terminal;
pub mod theme;
pub mod watch;

pub use help::{HelpOverlay, Section};
pub use layout::{LessonPanes, Panes, centered, centered_rect, lesson_panes, panes};
pub use memory::{CellDisplay, MemoryView, follow_pointer};
pub use output::OutputView;
pub use overlay::Overlay;
pub use source::{Position, SourceDocument, SourceView, clamp_scroll, follow_scroll};
pub use status::{Field, Header, Hint, StatusBar};
pub use tape::TapeStrip;
pub use terminal::{TerminalGuard, Tui, restore};
pub use theme::{Category, Theme, classify_line};
pub use watch::{WatchEntry, WatchList};

/// Re-exported so the binaries do not need their own crossterm dependency.
pub use ratatui;
pub use ratatui::crossterm;
