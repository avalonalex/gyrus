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

use crate::state::Session;
use crate::ui;

/// How often the screen refreshes while a program runs freely.
const REDRAW_INTERVAL: Duration = Duration::from_millis(60);

/// Instructions between clock readings while running freely.
///
/// Reading the clock on every instruction would show up in the profile of a
/// program that runs for millions of steps, and nothing on screen changes fast
/// enough to justify it.
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
pub struct DebuggerHook {
    session: Shared,
    since_poll: u32,
}

impl DebuggerHook {
    /// A hook driving `session`.
    pub fn new(session: Shared) -> Self {
        Self {
            session,
            since_poll: 0,
        }
    }

    /// Called once for every instruction, just before it executes.
    fn reach(&mut self, instruction: &Instruction, context: &HookContext) -> HookDecision {
        let mut session = lock(&self.session);
        let index = context.instruction_index();

        if session.wants_pause(index, instruction) {
            observe(&mut session, context, true);
            self.since_poll = 0;
            session.touch_draw();
            return ui::pause(&mut session);
        }

        self.since_poll += 1;
        if self.since_poll < POLL_INTERVAL {
            return HookDecision::Continue;
        }
        self.since_poll = 0;

        // Snapshot on every poll, not only when redrawing. If the program is
        // about to fail, this is the last look at the tape anyone gets: the VM
        // state is gone by the time the error reaches `main`.
        observe(&mut session, context, true);

        if session.should_redraw(REDRAW_INTERVAL) {
            ui::tick(&mut session)
        } else {
            HookDecision::Continue
        }
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
