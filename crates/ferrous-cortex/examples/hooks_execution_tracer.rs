//! Example: Execution Tracer Hook
//!
//! This example demonstrates how to use hooks to create a detailed execution trace,
//! showing every instruction executed with full state information.
//!
//! Run with: cargo run --example hooks_execution_tracer

use ferrous_cortex::hooks::{ExecutionHook, HookContext, HookDecision};
use ferrous_cortex::{BfError, ExecutionConfigBuilder, Instruction};
use std::sync::{Arc, Mutex};

/// A hook that traces every instruction with full state
struct ExecutionTracer {
    log: Arc<Mutex<Vec<String>>>,
    max_entries: usize,
    show_memory: bool,
}

impl ExecutionTracer {
    fn format_instruction(instruction: &Instruction) -> &'static str {
        match instruction {
            Instruction::IncrementValue => "+",
            Instruction::DecrementValue => "-",
            Instruction::IncrementPointer => ">",
            Instruction::DecrementPointer => "<",
            Instruction::Output => ".",
            Instruction::Input => ",",
            Instruction::Loop(_) => "[...]",
            Instruction::LoopCheck => "[", // Internal: loop condition check
        }
    }

    fn format_memory_context(context: &HookContext) -> String {
        let ptr = context.pointer().get();
        let start = ptr.saturating_sub(2);
        let end = (ptr + 3).min(context.memory().len());

        let mut cells = Vec::new();
        for i in start..end {
            if i == ptr {
                cells.push(format!("[{}]", context.memory()[i]));
            } else {
                cells.push(format!("{}", context.memory()[i]));
            }
        }
        cells.join(" ")
    }
}

impl ExecutionHook for ExecutionTracer {
    fn after_instruction(
        &mut self,
        instruction: &Instruction,
        context: &HookContext,
    ) -> HookDecision {
        let mut log = self.log.lock().unwrap();

        if log.len() < self.max_entries {
            let mut entry = format!(
                "Step {:4} | {} | @{} = {}",
                context.step_count().get(),
                Self::format_instruction(instruction),
                context.pointer().get(),
                context.current_cell()
            );

            if context.loop_depth() > 0 {
                entry.push_str(&format!(" | depth:{}", context.loop_depth()));
            }

            if self.show_memory {
                entry.push_str(&format!(
                    " | mem:[{}]",
                    Self::format_memory_context(context)
                ));
            }

            if let Some(loc) = context.source_location() {
                entry.push_str(&format!(" | {}:{}", loc.line, loc.column));
            }

            log.push(entry);
        }

        HookDecision::Continue
    }

    fn on_loop_enter(
        &mut self,
        context: &HookContext,
        _loop_info: Option<&ferrous_cortex::hooks::LoopInfo>,
    ) -> HookDecision {
        let mut log = self.log.lock().unwrap();

        if log.len() < self.max_entries {
            log.push(format!(
                "         | >>> LOOP ENTER (depth {}) | cell = {}",
                context.loop_depth(),
                context.current_cell()
            ));
        }

        HookDecision::Continue
    }

    fn on_loop_exit(&mut self, context: &HookContext) -> HookDecision {
        let mut log = self.log.lock().unwrap();

        if log.len() < self.max_entries {
            log.push(format!(
                "         | <<< LOOP EXIT (depth {}) | cell = {}",
                context.loop_depth(),
                context.current_cell()
            ));
        }

        HookDecision::Continue
    }
}

fn print_trace(log: &Arc<Mutex<Vec<String>>>, max_entries: usize) {
    let log_guard = log.lock().unwrap();

    println!("\n=== Execution Trace ({} entries) ===\n", log_guard.len());

    for entry in log_guard.iter() {
        println!("{}", entry);
    }

    if log_guard.len() >= max_entries {
        println!(
            "\n... trace limit reached, {} entries omitted ...",
            max_entries
        );
    }
}

fn main() -> Result<(), BfError> {
    println!("=== Execution Tracer Hook Example ===\n");

    // Simple program with a loop
    let source = "+++[>++<-]";
    println!("Program: {}", source);
    println!("Expected: Loop 3 times, cell 0 counts down from 3 to 0, cell 1 counts up to 6\n");

    let (instructions, debug_info) = ferrous_cortex::parse_with_debug(source)?;

    // Create shared log for the tracer (limit to 100 entries, show memory context)
    let log = Arc::new(Mutex::new(Vec::new()));

    let tracer = ExecutionTracer {
        log: log.clone(),
        max_entries: 100,
        show_memory: true,
    };

    let config = ExecutionConfigBuilder::new()
        .with_memory_size(100)
        .with_hook(Box::new(tracer))
        .build();

    println!("Executing with trace enabled...");

    // Run the interpreter
    match ferrous_cortex::interpret_with_config(&instructions, config, Some(&debug_info)) {
        Ok(stats) => {
            println!("\n✅ Program completed successfully!");
            println!("   Total steps: {}", stats.total_steps.get());
            println!("   Loop iterations: {}", stats.loop_iterations);

            // Print the execution trace using shared state
            print_trace(&log, 100);
        }
        Err(e) => {
            eprintln!("\n❌ Error: {}", e);
            return Err(e);
        }
    }

    println!("\n💡 Use cases for execution tracing:");
    println!("   - Debugging complex algorithms");
    println!("   - Understanding program flow");
    println!("   - Performance analysis");
    println!("   - Test case generation");
    println!("   - Time-travel debugging");

    println!("\n=== Example Complete ===");
    Ok(())
}
