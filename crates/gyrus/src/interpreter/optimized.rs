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
use crate::types::MemoryAddress;

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
/// - Hooks are not yet supported (future enhancement)
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
    stats.peak_memory_used = MemoryAddress::new(state.peak_pointer + 1);
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
#[inline]
fn check_limits(state: &VmState, limits: &Limits) -> Result<()> {
    if let Some(max_steps) = limits.max_steps
        && state.step_count.get() >= max_steps
    {
        return Err(BfError::StepLimitExceeded {
            limit: max_steps,
            actual_steps: state.step_count,
            hint: "Optimized interpreter counts 1 step per optimized instruction, \
                   plus 1 for each cell a seek walks over"
                .to_string(),
            source_location: None,
            instruction_index: state.step_count.get() as usize,
        });
    }

    if let Some(timeout_ms) = limits.timeout_ms
        && limits.start_time.elapsed().as_millis() as u64 > timeout_ms
    {
        return Err(BfError::ExecutionTimeout {
            limit_ms: timeout_ms,
            actual_steps: Some(state.step_count),
            hint: format!(
                "Program exceeded {}ms timeout after executing {} optimized instructions. \
                 Try increasing the timeout with --timeout {}.",
                timeout_ms,
                state.step_count.get(),
                timeout_ms * 2
            ),
        });
    }

    Ok(())
}

/// Step and time limits, resolved once so the hot loop does not re-read config.
struct Limits {
    max_steps: Option<u64>,
    timeout_ms: Option<u64>,
    start_time: std::time::Instant,
}

impl Limits {
    fn from_config(config: &ExecutionConfig) -> Self {
        Self {
            max_steps: config.max_steps(),
            timeout_ms: config.timeout_ms(),
            start_time: std::time::Instant::now(),
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
            state.pointer.get().saturating_sub(budget as usize)
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
            let ptr = state.pointer.get();
            // See Right for the fused-run argument. It bites harder here:
            // behavior() hands out a &dyn CellBehavior, so each unit step is an
            // indirect call the compiler cannot inline into the single
            // wrapping_add it wraps. Checked cells keep the per-step loop, which
            // is what reports the overflow at the right value.
            if WRAPPING {
                // Bind the cell once: `m[p] = m[p].wrapping_add(n)` is two Index
                // calls and LLVM does not merge them here (measured 5.6% of hanoi,
                // a third of whose instructions are cell arithmetic).
                let cell = &mut state.memory[ptr];
                *cell = cell.wrapping_add(*n);
                return Ok(ExecutionFlow::Continue);
            }
            // Resolve the trait object once rather than per unit increment
            let cells = config.cell_model().behavior();
            for _ in 0..*n {
                cells.try_increment(
                    &mut state.memory[ptr],
                    state.step_count,
                    None, // No debug info in optimized path
                )?;
            }
            Ok(ExecutionFlow::Continue)
        }

        OptimizedInstruction::Sub(n, _range) => {
            let ptr = state.pointer.get();
            if WRAPPING {
                let cell = &mut state.memory[ptr];
                *cell = cell.wrapping_sub(*n);
                return Ok(ExecutionFlow::Continue);
            }
            let cells = config.cell_model().behavior();
            for _ in 0..*n {
                cells.try_decrement(
                    &mut state.memory[ptr],
                    state.step_count,
                    None, // No debug info
                )?;
            }
            Ok(ExecutionFlow::Continue)
        }

        // Fused pointer movements
        OptimizedInstruction::Right(n, _range) => {
            // The optimizer fused `>>>>` into Right(4); stepping the pointer one
            // cell at a time here would hand that back. When the whole run lands
            // in bounds it is one add, and nothing in the per-step path could
            // have fired. Anything that could error or grow memory falls through
            // to the original loop, so error semantics are unchanged.
            // memory.len() is the right limit for both models: Fixed never
            // resizes, so len is its size; Unbounded may grow, and staying under
            // len is what says this move needs no growth (len <= max_size).
            let start = state.pointer.get();
            if let Some(end) = start.checked_add(*n)
                && end < state.memory.len()
            {
                state.pointer = MemoryAddress::new(end);
                state.peak_pointer = state.peak_pointer.max(end);
                return Ok(ExecutionFlow::Continue);
            }
            // One max() per fused move rather than per unit step
            for _ in 0..*n {
                // Use memory model's increment logic
                state.memory_model.try_increment_pointer(
                    &mut state.pointer,
                    &mut state.memory,
                    state.step_count,
                    None, // No debug info in optimized path
                    0,    // Instruction index not used without debug info
                )?;
            }
            state.peak_pointer = state.peak_pointer.max(state.pointer.get());
            Ok(ExecutionFlow::Continue)
        }

        OptimizedInstruction::Left(n, _range) => {
            // `start >= n` means every intermediate step had a non-zero pointer
            // on entry, which is exactly the condition under which the per-step
            // path cannot error. Underflow falls through to it unchanged.
            let start = state.pointer.get();
            if start >= *n {
                state.pointer = MemoryAddress::new(start - *n);
                return Ok(ExecutionFlow::Continue);
            }
            for _ in 0..*n {
                // Use memory model's decrement logic
                state.memory_model.try_decrement_pointer(
                    &mut state.pointer,
                    &state.memory,
                    config.allow_negative_pointer(),
                    state.step_count,
                    None, // No debug info
                    0,
                )?;
            }
            Ok(ExecutionFlow::Continue)
        }

        // I/O operations
        OptimizedInstruction::Output(_range) => {
            let ptr = state.pointer.get();
            let byte = state.memory[ptr];
            output.write_byte(byte).map_err(|source| BfError::IoError {
                operation: "writing output".to_string(),
                instruction_index: Some(state.step_count.into()),
                source,
            })?;
            state.bytes_written += 1;
            Ok(ExecutionFlow::Continue)
        }

        OptimizedInstruction::Input(_range) => {
            let ptr = state.pointer.get();
            match input.read_byte() {
                Ok(Some(byte)) => {
                    state.memory[ptr] = byte;
                    state.bytes_read += 1;
                    Ok(ExecutionFlow::Continue)
                }
                Ok(None) => {
                    // EOF - handle according to config
                    match config.eof_behavior() {
                        crate::config::EofBehavior::SetZero => {
                            state.memory[ptr] = 0;
                            Ok(ExecutionFlow::Continue)
                        }
                        crate::config::EofBehavior::SetNegOne => {
                            state.memory[ptr] = 255;
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
            let ptr = state.pointer.get();
            state.memory[ptr] = 0;
            Ok(ExecutionFlow::Continue)
        }

        OptimizedInstruction::SeekRight(_range) => {
            // [>] → seek to next zero cell.
            //
            // Same family as Right/Add: the loop below advances one cell per
            // iteration through the memory model. A seek is a search, so it
            // cannot collapse to a single add -- but it is a search for a zero
            // byte in a slice, which the standard library scans far faster than
            // a call per cell.
            //
            // The seek is charged one step per cell it examines. The unfused
            // `[>]` costs a step per cell, so charging one per fused instruction
            // would let a runaway seek outrun --max-steps forever: check_limits
            // would compare a step_count that never moved.
            //
            // The scan is clamped to what the step limit still allows rather
            // than switched off when a limit is set. Both give the same answer,
            // but branching away from the scan made --max-steps a 10x slowdown
            // on seek-heavy programs -- a limit should bound a run, not change
            // how it executes.
            let start = state.pointer.get();
            let hi = seek_window_end(CHECK_LIMITS, state, limits);
            if let Some(offset) = state.memory[start..hi].iter().position(|&b| b == 0) {
                let end = start + offset;
                state.pointer = MemoryAddress::new(end);
                state.peak_pointer = state.peak_pointer.max(end);
                state.step_count += offset as u64;
                return Ok(ExecutionFlow::Continue);
            }
            // No zero within the window. Every cell from `start` to `hi` is
            // non-zero, so walking them could only have succeeded; skip to the
            // last rather than scanning them again, and let the loop decide what
            // happens next -- fire the step limit, grow an unbounded tape, or
            // report a fixed one's overrun.
            if hi > start {
                state.step_count += (hi - 1 - start) as u64;
                state.pointer = MemoryAddress::new(hi - 1);
            }
            loop {
                let ptr = state.pointer.get();
                if state.memory[ptr] == 0 {
                    break;
                }
                // A seek is a loop: honour step/time limits so it cannot spin forever
                if CHECK_LIMITS {
                    check_limits(state, limits)?;
                }

                // Move right
                state.memory_model.try_increment_pointer(
                    &mut state.pointer,
                    &mut state.memory,
                    state.step_count,
                    None,
                    0,
                )?;
                state.step_count += 1;
            }
            state.peak_pointer = state.peak_pointer.max(state.pointer.get());
            Ok(ExecutionFlow::Continue)
        }

        OptimizedInstruction::SeekLeft(_range) => {
            // [<] → seek to previous zero cell. See SeekRight for why this is a
            // scan rather than a per-cell call, why it is charged per cell, and
            // why the window is clamped rather than abandoned under limits.
            let start = state.pointer.get();
            let lo = seek_window_start(CHECK_LIMITS, state, limits);
            if let Some(found) = state.memory[lo..=start].iter().rposition(|&b| b == 0) {
                // rposition indexes the window, so shift it back to the tape.
                let end = lo + found;
                state.pointer = MemoryAddress::new(end);
                state.step_count += (start - end) as u64;
                return Ok(ExecutionFlow::Continue);
            }
            if start > lo {
                state.step_count += (start - lo) as u64;
                state.pointer = MemoryAddress::new(lo);
            }
            loop {
                let ptr = state.pointer.get();
                if state.memory[ptr] == 0 {
                    break;
                }
                if CHECK_LIMITS {
                    check_limits(state, limits)?;
                }

                // Move left
                state.memory_model.try_decrement_pointer(
                    &mut state.pointer,
                    &state.memory,
                    config.allow_negative_pointer(),
                    state.step_count,
                    None,
                    0,
                )?;
                state.step_count += 1;
            }
            Ok(ExecutionFlow::Continue)
        }

        OptimizedInstruction::MultiplyAdd(operations, _range) => {
            // Generalized multiplication loop
            // For each (offset, multiplier): cell[ptr + offset] += cell[ptr] * multiplier
            // Then: cell[ptr] = 0

            let ptr = state.pointer.get();
            let source_value = state.memory[ptr];

            // Early exit if source is already zero (optimization)
            if source_value == 0 {
                return Ok(ExecutionFlow::Continue);
            }

            for (offset, multiplier) in operations {
                // Calculate target pointer
                let target_ptr = if *offset >= 0 {
                    ptr + (*offset as usize)
                } else {
                    ptr.checked_sub((-offset) as usize).ok_or_else(|| {
                        BfError::MemoryOutOfBounds {
                            instruction_index: state.step_count.into(),
                            attempted: ptr as isize + offset,
                            max: crate::types::MemorySize::new(state.memory.len()),
                            memory_dump: None,
                            source_location: None,
                            loop_call_stack: None,
                            hint: "Pointer underflow in MultiplyAdd operation".to_string(),
                        }
                    })?
                };

                // Walk the pointer out to the target so the memory model decides
                // what happens at the boundary: a fixed tape raises
                // MemoryOutOfBounds, an unbounded tape grows to cover the target.
                // Stepping one cell at a time (rather than once, as before) is what
                // makes offsets larger than 1 safe - a single step left the tape
                // short of the target and the write below indexed past its end.
                if target_ptr >= state.memory.len() {
                    while state.pointer.get() < target_ptr {
                        state.memory_model.try_increment_pointer(
                            &mut state.pointer,
                            &mut state.memory,
                            state.step_count,
                            None,
                            0,
                        )?;
                    }
                    state.peak_pointer = state.peak_pointer.max(state.pointer.get());
                    // Restore original pointer
                    state.pointer = MemoryAddress::new(ptr);
                }

                // Perform the multiplication and addition
                let target_value = state.memory[target_ptr];
                let delta = (source_value as i32) * multiplier;
                let new_value = (target_value as i32 + delta) as u8; // Wrapping
                state.memory[target_ptr] = new_value;
            }

            // Zero the source cell
            state.memory[ptr] = 0;
            Ok(ExecutionFlow::Continue)
        }

        // General loops (not optimized, recursively execute)
        OptimizedInstruction::Loop(body, _range) => {
            state.loop_depth += 1;

            // Loop while current cell is non-zero
            while state.memory[state.pointer.get()] != 0 {
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
        let err = run_opt(">>>>>>>>>>", config).unwrap_err();
        assert!(
            matches!(err, BfError::MemoryOutOfBounds { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn fused_left_below_zero_still_reports_out_of_bounds() {
        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        // Right(2) then Left(4): the third unit step is the one that underflows.
        let err = run_opt(">><<<<", config).unwrap_err();
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
            MemoryAddress::new(3),
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

    /// The step limit must be able to stop a seek that never terminates.
    /// `[<]` at cell 0 with a negative pointer allowed leaves the pointer where
    /// it is, so this loop makes no progress at all; before seeks were charged
    /// per cell it hung forever instead of hitting the limit.
    #[test]
    fn a_non_terminating_seek_still_hits_the_step_limit() {
        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_negative_pointer(true)
            .with_max_steps(1000)
            .build();
        let err = run_opt("+[<]", config).unwrap_err();
        assert!(
            matches!(err, BfError::StepLimitExceeded { .. }),
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

    /// `allow_negative_pointer` was hardcoded to `false` on the optimized path, so
    /// `<` at cell 0 errored where the debug interpreter allowed it.
    #[test]
    fn test_left_honours_allow_negative_pointer() {
        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_negative_pointer(true)
            .build();
        assert!(run_optimized("<", config).is_ok());
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
