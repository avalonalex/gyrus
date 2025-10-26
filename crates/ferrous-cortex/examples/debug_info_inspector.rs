//! Debug Info Inspector
//!
//! This example shows what debug symbols look like after parsing.
//! It helps understand how step indices map to source locations.
//!
//! Run with: cargo run --example debug_info_inspector

use ferrous_cortex::{parse_with_debug, BfError};

fn main() -> Result<(), BfError> {
    println!("=== Debug Info Inspector ===\n");

    // Example 1: Simple linear program
    println!("Example 1: Linear Program");
    println!("-------------------------");
    let source1 = "+++>--<.";
    inspect_debug_info(source1)?;

    // Example 2: Program with loops
    println!("\nExample 2: Program with Loop");
    println!("----------------------------");
    let source2 = "+[>+<-]";
    inspect_debug_info(source2)?;

    // Example 3: Nested loops
    println!("\nExample 3: Nested Loops");
    println!("-----------------------");
    let source3 = "+[[>+<]]";
    inspect_debug_info(source3)?;

    // Example 4: Program with comments
    println!("\nExample 4: With Line Comments");
    println!("-----------------------------");
    let source4 = "* This is a comment\n+++ * increment\n> * move\n";
    inspect_debug_info(source4)?;

    println!("\n=== Inspection Complete ===");
    Ok(())
}

fn inspect_debug_info(source: &str) -> Result<(), BfError> {
    println!("Source code:");
    println!("{:?}", source);
    println!();

    let (instructions, debug_info) = parse_with_debug(source)?;

    println!("Parsed {} instructions", instructions.len());
    println!("Debug symbols: {} locations recorded", debug_info.len());
    println!();

    // Show the mapping
    println!("Step Index → Source Location:");
    println!("{:<12} {:<15} {:<8} {:<8}", "Index", "Instruction", "Line", "Column");
    println!("{}", "-".repeat(50));

    // Collect all indices
    let mut indices: Vec<usize> = (0..debug_info.len()).collect();
    indices.sort();

    for idx in indices {
        if let Some(loc) = debug_info.lookup(idx) {
            // Try to show which character this is
            let char_at_loc = get_char_at_location(source, loc);
            let char_display = char_at_loc.map_or("?".to_string(), |c| {
                if c == '\n' {
                    "\\n".to_string()
                } else {
                    c.to_string()
                }
            });

            println!(
                "{:<12} {:<15} {:<8} {:<8}",
                idx,
                format!("'{}'", char_display),
                loc.line,
                loc.column
            );
        }
    }

    println!();
    Ok(())
}

/// Helper to get the character at a specific source location
fn get_char_at_location(source: &str, loc: ferrous_cortex::SourceLocation) -> Option<char> {
    source.chars().nth(loc.offset)
}
