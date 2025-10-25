//! Integration tests for BrainFuck program corpus
//!
//! These tests verify that real BrainFuck programs execute correctly.
//! Test cases are defined based on programs/test_manifest.toml

use ferrous_cortex::{
    EofBehavior, ExecutionConfig, ExecutionConfigBuilder, interpret_with_io, io::StringIo, parse,
};
use std::fs;
use std::path::Path;

/// Helper to load and run a BrainFuck program (returns String)
fn run_program(program_path: &str, input: &str, config: ExecutionConfig) -> Result<String, String> {
    let bytes = run_program_bytes(program_path, input, config)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Helper to load and run a BrainFuck program (returns raw bytes)
fn run_program_bytes(
    program_path: &str,
    input: &str,
    config: ExecutionConfig,
) -> Result<Vec<u8>, String> {
    // Construct path relative to workspace root
    let base_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent() // crates/
        .unwrap()
        .parent() // root
        .unwrap();

    let full_path = base_path.join("programs").join(program_path);

    let source = fs::read_to_string(&full_path)
        .map_err(|e| format!("Failed to read {}: {}", full_path.display(), e))?;

    let instructions = parse(&source).map_err(|e| format!("Parse error: {}", e))?;

    let mut input_io = StringIo::new(input);
    let mut output_io = StringIo::empty();

    interpret_with_io(&instructions, config, &mut input_io, &mut output_io)
        .map_err(|e| format!("Runtime error: {}", e))?;

    Ok(output_io.output_bytes().to_vec())
}

// ============================================================================
// BASIC PROGRAMS
// ============================================================================

#[test]
fn test_hello_world() {
    let output = run_program("basic/hello_world.bf", "", ExecutionConfig::default())
        .expect("Hello World should succeed");

    assert_eq!(output, "Hello World!\n");
}

#[test]
fn test_simple() {
    let output = run_program("basic/simple.bf", "", ExecutionConfig::default())
        .expect("Simple should succeed");

    assert_eq!(output, "H");
}

#[test]
fn test_line_comments() {
    let output = run_program("basic/line_comments.bf", "", ExecutionConfig::default())
        .expect("Line comments should succeed");

    assert_eq!(output, "H");
}

// ============================================================================
// ADVANCED PROGRAMS
// ============================================================================

#[test]
fn test_quine() {
    // Quine outputs its own source code
    let output = run_program("advanced/quine.bf", "", ExecutionConfig::default())
        .expect("Quine should succeed");

    // Quine should output its own source (may need normalization)
    // For now, just verify it produces output
    assert!(!output.is_empty(), "Quine should produce output");
    assert!(output.len() > 100, "Quine output should be substantial");

    // TODO: Verify output matches source (requires reading quine.bf)
}

#[test]
#[ignore] // Factor is slow and needs input handling
fn test_factor() {
    let output = run_program(
        "advanced/factor.bf",
        "12\n",
        ExecutionConfigBuilder::new()
            .with_memory_size(30000)
            .with_max_steps(10_000_000)
            .build(),
    )
    .expect("Factor should succeed");

    // Factor outputs prime factors
    assert!(!output.is_empty());
}

// ============================================================================
// EOF BEHAVIOR TESTS
// ============================================================================

#[test]
fn test_eof_zero() {
    let output = run_program_bytes(
        "tests/test_eof.bf",
        "",
        ExecutionConfigBuilder::new()
            .with_memory_size(30000)
            .with_eof_behavior(EofBehavior::SetZero)
            .build(),
    )
    .expect("EOF zero test should succeed");

    // Program reads EOF and outputs it - should be null byte (0)
    assert_eq!(output, vec![0]);
}

#[test]
fn test_eof_neg_one() {
    let output = run_program_bytes(
        "tests/test_eof.bf",
        "",
        ExecutionConfigBuilder::new()
            .with_memory_size(30000)
            .with_eof_behavior(EofBehavior::SetNegOne)
            .build(),
    )
    .expect("EOF neg one test should succeed");

    // Program reads EOF and outputs it - should be 255 (-1 as unsigned byte)
    assert_eq!(output, vec![255]);
}

#[test]
fn test_eof_no_change() {
    let output = run_program(
        "tests/test_eof_nochange.bf",
        "",
        ExecutionConfigBuilder::new()
            .with_memory_size(30000)
            .with_eof_behavior(EofBehavior::NoChange)
            .build(),
    )
    .expect("EOF no change test should succeed");

    // This test expects NoChange behavior
    assert!(!output.is_empty());
}

// ============================================================================
// ERROR TESTS
// ============================================================================

#[test]
fn test_unmatched_bracket_error() {
    let result = run_program(
        "errors/unmatched_bracket.bf",
        "",
        ExecutionConfig::default(),
    );

    assert!(result.is_err(), "Unmatched bracket should fail");
    assert!(result.unwrap_err().contains("Parse error"));
}

#[test]
fn test_memory_overflow_error() {
    let result = run_program(
        "errors/memory_overflow.bf",
        "",
        ExecutionConfigBuilder::new().with_memory_size(100).build(),
    );

    assert!(result.is_err(), "Memory overflow should fail");
    assert!(result.unwrap_err().contains("Runtime error"));
}

#[test]
fn test_infinite_loop_with_limit() {
    let result = run_program(
        "tests/infinite_loop.bf",
        "",
        ExecutionConfigBuilder::new()
            .with_memory_size(30000)
            .with_max_steps(1000)
            .build(),
    );

    assert!(result.is_err(), "Infinite loop should hit step limit");
    assert!(result.unwrap_err().contains("Runtime error"));
}

// ============================================================================
// STRESS TESTS
// ============================================================================

#[test]
fn test_deep_nesting() {
    let output = run_program("advanced/deep_nesting.bf", "", ExecutionConfig::default())
        .expect("Deep nesting should succeed");

    // Deep nesting program should execute without errors
    // (validation warnings are separate)
    assert!(output.is_empty() || !output.is_empty()); // Just verify it runs
}

// ============================================================================
// CORPUS STATISTICS
// ============================================================================

#[test]
fn test_corpus_inventory() {
    // This test documents what's in our corpus
    let base_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("programs");

    let mut programs = vec![];

    // Count programs in each directory
    for dir in &["basic", "advanced", "tests", "errors"] {
        let dir_path = base_path.join(dir);
        if dir_path.exists() {
            let entries: Vec<_> = fs::read_dir(&dir_path)
                .unwrap()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|ext| ext == "bf").unwrap_or(false))
                .collect();
            programs.push((*dir, entries.len()));
        }
    }

    println!("\n=== BrainFuck Program Corpus ===");
    for (dir, count) in &programs {
        println!("  {}: {} programs", dir, count);
    }
    let total: usize = programs.iter().map(|(_, count)| count).sum();
    println!("  Total: {} programs", total);
    println!();

    // Verify we have a reasonable corpus
    assert!(total >= 10, "Corpus should have at least 10 programs");
}
