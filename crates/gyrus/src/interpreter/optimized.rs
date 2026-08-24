//! Optimized interpreter for executing optimized IR.
//!
//! This module provides a specialized interpreter that executes `OptimizedInstruction`
//! sequences, achieving significant performance improvements over the standard interpreter.
//!
//! Key differences from the standard interpreter:
//! - Executes fused operations in single steps (e.g., Add(5) vs 5× IncrementValue)
//! - Pattern-recognized loops execute as single operations (e.g., Zero, MultiplyAdd)
//! - No debug symbol support (faster execution path)
//! - Simplified step counting (see `interpret_optimized`)

use super::state::{ExecutionFlow, ExecutionResult, VmState};
use crate::config::ExecutionConfig;
use crate::error::{BfError, Result};
use crate::io::{BfInput, BfOutput};
use crate::optimizer::{OptimizedInstruction, OptimizedProgram};
use crate::stats::ExecutionStats;
use crate::types::{MemoryAddress, StepCount};
use std::cell::Cell;

/// Execute optimized instructions
///
/// This is the fast path for executing optimized BrainFuck programs.
/// It assumes the IR has been optimized and validated, and provides
/// minimal overhead for maximum performance.
///
/// # Performance
///
/// Expected speedups compared to standard interpreter:
/// - Simple arithmetic: 5-10× (instruction fusion)
/// - Pointer movement: 10-20× (movement fusion)
/// - Loop patterns: 100-500× (pattern recognition)
///
/// # Limitations
///
/// - No debug symbol support (for debugging, use standard interpreter)
/// - Step counting is approximate: one step per optimized instruction, except
///   seeks, which cost one step per cell examined so that `--max-steps` bounds
///   a runaway seek the same way it bounds the unfused loop
/// - The timeout is sampled, not checked per instruction. The wall clock is
///   read at most once every 1024 steps -- microseconds apart in arithmetic
///   code -- so a program may run a little past its deadline before stopping.
///   Calls out of the interpreter (`,` and `.`) bring the next read forward, so
///   blocking I/O does not compound it. What no in-process deadline can do is
///   interrupt a call that never returns: a `,` waiting on an idle terminal
///   blocks until it gets a byte. It is a deadline, not a real-time guarantee
/// - Hooks are not yet supported (future enhancement). Whatever wires them in
///   must call [`arm_time_check`] around them: a hook is arbitrary code called
///   per instruction, so like `,` and `.` it can consume unbounded wall time in
///   a single step, which is what the timeout's step-sampling assumes away
///
/// # Arguments
///
/// * `program` - Optimized program from `optimize()` / `optimize_with_cell_model()`
/// * `config` - Execution configuration (memory model, limits, etc.)
/// * `input` - Input source
/// * `output` - Output destination
///
/// # Returns
///
/// Execution statistics on success, or an error if execution fails.
pub fn interpret_optimized<I: BfInput, O: BfOutput>(
    program: &OptimizedProgram,
    config: ExecutionConfig,
    input: &mut I,
    output: &mut O,
) -> Result<ExecutionStats> {
    // A program is only meaningful under the cell model it was optimized for:
    // `optimize` folds `[->+++<]` into a single wrapping multiply, which under
    // checked cells skips the overflow the unfused loop reports. Refusing the
    // mismatch here is what makes that safety property hold for every caller,
    // rather than only for the ones that read the rustdoc.
    if program.cell_model != *config.cell_model() {
        return Err(BfError::ConfigurationError {
            message: format!(
                "program was optimized for {} but is being run with {}. \
                 Build it with optimize_with_cell_model(instructions, config.cell_model()).",
                program.cell_model,
                config.cell_model()
            ),
        });
    }

    let instructions = &program.instructions;
    let mut state = VmState::new(*config.memory_model());
    let mut stats = ExecutionStats::default();
    let limits = Limits::from_config(&config);

    // Execute the optimized program.
    //
    // Monomorphize on whether any limit is configured. With neither --max-steps
    // nor --timeout set -- the default, and the case every benchmark measures --
    // CHECK_LIMITS is false and the compiler deletes the per-instruction check
    // entirely. Leaving it in cost 25-35% on mandelbrot.
    //
    // Monomorphize on the cell model for the same reason: it is fixed for the
    // whole run, so testing it per instruction puts a branch the compiler
    // cannot hoist inside every loop body. WRAPPING selects the whole-run
    // arithmetic and deletes the &dyn CellBehavior path from the build.
    let limited = limits.max_steps.is_some() || limits.timeout_ms.is_some();
    let wrapping = matches!(program.cell_model, crate::config::CellModel::U8Wrapping(_));
    match (limited, wrapping) {
        (true, true) => execute_block::<true, true, _, _>(
            instructions,
            &mut state,
            &config,
            &limits,
            input,
            output,
        )?,
        (true, false) => execute_block::<true, false, _, _>(
            instructions,
            &mut state,
            &config,
            &limits,
            input,
            output,
        )?,
        (false, true) => execute_block::<false, true, _, _>(
            instructions,
            &mut state,
            &config,
            &limits,
            input,
            output,
        )?,
        (false, false) => execute_block::<false, false, _, _>(
            instructions,
            &mut state,
            &config,
            &limits,
            input,
            output,
        )?,
    };

    // Collect final statistics.
    //
    // The optimized path runs without hooks, so these come from counters on
    // VmState rather than from `StatsTracker`. Two of them are in *optimized*
    // units and cannot match the debug interpreter: `total_steps` counts one
    // step per optimized instruction plus one per cell a seek walks (see
    // `interpret_optimized`), and `loop_iterations` counts only loops
    // the optimizer left as loops — a `[-]` fused into `Zero`, or a `[>]` fused
    // into `SeekRight`, no longer iterates. The rest (peak pointer, cells
    // modified, bytes in and out) describe the program's actual behavior and do
    // match.
    stats.total_steps = state.step_count;
    stats.peak_memory_used = state.peak_cells_used();
    // `len()`, not `capacity()`: the debug interpreter reports the tape length and
    // a Vec's spare capacity is not addressable memory.
    stats.memory_allocated = crate::types::MemorySize::new(state.memory.len());
    stats.loop_iterations = state.loop_iterations;
    stats.bytes_read = state.bytes_read;
    stats.bytes_written = state.bytes_written;
    stats.cells_modified = ExecutionStats::count_modified_cells(&state.memory);

    Ok(stats)
}

/// Enforce the configured step and time limits.
///
/// Called before every optimized instruction and once per iteration of a
/// general loop, so a loop with an empty body (`[]`) still terminates.
///
/// `inline(always)`, and the two error values are built by `#[cold]` functions
/// rather than here. A plain `#[inline]` hint was refused: the `to_string` and
/// `format!` for errors that fire at most once made the body big enough that
/// LLVM emitted a real call, so every instruction paid a 160-byte frame,
/// callee-saved spills and an sret round-trip around six instructions of
/// comparison -- and the `Limits` fields were reloaded each time instead of
/// living in registers. Outlining the cold half buys back 40% of the
/// `--max-steps` overhead and 25% of `--timeout`.
#[inline(always)]
fn check_limits(state: &VmState, limits: &Limits) -> Result<()> {
    let steps = state.step_count.get();
    if let Some(max_steps) = limits.max_steps
        && steps >= max_steps
    {
        return Err(step_limit_error(max_steps, state.step_count));
    }

    if let Some(timeout_ms) = limits.timeout_ms
        && steps >= limits.next_time_check.get()
    {
        limits
            .next_time_check
            .set(steps.saturating_add(TIME_CHECK_INTERVAL));
        if limits.start_time.elapsed().as_millis() as u64 > timeout_ms {
            return Err(timeout_error(timeout_ms, state.step_count));
        }
    }

    Ok(())
}

/// Build the step-limit error. Out of line and `#[cold]`: it runs at most once
/// per execution, and inlining it is what stopped `check_limits` from inlining.
#[cold]
#[inline(never)]
fn step_limit_error(max_steps: u64, steps: StepCount) -> BfError {
    BfError::StepLimitExceeded {
        limit: max_steps,
        actual_steps: steps,
        hint: "Optimized interpreter counts 1 step per optimized instruction, \
               plus 1 for each cell a seek walks over"
            .to_string(),
        source_location: None,
        instruction_index: steps.get() as usize,
    }
}

/// Build the timeout error. Out of line and `#[cold]`; see [`step_limit_error`].
#[cold]
#[inline(never)]
fn timeout_error(timeout_ms: u64, steps: StepCount) -> BfError {
    BfError::ExecutionTimeout {
        limit_ms: timeout_ms,
        actual_steps: Some(steps),
        hint: format!(
            "Program exceeded {}ms timeout after executing {} optimized instructions. \
             Try increasing the timeout with --timeout {}.",
            timeout_ms,
            steps.get(),
            timeout_ms * 2
        ),
    }
}

/// Bring the next wall-clock read forward to at most `within` steps from now.
///
/// Sampling by step count assumes steps cost roughly the same. That holds for
/// everything the interpreter does itself, and fails for every call *out* of
/// it, where an unknown amount of wall time can pass in one step. Today the
/// only out-calls are `,` and `.`; a hook or a debugger breakpoint would be
/// another, and would need the same treatment -- the rule is about leaving the
/// interpreter, not about which opcode does it.
///
/// `within` is how late the deadline may be as a result. `,` passes 0: it can
/// block indefinitely on a terminal, and it is never hot. `.` passes
/// [`OUTPUT_CHECK_WITHIN`]: a blocked write is bounded by the reader draining
/// the pipe, and output can be extremely hot, so forcing a read after every one
/// costs about 2x on an output-heavy program and defeats the sampling exactly
/// where it was meant to help.
#[inline]
fn arm_time_check<const CHECK_LIMITS: bool>(limits: &Limits, steps: u64, within: u64) {
    if !CHECK_LIMITS {
        return;
    }
    let cap = steps.saturating_add(within);
    if limits.next_time_check.get() > cap {
        limits.next_time_check.set(cap);
    }
}

/// How many steps a blocked write may delay the timeout by.
///
/// `.` can be millions of instructions in a row, so it brings the check forward
/// rather than demanding one outright: 64 bounds the lateness at 64 blocking
/// writes while reading the clock 64x less often than arming would.
const OUTPUT_CHECK_WITHIN: u64 = 64;

/// Steps that may pass between wall-clock reads when a timeout is set.
///
/// `Instant::elapsed` costs an order of magnitude more than executing an
/// optimized instruction, so reading it before every one made `--timeout` a 13x
/// slowdown rather than a safety net.
///
/// What makes a step count a usable proxy for elapsed time is that per-step
/// work is bounded: every instruction the interpreter executes itself does a
/// bounded amount of work, seeks are charged per cell, and unbounded memory
/// growth is amortized. The exception is calls out of the interpreter, which
/// [`arm_time_check`] handles. That property, not any particular nanoseconds
/// figure, is what keeps 1024 a defensible number as hardware changes.
///
/// The tree-walking interpreter does not sample; see `LimitEnforcerHook`.
const TIME_CHECK_INTERVAL: u64 = 1024;

/// Step and time limits, resolved once so the hot loop does not re-read config.
struct Limits {
    max_steps: Option<u64>,
    timeout_ms: Option<u64>,
    start_time: std::time::Instant,
    /// Step count at which the wall clock is next worth reading.
    ///
    /// A threshold rather than a `steps % INTERVAL == 0` test. Instructions
    /// advance the count by one, but a seek advances it by the number of cells
    /// it walked, so a test for an exact multiple is skipped whenever a seek
    /// steps over one, and the check waits for the next multiple instead.
    ///
    /// A threshold cannot be stepped over: the very next `check_limits` reads
    /// the clock. That does not make the gap INTERVAL steps -- one seek can
    /// still carry the count far past the threshold in a single instruction --
    /// it makes the gap at most INTERVAL steps *plus one instruction*, which is
    /// the best any step-keyed scheme can do.
    next_time_check: Cell<u64>,
}

impl Limits {
    fn from_config(config: &ExecutionConfig) -> Self {
        Self {
            max_steps: config.max_steps(),
            timeout_ms: config.timeout_ms(),
            start_time: std::time::Instant::now(),
            next_time_check: Cell::new(0),
        }
    }
}

/// Execute a block of optimized instructions
fn execute_block<const CHECK_LIMITS: bool, const WRAPPING: bool, I: BfInput, O: BfOutput>(
    instructions: &[OptimizedInstruction],
    state: &mut VmState,
    config: &ExecutionConfig,
    limits: &Limits,
    input: &mut I,
    output: &mut O,
) -> ExecutionResult {
    for instruction in instructions {
        // Check execution limits
        if CHECK_LIMITS {
            check_limits(state, limits)?;
        }

        // Execute the instruction
        execute_instruction::<CHECK_LIMITS, WRAPPING, _, _>(
            instruction,
            state,
            config,
            limits,
            input,
            output,
        )?;

        // One step per instruction; seeks add the cells they walk on top.
        state.step_count += 1;
    }

    Ok(ExecutionFlow::Continue)
}

/// How far right a seek may scan before the step limit would have stopped it.
///
/// Clamping the window rather than abandoning the scan is what keeps
/// `--max-steps` a bound on the run instead of a switch between two
/// implementations of it. Without a step limit the window is the whole tape.
#[inline]
fn seek_window_end(check_limits: bool, state: &VmState, limits: &Limits) -> usize {
    let len = state.memory.len();
    if !check_limits {
        return len;
    }
    match limits.max_steps {
        Some(max) => {
            let budget = max.saturating_sub(state.step_count.get());
            let reach = (state.pointer.get() as u64).saturating_add(budget);
            // +1 so the cell the limit lands on is still examined, matching the
            // per-cell loop, which checks the limit before moving.
            (reach.saturating_add(1)).min(len as u64) as usize
        }
        None => len,
    }
}

/// How far left a seek may scan. See [`seek_window_end`].
#[inline]
fn seek_window_start(check_limits: bool, state: &VmState, limits: &Limits) -> usize {
    if !check_limits {
        return 0;
    }
    match limits.max_steps {
        Some(max) => {
            let budget = max.saturating_sub(state.step_count.get());
            // Widen and subtract, as seek_window_end does: the result is never
            // above `pointer`, so narrowing back is lossless on any target.
            ((state.pointer.get() as u64).saturating_sub(budget)) as usize
        }
        None => 0,
    }
}

/// Execute a single optimized instruction
#[inline]
fn execute_instruction<const CHECK_LIMITS: bool, const WRAPPING: bool, I: BfInput, O: BfOutput>(
    instruction: &OptimizedInstruction,
    state: &mut VmState,
    config: &ExecutionConfig,
    limits: &Limits,
    input: &mut I,
    output: &mut O,
) -> ExecutionResult {
    match instruction {
        // Fused arithmetic operations
        OptimizedInstruction::Add(n, _range) => {
            // See Right for the fused-run argument. It bites harder here:
            // behavior() hands out a &dyn CellBehavior, so each unit step is an
            // indirect call the compiler cannot inline into the single
            // wrapping_add it wraps. Checked cells keep the per-step loop, which
            // is what reports the overflow at the right value.
            let steps = state.step_count;
            let cell = state.cell(None, 0)?;
            if WRAPPING {
                *cell = cell.wrapping_add(*n);
            } else {
                let cells = config.cell_model().behavior();
                for _ in 0..*n {
                    cells.try_increment(cell, steps, None)?;
                }
            }
            Ok(ExecutionFlow::Continue)
        }

        OptimizedInstruction::Sub(n, _range) => {
            let steps = state.step_count;
            let cell = state.cell(None, 0)?;
            if WRAPPING {
                *cell = cell.wrapping_sub(*n);
            } else {
                let cells = config.cell_model().behavior();
                for _ in 0..*n {
                    cells.try_decrement(cell, steps, None)?;
                }
            }
            Ok(ExecutionFlow::Continue)
        }

        // Fused pointer movements.
        //
        // Under the tape contract these cannot fail: the cursor may sit anywhere,
        // and only using it is checked. A fused `>>>>` is therefore one add, with
        // no bounds test, no fallback loop and no memory model involved -- the
        // check that used to live here now happens once, at whatever access comes
        // next, instead of once per move.
        OptimizedInstruction::Right(n, _range) => {
            state.pointer.advance(*n as isize);
            Ok(ExecutionFlow::Continue)
        }

        OptimizedInstruction::Left(n, _range) => {
            state.pointer.advance(-(*n as isize));
            Ok(ExecutionFlow::Continue)
        }

        // I/O operations
        OptimizedInstruction::Output(_range) => {
            let byte = *state.cell(None, 0)?;
            output.write_byte(byte).map_err(|source| BfError::IoError {
                operation: "writing output".to_string(),
                instruction_index: Some(state.step_count.into()),
                source,
            })?;
            state.bytes_written += 1;
            // After the out-call: an unknown amount of wall time just passed.
            arm_time_check::<CHECK_LIMITS>(limits, state.step_count.get(), OUTPUT_CHECK_WITHIN);
            Ok(ExecutionFlow::Continue)
        }

        OptimizedInstruction::Input(_range) => {
            let byte = input.read_byte();
            // After the out-call: `,` can have blocked on a terminal for any
            // length of time, so the next check reads the clock unconditionally.
            arm_time_check::<CHECK_LIMITS>(limits, state.step_count.get(), 0);
            match byte {
                Ok(Some(byte)) => {
                    *state.cell(None, 0)? = byte;
                    state.bytes_read += 1;
                    Ok(ExecutionFlow::Continue)
                }
                Ok(None) => {
                    // EOF - handle according to config
                    match config.eof_behavior() {
                        crate::config::EofBehavior::SetZero => {
                            *state.cell(None, 0)? = 0;
                            Ok(ExecutionFlow::Continue)
                        }
                        crate::config::EofBehavior::SetNegOne => {
                            *state.cell(None, 0)? = 255;
                            Ok(ExecutionFlow::Continue)
                        }
                        crate::config::EofBehavior::NoChange => Ok(ExecutionFlow::Continue),
                        crate::config::EofBehavior::Error => Err(BfError::IoError {
                            operation: "reading input (EOF reached)".to_string(),
                            instruction_index: Some(state.step_count.into()),
                            source: std::io::Error::new(
                                std::io::ErrorKind::UnexpectedEof,
                                "unexpected EOF on input",
                            ),
                        }),
                    }
                }
                Err(source) => Err(BfError::IoError {
                    operation: "reading input".to_string(),
                    instruction_index: Some(state.step_count.into()),
                    source,
                }),
            }
        }

        // Optimized loop patterns
        OptimizedInstruction::Zero(_range) => {
            // [-] → set cell to 0
            *state.cell(None, 0)? = 0;
            Ok(ExecutionFlow::Continue)
        }

        OptimizedInstruction::SeekRight(_range) => {
            // [>] → seek to next zero cell.
            //
            // A seek reads every cell it tests, so unlike a plain move it is
            // bounded by the tape: the starting cell must be on it, and running
            // off the right is an access, which errors or grows as the model
            // decides. The scan itself is a search for a zero byte in a slice,
            // which the standard library does far faster than a call per cell.
            //
            // The seek is charged one step per cell it examines. The unfused
            // `[>]` costs a step per cell, so charging one per fused instruction
            // would let a runaway seek outrun --max-steps forever. The scan is
            // clamped to what the step budget allows rather than switched off
            // when a limit is set: both give the same answer, but branching away
            // from the scan made --max-steps a 10x slowdown on seek-heavy
            // programs, and a limit should bound a run, not change how it runs.
            if *state.cell(None, 0)? == 0 {
                return Ok(ExecutionFlow::Continue);
            }
            let start = state.pointer.get() as usize;
            let hi = seek_window_end(CHECK_LIMITS, state, limits);
            if hi > start
                && let Some(offset) = state.memory[start..hi].iter().position(|&b| b == 0)
            {
                let end = start + offset;
                state.pointer = MemoryAddress::new(end as isize);
                state.step_count += offset as u64;
                return Ok(ExecutionFlow::Continue);
            }
            // No zero within the window. Every cell from `start` to `hi` is
            // non-zero, so testing them again would only repeat work; skip to the
            // last and let the loop decide what happens next -- fire the step
            // limit, grow an unbounded tape, or report a fixed one's overrun.
            if hi > start {
                state.step_count += (hi - 1 - start) as u64;
                state.pointer = MemoryAddress::new((hi - 1) as isize);
            }
            loop {
                // Terminate before charging: a seek that arrives on the zero
                // cell with its budget exactly spent has finished, and should
                // not be failed for the step it did not need.
                if *state.cell(None, 0)? == 0 {
                    break;
                }
                // A seek is a loop: honour step/time limits so it cannot spin forever
                if CHECK_LIMITS {
                    check_limits(state, limits)?;
                }
                state.pointer.increment();
                state.step_count += 1;
            }
            Ok(ExecutionFlow::Continue)
        }

        OptimizedInstruction::SeekLeft(_range) => {
            // [<] → seek to previous zero cell. See SeekRight for why this is a
            // scan rather than a per-cell call, why it is charged per cell, and
            // why the window is clamped rather than abandoned under limits.
            if *state.cell(None, 0)? == 0 {
                return Ok(ExecutionFlow::Continue);
            }
            let start = state.pointer.get() as usize;
            let lo = seek_window_start(CHECK_LIMITS, state, limits);
            if let Some(found) = state.memory[lo..=start].iter().rposition(|&b| b == 0) {
                // rposition indexes the window, so shift it back to the tape.
                let end = lo + found;
                state.pointer = MemoryAddress::new(end as isize);
                state.step_count += (start - end) as u64;
                return Ok(ExecutionFlow::Continue);
            }
            if start > lo {
                state.step_count += (start - lo) as u64;
                state.pointer = MemoryAddress::new(lo as isize);
            }
            loop {
                // Terminate before charging; see SeekRight.
                if *state.cell(None, 0)? == 0 {
                    break;
                }
                if CHECK_LIMITS {
                    check_limits(state, limits)?;
                }
                state.pointer.decrement();
                state.step_count += 1;
            }
            Ok(ExecutionFlow::Continue)
        }

        OptimizedInstruction::MultiplyAdd(operations, _range) => {
            // Generalized multiplication loop
            // For each (offset, multiplier): cell[ptr + offset] += cell[ptr] * multiplier
            // Then: cell[ptr] = 0
            let source_value = *state.cell(None, 0)?;

            // Early exit if source is already zero (optimization)
            if source_value == 0 {
                return Ok(ExecutionFlow::Continue);
            }

            for (offset, multiplier) in operations {
                // `cell_at` resolves the target and enforces the tape bound
                // there, which is the whole of what the old pointer-walk was
                // doing: a fixed tape reports the overrun, an unbounded one
                // grows to cover the target. No walking, and no restoring the
                // cursor afterwards, because it never moved.
                let target = state.cell_at(*offset, None, 0)?;
                let delta = (*target as i32) + (source_value as i32) * multiplier;
                *target = delta as u8; // Wrapping
            }

            // Zero the source cell
            *state.cell(None, 0)? = 0;
            Ok(ExecutionFlow::Continue)
        }

        // General loops (not optimized, recursively execute)
        OptimizedInstruction::Loop(body, _range) => {
            state.loop_depth += 1;

            // Loop while current cell is non-zero. The condition reads the
            // cell, so it is an access and the cursor must be on the tape.
            while *state.cell(None, 0)? != 0 {
                // Check limits per iteration, not only per instruction: a loop with
                // an empty body (`[]`) executes no instructions, so without this the
                // step limit and timeout would never be consulted and it would hang.
                if CHECK_LIMITS {
                    check_limits(state, limits)?;
                }
                // The `[` condition check is itself a step, which is also what lets
                // the step limit make progress on an empty body.
                state.step_count += 1;
                state.loop_iterations += 1;
                execute_block::<CHECK_LIMITS, WRAPPING, _, _>(
                    body, state, config, limits, input, output,
                )?;
            }

            state.loop_depth -= 1;
            Ok(ExecutionFlow::Continue)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ExecutionConfigBuilder;
    use crate::config::CellModel;
    use crate::io::StringIo;
    use crate::optimizer::{OptimizedInstruction, SourceRange, optimize, optimize_with_cell_model};
    use crate::parse;

    // --- fused-run fast paths -------------------------------------------
    //
    // Right/Left/Add/Sub each execute a whole fused run in one step when it is
    // safe, and defer to the per-unit loop when it is not. These pin the seam:
    // the fast path must agree with the loop, and the loop must still produce
    // the errors it always did.

    /// Optimize for the model the config actually uses, then run.
    ///
    /// Optimizing with `optimize()` here would build a wrapping program and hand
    /// it to a checked-cells config -- which `interpret_optimized` now rejects,
    /// but which used to pass silently and made checked-cell tests test nothing.
    fn run_optimized(
        source: &str,
        config: ExecutionConfig,
    ) -> crate::error::Result<(ExecutionStats, String)> {
        let instructions = parse(source).unwrap();
        let program = optimize_with_cell_model(&instructions, *config.cell_model());
        let mut input = StringIo::empty();
        let mut output = StringIo::empty();
        let stats = interpret_optimized(&program, config, &mut input, &mut output)?;
        Ok((stats, output.output_string()))
    }

    /// Just the output, for tests that do not care about statistics.
    fn run_opt(src: &str, config: ExecutionConfig) -> Result<String> {
        run_optimized(src, config).map(|(_, out)| out)
    }

    #[test]
    fn fused_pointer_move_lands_where_the_unit_steps_would() {
        // 9 rights then 4 lefts leaves the pointer on cell 5.
        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        let out = run_opt(">>>>>>>>><<<<+.", config).unwrap();
        assert_eq!(out.as_bytes(), &[1]);
    }

    #[test]
    fn fused_right_that_exactly_fits_is_not_an_error() {
        // memory_size 10 => cells 0..=9; Right(9) lands on the last valid cell.
        let config = ExecutionConfigBuilder::new().with_memory_size(10).build();
        assert!(run_opt(">>>>>>>>>+.", config).is_ok());
    }

    #[test]
    fn fused_right_past_the_end_still_reports_out_of_bounds() {
        let config = ExecutionConfigBuilder::new().with_memory_size(10).build();
        // Moving past the end is legal; using the cell out there is not.
        let err = run_opt(">>>>>>>>>>+", config).unwrap_err();
        assert!(
            matches!(err, BfError::MemoryOutOfBounds { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn fused_left_below_zero_still_reports_out_of_bounds() {
        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        // Right(2) then Left(4) puts the cursor at -2, which is legal; the
        // `+` that then uses it is not.
        let err = run_opt(">><<<<+", config).unwrap_err();
        assert!(
            matches!(err, BfError::MemoryOutOfBounds { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn fused_left_to_exactly_zero_is_not_an_error() {
        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        assert!(run_opt(">>>>><<<<<+.", config).is_ok());
    }

    #[test]
    fn fused_cell_arithmetic_wraps_like_the_unit_steps() {
        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_wrapping_cells()
            .build();
        // 255 increments then 3 more: wraps to 2.
        let src = format!("{}.", "+".repeat(258));
        assert_eq!(run_opt(&src, config).unwrap().as_bytes(), &[2]);
    }

    #[test]
    fn fused_cell_arithmetic_still_errors_under_checked_cells() {
        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_checked_cells()
            .build();
        let src = "+".repeat(256);
        let err = run_opt(&src, config).unwrap_err();
        assert!(matches!(err, BfError::CellOverflow { .. }), "got {err:?}");
    }

    /// A multiply loop must report the overflow the unfused program reports,
    /// rather than folding it into one wrapping multiply. This is the bug the
    /// cell-model gate in `optimize_with_cell_model` exists to prevent.
    #[test]
    fn multiply_loop_reports_overflow_under_checked_cells() {
        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_checked_cells()
            .build();
        let src = format!("{}[->+++<]>.", "+".repeat(100));
        let err = run_opt(&src, config).unwrap_err();
        assert!(matches!(err, BfError::CellOverflow { .. }), "got {err:?}");
    }

    #[test]
    fn seek_finds_the_same_cell_as_stepping_would() {
        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        // cells 0..2 non-zero, cell 3 zero: [>] from 0 must stop on cell 3.
        assert_eq!(run_opt("+>+>+><<<[>]+.", config).unwrap().as_bytes(), &[1]);
    }

    #[test]
    fn seek_on_a_zero_cell_does_not_move() {
        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        // Cell 0 is already zero, so [>] is a no-op and the + lands on cell 0.
        assert_eq!(run_opt("[>]+.", config).unwrap().as_bytes(), &[1]);
    }

    #[test]
    fn seek_left_finds_the_nearest_zero_to_the_left() {
        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        // cell 0 zero, cells 1..3 non-zero; [<] from cell 3 stops on cell 0.
        assert_eq!(run_opt(">+>+>+[<]+.", config).unwrap().as_bytes(), &[1]);
    }

    #[test]
    fn test_optimized_add() {
        let program = OptimizedProgram::new(
            vec![OptimizedInstruction::Add(5, SourceRange::new(0, 5))],
            5,
            CellModel::default(),
        );
        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        let mut input = StringIo::empty();
        let mut output = StringIo::empty();

        let stats = interpret_optimized(&program, config, &mut input, &mut output).unwrap();

        assert_eq!(stats.total_steps, crate::types::StepCount::new(1)); // Single optimized instruction
    }

    #[test]
    fn test_optimized_zero() {
        let source = "+++[-]";
        let instructions = parse(source).unwrap();
        let optimized = optimize(&instructions);

        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        let mut input = StringIo::empty();
        let mut output = StringIo::empty();

        let stats = interpret_optimized(&optimized, config, &mut input, &mut output).unwrap();

        // Should be 2 steps: Add(3) + Zero
        assert_eq!(stats.total_steps, crate::types::StepCount::new(2));
    }

    /// The optimized path runs without hooks, so it must accumulate statistics
    /// itself. These previously came back as zeroes, and `peak_memory_used`
    /// reported the whole allocation (30,000 for the default fixed model)
    /// rather than the highest cell the pointer reached.
    #[test]
    fn test_optimized_collects_statistics() {
        // Writes 3 cells, walks to cell 2, emits 2 bytes.
        let source = ">+++>++.<<+.";
        let instructions = parse(source).unwrap();
        let optimized = optimize(&instructions);

        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        let mut input = StringIo::empty();
        let mut output = StringIo::empty();

        let stats = interpret_optimized(&optimized, config, &mut input, &mut output).unwrap();

        assert_eq!(stats.bytes_written, 2, "two Output instructions ran");
        assert_eq!(stats.bytes_read, 0, "program reads no input");
        assert_eq!(
            stats.cells_modified, 3,
            "cells 0, 1 and 2 hold non-zero values"
        );
        assert_eq!(
            stats.peak_memory_used,
            crate::types::MemorySize::new(3),
            "pointer reached cell 2, so the peak is 3 cells - not the 100-cell allocation"
        );
    }

    #[test]
    fn test_optimized_counts_input_bytes() {
        let source = ",.,."; // echo two bytes
        let instructions = parse(source).unwrap();
        let optimized = optimize(&instructions);

        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        let mut input = StringIo::new("hi");
        let mut output = StringIo::empty();

        let stats = interpret_optimized(&optimized, config, &mut input, &mut output).unwrap();

        assert_eq!(stats.bytes_read, 2);
        assert_eq!(stats.bytes_written, 2);
        assert_eq!(output.output_string(), "hi");
    }

    /// Loops the optimizer leaves alone are counted; loops it fuses away are
    /// not, because they no longer iterate. This is the one statistic that
    /// deliberately differs from the debug interpreter.
    #[test]
    fn test_optimized_counts_surviving_loop_iterations() {
        // An outer loop the optimizer cannot fuse (it contains Output).
        let source = "+++[-.]";
        let instructions = parse(source).unwrap();
        let optimized = optimize(&instructions);

        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        let mut input = StringIo::empty();
        let mut output = StringIo::empty();

        let stats = interpret_optimized(&optimized, config, &mut input, &mut output).unwrap();

        assert_eq!(stats.loop_iterations, 3, "loop body ran once per decrement");
        assert_eq!(stats.bytes_written, 3);
    }

    /// A cleared cell is not a modified cell: `cells_modified` counts non-zero
    /// cells at exit, matching the debug interpreter's definition.
    #[test]
    fn test_optimized_cleared_cells_are_not_counted() {
        let source = "+++[-]"; // fused into Add(3) + Zero
        let instructions = parse(source).unwrap();
        let optimized = optimize(&instructions);

        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        let mut input = StringIo::empty();
        let mut output = StringIo::empty();

        let stats = interpret_optimized(&optimized, config, &mut input, &mut output).unwrap();

        assert_eq!(stats.cells_modified, 0, "the cell was cleared before exit");
        assert_eq!(stats.loop_iterations, 0, "the loop was fused into Zero");
    }

    #[test]
    fn test_optimized_multiply_add() {
        let source = "+++++[->+++<]"; // 5 * 3 = 15
        let instructions = parse(source).unwrap();
        let optimized = optimize(&instructions);

        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        let mut input = StringIo::empty();
        let mut output = StringIo::empty();

        let _stats = interpret_optimized(&optimized, config, &mut input, &mut output).unwrap();

        // Result should be: cell[0] = 0, cell[1] = 15
        // We'd need to expose final state to verify, but at least it should not error
    }

    #[test]
    fn test_optimized_simple_arithmetic() {
        // Simple arithmetic: should fuse well
        let source = "+++++>+++>++";
        let instructions = parse(source).unwrap();
        let optimized = optimize(&instructions);

        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        let mut input = StringIo::empty();
        let mut output = StringIo::empty();

        let stats = interpret_optimized(&optimized, config, &mut input, &mut output).unwrap();

        // Should be 3 steps: Add(5), Right(1), Add(3), Right(1), Add(2)
        // Wait, that's 5 steps. Let me recalculate:
        // Optimized IR: Add(5), Right(1), Add(3), Right(1), Add(2) = 5 instructions
        assert_eq!(stats.total_steps.get(), 5);
    }

    #[test]
    fn test_optimized_seek_pattern() {
        // Seek pattern: [>] executes as a single operation, but is charged for
        // the cells it walks -- see `seek_is_charged_one_step_per_cell`.
        let source = "+++++[>]";
        let instructions = parse(source).unwrap();
        let optimized = optimize(&instructions);

        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        let mut input = StringIo::empty();
        let mut output = StringIo::empty();

        let stats = interpret_optimized(&optimized, config, &mut input, &mut output).unwrap();

        // Add(5) is 1 step; the seek is 1 for the instruction plus 1 for the
        // single cell it moved over (cell 0 is non-zero, cell 1 is not).
        assert_eq!(stats.total_steps.get(), 3);
    }

    /// A seek costs a step per cell examined, not a flat one per instruction.
    /// Charging a flat step is what let a runaway seek outrun --max-steps: the
    /// limit check compared a step_count that never moved while the seek span.
    #[test]
    fn seek_is_charged_one_step_per_cell() {
        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        let instructions = parse(">+>+>+[<]").unwrap();
        let program = optimize_with_cell_model(&instructions, *config.cell_model());
        let (mut i, mut o) = (StringIo::empty(), StringIo::empty());
        let stats = interpret_optimized(&program, config, &mut i, &mut o).unwrap();

        // Seven optimized instructions -- Right(1), Add(1) three times over,
        // then SeekLeft -- and the seek walks cells 3, 2 and 1 to reach the
        // zero at cell 0, so it is charged three more.
        assert_eq!(program.optimized_count, 7);
        assert_eq!(stats.total_steps.get(), 7 + 3);
    }

    /// A program carries the cell model it was optimized for, and running it
    /// under a different one is refused rather than silently executing folds
    /// that do not hold there.
    #[test]
    fn running_a_program_under_the_wrong_cell_model_is_refused() {
        let instructions = parse("[->+++<]").unwrap();
        let program = optimize(&instructions); // wrapping
        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_checked_cells()
            .build();
        let (mut i, mut o) = (StringIo::empty(), StringIo::empty());
        let err = interpret_optimized(&program, config, &mut i, &mut o).unwrap_err();
        assert!(
            matches!(err, BfError::ConfigurationError { .. }),
            "got {err:?}"
        );
    }

    /// Seeks take one path with limits armed and another without. Both must
    /// land on the same cell and charge the same steps, or --max-steps would
    /// change a program's result rather than just bounding it.
    #[test]
    fn both_seek_paths_agree() {
        let build = |limited: bool| {
            let b = ExecutionConfigBuilder::new().with_memory_size(100);
            if limited {
                b.with_max_steps(1_000_000).build()
            } else {
                b.build()
            }
        };
        for src in [">+>+>+[<]", "+>+>+><<<[>]", "[>]", "[<]", "+>+>[<]"] {
            let mut seen = Vec::new();
            for limited in [false, true] {
                let config = build(limited);
                let instructions = parse(src).unwrap();
                let program = optimize_with_cell_model(&instructions, *config.cell_model());
                let (mut i, mut o) = (StringIo::empty(), StringIo::empty());
                let stats = interpret_optimized(&program, config, &mut i, &mut o).unwrap();
                seen.push((stats.total_steps.get(), stats.peak_memory_used.get()));
            }
            assert_eq!(
                seen[0], seen[1],
                "{src}: unlimited={:?} limited={:?}",
                seen[0], seen[1]
            );
        }
    }

    /// Input that blocks, like a terminal or a slow pipe.
    struct SlowInput;
    impl crate::io::BfInput for SlowInput {
        fn read_byte(&mut self) -> std::io::Result<Option<u8>> {
            std::thread::sleep(std::time::Duration::from_millis(5));
            Ok(Some(b'x'))
        }
    }

    /// Output that blocks, like a pipe whose reader is slow.
    struct SlowOutput;
    impl crate::io::BfOutput for SlowOutput {
        fn write_byte(&mut self, _byte: u8) -> std::io::Result<()> {
            std::thread::sleep(std::time::Duration::from_millis(5));
            Ok(())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// `.` brings the check forward by at most [`OUTPUT_CHECK_WITHIN`] rather
    /// than demanding one outright, so a blocked write delays the deadline by a
    /// bounded number of writes instead of defeating the sampling. Without any
    /// arming on `.` the check waits a full TIME_CHECK_INTERVAL -- ~512 writes
    /// for this loop, seconds at 5ms each.
    #[test]
    fn timeout_fires_while_blocked_on_output() {
        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_timeout_ms(20)
            .build();
        let instructions = parse("+[.]").unwrap();
        let program = optimize_with_cell_model(&instructions, *config.cell_model());
        let (mut input, mut output) = (StringIo::empty(), SlowOutput);

        let started = std::time::Instant::now();
        let err = interpret_optimized(&program, config, &mut input, &mut output).unwrap_err();
        let elapsed = started.elapsed();

        assert!(
            matches!(err, BfError::ExecutionTimeout { .. }),
            "got {err:?}"
        );
        assert!(
            elapsed < std::time::Duration::from_millis(1500),
            "a 20ms timeout took {elapsed:?} while blocked on output"
        );
    }

    /// An echo loop blocked on input spends almost no steps and almost all of
    /// its wall time -- the case [`arm_time_check`] exists for. Measured against
    /// a pipe fed one byte per 300ms, a build without it did not stop within 15s
    /// of a 500ms deadline.
    ///
    /// The elapsed-time assertion is the point here, not the error type: without
    /// the arming the timeout still fires, just far too late.
    #[test]
    fn timeout_fires_while_blocked_on_input() {
        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_timeout_ms(20)
            .build();
        // `,[,]`, not `,[.,]`: with a `.` in the loop the *output* arming
        // would bring the check forward and the test would pass whether or not
        // `,` armed at all. Reads only, so it isolates the input path.
        let instructions = parse(",[,]").unwrap();
        let program = optimize_with_cell_model(&instructions, *config.cell_model());
        let (mut input, mut output) = (SlowInput, StringIo::empty());

        let started = std::time::Instant::now();
        let err = interpret_optimized(&program, config, &mut input, &mut output).unwrap_err();
        let elapsed = started.elapsed();

        assert!(
            matches!(err, BfError::ExecutionTimeout { .. }),
            "got {err:?}"
        );
        assert!(
            elapsed < std::time::Duration::from_millis(1000),
            "a 20ms timeout took {elapsed:?} while blocked on input"
        );
    }

    /// The wall clock is now sampled rather than read every instruction, so
    /// this pins that a timeout still stops a program whose step count moves in
    /// jumps: it spins forever with two ~400-cell seeks per iteration.
    ///
    /// A step limit is armed alongside the timeout, far above what 20ms of this
    /// program needs, purely so a regression fails instead of hanging. Stop
    /// updating `next_time_check` and the clock is read once, at step 0, and
    /// never again -- with only a timeout armed this spins forever, and
    /// `cargo test` has no per-test timeout, so CI wedges rather than reporting.
    /// Asserting elapsed wall time does not help: the assert is after the call,
    /// and the call is what fails to return. The backstop terminates it, and
    /// then the error type is the assertion that catches it.
    ///
    /// It does not discriminate between sampling schemes -- a `% INTERVAL == 0`
    /// test passes it too, because instructions still walk the count through
    /// every value between seeks. See `Limits::next_time_check` for why the
    /// threshold is preferred anyway.
    #[test]
    fn timeout_fires_when_the_step_count_moves_in_jumps() {
        let config = ExecutionConfigBuilder::new()
            .with_memory_size(1000)
            .with_timeout_ms(20)
            // Sized to this program: ~800 steps an iteration, so it gives up
            // after about a second. It is the hang guard, not a limit under
            // test -- if the program below changes, this wants resizing.
            .with_max_steps(2_000_000_000)
            .build();
        // cells 1..=400 non-zero, sentinels at 0 and 401; the body seeks right
        // to 401, then left to 0, then steps back onto cell 1 and repeats.
        let src = format!(">{}{}[[>]<[<]>]", "+>".repeat(400), "<".repeat(400));
        let err = run_opt(&src, config).unwrap_err();
        assert!(
            matches!(err, BfError::ExecutionTimeout { .. }),
            "expected the timeout to stop this, got {err:?}"
        );
    }

    /// A seek that lands on its zero cell with the step budget exactly spent has
    /// finished, and must not be failed for a step it did not take.
    ///
    /// This exercises the scanning path, which is the one that runs when the
    /// zero is inside the window. It does *not* discriminate the fall-through
    /// loop's ordering -- that loop tests for termination before consulting the
    /// limit, deliberately, but reaching it with a budget that expires exactly
    /// on the zero cell needs an unbounded tape that grows into the zero, and I
    /// could not construct one that reliably distinguishes the two orders.
    #[test]
    fn a_seek_that_just_fits_its_budget_completes() {
        // cells 1..=3 non-zero with a zero at 4; `[>]` from cell 1 walks three
        // cells. Six instructions precede it, and the seek is charged one step
        // for itself plus one per cell walked.
        let src = ">+>+>+<<[>]";
        let exact = run_optimized(
            src,
            ExecutionConfigBuilder::new().with_memory_size(100).build(),
        )
        .unwrap()
        .0
        .total_steps
        .get();

        let cfg = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_max_steps(exact)
            .build();
        assert!(
            run_opt(src, cfg).is_ok(),
            "a seek needing exactly {exact} steps must not fail at {exact}"
        );
    }

    /// A seek walking off the left of the tape stops at the access, not at the
    /// move: `[<]` from cell 0 puts the cursor at -1, which is a legal position,
    /// and the seek's own read of that cell is what fails.
    #[test]
    fn a_seek_off_the_tape_fails_at_the_read() {
        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_max_steps(1000)
            .build();
        let err = run_opt("+[<]", config).unwrap_err();
        assert!(
            matches!(err, BfError::MemoryOutOfBounds { .. }),
            "got {err:?}"
        );
    }

    /// A MultiplyAdd target more than one cell past the end of a fixed tape used to
    /// index past `memory.len()` and panic: the bounds check stepped the pointer only
    /// once, which cannot reach an offset of 2.
    #[test]
    fn test_multiply_add_beyond_tape_end_errors_instead_of_panicking() {
        let source = format!("{}+++[->>+<<]", ">".repeat(98));
        let result = run_optimized(
            &source,
            ExecutionConfigBuilder::new().with_memory_size(100).build(),
        );
        assert!(
            matches!(result, Err(BfError::MemoryOutOfBounds { .. })),
            "expected MemoryOutOfBounds, got {:?}",
            result
        );
    }

    /// The same shape on an unbounded tape must grow far enough to cover the target.
    #[test]
    fn test_multiply_add_grows_unbounded_memory_to_reach_target() {
        let source = format!("{}+++[->>+<<]", ">".repeat(8));
        let config = ExecutionConfigBuilder::new()
            .with_unbounded_memory(10, 1000)
            .unwrap()
            .build();
        assert!(run_optimized(&source, config).is_ok());
    }

    /// A loop with an empty body executes no instructions, so limits have to be
    /// checked per iteration or `[]` spins forever.
    #[test]
    fn test_empty_loop_respects_step_limit() {
        let result = run_optimized(
            "+[]",
            ExecutionConfigBuilder::new()
                .with_memory_size(100)
                .with_max_steps(1000)
                .build(),
        );
        assert!(matches!(result, Err(BfError::StepLimitExceeded { .. })));
    }

    /// The optimized path used to ignore `timeout_ms` entirely.
    #[test]
    fn test_timeout_is_enforced() {
        let result = run_optimized(
            "+[.]", // Output blocks fusion, so this stays a real infinite loop
            ExecutionConfigBuilder::new()
                .with_memory_size(100)
                .with_timeout_ms(20)
                .build(),
        );
        assert!(matches!(result, Err(BfError::ExecutionTimeout { .. })));
    }

    /// The tape contract: moving the cursor off the tape is legal on its own,
    /// and only becomes an error when something uses it. `allow_negative_pointer`
    /// used to govern this, by making `<` at cell 0 *stay* at cell 0 -- which
    /// silently aliased cell -1 onto cell 0 and let a write land on live data.
    #[test]
    fn cursor_may_leave_the_tape_but_not_be_used_there() {
        let cfg = || ExecutionConfigBuilder::new().with_memory_size(100).build();

        // Off the left and back, touching nothing: fine.
        assert!(run_optimized("<>", cfg()).is_ok());
        // Off the right and back, further than the tape is long: also fine.
        assert!(run_optimized(&format!("{}{}", ">".repeat(150), "<".repeat(150)), cfg()).is_ok());
        // Resting off the tape without using it: still fine.
        assert!(run_optimized("<", cfg()).is_ok());

        // Using it is not. This is the case that used to corrupt cell 0.
        assert!(matches!(
            run_optimized("<+", cfg()),
            Err(BfError::MemoryOutOfBounds { .. })
        ));
        assert!(matches!(
            run_optimized(&format!("{}+", ">".repeat(150)), cfg()),
            Err(BfError::MemoryOutOfBounds { .. })
        ));
    }

    /// `[+]` reaches zero only by wrapping past 255, which checked cells reject.
    /// Folding it to a store of 0 would disagree with the debug interpreter.
    #[test]
    fn test_increment_clear_loop_still_overflows_under_checked_cells() {
        let result = run_optimized(
            "+++++[+]",
            ExecutionConfigBuilder::new()
                .with_memory_size(100)
                .with_checked_cells()
                .build(),
        );
        assert!(matches!(result, Err(BfError::CellOverflow { .. })));
    }
}
