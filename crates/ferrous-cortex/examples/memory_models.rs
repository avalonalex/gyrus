//! Memory models example
//!
//! This example demonstrates the three different memory models:
//! - Fixed: Traditional fixed-size array with bounds checking
//! - Wrapping: Circular buffer that wraps at boundaries
//! - Unbounded: Dynamic growth up to a maximum limit
//!
//! Run with: cargo run --example memory_models

use ferrous_cortex::{BfError, ExecutionConfigBuilder, interpret_with_io, io::StringIo, parse};

fn main() -> Result<(), BfError> {
    println!("=== Memory Models Example ===\n");

    // Example 1: Fixed memory (default)
    println!("Example 1: Fixed Memory Model");
    println!("------------------------------");
    println!("Traditional BrainFuck: fixed-size array with bounds checking\n");

    let program = ">>>>>"; // Move right 5 times
    let instructions = parse(program)?;

    let config = ExecutionConfigBuilder::new()
        .with_memory_size(10) // Small memory for demonstration
        .build();

    let mut input = StringIo::empty();
    let mut output = StringIo::empty();

    match interpret_with_io(&instructions, config, &mut input, &mut output) {
        Ok(stats) => {
            println!("✓ Success!");
            println!("  Peak memory: {} cells", stats.peak_memory_used);
        }
        Err(e) => println!("✗ Error: {}", e),
    }
    println!();

    // Example 2: Out of bounds with fixed memory
    println!("Example 2: Fixed Memory - Out of Bounds");
    println!("----------------------------------------");

    let program = ">>>>>>>>>>>>>"; // Move beyond bounds (13 times)
    let instructions = parse(program)?;

    let config = ExecutionConfigBuilder::new().with_memory_size(10).build();

    let mut input = StringIo::empty();
    let mut output = StringIo::empty();

    match interpret_with_io(&instructions, config, &mut input, &mut output) {
        Ok(_) => println!("Unexpectedly succeeded!"),
        Err(e) => {
            println!("✓ Caught error as expected:");
            println!("  {}", e);
        }
    }
    println!();

    // Example 3: Wrapping memory
    println!("Example 3: Wrapping Memory Model");
    println!("---------------------------------");
    println!("Pointer wraps around at boundaries (circular buffer)\n");

    let program = ">>>>>>>>>>>>>+."; // Move 13 times (wraps to cell 3), then increment and output
    let instructions = parse(program)?;

    let config = ExecutionConfigBuilder::new()
        .with_wrapping_memory(10)
        .build();

    let mut input = StringIo::empty();
    let mut output = StringIo::empty();

    let stats = interpret_with_io(&instructions, config.clone(), &mut input, &mut output)?;

    println!("✓ Success with wrapping!");
    println!("  Memory model: {:?}", config.memory_model());
    println!("  Peak memory: {} cells", stats.peak_memory_used);
    println!("  Note: Pointer at position 13 wrapped to position 3 (13 % 10)");
    println!();

    // Example 4: Unbounded memory
    println!("Example 4: Unbounded Memory Model");
    println!("----------------------------------");
    println!("Memory grows dynamically as needed\n");

    let program = ">".repeat(500) + "+."; // Move right 500 times, increment, output
    let instructions = parse(&program)?;

    let config = ExecutionConfigBuilder::new()
        .with_unbounded_memory(100, 1000)? // Start with 100, max 1000
        .build();

    let mut input = StringIo::empty();
    let mut output = StringIo::empty();

    let stats = interpret_with_io(&instructions, config, &mut input, &mut output)?;

    println!("✓ Success with unbounded memory!");
    println!("  Initial size: 100 bytes");
    println!("  Final size:   {} bytes", stats.memory_allocated);
    println!("  Peak usage:   {} cells", stats.peak_memory_used);
    println!("  Growth:       automatic as needed");
    println!();

    // Example 5: Unbounded memory limit
    println!("Example 5: Unbounded Memory - Hitting Limit");
    println!("--------------------------------------------");

    let program = ">".repeat(2000) + "+."; // Try to exceed max
    let instructions = parse(&program)?;

    let config = ExecutionConfigBuilder::new()
        .with_unbounded_memory(100, 1500)? // Max 1500 bytes
        .build();

    let mut input = StringIo::empty();
    let mut output = StringIo::empty();

    match interpret_with_io(&instructions, config, &mut input, &mut output) {
        Ok(_) => println!("Unexpectedly succeeded!"),
        Err(e) => {
            println!("✓ Caught error when exceeding max size:");
            println!("  {}", e);
        }
    }
    println!();

    // Summary
    println!("=== Memory Model Summary ===");
    println!();
    println!("Fixed Memory:");
    println!("  ✓ Strict bounds checking");
    println!("  ✓ Predictable memory usage");
    println!("  ✗ Fails on out-of-bounds access");
    println!();
    println!("Wrapping Memory:");
    println!("  ✓ Never fails on bounds");
    println!("  ✓ Circular buffer behavior");
    println!("  ~ Different semantics (use carefully)");
    println!();
    println!("Unbounded Memory:");
    println!("  ✓ Grows automatically");
    println!("  ✓ Efficient for unknown requirements");
    println!("  ✓ Maximum limit prevents runaway");
    println!();

    Ok(())
}
