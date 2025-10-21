use crate::config::{EofBehavior, ExecutionConfig, MemoryModel};
use crate::error::BfError;
use crate::instruction::Instruction;
use crate::stats::ExecutionStats;
use std::io::{self, Read, Write};

/// Interpret and execute BrainFuck instructions with default configuration
///
/// This is a convenience function that uses default settings.
/// For custom configuration, use `interpret_with_config()`.
/// Discards execution statistics.
#[allow(dead_code)]
pub fn interpret(instructions: &[Instruction]) -> Result<(), BfError> {
    interpret_with_config(instructions, ExecutionConfig::default()).map(|_| ())
}

/// Interpret and execute BrainFuck instructions with custom configuration
/// Returns execution statistics
pub fn interpret_with_config(
    instructions: &[Instruction],
    config: ExecutionConfig,
) -> Result<ExecutionStats, BfError> {
    use std::time::Instant;

    let mut memory = vec![0u8; config.memory_model.initial_size()];
    let mut pointer = 0usize;
    let mut step_count = 0u64;
    let mut stats = ExecutionStats::new();
    let start_time = config.timeout_ms.map(|_| Instant::now());

    execute_block(
        instructions,
        &mut memory,
        &mut pointer,
        &mut step_count,
        &config,
        &start_time,
        &mut stats,
    )?;

    // Finalize stats
    stats.total_steps = step_count;
    stats.cells_modified = ExecutionStats::count_modified_cells(&memory);
    stats.memory_allocated = memory.len();

    Ok(stats)
}

/// Handle pointer increment based on memory model
fn increment_pointer(
    pointer: &mut usize,
    memory: &mut Vec<u8>,
    config: &ExecutionConfig,
    step_count: u64,
) -> Result<(), BfError> {
    *pointer += 1;

    match &config.memory_model {
        MemoryModel::Fixed(size) => {
            if *pointer >= *size {
                return Err(BfError::MemoryOutOfBounds {
                    instruction_index: step_count as usize,
                    attempted: *pointer as isize,
                    max: size - 1,
                });
            }
        }
        MemoryModel::Wrapping(size) => {
            if *pointer >= *size {
                *pointer = 0; // Wrap around to beginning
            }
        }
        MemoryModel::Unbounded {
            initial_size: _,
            max_size,
        } => {
            if *pointer >= *max_size {
                return Err(BfError::MemoryOutOfBounds {
                    instruction_index: step_count as usize,
                    attempted: *pointer as isize,
                    max: max_size - 1,
                });
            }
            // Grow memory if needed
            if *pointer >= memory.len() {
                memory.resize(*pointer + 1, 0);
            }
        }
    }

    Ok(())
}

/// Handle pointer decrement based on memory model
fn decrement_pointer(
    pointer: &mut usize,
    config: &ExecutionConfig,
    step_count: u64,
) -> Result<(), BfError> {
    match &config.memory_model {
        MemoryModel::Fixed(size) | MemoryModel::Unbounded { max_size: size, .. } => {
            if *pointer == 0 && !config.allow_negative_pointer {
                return Err(BfError::MemoryOutOfBounds {
                    instruction_index: step_count as usize,
                    attempted: -1,
                    max: size - 1,
                });
            }
            if *pointer > 0 {
                *pointer -= 1;
            }
        }
        MemoryModel::Wrapping(size) => {
            if *pointer == 0 {
                *pointer = size - 1; // Wrap around to end
            } else {
                *pointer -= 1;
            }
        }
    }

    Ok(())
}

fn execute_block(
    instructions: &[Instruction],
    memory: &mut Vec<u8>,
    pointer: &mut usize,
    step_count: &mut u64,
    config: &ExecutionConfig,
    start_time: &Option<std::time::Instant>,
    stats: &mut ExecutionStats,
) -> Result<(), BfError> {
    for instruction in instructions {
        // Check step limit
        *step_count += 1;
        if let Some(max_steps) = config.max_steps {
            if *step_count > max_steps {
                return Err(BfError::StepLimitExceeded { limit: max_steps });
            }
        }

        // Check timeout
        if let Some(start) = start_time {
            if let Some(timeout_ms) = config.timeout_ms {
                let elapsed = start.elapsed().as_millis() as u64;
                if elapsed > timeout_ms {
                    return Err(BfError::ExecutionTimeout {
                        limit_ms: timeout_ms,
                    });
                }
            }
        }

        match instruction {
            Instruction::IncrementPointer => {
                increment_pointer(pointer, memory, config, *step_count)?;
                // Track peak memory usage
                if *pointer + 1 > stats.peak_memory_used {
                    stats.peak_memory_used = *pointer + 1;
                }
            }
            Instruction::DecrementPointer => {
                decrement_pointer(pointer, config, *step_count)?;
            }
            Instruction::IncrementValue => {
                memory[*pointer] = memory[*pointer].wrapping_add(1);
            }
            Instruction::DecrementValue => {
                memory[*pointer] = memory[*pointer].wrapping_sub(1);
            }
            Instruction::Output => {
                io::stdout()
                    .write_all(&[memory[*pointer]])
                    .map_err(|e| BfError::IoError {
                        message: e.to_string(),
                    })?;
                io::stdout().flush().map_err(|e| BfError::IoError {
                    message: e.to_string(),
                })?;
                stats.bytes_written += 1;
            }
            Instruction::Input => {
                let mut buf = [0u8; 1];
                match io::stdin().read_exact(&mut buf) {
                    Ok(_) => {
                        memory[*pointer] = buf[0];
                        stats.bytes_read += 1;
                    }
                    Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                        // Handle EOF based on configuration
                        match config.eof_behavior {
                            EofBehavior::SetZero => {
                                memory[*pointer] = 0;
                            }
                            EofBehavior::SetNegOne => {
                                memory[*pointer] = 255; // -1 as u8
                            }
                            EofBehavior::NoChange => {
                                // Do nothing, leave cell as-is
                            }
                            EofBehavior::Error => {
                                return Err(BfError::IoError {
                                    message: "End of input reached".to_string(),
                                });
                            }
                        }
                    }
                    Err(e) => {
                        return Err(BfError::IoError {
                            message: e.to_string(),
                        });
                    }
                }
            }
            Instruction::Loop(body) => {
                while memory[*pointer] != 0 {
                    stats.loop_iterations += 1;
                    execute_block(body, memory, pointer, step_count, config, start_time, stats)?;
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let instructions = parse(&source).unwrap();
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
        let instructions = parse(&source).unwrap();
        let config = ExecutionConfig::default().with_max_steps(100);
        let result = interpret_with_config(&instructions, config);
        assert!(matches!(
            result,
            Err(BfError::StepLimitExceeded { limit: 100 })
        ));
    }

    #[test]
    fn test_custom_memory_size() {
        let source = ">".repeat(101);
        let instructions = parse(&source).unwrap();
        let config = ExecutionConfig::default().with_memory_size(100);
        let result = interpret_with_config(&instructions, config);
        assert!(matches!(
            result,
            Err(BfError::MemoryOutOfBounds { max: 99, .. })
        ));
    }

    #[test]
    fn test_execution_timeout() {
        // Create a program that runs longer - moving pointer takes more time
        let source = "+[>+<]".repeat(1000); // Infinite loop with more instructions
        let instructions = parse(&source).unwrap();
        let config = ExecutionConfig::default()
            .with_timeout_ms(100)
            .with_memory_size(1000); // Smaller memory to hit bounds faster
        let result = interpret_with_config(&instructions, config);
        // Should fail with either timeout or memory bounds
        assert!(result.is_err());
    }

    #[test]
    fn test_memory_model_fixed() {
        // Fixed memory model should error on out-of-bounds access
        let source = ">".repeat(100); // Move right 100 times
        let instructions = parse(&source).unwrap();

        let config = ExecutionConfig::default().with_memory_size(50); // Only 50 cells
        let result = interpret_with_config(&instructions, config);

        assert!(result.is_err());
        assert!(matches!(result, Err(BfError::MemoryOutOfBounds { .. })));
    }

    #[test]
    fn test_memory_model_wrapping_forward() {
        // Wrapping memory model should wrap from end to beginning
        let source = format!("{}+.", ">".repeat(10)); // Move to cell 10, increment, output
        let instructions = parse(&source).unwrap();

        let config = ExecutionConfig::default().with_wrapping_memory(10); // 10 cells (0-9)

        // Should wrap to cell 0 and output value
        let result = interpret_with_config(&instructions, config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_memory_model_wrapping_backward() {
        // Wrapping memory model should wrap from beginning to end
        let source = "<+."; // Move left from 0, increment, output
        let instructions = parse(&source).unwrap();

        let config = ExecutionConfig::default().with_wrapping_memory(10); // 10 cells

        // Should wrap to cell 9
        let result = interpret_with_config(&instructions, config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_memory_model_unbounded_growth() {
        // Unbounded memory should grow as needed
        let source = format!("{}+.", ">".repeat(100)); // Move right 100 times
        let instructions = parse(&source).unwrap();

        let config = ExecutionConfig::default().with_unbounded_memory(10, 200); // Start small, allow growth

        let result = interpret_with_config(&instructions, config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_memory_model_unbounded_max_limit() {
        // Unbounded memory should still error at max limit
        let source = format!("{}+.", ">".repeat(150)); // Move right 150 times
        let instructions = parse(&source).unwrap();

        let config = ExecutionConfig::default().with_unbounded_memory(10, 100); // Max 100 cells

        let result = interpret_with_config(&instructions, config);
        assert!(result.is_err());
        assert!(matches!(result, Err(BfError::MemoryOutOfBounds { .. })));
    }

    #[test]
    fn test_memory_model_fixed_left_boundary() {
        // Fixed model should error when going below 0
        let source = "<"; // Move left from 0
        let instructions = parse(&source).unwrap();

        let config = ExecutionConfig::default().with_memory_size(100);
        let result = interpret_with_config(&instructions, config);

        assert!(result.is_err());
        assert!(matches!(result, Err(BfError::MemoryOutOfBounds { .. })));
    }

    #[test]
    fn test_memory_model_wrapping_multiple_wraps() {
        // Test multiple wraps in wrapping mode
        let source = format!("{}>+.", ">".repeat(25)); // Move right 25 times (2.5 wraps with size 10)
        let instructions = parse(&source).unwrap();

        let config = ExecutionConfig::default().with_wrapping_memory(10);
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

        assert_eq!(stats.total_steps, 7);
        assert_eq!(stats.loop_iterations, 0);
        assert_eq!(stats.peak_memory_used, 3); // Moved to cell 2, so peak is 3 (0-based + 1)
    }

    #[test]
    fn test_stats_loop_iterations() {
        // Test loop iteration counting
        let source = "+++[>+<-]"; // Loop runs 3 times
        let instructions = parse(source).unwrap();

        let config = ExecutionConfig::default();
        let stats = interpret_with_config(&instructions, config).unwrap();

        assert_eq!(stats.loop_iterations, 3);
        assert!(stats.total_steps > 3); // Should be more than just the setup
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
        assert_eq!(stats.peak_memory_used, 3); // Highest cell accessed + 1
    }

    #[test]
    fn test_stats_unbounded_allocation() {
        // Test memory allocation tracking for unbounded model
        let source = format!("{}+", ">".repeat(50)); // Move to cell 50
        let instructions = parse(&source).unwrap();

        let config = ExecutionConfig::default().with_unbounded_memory(10, 100);
        let stats = interpret_with_config(&instructions, config).unwrap();

        assert_eq!(stats.peak_memory_used, 51); // Cell 50 + 1
        assert_eq!(stats.memory_allocated, 51); // Should have grown to 51 cells
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
        let config = ExecutionConfig::default().with_eof_behavior(EofBehavior::SetZero);
        assert!(matches!(config.eof_behavior, EofBehavior::SetZero));

        let config = ExecutionConfig::default().with_eof_behavior(EofBehavior::SetNegOne);
        assert!(matches!(config.eof_behavior, EofBehavior::SetNegOne));

        let config = ExecutionConfig::default().with_eof_behavior(EofBehavior::NoChange);
        assert!(matches!(config.eof_behavior, EofBehavior::NoChange));

        let config = ExecutionConfig::default().with_eof_behavior(EofBehavior::Error);
        assert!(matches!(config.eof_behavior, EofBehavior::Error));
    }

    #[test]
    fn test_eof_behavior_default() {
        // Test that default EOF behavior is SetZero
        let config = ExecutionConfig::default();
        assert!(matches!(config.eof_behavior, EofBehavior::SetZero));
    }
}
