//! What the tutorial knows: which lesson, what the learner typed, and what
//! happened when it ran.

use gyrus_tui::{SourceDocument, Theme};

use crate::editor::Editor;
use crate::lesson::{LESSONS, Lesson, STEP_LIMIT, Verdict, evaluate};
use crate::trace::{self, Trace};

/// Which panel the arrow keys drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// Typing a program.
    Code,
    /// Walking through the run that program produced.
    Steps,
}

/// Which popup is open, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Popup {
    Hints,
    Answer,
    Help,
}

/// How prominently to draw a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Note {
    Info,
    Good,
    Bad,
}

/// The tutorial.
pub struct App {
    pub theme: Theme,
    /// Index into [`LESSONS`].
    pub lesson: usize,
    pub editor: Editor,
    /// The editor's text, split and colored for the source panel.
    pub document: SourceDocument,
    /// The lesson's prose and task, joined once. It changes only when the
    /// lesson does, and the panel redraws on every keystroke.
    pub prose: String,
    /// The last run, if the learner has run anything.
    pub trace: Option<Trace>,
    /// Which recorded step is on screen. Private: [`Self::step`] is the clamped
    /// value everything else should read, and two spellings one character apart
    /// meaning different things is a trap.
    step: usize,
    /// Whether the last run satisfied the lesson.
    pub verdict: Option<Verdict>,
    pub focus: Focus,
    pub popup: Option<Popup>,
    /// How many hints the learner has asked for on this lesson.
    pub hints_shown: usize,
    /// Which lessons have been solved, for the header.
    pub solved: Vec<bool>,
    pub message: Option<(String, Note)>,
    pub lesson_scroll: usize,
    pub tape_offset: usize,
    pub quit: bool,
}

/// A lesson's explanation and its task, as one block of text.
fn prose(lesson: &Lesson) -> String {
    format!("{}\n\n— — —\n\n{}", lesson.body, lesson.task)
}

impl App {
    /// Start at `lesson`.
    pub fn new(lesson: usize) -> Self {
        let lesson = lesson.min(LESSONS.len() - 1);
        let editor = Editor::new(&LESSONS[lesson].starter);
        let document = SourceDocument::new(&editor.text());
        Self {
            theme: Theme::default(),
            lesson,
            editor,
            document,
            prose: prose(&LESSONS[lesson]),
            trace: None,
            step: 0,
            verdict: None,
            focus: Focus::Code,
            popup: None,
            hints_shown: 0,
            solved: vec![false; LESSONS.len()],
            message: None,
            lesson_scroll: 0,
            tape_offset: 0,
            quit: false,
        }
    }

    /// The lesson being worked on.
    pub fn current(&self) -> &'static Lesson {
        &LESSONS[self.lesson]
    }

    /// Re-derive the syntax-colored document after an edit.
    pub fn refresh_document(&mut self) {
        self.document = SourceDocument::new(&self.editor.text());
        // The old trace describes a program that no longer exists; keeping it
        // on screen would point ▶ at a character the learner has since moved.
        self.trace = None;
        self.verdict = None;
        self.step = 0;
    }

    /// Run the learner's program and record every step of it.
    pub fn run(&mut self) {
        let lesson = self.current();
        let source = self.editor.text();
        match trace::record(&source, &lesson.input, lesson.cells, STEP_LIMIT) {
            Ok(trace) => {
                let verdict = evaluate(&lesson.criteria, &trace, &source);
                if verdict.is_solved() {
                    self.solved[self.lesson] = true;
                }
                self.message = Some(match &verdict {
                    Verdict::Solved => {
                        ("that is it — ctrl-n for the next lesson".into(), Note::Good)
                    }
                    Verdict::Nothing => ("ran it — ctrl-n for the next lesson".into(), Note::Info),
                    Verdict::NotYet(why) => (format!("not yet: {why}"), Note::Bad),
                });
                // Land on the end of the run, which answers "what did my
                // program do". Walking back through it with ← or home is the
                // second question, and the status bar says how.
                self.step = trace.positions().saturating_sub(1);
                self.trace = Some(trace);
                self.verdict = Some(verdict);
                self.focus = Focus::Steps;
            }
            Err(error) => {
                // A parse error means unbalanced brackets, which is the only
                // way a BrainFuck program can fail to parse at all.
                self.trace = None;
                self.verdict = None;
                self.message = Some((
                    error.to_string().lines().next().unwrap_or("").to_string(),
                    Note::Bad,
                ));
            }
        }
    }

    /// The trace position on screen, clamped to what exists.
    pub fn step(&self) -> usize {
        match &self.trace {
            Some(trace) => self.step.min(trace.positions().saturating_sub(1)),
            None => 0,
        }
    }

    /// Move through the recorded run.
    pub fn scrub(&mut self, delta: isize) {
        let Some(trace) = &self.trace else { return };
        let last = trace.positions().saturating_sub(1);
        self.step = (self.step as isize + delta).clamp(0, last as isize) as usize;
    }

    /// Jump to the first or last recorded step.
    pub fn scrub_to_end(&mut self, end: bool) {
        let Some(trace) = &self.trace else { return };
        self.step = if end {
            trace.positions().saturating_sub(1)
        } else {
            0
        };
    }

    /// Move to another lesson, loading its starter program.
    pub fn go_to(&mut self, lesson: usize) {
        if lesson >= LESSONS.len() {
            self.message = Some((
                "that was the last lesson — ctrl-q to leave".into(),
                Note::Info,
            ));
            return;
        }
        self.lesson = lesson;
        self.prose = prose(self.current());
        self.reset_editor();
        self.hints_shown = 0;
        self.lesson_scroll = 0;
        self.popup = None;
        self.message = None;
        self.focus = Focus::Code;
    }

    /// Put the lesson's starting program back in the editor.
    pub fn reset_editor(&mut self) {
        self.editor = Editor::new(&self.current().starter);
        self.refresh_document();
    }

    /// Load the lesson's answer into the editor.
    pub fn load_answer(&mut self) {
        self.editor = Editor::new(&self.current().answer);
        self.refresh_document();
        self.message = Some((
            "answer loaded — ctrl-r runs it, and F6 puts yours back".into(),
            Note::Info,
        ));
    }

    /// Reveal one more hint.
    pub fn next_hint(&mut self) {
        let available = self.current().hints.len();
        if self.hints_shown < available {
            self.hints_shown += 1;
        }
        self.popup = Some(Popup::Hints);
    }
}
