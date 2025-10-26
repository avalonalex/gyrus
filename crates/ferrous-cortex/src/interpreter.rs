use crate::config::{EofBehavior, ExecutionConfig};
use crate::debug::DebugInfo;
use crate::error::{BfError, Result};
use crate::instruction::Instruction;
use crate::io::{BfInput, BfOutput, StdInput, StdOutput};
use crate::stats::ExecutionStats;
use crate::types::{MemoryAddress, MemorySize, StepCount};
use std::io;

use crate::config::MemoryModel;

/// Virtual machine state for BrainFuck execution
struct VmState<'a> {
    /// Memory tape (array of cells)
    memory: Vec<u8>,
    /// Current memory pointer position
    pointer: MemoryAddress,
    /// Number of steps executed so far
    step_count: StepCount,
    /// Execution statistics
    stats: ExecutionStats,
    /// Start time for timeout tracking (if enabled)
    start_time: Option<std::time::Instant>,
    /// Memory model that dictates how memory operations behave
    memory_model: MemoryModel,
    /// Debug information for mapping step indices to source locations
    debug_info: Option<&'a DebugInfo>,
}

impl<'a> VmState<'a> {
    /// Create a new VM state with the given memory model and optional start time
    fn new(
        memory_model: MemoryModel,
        start_time: Option<std::time::Instant>,
        debug_info: Option<&'a DebugInfo>,
    ) -> Self {
        let memory_size = memory_model.initial_size().get();
        Self {
            memory: vec![0u8; memory_size],
            pointer: MemoryAddress::new(0),
            step_count: StepCount::new(0),
            stats: ExecutionStats::new(),
            start_time,
            memory_model,
            debug_info,
        }
    }
}

/// Interpret and execute BrainFuck instructions with default configuration
///
/// This is a convenience function that uses default settings.
/// For custom configuration, use `interpret_with_config()`.
/// Discards execution statistics.
#[allow(dead_code)]
pub fn interpret(instructions: &[Instruction]) -> Result<()> {
    interpret_with_config(instructions, ExecutionConfig::default()).map(|_| ())
}

/// Interpret and execute BrainFuck instructions with custom I/O.
///
/// This is the primary interpreter function that allows custom input and output.
///
/// # Examples
///
/// ```rust
/// use ferrous_cortex::{parse, interpret_with_io, io::StringIo, ExecutionConfig};
///
/// // Echo program: reads input and outputs it
/// let instructions = parse(",[.,]")?;
/// let mut input = StringIo::new("Hi");
/// let mut output = StringIo::empty();
/// let stats = interpret_with_io(&instructions, ExecutionConfig::default(), &mut input, &mut output, None)?;
/// assert_eq!(output.output_string(), "Hi");
/// # Ok::<(), ferrous_cortex::BfError>(())
/// ```
pub fn interpret_with_io<I: BfInput, O: BfOutput>(
    instructions: &[Instruction],
    config: ExecutionConfig,
    input: &mut I,
    output: &mut O,
    debug_info: Option<&DebugInfo>,
) -> Result<ExecutionStats> {
    use std::time::Instant;

    let start_time = config.timeout_ms().map(|_| Instant::now());
    let mut state = VmState::new(*config.memory_model(), start_time, debug_info);

    execute_block(instructions, &mut state, &config, input, output)?;

    // Finalize stats
    state.stats.total_steps = state.step_count;
    state.stats.cells_modified = ExecutionStats::count_modified_cells(&state.memory);
    state.stats.memory_allocated = MemorySize::new(state.memory.len());

    Ok(state.stats)
}

/// Interpret and execute BrainFuck instructions with custom configuration.
///
/// This is a convenience function that uses stdin/stdout for I/O.
/// For custom I/O, use `interpret_with_io()`.
/// Returns execution statistics.
///
/// # Examples
///
/// ```rust,no_run
/// use ferrous_cortex::{parse, interpret_with_config, ExecutionConfig};
///
/// let instructions = parse("++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.")?;
/// let stats = interpret_with_config(&instructions, ExecutionConfig::default())?;
/// println!("Executed {} steps", stats.total_steps);
/// # Ok::<(), ferrous_cortex::BfError>(())
/// ```
pub fn interpret_with_config(
    instructions: &[Instruction],
    config: ExecutionConfig,
) -> Result<ExecutionStats> {
    let mut input = StdInput;
    let mut output = StdOutput;
    interpret_with_io(instructions, config, &mut input, &mut output, None)
}

/// Handle pointer increment based on memory model
#[inline]
fn increment_pointer(state: &mut VmState) -> Result<()> {
    state.memory_model.try_increment_pointer(
        &mut state.pointer,
        &mut state.memory,
        state.step_count,
        &mut state.stats.warnings,
        state.debug_info,
    )
}

/// Handle pointer decrement based on memory model
#[inline]
fn decrement_pointer(state: &mut VmState, allow_negative_pointer: bool) -> Result<()> {
    state.memory_model.try_decrement_pointer(
        &mut state.pointer,
        &state.memory,
        allow_negative_pointer,
        state.step_count,
        &mut state.stats.warnings,
        state.debug_info,
    )
}

fn execute_block<I: BfInput, O: BfOutput>(
    instructions: &[Instruction],
    state: &mut VmState,
    config: &ExecutionConfig,
    input: &mut I,
    output: &mut O,
) -> Result<()> {
    for instruction in instructions {
        // Check step limit
        state.step_count.increment();
        if let Some(max_steps) = config.max_steps()
            && state.step_count.get() > max_steps
        {
            return Err(BfError::StepLimitExceeded {
                limit: max_steps,
                actual_steps: state.step_count,
                hint: format!(
                    "Program executed {} steps, exceeding the limit of {}. \
                         This may indicate an infinite loop. Try increasing the limit with --max-steps {} \
                         or add breakpoints to debug.",
                    state.step_count.get(),
                    max_steps,
                    max_steps * 2
                ),
            });
        }

        // Check timeout
        if let Some(start) = &state.start_time
            && let Some(timeout_ms) = config.timeout_ms()
        {
            let elapsed = start.elapsed().as_millis() as u64;
            if elapsed > timeout_ms {
                return Err(BfError::ExecutionTimeout {
                    limit_ms: timeout_ms,
                    actual_steps: Some(state.step_count),
                    hint: format!(
                        "Program exceeded {}ms timeout after executing {} steps. \
                             Try increasing timeout with --timeout {} or optimize your BrainFuck code.",
                        timeout_ms,
                        state.step_count.get(),
                        timeout_ms * 2
                    ),
                });
            }
        }

        match instruction {
            Instruction::IncrementPointer => {
                increment_pointer(state)?;
                // Track peak memory usage
                if state.pointer.get() + 1 > state.stats.peak_memory_used.get() {
                    state.stats.peak_memory_used = MemoryAddress::new(state.pointer.get() + 1);
                }
            }
            Instruction::DecrementPointer => {
                decrement_pointer(state, config.allow_negative_pointer())?;
            }
            // Cell arithmetic: Delegated to CellModel (now configurable!)
            //
            // IMPORTANT: Cell arithmetic is NOW configurable via ExecutionConfig.cell_model().
            // Different models provide different overflow/underflow behaviors:
            // - U8Wrapping: 255+1=0, 0-1=255 (default, most compatible)
            // - U8Checked: Overflow/underflow returns error
            //
            // This is INDEPENDENT of MemoryModel, which only controls pointer movement.
            // See config.rs module docs for CellModel and MemoryModel orthogonality.
            // See validator.rs module docs for cell-model-aware validation.
            Instruction::IncrementValue => {
                config.cell_model().behavior().try_increment(
                    &mut state.memory[state.pointer.get()],
                    state.step_count,
                    &mut state.stats.warnings,
                    state.debug_info,
                )?;
            }
            Instruction::DecrementValue => {
                config.cell_model().behavior().try_decrement(
                    &mut state.memory[state.pointer.get()],
                    state.step_count,
                    &mut state.stats.warnings,
                    state.debug_info,
                )?;
            }
            Instruction::Output => {
                output
                    .write_byte(state.memory[state.pointer.get()])
                    .map_err(|source| BfError::IoError {
                        operation: "writing output".to_string(),
                        instruction_index: Some(state.step_count.into()),
                        source,
                    })?;
                output.flush().map_err(|source| BfError::IoError {
                    operation: "flushing output".to_string(),
                    instruction_index: Some(state.step_count.into()),
                    source,
                })?;
                state.stats.bytes_written += 1;
            }
            Instruction::Input => {
                match input.read_byte() {
                    Ok(Some(byte)) => {
                        state.memory[state.pointer.get()] = byte;
                        state.stats.bytes_read += 1;
                    }
                    Ok(None) => {
                        // Handle EOF based on configuration
                        match config.eof_behavior() {
                            EofBehavior::SetZero => {
                                state.memory[state.pointer.get()] = 0;
                            }
                            EofBehavior::SetNegOne => {
                                state.memory[state.pointer.get()] = 255; // -1 as u8
                            }
                            EofBehavior::NoChange => {
                                // Do nothing, leave cell as-is
                            }
                            EofBehavior::Error => {
                                return Err(BfError::IoError {
                                    operation: "reading input (EOF reached)".to_string(),
                                    instruction_index: Some(state.step_count.into()),
                                    source: io::Error::new(
                                        io::ErrorKind::UnexpectedEof,
                                        "EOF reached",
                                    ),
                                });
                            }
                        }
                    }
                    Err(source) => {
                        return Err(BfError::IoError {
                            operation: "reading input".to_string(),
                            instruction_index: Some(state.step_count.into()),
                            source,
                        });
                    }
                }
            }
            Instruction::Loop(body) => {
                while state.memory[state.pointer.get()] != 0 {
                    state.stats.loop_iterations += 1;
                    execute_block(body, state, config, input, output)?;
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ExecutionConfigBuilder, MEMORY_SIZE};
    use crate::parser::parse;

    #[test]
    fn test_memory_overflow() {
        let source = ">".repeat(30001); // Try to go beyond memory
        let instructions = parse(&source).unwrap();
        let config = ExecutionConfig::default();
        let result = interpret_with_config(&instructions, config);
        assert!(matches!(result, Err(BfError::MemoryOutOfBounds { .. })));
    }

    #[test]
    fn test_memory_underflow() {
        let source = "<"; // Try to go below 0
        let instructions = parse(source).unwrap();
        let config = ExecutionConfig::default();
        let result = interpret_with_config(&instructions, config);
        assert!(matches!(
            result,
            Err(BfError::MemoryOutOfBounds { attempted: -1, .. })
        ));
    }

    #[test]
    fn test_step_limit() {
        // Using test_utils for cleaner error testing
        use crate::test_utils::{configs, run_bf_with_config};

        let result = run_bf_with_config("+[+]", "", configs::with_step_limit(100));
        assert!(matches!(
            result,
            Err(BfError::StepLimitExceeded { limit: 100, .. })
        ));
    }

    #[test]
    fn test_custom_memory_size() {
        // Using test_utils for cleaner config testing
        use crate::test_utils::{configs, run_bf_with_config};

        let source = ">".repeat(101);
        let result = run_bf_with_config(&source, "", configs::small_memory());
        assert!(result.is_err());
        assert!(matches!(result, Err(BfError::MemoryOutOfBounds { .. })));
    }

    #[test]
    fn test_execution_timeout() {
        // Create a program that runs longer - moving pointer takes more time
        let source = "+[>+<]".repeat(1000); // Infinite loop with more instructions
        let instructions = parse(&source).unwrap();
        let config = ExecutionConfigBuilder::new()
            .with_memory_size(1000)
            .with_timeout_ms(100)
            .build(); // Smaller memory to hit bounds faster
        let result = interpret_with_config(&instructions, config);
        // Should fail with either timeout or memory bounds
        assert!(result.is_err());
    }

    #[test]
    fn test_memory_model_fixed() {
        // Fixed memory model should error on out-of-bounds access
        let source = ">".repeat(100); // Move right 100 times
        let instructions = parse(&source).unwrap();

        let config = ExecutionConfigBuilder::new().with_memory_size(50).build(); // Only 50 cells
        let result = interpret_with_config(&instructions, config);

        assert!(result.is_err());
        assert!(matches!(result, Err(BfError::MemoryOutOfBounds { .. })));
    }

    #[test]
    fn test_memory_model_unbounded_growth() {
        // Unbounded memory should grow as needed
        let source = format!("{}+.", ">".repeat(100)); // Move right 100 times
        let instructions = parse(&source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_unbounded_memory(10, 200)
            .unwrap()
            .build(); // Start small, allow growth

        let result = interpret_with_config(&instructions, config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_memory_model_unbounded_max_limit() {
        // Unbounded memory should still error at max limit
        let source = format!("{}+.", ">".repeat(150)); // Move right 150 times
        let instructions = parse(&source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_unbounded_memory(10, 100)
            .unwrap()
            .build(); // Max 100 cells

        let result = interpret_with_config(&instructions, config);
        assert!(result.is_err());
        assert!(matches!(result, Err(BfError::MemoryOutOfBounds { .. })));
    }

    #[test]
    fn test_memory_model_fixed_left_boundary() {
        // Fixed model should error when going below 0
        let source = "<"; // Move left from 0
        let instructions = parse(source).unwrap();

        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        let result = interpret_with_config(&instructions, config);

        assert!(result.is_err());
        assert!(matches!(result, Err(BfError::MemoryOutOfBounds { .. })));
    }

    #[test]
    fn test_stats_basic_counting() {
        // Test basic step counting
        let source = "+++>>--"; // 7 instructions
        let instructions = parse(source).unwrap();

        let config = ExecutionConfig::default();
        let stats = interpret_with_config(&instructions, config).unwrap();

        assert_eq!(stats.total_steps, StepCount::new(7));
        assert_eq!(stats.loop_iterations, 0);
        assert_eq!(stats.peak_memory_used, MemoryAddress::new(3)); // Moved to cell 2, so peak is 3 (0-based + 1)
    }

    #[test]
    fn test_stats_loop_iterations() {
        // Test loop iteration counting
        let source = "+++[>+<-]"; // Loop runs 3 times
        let instructions = parse(source).unwrap();

        let config = ExecutionConfig::default();
        let stats = interpret_with_config(&instructions, config).unwrap();

        assert_eq!(stats.loop_iterations, 3);
        assert!(stats.total_steps > StepCount::new(3)); // Should be more than just the setup
    }

    #[test]
    fn test_stats_io_tracking() {
        // Test I/O tracking
        let source = "++++++++++.>++."; // Output 2 bytes
        let instructions = parse(source).unwrap();

        let config = ExecutionConfig::default();
        let stats = interpret_with_config(&instructions, config).unwrap();

        assert_eq!(stats.bytes_written, 2);
        assert_eq!(stats.bytes_read, 0);
    }

    #[test]
    fn test_stats_memory_tracking() {
        // Test memory usage tracking
        let source = "+++>++>+"; // Use 3 cells
        let instructions = parse(source).unwrap();

        let config = ExecutionConfig::default();
        let stats = interpret_with_config(&instructions, config).unwrap();

        assert_eq!(stats.cells_modified, 3); // 3 non-zero cells
        assert_eq!(stats.peak_memory_used, MemoryAddress::new(3)); // Highest cell accessed + 1
    }

    #[test]
    fn test_stats_unbounded_allocation() {
        // Test memory allocation tracking for unbounded model
        let source = format!("{}+", ">".repeat(50)); // Move to cell 50
        let instructions = parse(&source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_unbounded_memory(10, 100)
            .unwrap()
            .build();
        let stats = interpret_with_config(&instructions, config).unwrap();

        assert_eq!(stats.peak_memory_used, MemoryAddress::new(51)); // Cell 50 + 1
        assert_eq!(stats.memory_allocated, MemorySize::new(51)); // Should have grown to 51 cells
        assert_eq!(stats.cells_modified, 1); // Only 1 non-zero cell
    }

    #[test]
    fn test_stats_nested_loops() {
        // Test nested loop iteration counting
        let source = "+++[>++[<.>-]<-]"; // Nested loops
        let instructions = parse(source).unwrap();

        let config = ExecutionConfig::default();
        let stats = interpret_with_config(&instructions, config).unwrap();

        // Outer loop runs 3 times, inner loop runs 2 times per outer iteration = 6
        assert_eq!(stats.loop_iterations, 3 + 6); // 3 outer + 6 inner
    }

    #[test]
    fn test_eof_behavior_config() {
        // Test that EOF behavior can be configured
        let config = ExecutionConfig::builder()
            .with_memory_size(MEMORY_SIZE)
            .with_eof_behavior(EofBehavior::SetZero)
            .build();
        assert!(matches!(config.eof_behavior(), EofBehavior::SetZero));

        let config = ExecutionConfig::builder()
            .with_memory_size(MEMORY_SIZE)
            .with_eof_behavior(EofBehavior::SetNegOne)
            .build();
        assert!(matches!(config.eof_behavior(), EofBehavior::SetNegOne));

        let config = ExecutionConfig::builder()
            .with_memory_size(MEMORY_SIZE)
            .with_eof_behavior(EofBehavior::NoChange)
            .build();
        assert!(matches!(config.eof_behavior(), EofBehavior::NoChange));

        let config = ExecutionConfig::builder()
            .with_memory_size(MEMORY_SIZE)
            .with_eof_behavior(EofBehavior::Error)
            .build();
        assert!(matches!(config.eof_behavior(), EofBehavior::Error));
    }

    #[test]
    fn test_eof_behavior_default() {
        // Test that default EOF behavior is SetZero
        let config = ExecutionConfig::default();
        assert!(matches!(config.eof_behavior(), EofBehavior::SetZero));
    }

    #[test]
    fn test_string_io_echo() {
        // Test that we can use StringIo to capture output (using test_utils)
        use crate::test_utils::run_bf;

        let (output, stats) = run_bf(",[.,]", "Hello").unwrap();

        assert_eq!(output, "Hello");
        assert_eq!(stats.bytes_read, 5);
        assert_eq!(stats.bytes_written, 5);
    }

    #[test]
    fn test_string_io_hello_world() {
        // Test classic Hello World program with string output (using test_utils)
        use crate::test_utils::run_bf;

        let source = "++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.>>.<-.<.+++.------.--------.>>+.>++.";
        let (output, stats) = run_bf(source, "").unwrap();

        assert_eq!(output, "Hello World!\n");
        assert_eq!(stats.bytes_written, 13);
    }

    #[test]
    fn test_string_io_add_numbers() {
        // Test program that adds two single-digit numbers (using test_utils)
        use crate::test_utils::run_bf;

        // Program: read two numbers, add them, output result
        // ,>     Read first number into cell 0, move to cell 1
        // ,      Read second number into cell 1
        // [<+>-] Add cell 1 to cell 0 (move all from cell 1 to cell 0)
        // <.     Move back to cell 0 and output result
        let (output, stats) = run_bf(",>,[-<+>]<.", "\x05\x03").unwrap();

        assert_eq!(output.as_bytes(), &[8]); // 5 + 3 = 8
        assert_eq!(stats.bytes_read, 2);
        assert_eq!(stats.bytes_written, 1);
    }

    // ===================================================================
    // CRITICAL CORRECTNESS TESTS: [+] Pattern with u8 Wrapping
    // ===================================================================
    //
    // These tests prove that [+] is NOT infinite with u8 wrapping arithmetic,
    // contrary to what the validator previously claimed!
    //
    // Background: The validator used to warn that [+] creates an "infinite loop"
    // because "incrementing never reaches zero". This is WRONG with u8 wrapping!
    //
    // Reality: With u8 wrapping (255 + 1 = 0), the loop wraps and exits.
    // See PRD/CELL_MODEL.md for full analysis.

    #[test]
    fn test_plus_loop_terminates_from_one() {
        // CRITICAL TEST: Proves [+] is NOT infinite with u8 wrapping
        //
        // Starting with cell value 1:
        // Iteration 1: + -> 2
        // Iteration 2: + -> 3
        // ...
        // Iteration 255: + -> 255
        // Iteration 256: + -> 0 (WRAPS!)
        // Loop exits because cell == 0
        //
        // Total iterations: 256 (1→2→...→255→0)

        use crate::test_utils::run_bf;

        let source = "+[+]"; // Start with 1, loop until wrap
        let result = run_bf(source, "");

        // Should succeed (not hit default step limit)
        assert!(
            result.is_ok(),
            "Loop should terminate via wrapping, not be infinite!"
        );

        let (_, stats) = result.unwrap();

        // Should take exactly 256 iterations (starting from 1)
        // Each iteration executes the + instruction
        // Plus 1 for initial +
        // Total: 1 (initial +) + 256 (loop iterations) = 257 steps
        assert!(
            stats.total_steps < StepCount::new(270),
            "Should take ~257 steps (1 + 256), got {}",
            stats.total_steps
        );
        assert!(
            stats.total_steps > StepCount::new(250),
            "Should take ~257 steps, got {} (too few!)",
            stats.total_steps
        );
    }

    #[test]
    fn test_plus_loop_terminates_from_255() {
        // Edge case: Starting with cell value 255
        // Iteration 1: + -> 0 (immediate wrap!)
        // Loop exits immediately

        use crate::test_utils::run_bf;

        // Set cell to 255 (0 - 1 = 255 with wrapping)
        let source = "-[+]"; // Start with 255, loop once
        let result = run_bf(source, "");

        assert!(result.is_ok(), "Loop should exit after 1 iteration");

        let (_, stats) = result.unwrap();

        // Should take very few steps (just 1 increment to wrap to 0)
        assert!(
            stats.total_steps < StepCount::new(10),
            "Should exit almost immediately, got {} steps",
            stats.total_steps
        );
    }

    #[test]
    fn test_plus_loop_terminates_from_128() {
        // Middle case: Starting with cell value 128
        // Will take exactly 128 iterations to wrap to 0

        use crate::test_utils::run_bf;

        // Set cell to 128 using multiple additions
        let source = format!("{}[+]", "+".repeat(128));
        let result = run_bf(&source, "");

        assert!(
            result.is_ok(),
            "Loop should terminate after ~128 iterations"
        );

        let (_, stats) = result.unwrap();

        // 128 initial + operations, then ~128 loop iterations
        // Total: 128 (initial +s) + 128 (loop iterations) + 1 (final check) = 257 steps
        assert!(
            stats.total_steps < StepCount::new(270),
            "Should take ~257 steps, got {}",
            stats.total_steps
        );
        assert!(
            stats.total_steps > StepCount::new(250),
            "Should take ~257 steps, got {} (too few!)",
            stats.total_steps
        );
    }

    #[test]
    fn test_double_plus_loop_is_infinite_from_odd() {
        // IMPORTANT MATHEMATICAL INSIGHT: [++] does NOT always terminate!
        //
        // Starting from 1 (odd):
        // Iteration 1: 1 + 2 = 3
        // Iteration 2: 3 + 2 = 5
        // ...
        // Iteration N: 255 + 2 = 257 → wraps to 1 (257 % 256 = 1)
        //
        // Result: Cycles through ODD numbers only (1→3→5→...→255→1)
        // NEVER hits 0 (which is even)!
        //
        // This is actually INFINITE, unlike [+]!

        use crate::test_utils::configs;
        use crate::test_utils::run_bf_with_config;

        let source = "+[++]"; // Start with 1 (odd), increment by 2

        // Should hit step limit (is actually infinite)
        let result = run_bf_with_config(source, "", configs::with_step_limit(1000));

        assert!(
            result.is_err(),
            "[++] from odd starting value should be infinite (never hits 0)!"
        );
        assert!(
            matches!(result, Err(BfError::StepLimitExceeded { .. })),
            "Should hit step limit, got {:?}",
            result
        );
    }

    #[test]
    fn test_double_plus_loop_terminates_from_even() {
        // [++] DOES terminate if starting from an even number!
        //
        // Starting from 2 (even):
        // Iteration 1: 2 + 2 = 4
        // Iteration 2: 4 + 2 = 6
        // ...
        // Eventually: 254 + 2 = 256 → wraps to 0
        //
        // Cycles through EVEN numbers (2→4→6→...→254→0)

        use crate::test_utils::run_bf;

        let source = "++[++]"; // Start with 2 (even), increment by 2
        let result = run_bf(source, "");

        assert!(
            result.is_ok(),
            "[++] from even starting value should terminate at 0"
        );

        let (_, stats) = result.unwrap();

        // Should take ~128 iterations (2→4→...→254→256→0)
        // Total: 2 (initial ++) + 255 (loop increments) = 257 steps
        // (128 iterations * 2 steps per iteration, but one less due to wrapping)
        assert!(
            stats.total_steps < StepCount::new(270),
            "Should take ~257 steps, got {}",
            stats.total_steps
        );
        assert!(
            stats.total_steps > StepCount::new(250),
            "Should take ~257 steps, got {} (too few!)",
            stats.total_steps
        );
    }

    #[test]
    fn test_plus_loop_with_step_limit_proves_termination() {
        // This test uses a generous step limit to prove [+] terminates
        // If [+] were truly infinite, it would hit the limit
        // Since it doesn't, we prove it terminates

        use crate::test_utils::configs;
        use crate::test_utils::run_bf_with_config;

        let source = "+[+]";

        // Set limit to 1000 steps (way more than needed for ~513 steps)
        let result = run_bf_with_config(source, "", configs::with_step_limit(1000));

        // Should succeed WITHOUT hitting step limit
        assert!(
            result.is_ok(),
            "Loop should terminate naturally, not hit step limit. \
             If this fails, [+] is actually infinite!"
        );
    }

    #[test]
    fn test_plus_loop_hits_step_limit_if_too_low() {
        // Complementary test: If we set limit too low, we CAN make it fail
        // This proves our termination tests above aren't trivially passing

        use crate::test_utils::configs;
        use crate::test_utils::run_bf_with_config;

        let source = "+[+]"; // Needs ~513 steps

        // Set limit too low (only 100 steps)
        let result = run_bf_with_config(source, "", configs::with_step_limit(100));

        // Should FAIL with step limit exceeded
        assert!(
            result.is_err(),
            "Should hit step limit with only 100 steps allowed"
        );
        assert!(
            matches!(result, Err(BfError::StepLimitExceeded { .. })),
            "Should fail with StepLimitExceeded, got {:?}",
            result
        );
    }

    // ===================================================================
    // CELL MODEL TESTS: Testing different cell arithmetic behaviors
    // ===================================================================

    #[test]
    fn test_cell_model_wrapping_overflow() {
        // Test u8 wrapping: 255 + 1 = 0
        use crate::config::ExecutionConfigBuilder;
        use crate::io::StringIo;

        // Set cell to 255 using many increments, then increment once more
        let source = format!("{}", "+".repeat(256)) + "."; // 0+256=0 (wraps at 255), output 0
        let instructions = parse(&source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_wrapping_cells()
            .build();

        let mut input = StringIo::empty();
        let mut output = StringIo::empty();
        let result = interpret_with_io(&instructions, config, &mut input, &mut output, None);

        assert!(result.is_ok(), "Wrapping should not error on overflow");
        assert_eq!(output.output_bytes()[0], 0, "Should wrap to 0");
    }

    #[test]
    fn test_cell_model_wrapping_underflow() {
        // Test u8 wrapping: 0 - 1 = 255
        use crate::config::ExecutionConfigBuilder;
        use crate::io::StringIo;

        // Start at 0, decrement once, output
        let source = "-."; // 0-1=255 (wraps), output 255
        let instructions = parse(source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_wrapping_cells()
            .build();

        let mut input = StringIo::empty();
        let mut output = StringIo::empty();
        let result = interpret_with_io(&instructions, config, &mut input, &mut output, None);

        assert!(result.is_ok(), "Wrapping should not error on underflow");
        assert_eq!(output.output_bytes()[0], 255, "Should wrap to 255");
    }

    #[test]
    fn test_cell_model_checked_overflow() {
        // Test u8 checked: 255 + 1 → error
        use crate::config::ExecutionConfigBuilder;
        use crate::test_utils::run_bf_with_config;

        // Set cell to 255 using increments, then try to increment → should error
        let source = format!("{}", "+".repeat(256)); // 0+255=255, +1=error!

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_checked_cells()
            .build();

        let result = run_bf_with_config(&source, "", config);
        assert!(result.is_err(), "Checked should error on overflow");
        assert!(
            matches!(result, Err(BfError::CellOverflow { .. })),
            "Should fail with CellOverflow, got {:?}",
            result
        );
    }

    #[test]
    fn test_cell_model_checked_underflow() {
        // Test u8 checked: 0 - 1 → error
        use crate::config::ExecutionConfigBuilder;
        use crate::test_utils::run_bf_with_config;

        // Start at 0, try to decrement → should error
        let source = "-"; // 0-1=error!

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_checked_cells()
            .build();

        let result = run_bf_with_config(source, "", config);
        assert!(result.is_err(), "Checked should error on underflow");
        assert!(
            matches!(result, Err(BfError::CellUnderflow { .. })),
            "Should fail with CellUnderflow, got {:?}",
            result
        );
    }

    #[test]
    fn test_cell_model_independence_from_memory_model() {
        // Test that CellModel and MemoryModel are orthogonal
        // Use Wrapping memory + Checked cells
        use crate::config::ExecutionConfigBuilder;
        use crate::test_utils::run_bf_with_config;

        // Set cell to 255, try to increment → cell overflow
        let source = format!("{}", "+".repeat(256)); // Should error on cell arithmetic

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100) // Fixed MEMORY
            .with_checked_cells() // Checked CELLS
            .build();

        let result = run_bf_with_config(&source, "", config);
        assert!(
            matches!(result, Err(BfError::CellOverflow { .. })),
            "Should fail with CellOverflow, proving cell model is independent"
        );
    }

    #[test]
    fn test_cell_model_normal_operations() {
        // Test that normal operations work with all cell models
        use crate::config::ExecutionConfigBuilder;
        use crate::test_utils::run_bf_with_config;

        let source = "+++."; // 0+3=3, output 3

        // Test with wrapping
        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_wrapping_cells()
            .build();
        let result = run_bf_with_config(source, "", config);
        assert!(result.is_ok());
        let (output, _) = result.unwrap();
        assert_eq!(output.as_bytes()[0], 3);

        // Test with checked
        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_checked_cells()
            .build();
        let result = run_bf_with_config(source, "", config);
        assert!(result.is_ok());
        let (output, _) = result.unwrap();
        assert_eq!(output.as_bytes()[0], 3);
    }
}
