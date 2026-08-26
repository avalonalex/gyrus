//! Drawing the debugger, and the keys that drive it.

use std::io;
use std::time::{Duration, Instant};

use gyrus::hooks::HookDecision;
use gyrus_tui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use gyrus_tui::ratatui::Frame;
use gyrus_tui::ratatui::layout::Rect;
use gyrus_tui::ratatui::style::Color;
use gyrus_tui::{
    CellDisplay, Header, HelpOverlay, MemoryView, OutputView, Overlay, Panes, Section, SourceView,
    StatusBar, WatchEntry, WatchList, cell_under, clamp_scroll, follow_pointer, follow_scroll,
};

use crate::state::{
    Exit, Focus, Modal, Note, Outcome, Prompt, PromptKind, RunState, Session, StopReason, Watch,
};

/// How wide the source column is, as a percentage.
const SOURCE_PERCENT: u16 = 58;
/// Rows given to the output panel.
const OUTPUT_HEIGHT: u16 = 7;
/// Rows given to the watch panel.
const WATCH_HEIGHT: u16 = 7;

/// What a key press decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flow {
    /// Stay paused; redraw and wait for another key.
    Stay,
    /// Let the interpreter carry on.
    Resume,
    /// Unwind out of the interpreter, to quit or restart.
    Break,
}

/// Give up on the terminal.
///
/// The failure is carried out through `Exit` so `main` can report it, rather
/// than left on a status line that only the thing which just failed knows how
/// to draw.
fn fail(session: &mut Session, error: io::Error) -> HookDecision {
    session.exit = Some(Exit::Failed(error));
    HookDecision::Break
}

/// Wait up to `timeout` for a key and act on it.
///
/// `None` means nothing arrived in time — or that the terminal stopped
/// answering, which the caller finds out about on its next draw.
fn take_key(session: &mut Session, timeout: Duration) -> Option<Flow> {
    match event::poll(timeout) {
        Ok(true) => match event::read() {
            Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                Some(handle_key(session, key))
            }
            Ok(_) => Some(Flow::Stay),
            Err(_) => None,
        },
        _ => None,
    }
}

/// Block until the user resumes, restarts, or quits.
pub fn pause(session: &mut Session) -> HookDecision {
    match interact(session) {
        Ok(Flow::Break) => HookDecision::Break,
        Ok(_) => HookDecision::Continue,
        Err(error) => fail(session, error),
    }
}

/// Redraw once mid-run and check whether the user wants to interrupt.
pub fn tick(session: &mut Session) -> HookDecision {
    if let Err(error) = draw(session) {
        return fail(session, error);
    }
    while let Some(flow) = take_key(session, Duration::ZERO) {
        if flow == Flow::Break {
            return HookDecision::Break;
        }
    }
    HookDecision::Continue
}

/// Draw one frame of a paced run, and wait out the gap before the next.
///
/// The wait is `event::poll`, not a sleep: at one instruction per second a sleep
/// would make the debugger ignore the keyboard for a second at a time, so `p`
/// and `q` would feel broken exactly when someone is most likely to reach for
/// them. Polling waits the same length and wakes on the first key.
pub fn paced(session: &mut Session, delay: Duration) -> HookDecision {
    let deadline = Instant::now() + delay;
    loop {
        // At the top of the loop, so a key that changed something is shown
        // before the wait resumes -- and so there is one draw with one failure
        // policy rather than two.
        if let Err(error) = draw(session) {
            return fail(session, error);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        match take_key(session, remaining) {
            Some(Flow::Break) => return HookDecision::Break,
            // A key asked for something else -- a step, a run to the cursor,
            // full speed. Do not sit out the rest of a gap that no longer
            // applies; that is the unresponsiveness this function exists to
            // avoid.
            Some(Flow::Resume) => break,
            Some(Flow::Stay) => {}
            // Waited the whole gap without a key: time for the next instruction.
            None => break,
        }
    }
    session.touch_draw();
    HookDecision::Continue
}

/// Show the final state and wait for restart or quit.
pub fn post_mortem(session: &mut Session) -> io::Result<()> {
    session.finished = true;
    if session.outcome.is_some() {
        session.ui.modal = Some(Modal::Result);
    }
    interact(session).map(|_| ())
}

fn interact(session: &mut Session) -> io::Result<Flow> {
    if !session.finished {
        // Whatever brought us here — a step, a breakpoint, a run-to-cursor —
        // the program is stopped now, and the status bar should say so rather
        // than still reading "running" from the state that got us here.
        session.run = RunState::Step;
        if needs_input(session) {
            session.note(
                "`,` wants a byte — press i to type some, or step to send EOF",
                Note::Info,
            );
        }
    }
    loop {
        draw(session)?;
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match handle_key(session, key) {
                Flow::Stay => {}
                flow => return Ok(flow),
            }
        }
    }
}

// ---------------------------------------------------------------- rendering

fn layout(session: &Session) -> io::Result<Panes> {
    let size = session.terminal.size()?;
    let area = Rect::new(0, 0, size.width, size.height);
    Ok(gyrus_tui::panes(
        area,
        SOURCE_PERCENT,
        OUTPUT_HEIGHT,
        WATCH_HEIGHT,
    ))
}

/// Scroll the panels so the interesting thing is on screen.
fn follow(session: &mut Session, panes: &Panes) {
    let lines = SourceView::visible_lines(panes.left);
    let columns = SourceView::visible_columns(panes.left);

    // The instruction about to execute wins; when there is none — before the
    // first step, or after the last — the user's cursor is what matters.
    // After a run ends cleanly there is no instruction to point at, and the
    // snapshot's location is cleared; after a failure it holds the instruction
    // that failed, which is exactly where the user wants to be looking.
    let (line, column) = session
        .snapshot
        .location
        .map(|location| (location.line, location.column))
        .unwrap_or(session.ui.cursor);

    session.ui.source_scroll = follow_scroll(session.ui.source_scroll, line, lines, 3);
    session.ui.source_scroll = clamp_scroll(
        session.ui.source_scroll,
        session.program.document.line_count(),
        lines,
    );
    session.ui.source_h_scroll = follow_scroll(session.ui.source_h_scroll, column, columns, 8);

    let cells = MemoryView::columns(panes.right_top, session.ui.memory_display);
    let rows = MemoryView::visible_rows(panes.right_top);

    // "Go to cell N" and "end of tape" name an address, not a row: only here is
    // it known how many cells fit on one, and it is 8, 16 or 24 depending on the
    // width and the display mode.
    if let Some(address) = session.ui.reveal.take() {
        session.ui.memory_scroll =
            follow_pointer(session.ui.memory_scroll, address as isize, cells, rows);
    } else if session.ui.memory_follow {
        session.ui.memory_scroll = follow_pointer(
            session.ui.memory_scroll,
            session.snapshot.pointer,
            cells,
            rows,
        );
    }
    if cells > 0 {
        let last_row = session.snapshot.memory.len().div_ceil(cells);
        session.ui.memory_scroll = clamp_scroll(session.ui.memory_scroll, last_row, rows);
    }
}

fn draw(session: &mut Session) -> io::Result<()> {
    let panes = layout(session)?;
    follow(session, &panes);
    session.refresh_modified();

    let fields = status_fields(session);
    let hints = status_hints(session);
    let watches = watch_entries(session);
    let (state, state_color) = run_state_label(session);
    // A handful of positions; cloning them is cheaper than widening their
    // visibility so the destructure below can reach them.
    let breakpoints = session.breakpoints().clone();
    let help_scroll = match session.ui.modal {
        Some(Modal::Help { scroll }) => Some(scroll),
        _ => None,
    };
    let result = matches!(session.ui.modal, Some(Modal::Result))
        .then(|| {
            session.outcome.as_ref().map(|outcome| {
                (
                    outcome.title(),
                    match outcome {
                        Outcome::Failed(_) => session.theme.error,
                        Outcome::Completed(_) => session.theme.success,
                    },
                )
            })
        })
        .flatten();

    let Session {
        terminal,
        program,
        theme,
        ui,
        output,
        snapshot,
        modified,
        message,
        outcome,
        ..
    } = session;

    terminal.draw(|frame: &mut Frame| {
        frame.render_widget(
            Header::new("gyrus-debug", theme)
                .subject(program.name.clone())
                .state(state, state_color),
            panes.header,
        );

        frame.render_widget(
            SourceView::new(&program.document, theme)
                .title(program.name.clone())
                .focused(ui.focus == Focus::Source)
                .current(snapshot.location.map(|l| (l.line, l.column)))
                .cursor(Some(ui.cursor), ui.focus == Focus::Source)
                .breakpoints(&breakpoints)
                .scroll(ui.source_scroll)
                .h_scroll(ui.source_h_scroll),
            panes.left,
        );

        frame.render_widget(
            MemoryView::new(&snapshot.memory, snapshot.pointer, theme)
                .modified(modified)
                .display(ui.memory_display)
                .following(ui.memory_follow)
                .focused(ui.focus == Focus::Memory)
                .scroll(ui.memory_scroll),
            panes.right_top,
        );

        if panes.right_bottom.height > 0 {
            frame.render_widget(
                WatchList::new(&watches, theme)
                    .empty_hint("none — press w")
                    .selected(if watches.is_empty() {
                        None
                    } else {
                        Some(ui.watch_selected.min(watches.len() - 1))
                    })
                    .focused(ui.focus == Focus::Watch),
                panes.right_bottom,
            );
        }

        frame.render_widget(
            OutputView::new(output, theme)
                .focused(ui.focus == Focus::Output)
                .scroll(ui.output_scroll),
            panes.output,
        );

        let note = ui
            .prompt
            .as_ref()
            .map(|prompt| {
                (
                    format!("{}: {}_", prompt.kind.label(), prompt.buffer),
                    theme.accent,
                )
            })
            .or_else(|| {
                message.as_ref().map(|(text, kind)| {
                    let color = match kind {
                        Note::Info => theme.accent,
                        Note::Warn => theme.modified,
                        Note::Error => theme.error,
                    };
                    (text.clone(), color)
                })
            });

        frame.render_widget(
            StatusBar::new(&fields, &hints, theme)
                .always(ESSENTIAL_HINTS)
                .message(note.as_ref().map(|(text, color)| (text.as_str(), *color))),
            panes.status,
        );

        if let Some((heading, color)) = &result {
            // `detail` re-formats the whole stats block or clones a formatted
            // error, so it is built once here rather than per frame.
            let detail = outcome.as_ref().map(Outcome::detail).unwrap_or_default();
            frame.render_widget(
                Overlay::new(heading, &detail, theme)
                    .accent(*color)
                    .footer("any key to look around")
                    .size(76, 60)
                    .wrap(true),
                frame.area(),
            );
        }

        if let Some(scroll) = help_scroll {
            frame.render_widget(
                HelpOverlay::new(HELP, theme)
                    .title("gyrus-debug keys")
                    .dismiss("? or esc to close")
                    .scroll(scroll),
                frame.area(),
            );
        }
    })?;
    Ok(())
}

fn run_state_label(session: &Session) -> (String, Color) {
    let theme = &session.theme;
    match (&session.outcome, session.finished) {
        (Some(Outcome::Failed(_)), _) => ("error".to_string(), theme.error),
        (Some(Outcome::Completed(_)), _) => ("finished".to_string(), theme.success),
        (None, true) => ("stopped".to_string(), theme.dim),
        // Waiting for input is a state, not an event: a message on the status
        // line is cleared by the next keypress, and the user is then looking at
        // a stopped program with no indication of why it stopped.
        (None, false) if session.run == RunState::Step => match session.stop_reason {
            StopReason::NeedsInput => ("needs input".to_string(), theme.modified),
            StopReason::OutputWatch => ("output watch".to_string(), theme.breakpoint),
            StopReason::Breakpoint => ("breakpoint".to_string(), theme.breakpoint),
            StopReason::Stepped => ("paused".to_string(), theme.accent),
        },
        (None, false) => {
            // The pace is composed onto the run state rather than replacing it:
            // a paced run-to-cursor is still a run to the cursor, and a label
            // that forgets which of them it is loses the more useful half.
            let running = match session.run {
                RunState::Step => return ("paused".to_string(), theme.accent),
                RunState::Continue => "running",
                RunState::RunTo(_) => "running to cursor",
                RunState::Leave { .. } => "stepping over",
            };
            let label = match session.pace.is_armed() {
                true => format!("{running} · {}/s", session.pace.rate()),
                false => running.to_string(),
            };
            (label, theme.modified)
        }
    }
}

fn status_fields(session: &Session) -> Vec<(&'static str, String)> {
    let snapshot = &session.snapshot;
    // `step` counts what has run; the interpreter's counter includes the
    // instruction about to run, which would make the first pause read "1".
    let mut fields = vec![("ran", snapshot.step.saturating_sub(1).to_string())];
    match snapshot.location {
        Some(location) => fields.push(("at", format!("{}:{}", location.line, location.column))),
        None => fields.push(("at", "—".to_string())),
    }
    fields.push(("depth", snapshot.loop_depth.to_string()));
    fields.push(("ptr", snapshot.pointer.to_string()));
    fields.push((
        "cell",
        usize::try_from(snapshot.pointer)
            .ok()
            .and_then(|index| snapshot.memory.get(index))
            .map_or_else(|| "off tape".to_string(), u8::to_string),
    ));
    fields.push(("changed", session.modified.len().to_string()));
    fields.push((
        "next",
        match snapshot.location {
            Some(_) => format!(
                "#{} of {}",
                snapshot.index,
                session.program.instruction_count()
            ),
            None => "—".to_string(),
        },
    ));
    fields
}

/// Hints held back from the fill, so a narrow terminal never drops them.
const ESSENTIAL_HINTS: &[(&str, &str)] = &[("?", "help"), ("q", "quit")];

fn status_hints(session: &Session) -> Vec<(&'static str, &'static str)> {
    if session.ui.prompt.is_some() {
        return vec![("enter", "accept"), ("esc", "cancel")];
    }
    if session.finished {
        return vec![("r", "restart"), ("tab", "panel")];
    }
    let mut hints = match session.run {
        RunState::Step => vec![
            ("space", "step"),
            ("s", "slow"),
            ("c", "continue"),
            ("n", "over"),
            ("o", "out"),
            ("g", "to cursor"),
            ("b", "break"),
            ("r", "restart"),
        ],
        _ if session.pace.is_armed() => {
            vec![("p", "pause"), ("+ -", "speed"), ("c", "full speed")]
        }
        _ => vec![("p", "pause"), ("s", "slow")],
    };
    // First, so that it is the last thing a narrow terminal drops. `i` is the
    // one key the program is currently waiting on, and it is otherwise buried
    // in the key list.
    if needs_input(session) {
        hints.insert(0, ("i", "type input"));
    }
    hints
}

fn watch_entries(session: &Session) -> Vec<WatchEntry> {
    session
        .watches
        .as_slice()
        .iter()
        .map(|watch| match watch {
            Watch::Cell(address) => WatchEntry::new(
                watch.label(),
                cell_under(&session.snapshot.memory, *address as isize)
                    .map_or_else(|| "off tape".to_string(), |byte| byte.to_string()),
            )
            .changed(session.modified.contains(address)),
            Watch::AnyOutput => WatchEntry::new(watch.label(), "any byte").stopping(true),
            Watch::Output(byte) => {
                WatchEntry::new(watch.label(), format!("byte {byte}")).stopping(true)
            }
        })
        .collect()
}

/// The help overlay's contents. Kept next to the key handler below so the two
/// cannot drift: a binding added there without a line here is a binding nobody
/// discovers.
const HELP: &[Section<'static>] = &[
    (
        "Execution",
        &[
            ("space", "execute one instruction"),
            ("s", "run in slow motion, so you can watch a loop turn"),
            (
                "+ / -",
                "faster / slower, while it runs or before it starts",
            ),
            ("n", "step over: run the whole loop, if this is a `[`"),
            ("o", "step out: run to the end of the enclosing loop"),
            (
                "c / enter",
                "continue at full speed, to the next breakpoint",
            ),
            ("g", "run to the cursor"),
            ("p / esc", "pause a running program"),
            ("r", "restart from the beginning"),
            ("q / ctrl-c", "quit"),
        ],
    ),
    (
        "Breakpoints",
        &[
            ("b", "toggle one at the cursor, snapped to real code"),
            ("B", "remove every breakpoint"),
        ],
    ),
    (
        "Looking around",
        &[
            ("tab", "move focus to the next panel"),
            ("↑ ↓ ← → / h j k l", "move within the focused panel"),
            (
                "shift ← →",
                "jump the cursor to the previous or next instruction",
            ),
            ("pgup / pgdn", "page the focused panel"),
            ("home / end", "jump to the start or the end"),
            ("m", "memory display: hex, decimal, ASCII"),
            ("f", "follow the pointer on or off"),
            ("w", "watch a cell"),
            (
                "O",
                "stop before the program prints — anything, or one byte",
            ),
            ("W", "remove the selected watch"),
            ("G", "scroll memory to a cell address"),
            ("L", "move the cursor to a source line"),
        ],
    ),
    (
        "Program input",
        &[
            ("i", "queue bytes for the program's next `,`"),
            (
                "",
                "an empty queue reads EOF, and the debugger stops to ask",
            ),
        ],
    ),
    ("Help", &[("? / F1", "open and close this list")]),
];

// ------------------------------------------------------------------- input

fn handle_key(session: &mut Session, key: KeyEvent) -> Flow {
    if session.ui.prompt.is_some() {
        return handle_prompt_key(session, key);
    }
    session.message = None;

    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    if control && matches!(key.code, KeyCode::Char('c')) {
        session.exit = Some(Exit::Quit);
        return Flow::Break;
    }

    match session.ui.modal {
        // Anything that is not one of the debugger's own commands dismisses the
        // result and gets on with looking around.
        Some(Modal::Result) => {
            if !matches!(key.code, KeyCode::Char('q' | 'r' | '?') | KeyCode::F(1)) {
                session.ui.modal = None;
                return Flow::Stay;
            }
        }
        Some(Modal::Help { scroll }) => {
            match key.code {
                KeyCode::Char('?') | KeyCode::F(1) | KeyCode::Esc => session.ui.modal = None,
                // `q` means quit everywhere else; making it mean "close" here
                // would be the one place it does not.
                KeyCode::Char('q') => {
                    session.exit = Some(Exit::Quit);
                    return Flow::Break;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    session.ui.modal = Some(Modal::Help {
                        scroll: scroll.saturating_sub(1),
                    });
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let height = HelpOverlay::height(HELP);
                    session.ui.modal = Some(Modal::Help {
                        scroll: (scroll + 1).min(height.saturating_sub(1)),
                    });
                }
                _ => {}
            }
            return Flow::Stay;
        }
        None => {}
    }

    match key.code {
        KeyCode::Char('?') | KeyCode::F(1) => {
            session.ui.modal = Some(Modal::Help { scroll: 0 });
            Flow::Stay
        }
        KeyCode::Char('q') => {
            session.exit = Some(Exit::Quit);
            Flow::Break
        }
        KeyCode::Char('r') => {
            session.exit = Some(Exit::Restart);
            Flow::Break
        }

        // Execution
        KeyCode::Char(' ') => resume(session, RunState::Step),
        KeyCode::Char('c') | KeyCode::Enter => {
            session.pace.disarm();
            resume(session, RunState::Continue)
        }
        KeyCode::Char('s') => pace_run(session),
        KeyCode::Char('+') | KeyCode::Char('=') => {
            session.pace.faster();
            pace_run(session)
        }
        KeyCode::Char('-') | KeyCode::Char('_') => {
            session.pace.slower();
            pace_run(session)
        }
        KeyCode::Char('n') => step_over(session),
        KeyCode::Char('o') => step_out(session),
        KeyCode::Char('g') => run_to_cursor(session),
        KeyCode::Char('p') | KeyCode::Esc => {
            // Esc is a reflex key. Answering it costs a line and stops the
            // debugger from looking unresponsive to someone trying to get out.
            if session.run == RunState::Step || session.finished {
                session.note("already stopped — c continues, q quits", Note::Info);
            }
            session.run = RunState::Step;
            Flow::Stay
        }

        // Breakpoints
        KeyCode::Char('b') => {
            match session.cursor_instruction() {
                Some((position, _)) => {
                    let added = session.toggle_breakpoint(position);
                    session.ui.cursor = position;
                    let verb = if added { "set" } else { "cleared" };
                    session.note(
                        format!("breakpoint {verb} at {}:{}", position.0, position.1),
                        Note::Info,
                    );
                }
                None => session.note("no instruction on this line", Note::Warn),
            }
            Flow::Stay
        }
        KeyCode::Char('B') => {
            let count = session.breakpoints().len();
            session.clear_breakpoints();
            session.note(format!("removed {count} breakpoints"), Note::Info);
            Flow::Stay
        }

        // Panels
        KeyCode::Tab => {
            session.ui.focus = session.ui.focus.next();
            Flow::Stay
        }
        KeyCode::BackTab => {
            session.ui.focus = session.ui.focus.previous();
            Flow::Stay
        }
        KeyCode::Char('m') => {
            session.ui.memory_display = session.ui.memory_display.next();
            Flow::Stay
        }
        KeyCode::Char('f') => {
            session.ui.memory_follow = !session.ui.memory_follow;
            let state = if session.ui.memory_follow {
                "on"
            } else {
                "off"
            };
            session.note(format!("follow pointer {state}"), Note::Info);
            Flow::Stay
        }
        KeyCode::Char('w') => {
            session.ui.prompt = Some(Prompt {
                kind: PromptKind::Watch,
                buffer: session.snapshot.pointer.max(0).to_string(),
            });
            Flow::Stay
        }
        // The same prompt with the prefix already typed, so there is one parser
        // and one label rather than two of each.
        KeyCode::Char('O') => {
            session.ui.prompt = Some(Prompt {
                kind: PromptKind::Watch,
                buffer: "out ".to_string(),
            });
            Flow::Stay
        }
        KeyCode::Char('W') => {
            if session.watches.is_empty() {
                session.note("nothing is being watched", Note::Warn);
            } else {
                let index = session.ui.watch_selected.min(session.watches.len() - 1);
                if let Some(watch) = session.remove_watch(index) {
                    session.note(format!("stopped watching {}", watch.label()), Note::Info);
                }
            }
            Flow::Stay
        }
        KeyCode::Char('G') => {
            session.ui.prompt = Some(Prompt {
                kind: PromptKind::GotoCell,
                buffer: String::new(),
            });
            Flow::Stay
        }
        KeyCode::Char('L') => {
            session.ui.prompt = Some(Prompt {
                kind: PromptKind::GotoLine,
                buffer: String::new(),
            });
            Flow::Stay
        }
        KeyCode::Char('i') => {
            session.ui.prompt = Some(Prompt {
                kind: PromptKind::Input,
                buffer: String::new(),
            });
            Flow::Stay
        }

        // Movement
        KeyCode::Up | KeyCode::Char('k') => {
            move_focused(session, -1, 0);
            Flow::Stay
        }
        KeyCode::Down | KeyCode::Char('j') => {
            move_focused(session, 1, 0);
            Flow::Stay
        }
        KeyCode::Left | KeyCode::Char('h') if !key.modifiers.contains(KeyModifiers::SHIFT) => {
            move_focused(session, 0, -1);
            Flow::Stay
        }
        KeyCode::Right | KeyCode::Char('l') if !key.modifiers.contains(KeyModifiers::SHIFT) => {
            move_focused(session, 0, 1);
            Flow::Stay
        }
        // Shift-arrow skips the comment text between instructions, which on a
        // heavily commented program is most of the line.
        KeyCode::Right => {
            if let Some(position) = session.program.next_instruction(session.ui.cursor) {
                session.ui.cursor = position;
                session.ui.focus = Focus::Source;
            }
            Flow::Stay
        }
        KeyCode::Left => {
            if let Some(position) = session.program.previous_instruction(session.ui.cursor) {
                session.ui.cursor = position;
                session.ui.focus = Focus::Source;
            }
            Flow::Stay
        }
        KeyCode::PageUp => {
            move_focused(session, -10, 0);
            Flow::Stay
        }
        KeyCode::PageDown => {
            move_focused(session, 10, 0);
            Flow::Stay
        }
        KeyCode::Home => {
            jump_focused(session, true);
            Flow::Stay
        }
        KeyCode::End => {
            jump_focused(session, false);
            Flow::Stay
        }
        _ => Flow::Stay,
    }
}

/// Read a watch: a cell address, or a condition on what the program prints.
///
/// A bare number is a cell, because that is what `w` has always meant. Output
/// conditions are spelled the way the watch panel displays them — `out`,
/// `out W`, `out \n` — so what you type and what you read back match.
///
/// The value after `out ` is taken exactly, not trimmed: a space is a byte
/// someone may reasonably want to stop on.
pub fn parse_watch(text: &str) -> Result<Watch, String> {
    let start = text.trim_start();
    // The prefix is tested before the bare words, because `out ` followed by a
    // space is the space byte -- and trimming first would reduce it to `out`
    // and answer "any byte" instead.
    for prefix in ["output ", "out "] {
        if let Some(value) = start.strip_prefix(prefix) {
            return parse_output_watch(value);
        }
    }
    if matches!(start.trim_end(), "any" | "out" | "output") {
        return Ok(Watch::AnyOutput);
    }
    start
        .trim_end()
        .parse::<usize>()
        .map(Watch::Cell)
        .map_err(|_| {
            format!(
                "{:?} is not a cell number — try 5, or `out W` to stop when W is printed",
                start.trim_end()
            )
        })
}

/// Read a `--break-output`-style value: `any`, one character, an escape, or `#N`.
pub fn parse_output_watch(text: &str) -> Result<Watch, String> {
    if text.eq_ignore_ascii_case("any") || text.is_empty() {
        return Ok(Watch::AnyOutput);
    }
    // A single character means that character, so `1` is the digit one. Byte
    // values need `#`, which keeps the two unambiguous in both directions.
    let mut chars = text.chars();
    if let (Some(ch), None) = (chars.next(), chars.next()) {
        let mut buffer = [0u8; 4];
        let encoded = ch.encode_utf8(&mut buffer);
        if encoded.len() == 1 {
            return Ok(Watch::Output(encoded.as_bytes()[0]));
        }
        return Err(format!("{ch:?} is not a single byte"));
    }
    let byte = match text {
        "\\n" => Some(b'\n'),
        "\\t" => Some(b'\t'),
        "\\r" => Some(b'\r'),
        "\\0" => Some(0),
        _ => text.strip_prefix('#').and_then(|n| n.parse::<u8>().ok()),
    };
    byte.map(Watch::Output)
        .ok_or_else(|| format!("{text:?} is not a character, an escape, #0..#255, or \"any\""))
}

/// Pace the run in progress, or start one.
///
/// This is what makes "a constraint on running, not a way of running" true. A
/// program already running — to the cursor, over a loop — keeps doing that and
/// only slows down; replacing its run state here would silently discard the
/// cursor it was heading for. A stopped one starts, which is what `-` means
/// when nothing is moving: "go, but slowly".
///
/// Starting goes through `resume` rather than setting the run state directly,
/// so it inherits the rest of what resuming means — including that resuming
/// from a `,` with an empty queue is how the user chooses EOF.
fn pace_run(session: &mut Session) -> Flow {
    if session.run != RunState::Step && !session.finished {
        session.pace.arm();
        return Flow::Stay;
    }
    let flow = resume(session, RunState::Continue);
    // Only if it actually started: on a finished program `resume` explains
    // itself and stays put, and arming there would pace whatever came next.
    if flow == Flow::Resume {
        session.pace.arm();
    }
    flow
}

/// Whether the instruction about to run is a `,` with nothing queued to read.
fn needs_input(session: &Session) -> bool {
    session.snapshot.location.is_some()
        && session.program.reads_input(session.snapshot.index)
        && session.starving_for_input()
}

/// Set the run state and let the interpreter go, or explain why it cannot.
fn resume(session: &mut Session, run: RunState) -> Flow {
    if session.finished {
        session.note(
            "the program has finished — press r to run it again",
            Note::Warn,
        );
        return Flow::Stay;
    }
    // Resuming from a `,` with an empty queue is the user choosing EOF.
    if needs_input(session) {
        session.input_eof = true;
    }
    session.run = run;
    session.message = None;
    Flow::Resume
}

fn step_over(session: &mut Session) -> Flow {
    let index = session.snapshot.index;
    match session.program.loop_extent(index) {
        Some((start, end)) => resume(session, RunState::Leave { start, end }),
        // Not a loop head: stepping over an ordinary instruction is stepping.
        None => resume(session, RunState::Step),
    }
}

fn step_out(session: &mut Session) -> Flow {
    match session.loop_stack.last().copied() {
        Some((start, end)) => resume(session, RunState::Leave { start, end }),
        None => {
            session.note("not inside a loop — c continues to the end", Note::Warn);
            Flow::Stay
        }
    }
}

fn run_to_cursor(session: &mut Session) -> Flow {
    match session.cursor_instruction() {
        Some((position, index)) => {
            session.ui.cursor = position;
            resume(session, RunState::RunTo(index))
        }
        None => {
            session.note("no instruction on this line", Note::Warn);
            Flow::Stay
        }
    }
}

// ------------------------------------------------------------------ prompts

fn handle_prompt_key(session: &mut Session, key: KeyEvent) -> Flow {
    let Some(prompt) = session.ui.prompt.as_mut() else {
        return Flow::Stay;
    };
    match key.code {
        KeyCode::Esc => {
            session.ui.prompt = None;
        }
        KeyCode::Backspace => {
            prompt.buffer.pop();
        }
        KeyCode::Char(ch) => prompt.buffer.push(ch),
        KeyCode::Enter => {
            let prompt = session.ui.prompt.take().expect("checked above");
            apply_prompt(session, prompt);
        }
        _ => {}
    }
    Flow::Stay
}

fn apply_prompt(session: &mut Session, prompt: Prompt) {
    // Not trimmed for an output watch: a single space is a byte someone might
    // reasonably want to stop on, and trimming would turn it into the empty
    // answer, which means "any byte" -- silently, and with a note that reads
    // like confirmation.
    // The watch prompt is not trimmed: `out ` followed by a space names the
    // space byte, and trimming would turn it into the empty answer, which means
    // any byte -- silently, and with a note that reads like confirmation.
    let text = match prompt.kind {
        PromptKind::Watch => prompt.buffer.clone(),
        _ => prompt.buffer.trim().to_string(),
    };
    match prompt.kind {
        PromptKind::Watch => match parse_watch(&text) {
            Ok(watch) if session.add_watch(watch) => {
                let note = if watch.stops() {
                    format!("stopping before {} — W removes it", watch.label())
                } else {
                    format!("watching {}", watch.label())
                };
                session.note(note, Note::Info);
            }
            Ok(watch) => session.note(format!("already watching {}", watch.label()), Note::Warn),
            Err(why) => session.note(why, Note::Error),
        },
        PromptKind::GotoCell => match text.parse::<usize>() {
            Ok(address) => {
                session.ui.memory_follow = false;
                session.ui.reveal = Some(address);
                session.note(
                    format!("showing cell[{address}] — f re-enables follow"),
                    Note::Info,
                );
            }
            Err(_) => session.note(format!("not a cell address: {text:?}"), Note::Error),
        },
        PromptKind::GotoLine => match text.parse::<usize>() {
            Ok(line) if line >= 1 && line <= session.program.document.line_count() => {
                session.ui.cursor = (line, 1);
                session.ui.focus = Focus::Source;
            }
            Ok(_) => session.note("no such line", Note::Error),
            Err(_) => session.note(format!("not a line number: {text:?}"), Note::Error),
        },
        PromptKind::Input => {
            let bytes = prompt.buffer.as_bytes();
            session.pending_input.extend(bytes.iter().copied());
            session.pending_input.push_back(b'\n');
            session.input_eof = false;
            session.note(
                format!("queued {} bytes (with a trailing newline)", bytes.len() + 1),
                Note::Info,
            );
        }
    }
}

// ----------------------------------------------------------------- movement

fn move_focused(session: &mut Session, rows: isize, columns: isize) {
    match session.ui.focus {
        Focus::Source => {
            let (line, column) = session.ui.cursor;
            let line = (line as isize + rows)
                .clamp(1, session.program.document.line_count() as isize)
                as usize;
            let width = session.program.document.line_width(line);
            let column = (column as isize + columns).clamp(1, (width.max(1)) as isize) as usize;
            session.ui.cursor = (line, column);
        }
        Focus::Memory => {
            if rows != 0 {
                scroll_memory(session, rows);
            }
        }
        Focus::Watch => {
            if rows != 0 && !session.watches.is_empty() {
                let last = session.watches.len() - 1;
                session.ui.watch_selected =
                    (session.ui.watch_selected as isize + rows).clamp(0, last as isize) as usize;
            }
        }
        Focus::Output => {
            if rows != 0 {
                scroll_output(session, rows);
            }
        }
    }
}

fn scroll_memory(session: &mut Session, rows: isize) {
    session.ui.memory_follow = false;
    session.ui.memory_scroll = (session.ui.memory_scroll as isize + rows).max(0) as usize;
}

fn scroll_output(session: &mut Session, rows: isize) {
    let lines = OutputView::display_lines(&session.output).len();
    let current = session.ui.output_scroll.unwrap_or(lines.saturating_sub(1));
    let next = (current as isize + rows).clamp(0, lines.saturating_sub(1) as isize) as usize;
    session.ui.output_scroll = if next + 1 >= lines { None } else { Some(next) };
}

fn jump_focused(session: &mut Session, start: bool) {
    match session.ui.focus {
        Focus::Source => {
            session.ui.cursor = if start {
                (1, 1)
            } else {
                let line = session.program.document.line_count();
                (line, session.program.document.line_width(line).max(1))
            };
        }
        Focus::Memory => {
            session.ui.memory_follow = false;
            if start {
                session.ui.memory_scroll = 0;
            } else {
                session.ui.reveal = Some(session.snapshot.memory.len().saturating_sub(1));
            }
        }
        Focus::Watch => {
            session.ui.watch_selected = if start {
                0
            } else {
                session.watches.len().saturating_sub(1)
            };
        }
        Focus::Output => {
            session.ui.output_scroll = if start { Some(0) } else { None };
        }
    }
}

/// Set the display mode from a command-line flag.
pub fn parse_display(name: &str) -> Option<CellDisplay> {
    match name {
        "hex" => Some(CellDisplay::Hex),
        "decimal" | "dec" => Some(CellDisplay::Decimal),
        "ascii" => Some(CellDisplay::Ascii),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_number_is_a_cell() {
        assert_eq!(parse_watch("5").unwrap(), Watch::Cell(5));
        assert_eq!(parse_watch("  12  ").unwrap(), Watch::Cell(12));
    }

    #[test]
    fn out_reads_the_way_the_panel_writes_it() {
        assert_eq!(parse_watch("out").unwrap(), Watch::AnyOutput);
        assert_eq!(parse_watch("output").unwrap(), Watch::AnyOutput);
        assert_eq!(parse_watch("any").unwrap(), Watch::AnyOutput);
        assert_eq!(parse_watch("out W").unwrap(), Watch::Output(b'W'));
        assert_eq!(parse_watch("output W").unwrap(), Watch::Output(b'W'));
        assert_eq!(parse_watch("out \\n").unwrap(), Watch::Output(b'\n'));
        assert_eq!(parse_watch("out #10").unwrap(), Watch::Output(10));
    }

    #[test]
    fn out_followed_by_a_space_means_the_space_byte() {
        // Trimming the answer would make this "any byte" instead, silently.
        assert_eq!(parse_watch("out  ").unwrap(), Watch::Output(b' '));
    }

    #[test]
    fn a_digit_after_out_is_the_character_not_the_cell() {
        assert_eq!(parse_watch("5").unwrap(), Watch::Cell(5));
        assert_eq!(parse_watch("out 5").unwrap(), Watch::Output(b'5'));
    }

    #[test]
    fn an_answer_that_is_neither_says_what_both_look_like() {
        let error = parse_watch("W").unwrap_err();
        assert!(error.contains("cell number"), "{error}");
        assert!(error.contains("out W"), "{error}");
    }

    #[test]
    fn any_is_the_word_and_the_empty_answer() {
        assert_eq!(parse_output_watch("any").unwrap(), Watch::AnyOutput);
        assert_eq!(parse_output_watch("ANY").unwrap(), Watch::AnyOutput);
        assert_eq!(parse_output_watch("").unwrap(), Watch::AnyOutput);
    }

    #[test]
    fn one_character_means_that_character() {
        // Including a digit: `1` is the character, and `#49` is the byte.
        assert_eq!(parse_output_watch("W").unwrap(), Watch::Output(b'W'));
        assert_eq!(parse_output_watch("1").unwrap(), Watch::Output(b'1'));
        assert_eq!(parse_output_watch(" ").unwrap(), Watch::Output(b' '));
    }

    #[test]
    fn escapes_and_byte_values_reach_the_bytes_you_cannot_type() {
        assert_eq!(parse_output_watch("\\n").unwrap(), Watch::Output(b'\n'));
        assert_eq!(parse_output_watch("\\t").unwrap(), Watch::Output(b'\t'));
        assert_eq!(parse_output_watch("\\0").unwrap(), Watch::Output(0));
        assert_eq!(parse_output_watch("#10").unwrap(), Watch::Output(10));
        assert_eq!(parse_output_watch("#255").unwrap(), Watch::Output(255));
    }

    #[test]
    fn what_cannot_be_one_byte_is_refused() {
        assert!(parse_output_watch("#256").is_err());
        assert!(parse_output_watch("hello").is_err());
        // A cell holds one byte, so a multi-byte character can never be printed
        // by a single `.`.
        assert!(parse_output_watch("é").is_err());
    }
}
