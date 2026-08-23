//! Custom I/O implementation example
//!
//! This example demonstrates how to implement custom I/O for gyrus,
//! enabling integration with your own systems (files, network, GUI, etc.)
//!
//! Run with: cargo run --example custom_io

use gyrus::{
    BfError, ExecutionConfig, interpret_with_io,
    io::{BfInput, BfOutput},
    parse,
};
use std::io;

/// Uppercase output: converts all lowercase letters to uppercase
struct UppercaseOutput {
    buffer: Vec<u8>,
}

impl UppercaseOutput {
    fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    fn get_output(&self) -> String {
        String::from_utf8_lossy(&self.buffer).into_owned()
    }
}

impl BfOutput for UppercaseOutput {
    fn write_byte(&mut self, byte: u8) -> io::Result<()> {
        // Convert lowercase to uppercase
        let uppercased = if byte.is_ascii_lowercase() {
            byte.to_ascii_uppercase()
        } else {
            byte
        };
        self.buffer.push(uppercased);
        Ok(())
    }
}

/// ROT13 input: applies ROT13 cipher to input
struct Rot13Input {
    data: Vec<u8>,
    position: usize,
}

impl Rot13Input {
    fn new(input: &str) -> Self {
        Self {
            data: input.as_bytes().to_vec(),
            position: 0,
        }
    }
}

impl BfInput for Rot13Input {
    fn read_byte(&mut self) -> io::Result<Option<u8>> {
        if self.position < self.data.len() {
            let byte = self.data[self.position];
            self.position += 1;

            // Apply ROT13 to letters
            let rotated = match byte {
                b'a'..=b'z' => ((byte - b'a' + 13) % 26) + b'a',
                b'A'..=b'Z' => ((byte - b'A' + 13) % 26) + b'A',
                _ => byte,
            };

            Ok(Some(rotated))
        } else {
            Ok(None)
        }
    }
}

/// Logging output: logs all writes
struct LoggingOutput {
    inner: Vec<u8>,
    write_count: usize,
}

impl LoggingOutput {
    fn new() -> Self {
        Self {
            inner: Vec::new(),
            write_count: 0,
        }
    }

    fn get_output(&self) -> String {
        String::from_utf8_lossy(&self.inner).into_owned()
    }
}

impl BfOutput for LoggingOutput {
    fn write_byte(&mut self, byte: u8) -> io::Result<()> {
        self.write_count += 1;
        println!(
            "[LOG] Write #{}: {} ('{}')",
            self.write_count,
            byte,
            if byte.is_ascii_graphic() {
                byte as char
            } else {
                '?'
            }
        );
        self.inner.push(byte);
        Ok(())
    }
}

fn main() -> Result<(), BfError> {
    println!("=== Custom I/O Example ===\n");

    // Example 1: Uppercase output
    println!("Example 1: Uppercase Output");
    println!("---------------------------");
    let program = "++++++++[>++++++++<-]>+."; // Prints 'A' (65)
    let instructions = parse(program)?;

    let mut input = gyrus::io::StringIo::empty();
    let mut output = UppercaseOutput::new();

    interpret_with_io(
        &instructions,
        ExecutionConfig::default(),
        &mut input,
        &mut output,
        None,
    )?;

    println!("Output: {}", output.get_output());
    println!();

    // Example 2: ROT13 input transformation
    println!("Example 2: ROT13 Input");
    println!("----------------------");
    let echo = ",[.,]";
    let instructions = parse(echo)?;

    let mut input = Rot13Input::new("Hello");
    let mut output = gyrus::io::StringIo::empty();

    interpret_with_io(
        &instructions,
        ExecutionConfig::default(),
        &mut input,
        &mut output,
        None,
    )?;

    println!("Original input: Hello");
    println!("ROT13 applied:  {}", output.output_string());
    println!();

    // Example 3: Logging output
    println!("Example 3: Logging Output");
    println!("-------------------------");
    let program = "+++.++."; // Print 3, then 5
    let instructions = parse(program)?;

    let mut input = gyrus::io::StringIo::empty();
    let mut output = LoggingOutput::new();

    interpret_with_io(
        &instructions,
        ExecutionConfig::default(),
        &mut input,
        &mut output,
        None,
    )?;

    println!("\nFinal output: {:?}", output.get_output().as_bytes());
    println!();

    println!("=== Custom I/O examples completed! ===");
    println!("\nThese custom I/O implementations can be used for:");
    println!("  - File I/O");
    println!("  - Network sockets");
    println!("  - GUI integration");
    println!("  - Testing and mocking");
    println!("  - Data transformation");

    Ok(())
}
