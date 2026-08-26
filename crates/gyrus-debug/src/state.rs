//! Everything the debugger knows, and the rules for when it stops.

use std::collections::{BTreeSet, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use gyrus::{ExecutionStats, Instruction, SourceLocation};
use gyrus_tui::{CellDisplay, Position, Theme, Tui};

use crate::program::Program;

/// Why execution is moving, and therefore when it should stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    /// Stop at the very next instruction.
    Step,
    /// Run until a breakpoint or the end of the program.
    Continue,
    /// Run until this instruction index, or a breakpoint.
    RunTo(usize),
    /// Run until execution leaves the instruction range `[start, end)`, or a
    /// breakpoint fires.
    ///
    /// This is how both "step over" and "step out" are expressed. Loop depth
    /// cannot express either: at a `[`, the depth is the same on the iteration
    /// that is about to start as it is once the loop has finished, so a
    /// depth-based rule stops on the next iteration instead of after the loop.
    Leave { start: usize, end: usize },
}

/// Which panel the arrow keys drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    #[default]
    Source,
    Memory,
    Watch,
    Output,
}

impl Focus {
    /// The next panel, for `Tab`.
    pub fn next(self) -> Self {
        match self {
            Focus::Source => Focus::Memory,
            Focus::Memory => Focus::Watch,
            Focus::Watch => Focus::Output,
            Focus::Output => Focus::Source,
        }
    }
}

/// How prominently to draw a transient message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Note {
    Info,
    Warn,
    Error,
}

/// Interpreter state as of the last time the debugger looked.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    /// The tape.
    pub memory: Vec<u8>,
    /// Cursor position. Signed: moving off the tape is legal, using it is not.
    pub pointer: isize,
    /// Instructions executed so far.
    pub step: u64,
    /// Index of the instruction about to execute.
    pub index: usize,
    /// Loop nesting depth.
    pub loop_depth: usize,
    /// Where that instruction is in the source.
    pub location: Option<SourceLocation>,
}

/// How a run ended.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// The program ran to the end.
    Completed(Box<ExecutionStats>),
    /// The program stopped on an error, already formatted for display.
    Failed(String),
}

impl Outcome {
    /// Heading for the result popup.
    pub fn title(&self) -> &'static str {
        match self {
            Outcome::Completed(_) => "Finished",
            Outcome::Failed(_) => "Stopped on an error",
        }
    }

    /// The popup's body: statistics, or the formatted error.
    pub fn detail(&self) -> String {
        match self {
            Outcome::Failed(message) => message.clone(),
            Outcome::Completed(stats) => {
                let mut lines = vec![
                    format!("steps            {}", stats.total_steps.0),
                    format!("loop iterations  {}", stats.loop_iterations),
                    format!("peak tape used   {} cells", stats.peak_memory_used.0),
                    format!("cells modified   {}", stats.cells_modified),
                    format!("bytes read       {}", stats.bytes_read),
                    format!("bytes written    {}", stats.bytes_written),
                ];
                if !stats.warnings.is_empty() {
                    lines.push(String::new());
                    lines.push(format!("{} runtime warnings:", stats.warnings.len()));
                    for warning in stats.warnings.iter().take(10) {
                        lines.push(format!("  {warning}"));
                    }
                    if stats.warnings.len() > 10 {
                        lines.push(format!("  … and {} more", stats.warnings.len() - 10));
                    }
                }
                lines.join("\n")
            }
        }
    }
}

/// What the user asked for while paused, once execution has unwound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Quit,
    Restart,
}

/// Whether execution stops before the instruction at `index`.
///
/// Split out from [`Session`] so the rule can be tested without a terminal:
/// this is the one piece of the debugger where being subtly wrong looks like
/// the interpreter misbehaving rather than like a bug in the interface.
pub fn should_pause(
    run: RunState,
    at_breakpoint: bool,
    index: usize,
    starving_for_input: bool,
) -> bool {
    if at_breakpoint || starving_for_input {
        return true;
    }
    match run {
        RunState::Step => true,
        RunState::Continue => false,
        RunState::RunTo(target) => index == target,
        RunState::Leave { start, end } => index < start || index >= end,
    }
}

/// Scroll positions, focus, and the other things that survive a restart but
/// mean nothing to the interpreter.
#[derive(Debug, Clone)]
pub struct Ui {
    pub focus: Focus,
    pub source_scroll: usize,
    pub source_h_scroll: usize,
    pub cursor: Position,
    pub memory_scroll: usize,
    pub memory_display: CellDisplay,
    pub memory_follow: bool,
    pub watch_selected: usize,
    /// `None` pins the output panel to the newest line.
    pub output_scroll: Option<usize>,
    pub help: bool,
    pub help_scroll: usize,
    /// Whether the popup describing how the run ended is showing.
    pub result: bool,
    /// A one-line question overlaying the status bar, if one is open.
    pub prompt: Option<Prompt>,
}

/// A one-line question: "watch cell:", "go to line:", "input:".
#[derive(Debug, Clone)]
pub struct Prompt {
    pub kind: PromptKind,
    pub buffer: String,
}

/// What answering a [`Prompt`] does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    /// Add a watch on a cell address.
    Watch,
    /// Scroll the memory panel to a cell address.
    GotoCell,
    /// Move the cursor to a source line.
    GotoLine,
    /// Queue bytes for the program's next `,`.
    Input,
}

impl PromptKind {
    /// The label drawn in front of the answer.
    pub fn label(self) -> &'static str {
        match self {
            PromptKind::Watch => "watch cell",
            PromptKind::GotoCell => "go to cell",
            PromptKind::GotoLine => "go to line",
            PromptKind::Input => "input (queued for the next `,`)",
        }
    }
}

impl Default for Ui {
    fn default() -> Self {
        Self {
            focus: Focus::default(),
            source_scroll: 0,
            source_h_scroll: 0,
            cursor: (1, 1),
            memory_scroll: 0,
            memory_display: CellDisplay::default(),
            memory_follow: true,
            watch_selected: 0,
            output_scroll: None,
            help: false,
            help_scroll: 0,
            result: false,
            prompt: None,
        }
    }
}

/// The whole debugger: the program, the interpreter's last known state, and the
/// terminal it is all drawn on.
///
/// One `Session` is shared, behind a mutex, by the execution hook and the two
/// I/O adapters. Everything runs on one thread — the interpreter calls the hook,
/// the hook draws and reads keys, and only then does the interpreter continue —
/// so the mutex is never contended. It is a mutex rather than a `RefCell`
/// because `ExecutionHook` requires `Send`.
pub struct Session {
    pub program: Arc<Program>,
    pub terminal: Tui,
    pub theme: Theme,
    pub ui: Ui,
    pub run: RunState,

    /// Breakpoints as the user sees them: source positions.
    pub breakpoints: BTreeSet<Position>,
    /// The same breakpoints as instruction indices, which is what the check on
    /// every instruction actually compares against.
    breakpoint_indices: HashSet<usize>,

    pub watches: Vec<usize>,
    pub output: Vec<u8>,

    /// Bytes queued for the program's next `,`.
    pub pending_input: VecDeque<u8>,
    /// Bytes the program has already read, kept so a restart can replay them
    /// instead of asking the user to type them again.
    pub consumed_input: Vec<u8>,

    pub snapshot: Snapshot,
    /// Cells whose value changed since the debugger last looked at the tape —
    /// one instruction ago while stepping, one redraw ago while running.
    pub modified: HashSet<usize>,
    /// Enclosing loops, innermost last, as `(start, end)` instruction ranges.
    pub loop_stack: Vec<(usize, usize)>,

    /// Set once the user has resumed past an empty input queue, so `continue`
    /// on a program that reads to EOF does not stop at every `,`.
    pub input_eof: bool,

    pub message: Option<(String, Note)>,
    pub exit: Option<Exit>,
    pub outcome: Option<Outcome>,
    /// Set once execution has stopped for good, so the UI stops offering steps.
    pub finished: bool,

    last_draw: Instant,
}

impl Session {
    /// A session for `program`, drawing on `terminal`.
    pub fn new(program: Arc<Program>, terminal: Tui, memory_size: usize) -> Self {
        let cursor = program.position(0).unwrap_or((1, 1));
        Self {
            program,
            terminal,
            theme: Theme::default(),
            ui: Ui {
                cursor,
                ..Ui::default()
            },
            run: RunState::Step,
            breakpoints: BTreeSet::new(),
            breakpoint_indices: HashSet::new(),
            watches: Vec::new(),
            output: Vec::new(),
            pending_input: VecDeque::new(),
            consumed_input: Vec::new(),
            snapshot: Snapshot {
                memory: vec![0; memory_size],
                ..Snapshot::default()
            },
            modified: HashSet::new(),
            loop_stack: Vec::new(),
            input_eof: false,
            message: None,
            exit: None,
            outcome: None,
            finished: false,
            last_draw: Instant::now(),
        }
    }

    /// Clear everything the interpreter owned, keeping what the user set up.
    ///
    /// Breakpoints, watches, scroll positions, and the display mode survive a
    /// restart; the tape, the output, and the step count do not. Input the
    /// program already consumed moves back to the front of the queue, so
    /// restarting a program that reads from the keyboard does not mean typing
    /// the same bytes again.
    pub fn reset(&mut self, memory_size: usize) {
        let replay: Vec<u8> = self
            .consumed_input
            .drain(..)
            .chain(self.pending_input.drain(..))
            .collect();
        self.pending_input = replay.into();
        self.output.clear();
        self.snapshot = Snapshot {
            memory: vec![0; memory_size],
            ..Snapshot::default()
        };
        self.modified.clear();
        self.loop_stack.clear();
        self.input_eof = false;
        self.run = RunState::Step;
        self.exit = None;
        self.outcome = None;
        self.finished = false;
        self.ui.output_scroll = None;
        self.ui.result = false;
    }

    /// Add or remove a breakpoint, returning what happened for the status line.
    pub fn toggle_breakpoint(&mut self, position: Position) -> bool {
        let added = if self.breakpoints.contains(&position) {
            self.breakpoints.remove(&position);
            false
        } else {
            self.breakpoints.insert(position);
            true
        };
        self.rebuild_breakpoints();
        added
    }

    /// Drop every breakpoint.
    pub fn clear_breakpoints(&mut self) {
        self.breakpoints.clear();
        self.breakpoint_indices.clear();
    }

    /// Recompute the instruction indices the breakpoint check compares against.
    pub fn rebuild_breakpoints(&mut self) {
        self.breakpoint_indices = self
            .breakpoints
            .iter()
            .filter_map(|position| self.program.index_at(*position))
            .collect();
    }

    /// Whether execution should stop before instruction `index`.
    pub fn wants_pause(&self, index: usize, instruction: &Instruction) -> bool {
        if self.exit.is_some() {
            return true;
        }
        // A `,` with nothing queued would silently take the EOF branch. Stopping
        // there is the whole point of a debugger: the user gets to supply the
        // byte. Once they resume without supplying one, they have chosen EOF,
        // and we stop asking.
        let starving = matches!(instruction, Instruction::Input)
            && self.pending_input.is_empty()
            && !self.input_eof;
        should_pause(
            self.run,
            self.breakpoint_indices.contains(&index),
            index,
            starving,
        )
    }

    /// Record interpreter state, and note which cells changed since last time.
    pub fn observe(
        &mut self,
        memory: &[u8],
        pointer: isize,
        step: u64,
        index: usize,
        loop_depth: usize,
        location: Option<SourceLocation>,
    ) {
        self.modified.clear();
        for (address, (&before, &after)) in
            self.snapshot.memory.iter().zip(memory.iter()).enumerate()
        {
            if before != after {
                self.modified.insert(address);
            }
        }
        // An unbounded tape that just grew: everything past the old end is new.
        for (address, &value) in memory.iter().enumerate().skip(self.snapshot.memory.len()) {
            if value != 0 {
                self.modified.insert(address);
            }
        }

        self.snapshot.memory.clear();
        self.snapshot.memory.extend_from_slice(memory);
        self.snapshot.pointer = pointer;
        self.snapshot.step = step;
        self.snapshot.index = index;
        self.snapshot.loop_depth = loop_depth;
        self.snapshot.location = location;
    }

    /// Whether enough time has passed to redraw while running freely.
    ///
    /// Redrawing every instruction would make `continue` slower than the
    /// program; never redrawing would make a long run look like a hang.
    pub fn should_redraw(&mut self, interval: std::time::Duration) -> bool {
        if self.last_draw.elapsed() >= interval {
            self.last_draw = Instant::now();
            true
        } else {
            false
        }
    }

    /// Reset the redraw clock, so a pause is followed by a full interval.
    pub fn touch_draw(&mut self) {
        self.last_draw = Instant::now();
    }

    /// Say something on the status line until the next action.
    pub fn note(&mut self, message: impl Into<String>, kind: Note) {
        self.message = Some((message.into(), kind));
    }

    /// Whether execution is stopped on a breakpoint.
    pub fn at_breakpoint(&self) -> bool {
        self.snapshot.location.is_some() && self.breakpoint_indices.contains(&self.snapshot.index)
    }

    /// The instruction the user's cursor names, snapping to the nearest one on
    /// the cursor's line.
    pub fn cursor_instruction(&self) -> Option<(Position, usize)> {
        self.program.nearest_on_line(self.ui.cursor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stepping_stops_everywhere() {
        assert!(should_pause(RunState::Step, false, 0, false));
        assert!(should_pause(RunState::Step, false, 99, false));
    }

    #[test]
    fn continuing_stops_only_at_breakpoints() {
        assert!(!should_pause(RunState::Continue, false, 7, false));
        assert!(should_pause(RunState::Continue, true, 7, false));
    }

    #[test]
    fn running_to_a_target_stops_there_and_at_breakpoints() {
        let run = RunState::RunTo(10);
        assert!(!should_pause(run, false, 9, false));
        assert!(should_pause(run, false, 10, false));
        assert!(should_pause(run, true, 3, false));
    }

    #[test]
    fn leaving_a_range_stops_on_the_first_instruction_outside_it() {
        // The extent of `+[->+<]+`: the loop is instructions 1 through 5.
        let run = RunState::Leave { start: 1, end: 6 };
        assert!(!should_pause(run, false, 1, false), "the `[` is inside");
        assert!(!should_pause(run, false, 5, false), "the last body op");
        assert!(should_pause(run, false, 6, false), "past the `]`");
        assert!(should_pause(run, false, 0, false), "before the `[`");
    }

    #[test]
    fn an_empty_input_queue_stops_a_running_program() {
        // Otherwise `,` silently takes the EOF branch in the middle of a
        // `continue`, and the user never learns their input was needed.
        assert!(should_pause(RunState::Continue, false, 4, true));
    }
}
