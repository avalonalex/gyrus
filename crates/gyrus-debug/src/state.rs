//! Everything the debugger knows, and the rules for when it stops.

use std::collections::{BTreeSet, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use gyrus::{ExecutionStats, SourceLocation};
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

    /// The previous panel, for `shift-Tab`.
    ///
    /// Spelled out rather than "press Tab three times": that phrasing hides a
    /// dependency on the number of panels, and adding a fifth would silently
    /// make shift-Tab cycle forwards.
    pub fn previous(self) -> Self {
        match self {
            Focus::Source => Focus::Output,
            Focus::Memory => Focus::Source,
            Focus::Watch => Focus::Memory,
            Focus::Output => Focus::Watch,
        }
    }
}

/// Something the debugger is keeping an eye on.
///
/// Two kinds, and the difference is whether reaching it stops the program: a
/// cell is only ever shown, while an output condition is a breakpoint expressed
/// in terms of what the program does rather than where it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Watch {
    /// Show this cell's value on every pause.
    Cell(usize),
    /// Stop before the program prints anything at all.
    AnyOutput,
    /// Stop before the program prints this byte.
    Output(u8),
}

impl Watch {
    /// Whether reaching this stops execution.
    ///
    /// Exhaustive rather than a negated `matches!`: a display-only kind added
    /// later would otherwise default to stopping, drawing a `●` beside a row
    /// that never stops anything and making the hook watch output for nothing.
    pub fn stops(self) -> bool {
        match self {
            Watch::Cell(_) => false,
            Watch::AnyOutput | Watch::Output(_) => true,
        }
    }

    /// Whether a `.` about to print `byte` satisfies this.
    pub fn matches_output(self, byte: u8) -> bool {
        match self {
            Watch::Cell(_) => false,
            Watch::AnyOutput => true,
            Watch::Output(wanted) => wanted == byte,
        }
    }

    /// How it reads in the watch panel.
    pub fn label(self) -> String {
        match self {
            Watch::Cell(address) => format!("cell[{address}]"),
            Watch::AnyOutput => "output".to_string(),
            Watch::Output(byte) => format!("output {}", gyrus_tui::describe_byte(byte)),
        }
    }
}

/// The watch list.
///
/// Split out of [`Session`] so it can be tested without a terminal — the same
/// reason [`should_pause`] is a free function. `Session` owns a `Tui` because
/// the hook draws through it, and building one in a test means asking a
/// terminal for its size, which there is not one of on a build machine.
#[derive(Debug, Default, Clone)]
pub struct Watches {
    list: Vec<Watch>,
    revision: u64,
}

impl Watches {
    /// The watches, in display order.
    pub fn as_slice(&self) -> &[Watch] {
        &self.list
    }

    /// How many of them there are.
    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// How many times the list has changed, so a cache knows to rebuild.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Add a watch, returning where it landed, or `None` if it was already
    /// there.
    ///
    /// The index matters to the caller: the list is kept sorted, so a watch
    /// added last is rarely displayed last, and `W` acts on the selection.
    pub fn add(&mut self, watch: Watch) -> Option<usize> {
        if self.list.contains(&watch) {
            return None;
        }
        self.list.push(watch);
        // Cells first, then output conditions: the cells are a table to read and
        // the conditions are a list of rules, and interleaving them reads badly.
        self.list.sort_by_key(|watch| match watch {
            Watch::Cell(address) => (0, *address),
            Watch::AnyOutput => (1, 0),
            Watch::Output(byte) => (1, usize::from(*byte) + 1),
        });
        self.revision += 1;
        self.list.iter().position(|existing| *existing == watch)
    }

    /// Remove the watch at `index`.
    pub fn remove(&mut self, index: usize) -> Option<Watch> {
        if index >= self.list.len() {
            return None;
        }
        self.revision += 1;
        Some(self.list.remove(index))
    }

    /// Whether any watch stops on output at all.
    pub fn stops_output(&self) -> bool {
        self.list.iter().any(|watch| watch.stops())
    }

    /// One entry per byte value: whether a `.` printing it should stop.
    ///
    /// A table rather than a predicate because the hook consults it on every
    /// `.` executed, and walking the list there would reintroduce the
    /// per-instruction work the hook was restructured to remove. Built from
    /// [`Watch::matches_output`] rather than re-deriving the rule, so the table
    /// and the definition cannot drift.
    pub fn output_stop_table(&self) -> [bool; 256] {
        let mut table = [false; 256];
        for (byte, entry) in table.iter_mut().enumerate() {
            let byte = byte as u8;
            *entry = self.list.iter().any(|watch| watch.matches_output(byte));
        }
        table
    }
}

/// Why execution stopped.
///
/// Recorded where the decision is made rather than reconstructed afterwards.
/// Working it out again from the snapshot answers "could something have stopped
/// here", which is a different question: stepping onto a `.` that an output
/// watch names is a step, not a watch, and a breakpoint on such a `.` is a
/// breakpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// A step, a run-to-cursor arriving, or the stop before the first
    /// instruction.
    Stepped,
    /// A breakpoint on this instruction.
    Breakpoint,
    /// An output watch matched the byte this `.` is about to print.
    OutputWatch,
    /// A `,` with nothing queued to read.
    NeedsInput,
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
#[derive(Debug)]
pub enum Exit {
    Quit,
    Restart,
    /// The terminal failed. Carried out rather than written to a status line
    /// that only the thing which just failed knows how to draw, so `main` can
    /// report it and exit non-zero like every other failure.
    Failed(std::io::Error),
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
    /// The popup on screen, if any. One field rather than a flag each, so two
    /// popups cannot end up stacked on top of one another.
    pub modal: Option<Modal>,
    /// A one-line question overlaying the status bar, if one is open. Separate
    /// from `modal` because it genuinely coexists — it draws in the status bar.
    pub prompt: Option<Prompt>,
    /// A cell address to bring into view on the next draw.
    ///
    /// An address rather than a row, because only the draw knows how many cells
    /// fit on a row; a caller that divided by a guess would scroll to the wrong
    /// place on every terminal but its author's.
    pub reveal: Option<usize>,
}

/// The popup covering the panels, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modal {
    /// The key-binding list.
    Help {
        /// First visible row, for a list taller than the terminal.
        scroll: usize,
    },
    /// How the run ended.
    Result,
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
    /// Add a watch: a cell to show, or a condition on what is printed.
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
            PromptKind::Watch => "watch (a cell number, or `out` / `out X` for output)",
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
            modal: None,
            prompt: None,
            reveal: None,
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
    ///
    /// Private because the form the per-instruction check needs is a dense
    /// bitmap, which lives in the hook that reads it. Every mutation here bumps
    /// `breakpoint_revision` so the hook knows to rebuild.
    breakpoints: BTreeSet<Position>,
    /// Bumped whenever `breakpoints` changes.
    breakpoint_revision: u64,

    pub watches: Watches,
    /// Why execution stopped, set where that is decided.
    pub stop_reason: StopReason,
    pub output: Vec<u8>,

    /// Bytes queued for the program's next `,`.
    pub pending_input: VecDeque<u8>,
    /// Bytes the program has already read, kept so a restart can replay them
    /// instead of asking the user to type them again.
    pub consumed_input: Vec<u8>,

    pub snapshot: Snapshot,
    /// Cells whose value changed since the frame before this one.
    pub modified: HashSet<usize>,
    /// The tape as the last frame drew it, and the step it was drawn at.
    displayed: Vec<u8>,
    displayed_step: Option<u64>,
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
            breakpoint_revision: 0,
            watches: Watches::default(),
            stop_reason: StopReason::Stepped,
            output: Vec::new(),
            pending_input: VecDeque::new(),
            consumed_input: Vec::new(),
            snapshot: Snapshot {
                memory: vec![0; memory_size],
                ..Snapshot::default()
            },
            modified: HashSet::new(),
            displayed: vec![0; memory_size],
            displayed_step: None,
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
        self.displayed = vec![0; memory_size];
        self.displayed_step = None;
        self.loop_stack.clear();
        self.input_eof = false;
        self.run = RunState::Step;
        self.exit = None;
        self.outcome = None;
        self.finished = false;
        self.ui.output_scroll = None;
        self.ui.modal = None;
    }

    /// The breakpoints, for drawing them.
    pub fn breakpoints(&self) -> &BTreeSet<Position> {
        &self.breakpoints
    }

    /// How many times the breakpoints have changed.
    pub fn breakpoint_revision(&self) -> u64 {
        self.breakpoint_revision
    }

    /// A bitmap of the instruction indices that carry a breakpoint.
    ///
    /// This is the form the check on every executed instruction wants: indices
    /// are dense from zero, so a bounds-checked `bool` read beats hashing.
    pub fn breakpoint_bitmap(&self) -> Vec<bool> {
        let mut stops = vec![false; self.program.instruction_count()];
        for position in &self.breakpoints {
            if let Some(index) = self.program.index_at(*position) {
                stops[index] = true;
            }
        }
        stops
    }

    /// Add a breakpoint. Returns whether it was not already there.
    pub fn set_breakpoint(&mut self, position: Position) -> bool {
        let added = self.breakpoints.insert(position);
        self.breakpoint_revision += 1;
        added
    }

    /// Add or remove a breakpoint, returning what happened for the status line.
    pub fn toggle_breakpoint(&mut self, position: Position) -> bool {
        if self.breakpoints.remove(&position) {
            self.breakpoint_revision += 1;
            false
        } else {
            self.set_breakpoint(position)
        }
    }

    /// Drop every breakpoint.
    pub fn clear_breakpoints(&mut self) {
        self.breakpoints.clear();
        self.breakpoint_revision += 1;
    }

    /// Whether a `,` reached now would have nothing to read.
    ///
    /// Stopping there is the whole point of a debugger: the user gets to supply
    /// the byte instead of silently taking the EOF branch. Once they resume
    /// without supplying one, they have chosen EOF, and we stop asking.
    pub fn starving_for_input(&self) -> bool {
        self.pending_input.is_empty() && !self.input_eof
    }

    /// Record interpreter state.
    ///
    /// Copies the tape and nothing more. Diffing it here would cost a scan of
    /// every cell on every poll — thousands of times a second — to produce a
    /// set that is only ever drawn; [`Self::refresh_modified`] does it once per
    /// frame instead, which also makes "changed" mean "since you last saw the
    /// screen" rather than "in the last few thousand instructions".
    pub fn observe(
        &mut self,
        memory: &[u8],
        pointer: isize,
        step: u64,
        index: usize,
        loop_depth: usize,
        location: Option<SourceLocation>,
    ) {
        self.snapshot.memory.clear();
        self.snapshot.memory.extend_from_slice(memory);
        self.snapshot.pointer = pointer;
        self.snapshot.step = step;
        self.snapshot.index = index;
        self.snapshot.loop_depth = loop_depth;
        self.snapshot.location = location;
    }

    /// Work out which cells have changed since the last frame that showed them.
    ///
    /// A redraw that follows no execution — a scroll, a display-mode change —
    /// leaves the highlight alone rather than clearing it, which is why this
    /// keys off the step count rather than just comparing.
    pub fn refresh_modified(&mut self) {
        if self.displayed_step == Some(self.snapshot.step) {
            return;
        }
        self.modified = gyrus_tui::changed_cells(&self.displayed, &self.snapshot.memory);
        self.displayed.clear();
        self.displayed.extend_from_slice(&self.snapshot.memory);
        self.displayed_step = Some(self.snapshot.step);
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

    /// How many times the watches have changed.
    pub fn watch_revision(&self) -> u64 {
        self.watches.revision()
    }

    /// One entry per byte value: whether a `.` printing it should stop.
    pub fn output_stop_table(&self) -> [bool; 256] {
        self.watches.output_stop_table()
    }

    /// Whether any watch stops on output at all.
    pub fn watches_output(&self) -> bool {
        self.watches.stops_output()
    }

    /// Add a watch and select it, unless it is already there.
    pub fn add_watch(&mut self, watch: Watch) -> bool {
        match self.watches.add(watch) {
            Some(index) => {
                self.ui.watch_selected = index;
                true
            }
            None => false,
        }
    }

    /// Remove the watch at `index`, keeping the selection in range.
    pub fn remove_watch(&mut self, index: usize) -> Option<Watch> {
        let watch = self.watches.remove(index)?;
        self.ui.watch_selected = index.min(self.watches.len().saturating_sub(1));
        Some(watch)
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

#[cfg(test)]
mod watch_tests {
    use super::*;

    #[test]
    fn only_output_watches_stop_execution() {
        assert!(!Watch::Cell(3).stops());
        assert!(Watch::AnyOutput.stops());
        assert!(Watch::Output(b'W').stops());
    }

    #[test]
    fn an_output_watch_matches_the_byte_it_names() {
        assert!(Watch::AnyOutput.matches_output(0));
        assert!(Watch::AnyOutput.matches_output(b'W'));
        assert!(Watch::Output(b'W').matches_output(b'W'));
        assert!(!Watch::Output(b'W').matches_output(b'w'));
        assert!(!Watch::Cell(0).matches_output(b'W'));
    }

    #[test]
    fn labels_say_what_is_being_watched() {
        assert_eq!(Watch::Cell(12).label(), "cell[12]");
        assert_eq!(Watch::AnyOutput.label(), "output");
        assert_eq!(Watch::Output(b'\n').label(), "output '\\n'");
    }
}

#[cfg(test)]
mod watch_list_tests {
    use super::*;

    #[test]
    fn cells_sort_before_output_conditions() {
        let mut watches = Watches::default();
        assert_eq!(watches.add(Watch::Output(b'W')), Some(0));
        watches.add(Watch::Cell(5));
        watches.add(Watch::AnyOutput);
        watches.add(Watch::Cell(1));
        assert_eq!(
            watches.as_slice(),
            [
                Watch::Cell(1),
                Watch::Cell(5),
                Watch::AnyOutput,
                Watch::Output(b'W'),
            ]
        );
    }

    #[test]
    fn adding_returns_where_it_landed_not_where_it_was_pushed() {
        // The list is kept sorted, so a watch added last is rarely displayed
        // last -- and `W` acts on the selection, which this index sets.
        let mut watches = Watches::default();
        watches.add(Watch::Output(b'\n'));
        assert_eq!(watches.add(Watch::Cell(10)), Some(0));
        assert_eq!(watches.as_slice()[0], Watch::Cell(10));
    }

    #[test]
    fn adding_the_same_watch_twice_does_nothing_the_second_time() {
        let mut watches = Watches::default();
        assert_eq!(watches.add(Watch::AnyOutput), Some(0));
        assert_eq!(watches.add(Watch::AnyOutput), None);
        assert_eq!(watches.len(), 1);
    }

    #[test]
    fn removing_past_the_end_is_not_a_panic() {
        let mut watches = Watches::default();
        assert_eq!(watches.remove(0), None);
        watches.add(Watch::Cell(1));
        assert_eq!(watches.remove(9), None);
        assert_eq!(watches.remove(0), Some(Watch::Cell(1)));
    }

    #[test]
    fn the_stop_table_says_exactly_which_bytes_stop() {
        let mut watches = Watches::default();
        assert!(watches.output_stop_table().iter().all(|stop| !stop));
        assert!(!watches.stops_output());

        // A cell watch is display-only and must not switch the feature on.
        watches.add(Watch::Cell(0));
        assert!(!watches.stops_output());
        assert!(watches.output_stop_table().iter().all(|stop| !stop));

        watches.add(Watch::Output(b'W'));
        assert!(watches.stops_output());
        let table = watches.output_stop_table();
        assert!(table[usize::from(b'W')]);
        assert!(!table[usize::from(b'w')]);
        assert_eq!(table.iter().filter(|stop| **stop).count(), 1);

        watches.add(Watch::AnyOutput);
        assert!(watches.output_stop_table().iter().all(|stop| *stop));
    }

    #[test]
    fn the_revision_moves_only_when_the_list_does() {
        let mut watches = Watches::default();
        let start = watches.revision();
        watches.add(Watch::AnyOutput);
        assert_ne!(watches.revision(), start);
        let after = watches.revision();
        watches.add(Watch::AnyOutput);
        assert_eq!(watches.revision(), after, "a rejected add is not a change");
        watches.remove(0);
        assert_ne!(watches.revision(), after);
    }
}
