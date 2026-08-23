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

    // Execute the optimized program
    execute_block(instructions, &mut state, &config, input, output)?;

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
    stats.memory_allocated = crate::types::MemorySize::new(state.memory.capacity());
    stats.loop_iterations = state.loop_iterations;
    stats.bytes_read = state.bytes_read;
    stats.bytes_written = state.bytes_written;
    stats.cells_modified = ExecutionStats::count_modified_cells(&state.memory);

    Ok(stats)
}

/// Execute a block of optimized instructions
fn execute_block<I: BfInput, O: BfOutput>(
    instructions: &[OptimizedInstruction],
    state: &mut VmState,
    config: &ExecutionConfig,
    input: &mut I,
    output: &mut O,
) -> ExecutionResult {
    for instruction in instructions {
        // Check execution limits
        if let Some(max_steps) = config.max_steps()
            && state.step_count.get() >= max_steps
        {
            return Err(BfError::StepLimitExceeded {
                limit: max_steps,
                actual_steps: state.step_count,
                hint: "Optimized interpreter counts each optimized instruction as 1 step"
                    .to_string(),
                source_location: None,
                instruction_index: state.step_count.get() as usize,
            });
        }

        // Execute the instruction
        execute_instruction(instruction, state, config, input, output)?;

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
    input: &mut I,
    output: &mut O,
) -> ExecutionResult {
    match instruction {
        // Fused arithmetic operations
        OptimizedInstruction::Add(n, _range) => {
            let ptr = state.pointer.get();
            // Perform N increments in one operation
            for _ in 0..*n {
                config.cell_model().behavior().try_increment(
                    &mut state.memory[ptr],
                    state.step_count,
                    None, // No debug info in optimized path
                )?;
            }
            Ok(ExecutionFlow::Continue)
        }

        OptimizedInstruction::Sub(n, _range) => {
            let ptr = state.pointer.get();
            // Perform N decrements in one operation
            for _ in 0..*n {
                config.cell_model().behavior().try_decrement(
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
                    false, // Don't allow negative pointers
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
            // [-] or [+] → set cell to 0
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

                // Move left
                state.memory_model.try_decrement_pointer(
                    &mut state.pointer,
                    &state.memory,
                    false,
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

                // Bounds check
                if target_ptr >= state.memory.len() {
                    // Try to grow memory if unbounded model
                    state.memory_model.try_increment_pointer(
                        &mut state.pointer,
                        &mut state.memory,
                        state.step_count,
                        None,
                        0,
                    )?;
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
                state.loop_iterations += 1;
                execute_block(body, state, config, input, output)?;
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
}
