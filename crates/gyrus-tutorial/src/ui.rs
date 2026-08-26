//! Drawing the tutorial, and the keys that drive it.

use std::io;

use gyrus_tui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use gyrus_tui::ratatui::Frame;
use gyrus_tui::ratatui::layout::Rect;
use gyrus_tui::ratatui::style::{Color, Style};
use gyrus_tui::ratatui::text::{Line, Span};
use gyrus_tui::ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};
use gyrus_tui::{
    Header, HelpOverlay, OutputView, Overlay, Section, SourceView, StatusBar, TapeStrip, Tui,
    cell_under, clamp_scroll,
};

use crate::app::{App, Focus, Note, Popup};
use crate::lesson::{LESSONS, Verdict};
// Aliased: ratatui's `Frame` is the drawing surface, and this one is a
// recorded step. Both appear in this file.
use crate::trace::{Ending, Frame as TraceFrame};

/// Width of the lesson-text column, as a percentage. Prose needs the room;
/// a lesson program is a line or two.
const LESSON_PERCENT: u16 = 52;
/// Rows given to the editor.
const CODE_HEIGHT: u16 = 7;
/// Rows given to the tape strip: four rows of labels, its border, and one
/// spare for the "off the tape" note.
const TAPE_HEIGHT: u16 = 7;

/// Draw, wait for a key, act. Returns when the learner quits.
pub fn run(terminal: &mut Tui, app: &mut App) -> io::Result<()> {
    while !app.quit {
        draw(terminal, app)?;
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => handle_key(app, key),
            _ => {}
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- rendering

fn draw(terminal: &mut Tui, app: &mut App) -> io::Result<()> {
    let size = terminal.size()?;
    let area = Rect::new(0, 0, size.width, size.height);
    let panes = gyrus_tui::lesson_panes(area, LESSON_PERCENT, CODE_HEIGHT, TAPE_HEIGHT);

    let step = app.step();
    let frame_state = app.trace.as_ref().map(|trace| trace.frame(step));
    let changed = app
        .trace
        .as_ref()
        .map(|trace| trace.changed_at(step))
        .unwrap_or_default();

    let empty_tape = vec![0; app.current().cells];
    let memory: &[u8] = frame_state
        .as_ref()
        .map_or(&empty_tape, |frame| frame.memory.as_slice());
    let pointer = frame_state.as_ref().map_or(0, |frame| frame.pointer);
    app.tape_offset = TapeStrip::follow(app.tape_offset, pointer, TapeStrip::capacity(panes.tape));

    // Borrowed, not copied: the whole output is already in the trace, and a
    // runaway lesson program writes thousands of bytes a frame would re-copy.
    let output: &[u8] = match (&app.trace, &frame_state) {
        (Some(trace), Some(frame)) => &trace.output[..frame.output_len.min(trace.output.len())],
        (Some(trace), None) => &trace.output,
        _ => &[],
    };

    let current = frame_state
        .as_ref()
        .and_then(|frame| frame.location)
        .map(|location| (location.line, location.column));

    let lesson = app.current();
    let prose_lines = app.prose.lines().count();
    let prose_height = panes.lesson.height.saturating_sub(2) as usize;
    app.lesson_scroll = clamp_scroll(app.lesson_scroll, prose_lines, prose_height);

    let (state, state_color) = header_state(app);
    let fields = status_fields(app, frame_state.as_ref());
    let hints = status_hints(app);
    let note = app.message.as_ref().map(|(text, kind)| {
        (
            text.clone(),
            match kind {
                Note::Info => app.theme.accent,
                Note::Good => app.theme.success,
                Note::Bad => app.theme.error,
            },
        )
    });
    let popup = popup_content(app);

    terminal.draw(|frame: &mut Frame| {
        frame.render_widget(
            Header::new("gyrus-tutorial", &app.theme)
                .subject(format!(
                    "{} of {} · {}",
                    app.lesson + 1,
                    LESSONS.len(),
                    lesson.title
                ))
                .state(state.clone(), state_color),
            panes.header,
        );

        frame.render_widget(
            LessonText {
                prose: &app.prose,
                scroll: app.lesson_scroll,
                theme: &app.theme,
                title: lesson.title,
            },
            panes.lesson,
        );

        frame.render_widget(
            SourceView::new(&app.document, &app.theme)
                .title("Your program")
                .focused(app.focus == Focus::Code)
                .cursor(Some(app.editor.cursor()), app.focus == Focus::Code)
                .current(current),
            panes.code,
        );

        frame.render_widget(
            TapeStrip::new(memory, pointer, &app.theme)
                .changed(&changed)
                .offset(app.tape_offset)
                .title(tape_title(app, step)),
            panes.tape,
        );

        frame.render_widget(
            OutputView::new(output, &app.theme).focused(false),
            panes.output,
        );

        frame.render_widget(
            StatusBar::new(&fields, &hints, &app.theme)
                .always(ESSENTIAL_HINTS)
                .message(note.as_ref().map(|(text, color)| (text.as_str(), *color))),
            panes.status,
        );

        if let Some((title, body, footer, color)) = &popup {
            frame.render_widget(
                Overlay::new(title, body, &app.theme)
                    .accent(*color)
                    .footer(footer)
                    .size(72, 80)
                    .wrap(true),
                frame.area(),
            );
        }

        if app.popup == Some(Popup::Help) {
            frame.render_widget(
                HelpOverlay::new(HELP, &app.theme)
                    .title("gyrus-tutorial keys")
                    .dismiss("F1 or esc to close"),
                frame.area(),
            );
        }
    })?;
    Ok(())
}

/// The lesson prose panel. Its own widget only because the text needs wrapping
/// and a scroll offset, which no shared panel does.
struct LessonText<'a> {
    prose: &'a str,
    scroll: usize,
    theme: &'a gyrus_tui::Theme,
    title: &'a str,
}

impl Widget for LessonText<'_> {
    fn render(self, area: Rect, buf: &mut gyrus_tui::ratatui::buffer::Buffer) {
        let lines: Vec<Line> = self
            .prose
            .lines()
            .skip(self.scroll)
            .map(|line| {
                let style = if line.starts_with("  ")
                    && line
                        .trim()
                        .starts_with(['+', '-', '<', '>', '[', ']', '.', ','])
                {
                    Style::default().fg(self.theme.accent)
                } else {
                    Style::default().fg(self.theme.title)
                };
                Line::from(Span::styled(line.to_string(), style))
            })
            .collect();

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(self.theme.border_style(false))
                    .title(Line::from(vec![
                        Span::styled(" ", self.theme.dim_style()),
                        Span::styled(self.title, self.theme.title_style()),
                        Span::styled("  ↑↓ to read on ", self.theme.dim_style()),
                    ])),
            )
            .render(area, buf);
    }
}

fn header_state(app: &App) -> (String, Color) {
    match &app.verdict {
        Some(Verdict::Solved) => ("solved".to_string(), app.theme.success),
        Some(Verdict::Nothing) => ("read".to_string(), app.theme.success),
        Some(Verdict::NotYet(_)) => ("not yet".to_string(), app.theme.modified),
        None if app.solved[app.lesson] => ("solved".to_string(), app.theme.success),
        None => ("try it".to_string(), app.theme.accent),
    }
}

fn tape_title(app: &App, step: usize) -> String {
    match &app.trace {
        Some(trace) => {
            let last = trace.positions().saturating_sub(1);
            let ending = match &trace.ending {
                Ending::Finished => String::new(),
                Ending::TooManySteps(limit) => format!("  · stopped after {limit} steps"),
                Ending::Failed(_) => "  · stopped on an error".to_string(),
            };
            format!("Tape  ·  step {step} of {last}{ending}")
        }
        None => "Tape  ·  ctrl-r to run".to_string(),
    }
}

fn status_fields(app: &App, frame: Option<&TraceFrame>) -> Vec<(&'static str, String)> {
    let mut fields = Vec::new();
    match frame {
        Some(frame) => {
            fields.push(("ptr", frame.pointer.to_string()));
            fields.push((
                "cell",
                cell_under(&frame.memory, frame.pointer)
                    .map_or_else(|| "off tape".to_string(), |byte| byte.to_string()),
            ));
            fields.push(("depth", frame.loop_depth.to_string()));
        }
        None => fields.push(("", "not run yet".to_string())),
    }
    fields.push((
        "solved",
        format!(
            "{} of {}",
            app.solved.iter().filter(|done| **done).count(),
            LESSONS.len()
        ),
    ));
    fields
}

/// Hints held back from the fill, so a narrow terminal never drops them.
const ESSENTIAL_HINTS: &[(&str, &str)] = &[("F1", "keys"), ("ctrl-q", "quit")];

fn status_hints(app: &App) -> Vec<(&'static str, &'static str)> {
    if app.popup.is_some() {
        return vec![("esc", "close")];
    }
    match app.focus {
        Focus::Code => vec![
            ("type", "edit"),
            ("ctrl-r", "run"),
            ("tab", "step through"),
            ("F2", "hint"),
            ("ctrl-n", "next lesson"),
        ],
        Focus::Steps => vec![
            ("← →", "step"),
            ("home", "first step"),
            ("tab", "edit"),
            ("ctrl-r", "run again"),
            ("F2", "hint"),
            ("ctrl-n", "next lesson"),
        ],
    }
}

fn popup_content(app: &App) -> Option<(String, String, &'static str, Color)> {
    match app.popup? {
        Popup::Help => None,
        Popup::Hints => {
            let lesson = app.current();
            let body = if app.hints_shown == 0 {
                "No hints for this one.".to_string()
            } else {
                lesson
                    .hints
                    .iter()
                    .take(app.hints_shown)
                    .enumerate()
                    .map(|(index, hint)| format!("{}. {hint}", index + 1))
                    .collect::<Vec<_>>()
                    .join("\n\n")
            };
            let footer = if app.hints_shown < lesson.hints.len() {
                "F2 for another, esc to close"
            } else {
                "that is all of them — esc to close"
            };
            Some(("Hints".to_string(), body, footer, app.theme.accent))
        }
        Popup::Answer => Some((
            "One answer".to_string(),
            format!(
                "{}\n\nThere is more than one right program. This is the one \
                 the lesson was written around.",
                app.current().answer
            ),
            "F4 to load it into the editor, esc to close",
            app.theme.modified,
        )),
    }
}

const HELP: &[Section<'static>] = &[
    (
        "Working",
        &[
            ("ctrl-r", "run your program and record every step"),
            ("tab", "move between typing and stepping"),
            ("← →", "step through the run, when stepping"),
            ("home / end", "jump to the first or last step"),
            ("pgup / pgdn", "ten steps at a time"),
        ],
    ),
    (
        "When you are stuck",
        &[
            ("F2", "reveal one more hint"),
            ("F3", "show an answer"),
            ("F4", "load that answer into the editor"),
            ("F6", "put the lesson's starting program back"),
        ],
    ),
    (
        "The course",
        &[
            ("ctrl-n", "next lesson"),
            ("ctrl-p", "previous lesson"),
            ("↑ ↓", "scroll the lesson text, when stepping"),
        ],
    ),
    (
        "Leaving",
        &[
            ("ctrl-q / ctrl-c", "quit"),
            ("F1", "open and close this list"),
        ],
    ),
];

// ------------------------------------------------------------------- input

fn handle_key(app: &mut App, key: KeyEvent) {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);

    // Chords work the same whether the caret is in the editor or not, so that
    // running a program never depends on which panel has focus.
    match (key.code, control) {
        (KeyCode::Char('q' | 'c'), true) => {
            app.quit = true;
            return;
        }
        (KeyCode::Char('r'), true) | (KeyCode::F(5), _) => {
            app.popup = None;
            app.run();
            return;
        }
        (KeyCode::Char('n'), true) => {
            app.go_to(app.lesson + 1);
            return;
        }
        (KeyCode::Char('p'), true) => {
            if app.lesson > 0 {
                app.go_to(app.lesson - 1);
            }
            return;
        }
        (KeyCode::F(1), _) => {
            app.popup = if app.popup == Some(Popup::Help) {
                None
            } else {
                Some(Popup::Help)
            };
            return;
        }
        (KeyCode::F(2), _) => {
            app.next_hint();
            return;
        }
        (KeyCode::F(3), _) => {
            app.popup = Some(Popup::Answer);
            return;
        }
        (KeyCode::F(4), _) => {
            app.load_answer();
            app.popup = None;
            return;
        }
        (KeyCode::F(6), _) => {
            app.reset_editor();
            app.message = Some(("back to the starting program".into(), Note::Info));
            return;
        }
        (KeyCode::Esc, _) => {
            app.popup = None;
            return;
        }
        (KeyCode::Tab, _) => {
            app.focus = match app.focus {
                Focus::Code => Focus::Steps,
                Focus::Steps => Focus::Code,
            };
            return;
        }
        _ => {}
    }

    if app.popup.is_some() {
        // A popup swallows everything else, so a stray key does not edit the
        // program the learner cannot currently see.
        return;
    }

    match app.focus {
        Focus::Code => edit_key(app, key),
        Focus::Steps => step_key(app, key),
    }
}

fn edit_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char(ch) => {
            app.editor.insert(ch);
            app.refresh_document();
        }
        KeyCode::Backspace => {
            app.editor.backspace();
            app.refresh_document();
        }
        KeyCode::Delete => {
            app.editor.delete();
            app.refresh_document();
        }
        KeyCode::Enter => {
            app.editor.newline();
            app.refresh_document();
        }
        KeyCode::Left => app.editor.left(),
        KeyCode::Right => app.editor.right(),
        KeyCode::Up => app.editor.up(),
        KeyCode::Down => app.editor.down(),
        KeyCode::Home => app.editor.home(),
        KeyCode::End => app.editor.end(),
        _ => {}
    }
}

fn step_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Left => app.scrub(-1),
        KeyCode::Right => app.scrub(1),
        KeyCode::PageUp => app.scrub(-10),
        KeyCode::PageDown => app.scrub(10),
        KeyCode::Home => app.scrub_to_end(false),
        KeyCode::End => app.scrub_to_end(true),
        KeyCode::Up => app.lesson_scroll = app.lesson_scroll.saturating_sub(1),
        KeyCode::Down => app.lesson_scroll += 1,
        _ => {}
    }
}
