//! Example: Profiling Hook
//!
//! This example demonstrates how to use the ProfilingHook to identify hot code
//! regions and measure loop performance in BrainFuck programs.
//!
//! Run with: cargo run --example hooks_profiler

use gyrus::hooks::builtin::{ProfilingHook, SharedProfilingHook};
use gyrus::{BfError, ExecutionConfigBuilder, parse_with_debug};
use std::sync::{Arc, Mutex};

fn main() -> Result<(), BfError> {
    println!("=== Profiling Hook Example ===\n");

    // Test program with nested loops
    // This demonstrates how the profiler tracks loop execution time
    let source = r#"
        * Initialize: cell 0 = 5
        +++++

        * Outer loop: 5 iterations
        [
            * Inner loop 1: set cell 1 = 10
            >++++++++++

            * Inner loop 2: multiply cell 1 by cell 0
            [<+>-]

            * Reset and continue
            <-
        ]

        * Another top-level loop: cell 0 = 255, then clear
        [-]
    "#;

    println!("Program: Nested loop profiling test");
    println!("Source:\n{}\n", source);

    // Parse with debug info to get source locations in profiling output
    let (instructions, debug_info) = parse_with_debug(source)?;

    // Create profiler with shared state
    let profiler = Arc::new(Mutex::new(ProfilingHook::new()));
    let profiler_clone = Arc::clone(&profiler);

    // Create config with profiler hook
    let mut config = ExecutionConfigBuilder::new().with_memory_size(1000).build();

    // Register profiler hook
    config.register_hook(Box::new(SharedProfilingHook::new_with_shared(
        profiler_clone,
    )));

    println!("Executing program with profiler...\n");

    // Run the interpreter
    match gyrus::interpret_with_io(
        &instructions,
        config,
        &mut gyrus::io::StringIo::empty(),
        &mut gyrus::io::StringIo::empty(),
        Some(&debug_info),
    ) {
        Ok(stats) => {
            println!("✅ Program completed successfully!");
            println!("\nExecution Statistics:");
            println!("  Total steps: {}", stats.total_steps.get());
            println!("  Loop iterations: {}", stats.loop_iterations);
            println!("  Cells modified: {}", stats.cells_modified);
        }
        Err(e) => {
            eprintln!("\n❌ Error: {}", e);
            return Err(e);
        }
    }

    // Print profiling results
    println!("\n{}", "=".repeat(60));
    println!("{}", profiler.lock().unwrap().format_ascii_tree());
    println!("{}", "=".repeat(60));

    println!("\n💡 Use cases for profiling:");
    println!("   - Identify hot loops consuming most execution time");
    println!("   - Optimize performance-critical sections");
    println!("   - Compare different algorithm implementations");
    println!("   - Understand program execution patterns");

    // Example of accessing raw profiling data
    {
        let profiler = profiler.lock().unwrap();
        println!("\n📊 Raw profiling data:");
        println!(
            "   Total execution time: {:.3}ms",
            profiler.total_time().as_secs_f64() * 1000.0
        );
        println!(
            "   Number of profiled loops: {}",
            profiler.completed_loops().len()
        );

        // Find the hottest loop
        if let Some(hottest) = profiler
            .completed_loops()
            .iter()
            .max_by_key(|l| l.total_time)
        {
            println!(
                "   Hottest loop: index {} took {:.3}ms ({} iterations)",
                hottest.loop_instruction_index,
                hottest.total_time.as_secs_f64() * 1000.0,
                hottest.iterations
            );
        }
    }

    println!("\n=== Example Complete ===");
    Ok(())
}
