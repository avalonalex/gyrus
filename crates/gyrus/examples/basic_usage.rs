//! Basic usage of gyrus as a library
//!
//! This example demonstrates how to:
//! - Parse BrainFuck source code
//! - Execute programs with custom I/O
//! - Handle errors gracefully
//!
//! Run with: cargo run --example basic_usage

use gyrus::{BfError, ExecutionConfig, interpret_with_io, io::StringIo, parse};

fn main() -> Result<(), BfError> {
    println!("=== gyrus Basic Usage Example ===\n");

    // Example 1: Hello World
    println!("Example 1: Hello World");
    println!("-----------------------");
    let hello_world = "++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.>>.<-.<.+++.------.--------.>>+.>++.";

    let instructions = parse(hello_world)?;
    let mut input = StringIo::empty();
    let mut output = StringIo::empty();

    interpret_with_io(
        &instructions,
        ExecutionConfig::default(),
        &mut input,
        &mut output,
        None,
    )?;

    println!("Output: {}", output.output_string());
    println!();

    // Example 2: Echo with input
    println!("Example 2: Echo Program");
    println!("-----------------------");
    let echo_program = ",[.,]"; // Read and output until EOF

    let instructions = parse(echo_program)?;
    let mut input = StringIo::new("Hello, gyrus!");
    let mut output = StringIo::empty();

    let stats = interpret_with_io(
        &instructions,
        ExecutionConfig::default(),
        &mut input,
        &mut output,
        None,
    )?;

    println!("Input:  Hello, gyrus!");
    println!("Output: {}", output.output_string());
    println!("Steps:  {}", stats.total_steps);
    println!();

    // Example 3: Simple arithmetic
    println!("Example 3: Arithmetic (3 + 5 = 8)");
    println!("----------------------------------");
    let add_program = "+++>+++++[<+>-]<."; // 3 + 5 = 8

    let instructions = parse(add_program)?;
    let mut input = StringIo::empty();
    let mut output = StringIo::empty();

    let stats = interpret_with_io(
        &instructions,
        ExecutionConfig::default(),
        &mut input,
        &mut output,
        None,
    )?;

    println!("Result: {} (ASCII code)", output.output_bytes()[0]);
    println!("Steps:  {}", stats.total_steps);
    println!();

    // Example 4: Error handling
    println!("Example 4: Error Handling");
    println!("-------------------------");
    let invalid_program = "[[["; // Unmatched brackets

    match parse(invalid_program) {
        Ok(_) => println!("Unexpectedly parsed!"),
        Err(err) => {
            println!("Parse error caught:");
            println!("{}", err);
        }
    }
    println!();

    println!("=== All examples completed successfully! ===");

    Ok(())
}
