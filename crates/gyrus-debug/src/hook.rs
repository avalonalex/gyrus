//! The bridge between the interpreter and the interface.
//!
//! This is the whole of the debugger's integration with `gyrus`: an
//! [`ExecutionHook`] that decides whether to stop, and two I/O adapters that
//! move bytes between the program and the panels. No change to the library was
//! needed to write it.

use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gyrus::Instruction;
use gyrus::hooks::{ExecutionHook, HookContext, HookDecision, LoopInfo};
use gyrus::io::{BfInput, BfOutput};
use gyrus_tui::cell_under;

use crate::state::{RunState, Session, StopReason, should_pause};
use crate::ui;

/// How often the screen refreshes while a program runs freely.
const REDRAW_INTERVAL: Duration = Duration::from_millis(60);

/// Instructions between clock readings while running freely.
///
/// Reading the clock on every instruction would show up in the profile of a
/// program that runs for millions of steps, and nothing on screen changes fast
/// enough to justify it. A paced run is the exception and sets its own interval
/// of 1: there, every instruction is something to look at.
const POLL_INTERVAL: u32 = 2048;

/// The shared session, held by the hook and both I/O adapters.
pub type Shared = Arc<Mutex<Session>>;

fn lock(session: &Shared) -> std::sync::MutexGuard<'_, Session> {
    // Only one thread ever touches the session, so the lock cannot be poisoned
    // by a concurrent panic; a poisoned lock here means this thread panicked
    // while drawing, and there is nothing sensible left to do.
    session.lock().expect("debugger session poisoned")
}

/// Stops the interpreter where the user asked, and draws the result.
///
/// The stopping rule is decided here rather than under the mutex. `reach` runs
/// on every executed instruction — tens of millions on a real program — and
/// taking an uncontended lock to answer "no" was the single most expensive
/// thing the debugger did, worth about a fifth of its per-instruction cost.
///
/// Caching is sound because every input to the rule changes only inside
/// `ui::pause` and `ui::tick`, which run below with the lock held; `sync`
/// refreshes the cache the moment either returns. The one exception is whether
/// a `,` has anything to read, which `DebugInput` drains while the program
/// runs — so that question, and only that one, still takes the lock.
pub struct DebuggerHook {
    session: Shared,
    since_poll: u32,

    // Cached stopping rule; see `sync`.
    run: RunState,
    exiting: bool,
    /// One `bool` per instruction index, indices being dense from zero.
    stops: Vec<bool>,
    /// The `breakpoint_revision` `stops` was built from.
    stops_revision: u64,
    /// Whether any watch stops on output, so a program with none pays only a
    /// `matches!` per instruction for the feature.
    watches_output: bool,
    /// One entry per byte value: whether printing it stops execution. A table
    /// rather than a walk of the watch list, and cached rather than locked, for
    /// the same reason the breakpoint bitmap is.
    output_stops: [bool; 256],
    /// The `watch_revision` the table was built from.
    watch_revision: u64,
    /// The gap between instructions in a paced run, if one is in progress.
    pace: Option<Duration>,
    /// Instructions between visits to the interface: 1 while pacing, and
    /// `POLL_INTERVAL` otherwise.
    ///
    /// A field rather than a second condition, so a full-speed run tests one
    /// counter against one number and pays nothing at all for slow motion.
    poll_interval: u32,
}

impl DebuggerHook {
    /// A hook driving `session`.
    pub fn new(session: Shared) -> Self {
        let shared = Arc::clone(&session);
        let mut hook = Self {
            session,
            since_poll: 0,
            run: RunState::Step,
            exiting: false,
            stops: Vec::new(),
            // `u64::MAX` cannot be a real revision, so the first `sync` builds
            // both caches. That is why this can just call `sync` rather than
            // repeating its body field by field.
            stops_revision: u64::MAX,
            watches_output: false,
            output_stops: [false; 256],
            watch_revision: u64::MAX,
            pace: None,
            poll_interval: POLL_INTERVAL,
        };
        let guard = lock(&shared);
        hook.sync(&guard);
        drop(guard);
        hook
    }

    /// Re-read the stopping rule after the interface has had a turn.
    fn sync(&mut self, session: &Session) {
        self.run = session.run;
        self.exiting = session.exit.is_some();
        self.watches_output = session.watches_output();
        self.pace = session.pace.delay();
        self.poll_interval = if self.pace.is_some() {
            1
        } else {
            POLL_INTERVAL
        };
        if self.stops_revision != session.breakpoint_revision() {
            self.stops = session.breakpoint_bitmap();
            self.stops_revision = session.breakpoint_revision();
        }
        if self.watch_revision != session.watch_revision() {
            self.output_stops = session.output_stop_table();
            self.watch_revision = session.watch_revision();
        }
    }

    /// Why this instruction should stop, if it should.
    ///
    /// One place, so the header can report the reason instead of guessing at it
    /// afterwards from a snapshot that cannot tell a step onto a `.` from an
    /// output watch firing on the same `.`.
    fn stop_reason(
        &self,
        instruction: &Instruction,
        context: &HookContext,
        index: usize,
    ) -> Option<StopReason> {
        if self.stops.get(index).copied().unwrap_or(false) {
            return Some(StopReason::Breakpoint);
        }
        if matches!(instruction, Instruction::Input) {
            // Rare enough in a hot loop that a lock costs nothing here, and the
            // queue is drained by `DebugInput` while the program runs, so it is
            // the one input to the rule that cannot be cached.
            if lock(&self.session).starving_for_input() {
                return Some(StopReason::NeedsInput);
            }
        }
        if self.watches_output && matches!(instruction, Instruction::Output) {
            // A cursor off the tape reads as zero because that is what the
            // unbounded model is about to print there; under the fixed model the
            // next thing that happens is an out-of-bounds error, and stopping
            // just before it is a help rather than a lie.
            let byte = cell_under(context.memory(), context.pointer().0).unwrap_or(0);
            if self.output_stops[usize::from(byte)] {
                return Some(StopReason::OutputWatch);
            }
        }
        should_pause(self.run, false, index, false).then_some(StopReason::Stepped)
    }

    /// Called once for every instruction, just before it executes.
    fn reach(&mut self, instruction: &Instruction, context: &HookContext) -> HookDecision {
        let index = context.instruction_index();

        // Stop *before* the instruction, output watches included: the tape still
        // holds the byte a `.` is about to print, which is the state that
        // explains it, and one `space` then shows it land in the output panel.
        let reason = if self.exiting {
            Some(StopReason::Stepped)
        } else {
            self.stop_reason(instruction, context, index)
        };

        let Some(reason) = reason else {
            self.since_poll += 1;
            if self.since_poll < self.poll_interval {
                return HookDecision::Continue;
            }
            self.since_poll = 0;
            let pace = self.pace;
            return self.visit(context, |session| match pace {
                // Slow motion draws every instruction rather than every
                // sixtieth of a second: the point is to see each one land.
                Some(delay) => ui::paced(session, delay),
                None if session.should_redraw(REDRAW_INTERVAL) => ui::tick(session),
                None => HookDecision::Continue,
            });
        };

        self.since_poll = 0;
        self.visit(context, |session| {
            session.stop_reason = reason;
            session.touch_draw();
            ui::pause(session)
        })
    }

    /// Record the interpreter's state, hand the session to the interface, and
    /// pick up whatever the interface changed.
    ///
    /// Snapshotting happens here rather than only before a redraw: if the
    /// program is about to fail, this is the last look at the tape anyone gets,
    /// because the VM state is gone by the time the error reaches `main`.
    ///
    /// The handle is cloned so the guard does not borrow `self`, which `sync`
    /// needs mutably afterwards. That is one atomic per visit, and a visit is
    /// at most one per instruction and usually one per few thousand.
    fn visit(
        &mut self,
        context: &HookContext,
        act: impl FnOnce(&mut Session) -> HookDecision,
    ) -> HookDecision {
        let shared = Arc::clone(&self.session);
        let mut session = lock(&shared);
        observe(&mut session, context, true);
        let decision = act(&mut session);
        self.sync(&session);
        decision
    }
}

fn observe(session: &mut Session, context: &HookContext, has_location: bool) {
    session.observe(
        context.memory(),
        context.pointer().0,
        context.step_count().0,
        context.instruction_index(),
        context.loop_depth(),
        if has_location {
            context.source_location().copied()
        } else {
            None
        },
    );
}

impl ExecutionHook for DebuggerHook {
    fn before_instruction(
        &mut self,
        instruction: &Instruction,
        context: &HookContext,
    ) -> HookDecision {
        self.reach(instruction, context)
    }

    fn after_instruction(
        &mut self,
        instruction: &Instruction,
        context: &HookContext,
    ) -> HookDecision {
        // `[` is the one instruction that never reaches `before_instruction`.
        // The interpreter runs the `LoopCheck` at the head of a loop body
        // itself, and dispatches only `after_instruction` for it — and does so
        // *before* the check executes. So for `LoopCheck`, and only for it,
        // this hook point is the "about to run" one, which is where a debugger
        // has to stop. Every other instruction has already executed by now.
        if matches!(instruction, Instruction::LoopCheck) {
            self.reach(instruction, context)
        } else {
            HookDecision::Continue
        }
    }

    fn on_loop_enter(
        &mut self,
        _context: &HookContext,
        loop_info: Option<&LoopInfo>,
    ) -> HookDecision {
        if let Some(info) = loop_info {
            let start = info.loop_instruction_index;
            lock(&self.session)
                .loop_stack
                .push((start, start + info.body_size));
        }
        HookDecision::Continue
    }

    fn on_loop_exit(&mut self, _context: &HookContext) -> HookDecision {
        lock(&self.session).loop_stack.pop();
        HookDecision::Continue
    }

    fn on_complete(&mut self, context: &HookContext) {
        let mut session = lock(&self.session);
        // The final tape is worth keeping, but there is no instruction about to
        // execute any more, so the source panel should stop pointing at one.
        observe(&mut session, context, false);
        session.loop_stack.clear();
    }
}

/// Sends the program's output to the output panel instead of the terminal,
/// which the interface is already using.
pub struct DebugOutput {
    session: Shared,
}

impl DebugOutput {
    /// An output adapter writing into `session`.
    pub fn new(session: Shared) -> Self {
        Self { session }
    }
}

impl BfOutput for DebugOutput {
    fn write_byte(&mut self, byte: u8) -> io::Result<()> {
        let mut session = lock(&self.session);
        session.output.push(byte);
        // A program that writes faster than the eye can read still has to end up
        // scrolled to the newest line.
        session.ui.output_scroll = None;
        Ok(())
    }
}

/// Feeds the program bytes the user queued, rather than reading the terminal,
/// whose keystrokes belong to the debugger.
pub struct DebugInput {
    session: Shared,
}

impl DebugInput {
    /// An input adapter reading from `session`.
    pub fn new(session: Shared) -> Self {
        Self { session }
    }
}

impl BfInput for DebugInput {
    fn read_byte(&mut self) -> io::Result<Option<u8>> {
        let mut session = lock(&self.session);
        match session.pending_input.pop_front() {
            Some(byte) => {
                // Kept so restarting replays what was typed instead of asking
                // for it again.
                session.consumed_input.push(byte);
                Ok(Some(byte))
            }
            None => Ok(None),
        }
    }
}
