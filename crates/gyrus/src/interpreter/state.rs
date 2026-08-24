//! VM state and execution flow types for the BrainFuck interpreter.

use crate::config::MemoryModel;
use crate::debug::DebugInfo;
use crate::error::{BfError, Result};
use crate::types::{MemoryAddress, MemorySize, StepCount};

/// Control flow decision after instruction execution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExecutionFlow {
    /// Continue normal execution
    Continue,
    /// Exit the current loop (LoopCheck instruction returns this when cell is zero)
    LoopExit,
}

/// Result type for instruction execution
///
/// `Ok(ExecutionFlow)` indicates successful execution with control flow decision
/// `Err(BfError)` indicates an actual error occurred
pub(super) type ExecutionResult = std::result::Result<ExecutionFlow, BfError>;

/// VM state containing memory, pointer, and execution counters
pub(super) struct VmState {
    /// Memory tape (array of cells)
    pub memory: Vec<u8>,
    /// Current memory pointer position
    pub pointer: MemoryAddress,
    /// Number of steps executed so far
    pub step_count: StepCount,
    /// Current loop nesting depth (0 = top-level, 1 = inside one loop, etc.)
    /// This is incremented when entering a loop body and decremented when exiting.
    /// Useful for debugging, profiling, and hook context.
    pub loop_depth: usize,
    /// Memory model that dictates how memory operations behave
    pub memory_model: MemoryModel,
    /// Highest cell index actually *used*.
    ///
    /// Not the furthest the cursor travelled: under the tape contract a program
    /// may walk to cell 100_000 and back without touching anything, and it has
    /// not used 100_000 cells. Maintained in `cell_at`, the only place a cell is
    /// reached, and read by *both* interpreters through
    /// [`Self::peak_cells_used`].
    ///
    /// That makes it the first statistic to have a single home. The others are
    /// still split: `bytes_read`, `bytes_written` and `loop_iterations` exist
    /// both here (maintained by the optimized path) and in `StatsTrackerHook`
    /// (counted for the debug path), which is why the two interpreters could
    /// disagree about peak before. Moving the rest here is worth doing; until
    /// then, a statistic that is a property of the VM belongs on the VM.
    pub peak_used: usize,
    /// Number of loop-body entries
    pub loop_iterations: u64,
    /// Total bytes read from input
    pub bytes_read: u64,
    /// Total bytes written to output
    pub bytes_written: u64,
}

impl VmState {
    /// Borrow the cell under the cursor, or fail because the cursor is not on
    /// the tape.
    ///
    /// Every read and write goes through here, because under the tape contract
    /// this is the only place a cursor's position can be wrong. Movement never
    /// fails, so nothing else needs to check.
    #[inline(always)]
    pub fn cell(
        &mut self,
        debug_info: Option<&DebugInfo>,
        instruction_index: usize,
    ) -> Result<&mut u8> {
        self.cell_at(0, debug_info, instruction_index)
    }

    /// Borrow the cell `offset` cells from the cursor. See [`Self::cell`].
    ///
    /// The fast path asks one question -- is this already a cell we have? --
    /// and that question is the same for every memory model, so it needs no
    /// dispatch. Only the answer "no" differs between models, and only then is
    /// [`Self::cell_off_tape`] consulted. Routing every access through the model
    /// instead cost 59% on mandelbrot: the model call returns a `Result` whose
    /// error variant is an 88-byte `BfError`, so it came back through memory
    /// and would not inline.
    #[inline(always)]
    pub fn cell_at(
        &mut self,
        offset: isize,
        debug_info: Option<&DebugInfo>,
        instruction_index: usize,
    ) -> Result<&mut u8> {
        // Wrapping, and safe for it: `index` decides, correctly, for any isize.
        let cursor = MemoryAddress::new(self.pointer.get().wrapping_add(offset));
        if let Some(idx) = cursor.index(self.memory.len()) {
            // Recorded here, with the index already in hand, because this is
            // the only place a cell is reached. `max` lowers to a conditional
            // move; the `if` form costs 11% on hanoi, taken almost never once
            // the tape is warm and mispredicted anyway.
            self.peak_used = self.peak_used.max(idx);
            // tape-access-ok: this is the accessor; `index` just proved idx < len.
            return Ok(&mut self.memory[idx]);
        }
        self.cell_off_tape(cursor, debug_info, instruction_index)
    }

    /// The cursor is not on the tape: grow to reach it, or report it.
    ///
    /// Cold and out of line so that the decision -- which is the only part that
    /// depends on the memory model -- stays out of the instruction loop.
    #[cold]
    #[inline(never)]
    fn cell_off_tape(
        &mut self,
        cursor: MemoryAddress,
        debug_info: Option<&DebugInfo>,
        instruction_index: usize,
    ) -> Result<&mut u8> {
        let (model, steps) = (self.memory_model, self.step_count);
        let cell = model.cell_off_tape(
            cursor,
            &mut self.memory,
            steps,
            debug_info,
            instruction_index,
        )?;
        // Reached a cell the tape did not previously cover -- an unbounded model
        // just grew to it. This is an access like any other and counts towards
        // the peak; the fast path above cannot see it because it only runs for
        // cells the tape already had.
        self.peak_used = self.peak_used.max(cursor.get().max(0) as usize);
        Ok(cell)
    }

    /// How many cells the program has used, for `ExecutionStats`.
    ///
    /// The `+ 1` that turns a highest-index into a count lives here rather than
    /// at each interpreter's exit, because having it in two places is what let
    /// the two disagree about this statistic before.
    #[inline]
    pub fn peak_cells_used(&self) -> MemorySize {
        MemorySize::new(self.peak_used + 1)
    }

    /// Create a new VM state with the given memory model
    pub fn new(memory_model: MemoryModel) -> Self {
        let memory_size = memory_model.initial_size().get();
        Self {
            memory: vec![0u8; memory_size],
            pointer: MemoryAddress::new(0),
            step_count: StepCount::new(0),
            loop_depth: 0, // Start at top level (not inside any loops)
            memory_model,
            peak_used: 0,
            loop_iterations: 0,
            bytes_read: 0,
            bytes_written: 0,
        }
    }
}
