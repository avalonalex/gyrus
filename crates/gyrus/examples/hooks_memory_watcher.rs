//! Example: Memory Watcher Hook
//!
//! This example demonstrates how to use hooks to watch for memory changes and
//! pause execution when a specific cell changes, similar to watchpoints in debuggers.
//!
//! Run with: cargo run --example hooks_memory_watcher

use gyrus::hooks::{ExecutionHook, HookContext, HookDecision};
use gyrus::{BfError, ExecutionConfigBuilder, Instruction};
use std::sync::{Arc, Mutex};

/// A hook that watches a specific memory address and pauses when it changes
struct MemoryWatchpoint {
    watch_address: usize,
    last_value: Arc<Mutex<Option<u8>>>,
    changes: Arc<Mutex<Vec<(u64, u8, u8)>>>, // (step, old_value, new_value)
}

impl ExecutionHook for MemoryWatchpoint {
    fn after_instruction(
        &mut self,
        instruction: &Instruction,
        context: &HookContext,
    ) -> HookDecision {
        // Only check if we're at the watched address
        if context.pointer().get() != self.watch_address {
            return HookDecision::Continue;
        }

        let current_value = context.current_cell();
        let mut last_value_guard = self.last_value.lock().unwrap();

        if let Some(last) = *last_value_guard
            && current_value != last
        {
            // Value changed!
            self.changes
                .lock()
                .unwrap()
                .push((context.step_count().get(), last, current_value));

            println!(
                "\n⚠️  Watchpoint triggered at step {}!",
                context.step_count().get()
            );
            println!(
                "   Cell {}: {} -> {}",
                self.watch_address, last, current_value
            );
            println!("   Instruction: {:?}", instruction);
            println!("   Loop depth: {}", context.loop_depth());

            if let Some(loc) = context.source_location() {
                println!("   Source: line {}, column {}", loc.line, loc.column);
            }
        }

        *last_value_guard = Some(current_value);
        HookDecision::Continue
    }

    fn on_complete(&mut self, _context: &HookContext) {
        let changes = self.changes.lock().unwrap();

        println!("\n=== Watchpoint Summary ===");
        println!("  Watched address: {}", self.watch_address);
        println!("  Total changes: {}", changes.len());

        if !changes.is_empty() {
            println!("\n  Change history:");
            for (step, old_val, new_val) in changes.iter() {
                println!("    Step {}: {} -> {}", step, old_val, new_val);
            }
        }
    }
}

use std::collections::HashMap;

/// Type alias for memory change tracking: address -> [(step, value), ...]
type MemoryChanges = Arc<Mutex<HashMap<usize, Vec<(u64, u8)>>>>;

/// A hook that tracks all memory modifications
struct MemoryChangeTracker {
    changes: MemoryChanges,
}

impl ExecutionHook for MemoryChangeTracker {
    fn after_instruction(
        &mut self,
        instruction: &Instruction,
        context: &HookContext,
    ) -> HookDecision {
        // Only track increment/decrement
        if matches!(
            instruction,
            Instruction::IncrementValue | Instruction::DecrementValue
        ) {
            let addr = context.pointer().get();
            let value = context.current_cell();
            let step = context.step_count().get();

            self.changes
                .lock()
                .unwrap()
                .entry(addr)
                .or_default()
                .push((step, value));
        }

        HookDecision::Continue
    }

    fn on_complete(&mut self, _context: &HookContext) {
        let changes = self.changes.lock().unwrap();

        println!("\n=== Memory Modification Summary ===");
        println!("  Total cells modified: {}", changes.len());

        // Show top 5 most modified cells
        let mut cells: Vec<_> = changes.iter().collect();
        cells.sort_by_key(|c| std::cmp::Reverse(c.1.len()));

        println!("\n  Top 5 most modified cells:");
        for (addr, history) in cells.iter().take(5) {
            let final_value = history.last().map(|(_, v)| *v).unwrap_or(0);
            println!(
                "    Cell {}: {} modifications, final value = {}",
                addr,
                history.len(),
                final_value
            );
        }
    }
}

fn main() -> Result<(), BfError> {
    println!("=== Memory Watcher Hook Example ===\n");

    // Program: increment cell 0 to 5, move right, increment cell 1 to 3
    let source = "+++++>+++";
    println!("Program: {}", source);
    println!("Expected: Cell 0 = 5, Cell 1 = 3\n");

    let (instructions, debug_info) = gyrus::parse_with_debug(source)?;

    // Create watchpoint for cell 0
    let last_value = Arc::new(Mutex::new(None));
    let changes = Arc::new(Mutex::new(Vec::new()));
    let change_tracker_changes = Arc::new(Mutex::new(HashMap::new()));

    let config = ExecutionConfigBuilder::new()
        .with_memory_size(100)
        .with_hook(Box::new(MemoryWatchpoint {
            watch_address: 0,
            last_value: last_value.clone(),
            changes: changes.clone(),
        }))
        .with_hook(Box::new(MemoryChangeTracker {
            changes: change_tracker_changes.clone(),
        }))
        .build();

    println!("Watching cell 0 for changes...\n");

    // Run the interpreter
    match gyrus::interpret_with_config(&instructions, config, Some(&debug_info)) {
        Ok(stats) => {
            println!("\n✅ Program completed successfully!");
            println!("   Total steps: {}", stats.total_steps.get());
        }
        Err(e) => {
            eprintln!("\n❌ Error: {}", e);
            return Err(e);
        }
    }

    println!("\n💡 Use cases for memory watching:");
    println!("   - Debugging memory corruption");
    println!("   - Tracking variable lifetime");
    println!("   - Finding unintended side effects");
    println!("   - Performance hotspot analysis");

    println!("\n=== Example Complete ===");
    Ok(())
}
