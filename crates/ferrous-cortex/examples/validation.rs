//! Program validation example
//!
//! This example demonstrates how to validate BrainFuck programs
//! and detect potential issues before execution.
//!
//! Run with: cargo run --example validation

use ferrous_cortex::{BfError, parse, validate};

fn main() -> Result<(), BfError> {
    println!("=== Program Validation Example ===\n");

    // Example 1: Clean program
    println!("Example 1: Clean Program");
    println!("------------------------");
    let clean_program = "+++++[>+++++<-]>+.";
    let instructions = parse(clean_program)?;
    let warnings = validate(&instructions);

    if warnings.is_empty() {
        println!("✓ Program is clean - no warnings");
    } else {
        println!("✗ Found {} warning(s)", warnings.len());
        for warning in warnings {
            println!("  {}", warning);
        }
    }
    println!();

    // Example 2: Empty loop
    println!("Example 2: Empty Loop");
    println!("---------------------");
    let empty_loop = "+++[]+++";
    let instructions = parse(empty_loop)?;
    let warnings = validate(&instructions);

    println!("Program: {}", empty_loop);
    if !warnings.is_empty() {
        println!("✓ Detected {} warning(s):", warnings.len());
        for warning in warnings {
            println!("  - {}", warning);
        }
        println!("\nExplanation: Empty loops [] do nothing and can be removed");
    }
    println!();

    // Example 3: Infinite increment loop
    println!("Example 3: Infinite Increment Loop");
    println!("-----------------------------------");
    let infinite_loop = "+++[+]";
    let instructions = parse(infinite_loop)?;
    let warnings = validate(&instructions);

    println!("Program: {}", infinite_loop);
    if !warnings.is_empty() {
        println!("✓ Detected {} warning(s):", warnings.len());
        for warning in warnings {
            println!("  - {}", warning);
        }
        println!("\nExplanation: Loop increments cell but never decrements,");
        println!("so it will never reach zero and loop will never exit");
    }
    println!();

    // Example 4: Extreme nesting
    println!("Example 4: Extreme Nesting");
    println!("--------------------------");
    let deeply_nested = "[[[[[[[[[[[+]]]]]]]]]]]"; // 11 levels deep
    let instructions = parse(deeply_nested)?;
    let warnings = validate(&instructions);

    println!("Program: {}", deeply_nested);
    println!("Nesting depth: 11 levels");
    if !warnings.is_empty() {
        println!("✓ Detected {} warning(s):", warnings.len());
        for warning in warnings {
            println!("  - {}", warning);
        }
        println!("\nExplanation: Deep nesting (>10 levels) can impact performance");
    }
    println!();

    // Example 5: Multiple issues
    println!("Example 5: Multiple Issues");
    println!("--------------------------");
    let multiple_issues = "[][+][[[[[[[[[[[++]]]]]]]]]]]"; // Empty loop + infinite loop + deep nesting
    let instructions = parse(multiple_issues)?;
    let warnings = validate(&instructions);

    println!("Program: {}", multiple_issues);
    println!("✓ Detected {} warning(s):", warnings.len());
    for (i, warning) in warnings.iter().enumerate() {
        println!("  {}. {}", i + 1, warning);
    }
    println!();

    // Example 6: Validation workflow
    println!("Example 6: Recommended Validation Workflow");
    println!("-------------------------------------------");

    let program = "++[++]"; // Intentionally problematic
    println!("Step 1: Parse the program");
    let instructions = match parse(program) {
        Ok(instrs) => {
            println!("  ✓ Parsing successful");
            instrs
        }
        Err(e) => {
            println!("  ✗ Parse error: {}", e);
            return Ok(());
        }
    };

    println!("\nStep 2: Validate for warnings");
    let warnings = validate(&instructions);

    if warnings.is_empty() {
        println!("  ✓ No warnings - safe to execute");
    } else {
        println!("  ⚠ Found {} warning(s):", warnings.len());
        for warning in &warnings {
            println!("    - {}", warning);
        }

        println!("\nStep 3: Decision");
        println!("  Option A: Fix warnings before execution (recommended)");
        println!("  Option B: Execute anyway (use --verbose for monitoring)");
        println!("  Option C: Fail in strict mode (CI/CD pipelines)");
    }
    println!();

    // Summary
    println!("=== Validation Summary ===");
    println!();
    println!("Common Warning Types:");
    println!("  1. Empty loops:     [] - Remove them");
    println!("  2. Infinite loops:  [+] or [++] - Cell never reaches zero");
    println!("  3. Deep nesting:    >10 levels - Performance impact");
    println!("  4. Inefficient:     [--] instead of [-]");
    println!();
    println!("Best Practices:");
    println!("  ✓ Validate during development");
    println!("  ✓ Use --strict mode in CI/CD");
    println!("  ✓ Fix warnings before deployment");
    println!("  ✓ Document intentional patterns");
    println!();

    Ok(())
}
