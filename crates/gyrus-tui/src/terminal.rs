//! Entering and — more importantly — leaving the alternate screen.

use std::io::{self, Stdout};

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

/// The terminal type both binaries draw to.
pub type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Put the terminal in raw mode on the alternate screen, and put it back
/// afterwards — including on a panic.
///
/// A TUI that panics without restoring leaves the user with a shell that does
/// not echo. The panic hook is what makes a bug an inconvenience rather than a
/// reason to close the window, so it is installed here rather than left to the
/// caller to remember.
pub struct TerminalGuard {
    _private: (),
}

impl TerminalGuard {
    /// Take over the terminal, returning the guard and the terminal to draw on.
    ///
    /// The two are separate so the terminal can be moved into whatever owns the
    /// application state while the guard stays on the stack of `main`, where a
    /// return or a `?` still runs its `Drop`.
    pub fn enter() -> io::Result<(Self, Tui)> {
        install_panic_hook();
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        Ok((Self { _private: () }, terminal))
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = restore();
    }
}

/// Undo everything [`TerminalGuard::enter`] did. Safe to call twice.
pub fn restore() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore();
        previous(info);
    }));
}
