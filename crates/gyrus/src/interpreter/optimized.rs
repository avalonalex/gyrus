//! Optimized interpreter for executing optimized IR.
//!
//! This module provides a specialized interpreter that executes `OptimizedInstruction`
//! sequences, achieving significant performance improvements over the standard interpreter.
//!
//! Key differences from the standard interpreter:
//! - Executes fused operations in single steps (e.g., Add(5) vs 5× IncrementValue)
//! - Pattern-recognized loops execute as single operations (e.g., Zero, MultiplyAdd)
//! - No debug symbol support (faster execution path)
//! - Simplified step counting (each optimized instruction = 1 step)

use super::state::{ExecutionFlow, ExecutionResult, VmState};
use crate::config::ExecutionConfig;
use crate::error::{BfError, Result};
use crate::io::{BfInput, BfOutput};
use crate::optimizer::OptimizedInstruction;
use crate::stats::ExecutionStats;

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
/// - Step counting is approximate (each optimized instruction = 1 step)
/// - Hooks are not yet supported (future enhancement)
///
/// # Arguments
///
/// * `instructions` - Optimized instruction sequence from `optimize()`
/// * `config` - Execution configuration (memory model, limits, etc.)
/// * `input` - Input source
/// * `output` - Output destination
///
/// # Returns
///
/// Execution statistics on success, or an error if execution fails.
pub fn interpret_optimized<I: BfInput, O: BfOutput>(
    instructions: &[OptimizedInstruction],
    config: ExecutionConfig,
    input: &mut I,
    output: &mut O,
) -> Result<ExecutionStats> {
    let mut state = VmState::new(*config.memory_model());
    let mut stats = ExecutionStats::default();
    let limits = Limits::from_config(&config);

    // Execute the optimized program
    execute_block(instructions, &mut state, &config, &limits, input, output)?;

    // Collect final statistics.
    //
    // The optimized path runs without hooks, so these come from counters on
    // VmState rather than from `StatsTracker`. Two of them are in *optimized*
    // units and cannot match the debug interpreter: `total_steps` counts one
    // step per optimized instruction, and `loop_iterations` counts only loops
    // the optimizer left as loops — a `[-]` fused into `Zero`, or a `[>]` fused
    // into `SeekRight`, no longer iterates. The rest (peak pointer, cells
    // modified, bytes in and out) describe the program's actual behavior and do
    // match.
    stats.total_steps = state.step_count;
    stats.peak_memory_used = crate::types::MemoryAddress::new(state.peak_pointer + 1);
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
            hint: "Optimized interpreter counts each optimized instruction as 1 step".to_string(),
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
fn execute_block<I: BfInput, O: BfOutput>(
    instructions: &[OptimizedInstruction],
    state: &mut VmState,
    config: &ExecutionConfig,
    limits: &Limits,
    input: &mut I,
    output: &mut O,
) -> ExecutionResult {
    for instruction in instructions {
        // Check execution limits
        check_limits(state, limits)?;

        // Execute the instruction
        execute_instruction(instruction, state, config, limits, input, output)?;

        // Increment step count (each optimized instruction = 1 step)
        state.step_count = crate::types::StepCount::new(state.step_count.get() + 1);
    }

    Ok(ExecutionFlow::Continue)
}

/// Execute a single optimized instruction
#[inline]
fn execute_instruction<I: BfInput, O: BfOutput>(
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
            // [>] → seek to next zero cell
            loop {
                let ptr = state.pointer.get();
                if state.memory[ptr] == 0 {
                    break;
                }
                // A seek is a loop: honour step/time limits so it cannot spin forever
                check_limits(state, limits)?;

                // Move right
                state.memory_model.try_increment_pointer(
                    &mut state.pointer,
                    &mut state.memory,
                    state.step_count,
                    None,
                    0,
                )?;
            }
            state.peak_pointer = state.peak_pointer.max(state.pointer.get());
            Ok(ExecutionFlow::Continue)
        }

        OptimizedInstruction::SeekLeft(_range) => {
            // [<] → seek to previous zero cell
            loop {
                let ptr = state.pointer.get();
                if state.memory[ptr] == 0 {
                    break;
                }
                check_limits(state, limits)?;

                // Move left
                state.memory_model.try_decrement_pointer(
                    &mut state.pointer,
                    &state.memory,
                    config.allow_negative_pointer(),
                    state.step_count,
                    None,
                    0,
                )?;
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
                    state.pointer = crate::types::MemoryAddress::new(ptr);
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
                check_limits(state, limits)?;
                // The `[` condition check is itself a step, which is also what lets
                // the step limit make progress on an empty body.
                state.step_count = crate::types::StepCount::new(state.step_count.get() + 1);
                state.loop_iterations += 1;
                execute_block(body, state, config, limits, input, output)?;
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
    use crate::io::StringIo;
    use crate::optimizer::{OptimizedInstruction, SourceRange, optimize};
    use crate::parse;

    #[test]
    fn test_optimized_add() {
        let instructions = vec![OptimizedInstruction::Add(5, SourceRange::new(0, 5))];
        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        let mut input = StringIo::empty();
        let mut output = StringIo::empty();

        let stats = interpret_optimized(&instructions, config, &mut input, &mut output).unwrap();

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

        let stats =
            interpret_optimized(&optimized.instructions, config, &mut input, &mut output).unwrap();

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

        let stats =
            interpret_optimized(&optimized.instructions, config, &mut input, &mut output).unwrap();

        assert_eq!(stats.bytes_written, 2, "two Output instructions ran");
        assert_eq!(stats.bytes_read, 0, "program reads no input");
        assert_eq!(
            stats.cells_modified, 3,
            "cells 0, 1 and 2 hold non-zero values"
        );
        assert_eq!(
            stats.peak_memory_used,
            crate::types::MemoryAddress::new(3),
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

        let stats =
            interpret_optimized(&optimized.instructions, config, &mut input, &mut output).unwrap();

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

        let stats =
            interpret_optimized(&optimized.instructions, config, &mut input, &mut output).unwrap();

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

        let stats =
            interpret_optimized(&optimized.instructions, config, &mut input, &mut output).unwrap();

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

        let _stats =
            interpret_optimized(&optimized.instructions, config, &mut input, &mut output).unwrap();

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

        let stats =
            interpret_optimized(&optimized.instructions, config, &mut input, &mut output).unwrap();

        // Should be 3 steps: Add(5), Right(1), Add(3), Right(1), Add(2)
        // Wait, that's 5 steps. Let me recalculate:
        // Optimized IR: Add(5), Right(1), Add(3), Right(1), Add(2) = 5 instructions
        assert_eq!(stats.total_steps.get(), 5);
    }

    #[test]
    fn test_optimized_seek_pattern() {
        // Seek pattern: [>] should execute as single operation
        let source = "+++++[>]";
        let instructions = parse(source).unwrap();
        let optimized = optimize(&instructions);

        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        let mut input = StringIo::empty();
        let mut output = StringIo::empty();

        let stats =
            interpret_optimized(&optimized.instructions, config, &mut input, &mut output).unwrap();

        // Should be 2 steps: Add(5) + SeekRight
        assert_eq!(stats.total_steps.get(), 2);
    }

    fn run_optimized(
        source: &str,
        config: ExecutionConfig,
    ) -> crate::error::Result<ExecutionStats> {
        let instructions = parse(source).unwrap();
        let optimized = optimize(&instructions);
        let mut input = StringIo::empty();
        let mut output = StringIo::empty();
        interpret_optimized(&optimized.instructions, config, &mut input, &mut output)
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
