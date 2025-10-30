//! Example: Instruction Counter Hook
//!
//! This example demonstrates how to use hooks to count different types of
//! instructions executed, providing statistics about program behavior.
//!
//! Run with: cargo run --example hooks_instruction_counter

use ferrous_cortex::hooks::{ExecutionHook, HookContext, HookDecision};
use ferrous_cortex::{BfError, ExecutionConfigBuilder, Instruction, parse};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A hook that counts each type of instruction executed
struct InstructionCounter {
    counts: Arc<Mutex<HashMap<String, usize>>>,
    total: Arc<Mutex<usize>>,
}

impl ExecutionHook for InstructionCounter {
    fn after_instruction(
        &mut self,
        instruction: &Instruction,
        _context: &HookContext,
    ) -> HookDecision {
        let instruction_name = match instruction {
            Instruction::IncrementValue => "IncrementValue (+)",
            Instruction::DecrementValue => "DecrementValue (-)",
            Instruction::IncrementPointer => "IncrementPointer (>)",
            Instruction::DecrementPointer => "DecrementPointer (<)",
            Instruction::Output => "Output (.)",
            Instruction::Input => "Input (,)",
            Instruction::Loop(_) => "Loop ([...])",
        };

        *self
            .counts
            .lock()
            .unwrap()
            .entry(instruction_name.to_string())
            .or_insert(0) += 1;
        *self.total.lock().unwrap() += 1;

        HookDecision::Continue
    }

    fn on_complete(&mut self, _context: &HookContext) {
        println!("\n=== Instruction Statistics ===\n");

        let counts = self.counts.lock().unwrap();
        let total = *self.total.lock().unwrap();

        // Sort by count (descending)
        let mut sorted: Vec<_> = counts.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));

        for (instruction, count) in sorted {
            let percentage = (*count as f64 / total as f64) * 100.0;
            println!("  {:<25} {:>6}  ({:>5.1}%)", instruction, count, percentage);
        }

        println!("\n  {:<25} {:>6}", "Total instructions", total);
    }
}

fn main() -> Result<(), BfError> {
    println!("=== Instruction Counter Hook Example ===\n");

    // Classic "Hello World!" program
    let source = "++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.>>.<-.<.+++.------.--------.>>+.>++.";
    println!("Program: Hello World!");
    println!("Source length: {} characters\n", source.len());

    // Parse the program
    let instructions = parse(source)?;

    // Create shared state for counters
    let counts = Arc::new(Mutex::new(HashMap::new()));
    let total = Arc::new(Mutex::new(0));

    // Create config with instruction counter
    let config = ExecutionConfigBuilder::new()
        .with_memory_size(30000)
        .with_hook(Box::new(InstructionCounter {
            counts: counts.clone(),
            total: total.clone(),
        }))
        .build();

    println!("Executing program...");

    // Run the interpreter
    match ferrous_cortex::interpret_with_config(&instructions, config, None) {
        Ok(stats) => {
            println!("\n✅ Program completed successfully!");
            println!("\nExecution Statistics:");
            println!("  Total steps (from stats): {}", stats.total_steps.get());
            println!("  Loop iterations: {}", stats.loop_iterations);
            println!("  Cells modified: {}", stats.cells_modified);
            println!("  Memory allocated: {} bytes", stats.memory_allocated.get());
            println!("  Bytes written: {}", stats.bytes_written);
        }
        Err(e) => {
            eprintln!("\n❌ Error: {}", e);
            return Err(e);
        }
    }

    println!("\n💡 Use cases for instruction counting:");
    println!("   - Performance profiling");
    println!("   - Optimization analysis");
    println!("   - Code complexity metrics");
    println!("   - Algorithm comparison");

    println!("\n=== Example Complete ===");
    Ok(())
}
