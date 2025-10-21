use crate::config::{EofBehavior, ExecutionConfig, MemoryModel};
use crate::error::{BfError, MemoryDump, Result};
use crate::instruction::Instruction;
use crate::io::{BfInput, BfOutput, StdInput, StdOutput};
use crate::stats::ExecutionStats;
use crate::types::{MemoryAddress, MemorySize, StepCount};
use std::io;

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
/// let stats = interpret_with_io(&instructions, ExecutionConfig::default(), &mut input, &mut output)?;
/// assert_eq!(output.output_string(), "Hi");
/// # Ok::<(), ferrous_cortex::BfError>(())
/// ```
pub fn interpret_with_io<I: BfInput, O: BfOutput>(
    instructions: &[Instruction],
    config: ExecutionConfig,
    input: &mut I,
    output: &mut O,
) -> Result<ExecutionStats> {
    use std::time::Instant;

    let mut memory = vec![0u8; config.memory_model().initial_size().get()];
    let mut pointer = MemoryAddress::new(0);
    let mut step_count = StepCount::new(0);
    let mut stats = ExecutionStats::new();
    let start_time = config.timeout_ms().map(|_| Instant::now());

    execute_block(
        instructions,
        &mut memory,
        &mut pointer,
        &mut step_count,
        &config,
        &start_time,
        &mut stats,
        input,
        output,
    )?;

    // Finalize stats
    stats.total_steps = step_count;
    stats.cells_modified = ExecutionStats::count_modified_cells(&memory);
    stats.memory_allocated = MemorySize::new(memory.len());

    Ok(stats)
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
    interpret_with_io(instructions, config, &mut input, &mut output)
}

/// Handle pointer increment based on memory model
#[inline]
fn increment_pointer(
    pointer: &mut MemoryAddress,
    memory: &mut Vec<u8>,
    config: &ExecutionConfig,
    step_count: StepCount,
) -> Result<()> {
    pointer.increment();

    match config.memory_model() {
        MemoryModel::Fixed(size) => {
            if pointer.get() >= size.get() {
                let dump = MemoryDump::from_memory(memory, *pointer);
                return Err(BfError::MemoryOutOfBounds {
                    instruction_index: step_count.into(),
                    attempted: pointer.get() as isize,
                    max: MemorySize::new(size.get() - 1),
                    memory_dump: Some(dump),
                    hint: format!(
                        "Attempted to access cell {}, but memory size is fixed at {} cells. \
                         Try increasing memory size with --memory-size {} or use --memory-model wrapping",
                        pointer.get(),
                        size.get(),
                        pointer.get() + 1000
                    ),
                });
            }
        }
        MemoryModel::Wrapping(size) => {
            if pointer.get() >= size.get() {
                *pointer = MemoryAddress::new(0); // Wrap around to beginning
            }
        }
        MemoryModel::Unbounded {
            initial_size: _,
            max_size,
        } => {
            if pointer.get() >= max_size.get() {
                let dump = MemoryDump::from_memory(memory, *pointer);
                return Err(BfError::MemoryOutOfBounds {
                    instruction_index: step_count.into(),
                    attempted: pointer.get() as isize,
                    max: MemorySize::new(max_size.get() - 1),
                    memory_dump: Some(dump),
                    hint: format!(
                        "Attempted to access cell {}, exceeding maximum size of {}. \
                         This may indicate an infinite loop moving the pointer",
                        pointer.get(),
                        max_size.get()
                    ),
                });
            }
            // Grow memory if needed
            if pointer.get() >= memory.len() {
                memory.resize(pointer.get() + 1, 0);
            }
        }
    }

    Ok(())
}

/// Handle pointer decrement based on memory model
#[inline]
fn decrement_pointer(
    pointer: &mut MemoryAddress,
    memory: &[u8],
    config: &ExecutionConfig,
    step_count: StepCount,
) -> Result<()> {
    match config.memory_model() {
        MemoryModel::Fixed(size) | MemoryModel::Unbounded { max_size: size, .. } => {
            if pointer.get() == 0 && !config.allow_negative_pointer() {
                let dump = MemoryDump::from_memory(memory, *pointer);
                return Err(BfError::MemoryOutOfBounds {
                    instruction_index: step_count.into(),
                    attempted: -1,
                    max: MemorySize::new(size.get() - 1),
                    memory_dump: Some(dump),
                    hint: "Attempted to move pointer below cell 0. Memory cells are indexed from 0 onwards.".to_string(),
                });
            }
            if pointer.get() > 0 {
                pointer.decrement();
            }
        }
        MemoryModel::Wrapping(size) => {
            if pointer.get() == 0 {
                *pointer = MemoryAddress::new(size.get() - 1); // Wrap around to end
            } else {
                pointer.decrement();
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn execute_block<I: BfInput, O: BfOutput>(
    instructions: &[Instruction],
    memory: &mut Vec<u8>,
    pointer: &mut MemoryAddress,
    step_count: &mut StepCount,
    config: &ExecutionConfig,
    start_time: &Option<std::time::Instant>,
    stats: &mut ExecutionStats,
    input: &mut I,
    output: &mut O,
) -> Result<()> {
    for instruction in instructions {
        // Check step limit
        step_count.increment();
        if let Some(max_steps) = config.max_steps()
            && step_count.get() > max_steps
        {
            return Err(BfError::StepLimitExceeded {
                limit: max_steps,
                actual_steps: *step_count,
                hint: format!(
                    "Program executed {} steps, exceeding the limit of {}. \
                         This may indicate an infinite loop. Try increasing the limit with --max-steps {} \
                         or add breakpoints to debug.",
                    step_count.get(),
                    max_steps,
                    max_steps * 2
                ),
            });
        }

        // Check timeout
        if let Some(start) = start_time
            && let Some(timeout_ms) = config.timeout_ms()
        {
            let elapsed = start.elapsed().as_millis() as u64;
            if elapsed > timeout_ms {
                return Err(BfError::ExecutionTimeout {
                    limit_ms: timeout_ms,
                    actual_steps: Some(*step_count),
                    hint: format!(
                        "Program exceeded {}ms timeout after executing {} steps. \
                             Try increasing timeout with --timeout {} or optimize your BrainFuck code.",
                        timeout_ms,
                        step_count.get(),
                        timeout_ms * 2
                    ),
                });
            }
        }

        match instruction {
            Instruction::IncrementPointer => {
                increment_pointer(pointer, memory, config, *step_count)?;
                // Track peak memory usage
                if pointer.get() + 1 > stats.peak_memory_used.get() {
                    stats.peak_memory_used = MemoryAddress::new(pointer.get() + 1);
                }
            }
            Instruction::DecrementPointer => {
                decrement_pointer(pointer, memory, config, *step_count)?;
            }
            Instruction::IncrementValue => {
                memory[pointer.get()] = memory[pointer.get()].wrapping_add(1);
            }
            Instruction::DecrementValue => {
                memory[pointer.get()] = memory[pointer.get()].wrapping_sub(1);
            }
            Instruction::Output => {
                output
                    .write_byte(memory[pointer.get()])
                    .map_err(|source| BfError::IoError {
                        operation: "writing output".to_string(),
                        instruction_index: Some((*step_count).into()),
                        source,
                    })?;
                output.flush().map_err(|source| BfError::IoError {
                    operation: "flushing output".to_string(),
                    instruction_index: Some((*step_count).into()),
                    source,
                })?;
                stats.bytes_written += 1;
            }
            Instruction::Input => {
                match input.read_byte() {
                    Ok(Some(byte)) => {
                        memory[pointer.get()] = byte;
                        stats.bytes_read += 1;
                    }
                    Ok(None) => {
                        // Handle EOF based on configuration
                        match config.eof_behavior() {
                            EofBehavior::SetZero => {
                                memory[pointer.get()] = 0;
                            }
                            EofBehavior::SetNegOne => {
                                memory[pointer.get()] = 255; // -1 as u8
                            }
                            EofBehavior::NoChange => {
                                // Do nothing, leave cell as-is
                            }
                            EofBehavior::Error => {
                                return Err(BfError::IoError {
                                    operation: "reading input (EOF reached)".to_string(),
                                    instruction_index: Some((*step_count).into()),
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
                            instruction_index: Some((*step_count).into()),
                            source,
                        });
                    }
                }
            }
            Instruction::Loop(body) => {
                while memory[pointer.get()] != 0 {
                    stats.loop_iterations += 1;
                    execute_block(
                        body, memory, pointer, step_count, config, start_time, stats, input, output,
                    )?;
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
        let source = "+[+]"; // Infinite loop
        let instructions = parse(source).unwrap();
        let config = ExecutionConfigBuilder::new()
            .with_memory_size(MEMORY_SIZE)
            .with_max_steps(100)
            .build();
        let result = interpret_with_config(&instructions, config);
        assert!(matches!(
            result,
            Err(BfError::StepLimitExceeded { limit: 100, .. })
        ));
    }

    #[test]
    fn test_custom_memory_size() {
        let source = ">".repeat(101);
        let instructions = parse(&source).unwrap();
        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        let result = interpret_with_config(&instructions, config);
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
    fn test_memory_model_wrapping_forward() {
        // Wrapping memory model should wrap from end to beginning
        let source = format!("{}+.", ">".repeat(10)); // Move to cell 10, increment, output
        let instructions = parse(&source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_wrapping_memory(10)
            .build(); // 10 cells (0-9)

        // Should wrap to cell 0 and output value
        let result = interpret_with_config(&instructions, config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_memory_model_wrapping_backward() {
        // Wrapping memory model should wrap from beginning to end
        let source = "<+."; // Move left from 0, increment, output
        let instructions = parse(source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_wrapping_memory(10)
            .build(); // 10 cells

        // Should wrap to cell 9
        let result = interpret_with_config(&instructions, config);
        assert!(result.is_ok());
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
    fn test_memory_model_wrapping_multiple_wraps() {
        // Test multiple wraps in wrapping mode
        let source = format!("{}>+.", ">".repeat(25)); // Move right 25 times (2.5 wraps with size 10)
        let instructions = parse(&source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_wrapping_memory(10)
            .build();
        let result = interpret_with_config(&instructions, config);

        // Should end at cell 5 (25 % 10 = 5), then move to 6
        assert!(result.is_ok());
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
        // Test that we can use StringIo to capture output
        use crate::io::StringIo;

        let source = ",[.,]"; // Echo program: read and output until EOF
        let instructions = parse(source).unwrap();
        let config = ExecutionConfig::default();

        let mut input = StringIo::new("Hello");
        let mut output = StringIo::empty();
        let stats = interpret_with_io(&instructions, config, &mut input, &mut output).unwrap();

        assert_eq!(output.output_string(), "Hello");
        assert_eq!(stats.bytes_read, 5);
        assert_eq!(stats.bytes_written, 5);
    }

    #[test]
    fn test_string_io_hello_world() {
        // Test classic Hello World program with string output
        use crate::io::StringIo;

        let source = "++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.>>.<-.<.+++.------.--------.>>+.>++.";
        let instructions = parse(source).unwrap();
        let config = ExecutionConfig::default();

        let mut input = StringIo::empty();
        let mut output = StringIo::empty();
        let stats = interpret_with_io(&instructions, config, &mut input, &mut output).unwrap();

        assert_eq!(output.output_string(), "Hello World!\n");
        assert_eq!(stats.bytes_written, 13);
    }

    #[test]
    fn test_string_io_add_numbers() {
        // Test program that adds two single-digit numbers
        use crate::io::StringIo;

        // Program: read two numbers, add them, output result
        // ,>     Read first number into cell 0, move to cell 1
        // ,      Read second number into cell 1
        // [<+>-] Add cell 1 to cell 0 (move all from cell 1 to cell 0)
        // <.     Move back to cell 0 and output result
        let source = ",>,[-<+>]<.";
        let instructions = parse(source).unwrap();
        let config = ExecutionConfig::default();

        // ASCII '5' = 53, ASCII '3' = 51, sum = 104 = ASCII 'h'
        let mut input = StringIo::new("\x05\x03");
        let mut output = StringIo::empty();
        let stats = interpret_with_io(&instructions, config, &mut input, &mut output).unwrap();

        assert_eq!(output.output_bytes(), &[8]); // 5 + 3 = 8
        assert_eq!(stats.bytes_read, 2);
        assert_eq!(stats.bytes_written, 1);
    }
}
