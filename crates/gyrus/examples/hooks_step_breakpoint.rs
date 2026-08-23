//! Example: Step Breakpoint Hook
//!
//! This example demonstrates how to use hooks to pause execution at a specific
//! step, simulating a debugger breakpoint.
//!
//! Run with: cargo run --example hooks_step_breakpoint

use gyrus::hooks::{ExecutionHook, HookContext, HookDecision};
use gyrus::{BfError, ExecutionConfigBuilder};

/// A hook that breaks execution at a specific step number
struct StepBreakpoint {
    target_step: u64,
}

impl ExecutionHook for StepBreakpoint {
    fn before_instruction(
        &mut self,
        _instruction: &gyrus::Instruction,
        context: &HookContext,
    ) -> HookDecision {
        if context.step_count().get() >= self.target_step {
            println!(
                "\n🛑 Breakpoint hit at step {}!",
                context.step_count().get()
            );
            println!("   Pointer: {}", context.pointer().get());
            println!("   Current cell: {}", context.current_cell());
            println!("   Loop depth: {}", context.loop_depth());

            if let Some(loc) = context.source_location() {
                println!("   Source: line {}, column {}", loc.line, loc.column);
            }

            HookDecision::Break
        } else {
            HookDecision::Continue
        }
    }
}

fn main() -> Result<(), BfError> {
    println!("=== Step Breakpoint Hook Example ===\n");

    // Simple program: increment cell 0 to 10, then move right and increment to 5
    let source = "++++++++++>>+++++";
    println!("Program: {}", source);
    println!("Expected behavior:");
    println!("  - Increment cell 0 ten times (steps 1-10)");
    println!("  - Move right twice (steps 11-12)");
    println!("  - Increment cell 2 five times (steps 13-17)");

    // Parse the program
    let (instructions, debug_info) = gyrus::parse_with_debug(source)?;

    // Create config with breakpoint at step 12
    println!("\nSetting breakpoint at step 12...\n");
    let config = ExecutionConfigBuilder::new()
        .with_memory_size(100)
        .with_hook(Box::new(StepBreakpoint { target_step: 12 }))
        .build();

    // Run the interpreter
    match gyrus::interpret_with_config(&instructions, config, Some(&debug_info)) {
        Ok(stats) => {
            // This shouldn't happen - we expect execution to be paused
            println!("✅ Program completed successfully!");
            println!("Total steps: {}", stats.total_steps.get());
        }
        Err(BfError::ExecutionPaused {
            instruction_index,
            source_location,
            message,
        }) => {
            // Expected: execution paused at breakpoint
            println!("\n✅ Execution paused as expected!");
            println!("   Instruction index: {}", instruction_index.get());

            if let Some(loc) = source_location {
                println!(
                    "   Source location: line {}, column {}, offset {}",
                    loc.line, loc.column, loc.offset
                );
            }

            if let Some(msg) = message {
                println!("   Message: {}", msg);
            }

            println!("\n💡 In a real debugger, you could now:");
            println!("   - Inspect memory state");
            println!("   - Step forward one instruction");
            println!("   - Continue to next breakpoint");
            println!("   - Modify memory or registers");
        }
        Err(e) => {
            // Unexpected error
            eprintln!("❌ Unexpected error: {}", e);
            return Err(e);
        }
    }

    println!("\n=== Example Complete ===");
    Ok(())
}
