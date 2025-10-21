# PRD: Architectural Improvements for Maintainability and Extensibility

## Overview

Enhance FerrousCortex's architecture to improve maintainability, extensibility, and usability as a library. This PRD addresses critical design limitations identified during the architectural review that block advanced features and library usage.

## Current State

### Strengths
- ✅ Excellent modular architecture (11 well-organized modules)
- ✅ Strong type safety with newtype pattern
- ✅ Comprehensive error handling with rich context
- ✅ Good test coverage (64 tests, 57 running)
- ✅ Clean code with zero technical debt markers
- ✅ Idiomatic Rust patterns (builder, type-states, Result types)

### Metrics (as of October 2025)
- **Total LOC**: ~2,122 lines
- **Modules**: 11 modules with clear responsibilities
- **Public API items**: 52
- **Tests**: 64 tests
- **Examples**: 18+ BrainFuck example files
- **Module-level docs**: 0 ⚠️
- **Clippy warnings**: 0 (fixed)

### ~~Critical Limitations~~ ✅ RESOLVED

#### ~~1. Hardcoded I/O~~ ✅ FIXED (Phase 2 Complete)
**Status**: ✅ COMPLETED - I/O abstraction fully implemented

**Solution Implemented**:
- ✅ `BfInput` and `BfOutput` traits in `crates/ferrous-cortex/src/io.rs`
- ✅ `StringIo` for testing and library usage
- ✅ `StdInput`/`StdOutput` for CLI backward compatibility
- ✅ `interpret_with_io()` function for custom I/O
- ✅ All tests converted to use StringIo
- ✅ CLI still works with stdin/stdout via `interpret_with_config()`

**Impact** ✅:
- ✅ Can test with custom input/output (StringIo)
- ✅ Can use interpreter as library with string I/O
- ✅ Can capture output programmatically
- ✅ Can support file I/O or network I/O
- ✅ Unblocks REPL implementation
- ✅ Unblocks debugger with step-through
- ✅ Unblocks GUI integration
- ✅ Can use in embedded contexts

#### 2. Missing Documentation ⏳ IN PROGRESS
**Problem**: Zero module-level documentation (`//!`)
**Status**: ⏳ NOT COMPLETED - Module docs still missing

**Current State**:
- ❌ No module-level docs (`//!`) in lib.rs or individual modules
- ❌ Generated docs lack context
- ✅ Good function-level documentation exists
- ✅ README is comprehensive

**Remaining Work**: Phase 1 of this PRD (add `//!` docs to all 11 modules)

#### 3. Limited Testing Infrastructure ⏳ IN PROGRESS
**Problem**: No property-based testing, no benchmarks
**Status**: ⏳ PARTIALLY COMPLETED

**Current State**:
- ✅ 67 unit tests (good coverage of core functionality)
- ✅ I/O abstraction enables better testing
- ❌ No dev-dependencies for proptest/criterion
- ❌ No benchmarks directory
- ❌ No integration tests directory
- ❌ No property-based tests

**Remaining Work**: Phase 4 of this PRD (add proptest, criterion)

#### 4. No Plugin/Hook Architecture
**Problem**: Cannot extend with custom behavior or instrumentation

**Impact**:
- Cannot add debugging hooks (breakpoints, step-through)
- Cannot add profiling/instrumentation
- Cannot implement memory access tracing
- Cannot support custom instruction extensions

#### 5. No Code Examples ❌ NOT STARTED
**Problem**: No Rust code examples in `examples/` directory, only BrainFuck files
**Status**: ❌ NOT COMPLETED

**Current State**:
- ❌ No .rs files in examples/ directory
- ✅ Many .bf example programs exist
- Users must read tests to understand API usage

**Remaining Work**: Phase 3 of this PRD (create 4+ .rs examples)

## Goals

### Primary Goals (Must-Have)
1. **I/O Abstraction**: Decouple interpreter from stdin/stdout using traits
2. **Documentation**: Add comprehensive module-level and API documentation
3. **Library Usability**: Enable easy integration as a library with examples

### Secondary Goals (Should-Have)
4. **Testing Infrastructure**: Add property-based testing and benchmarks
5. **Extensibility**: Add hook/plugin architecture for advanced features

### Non-Goals (Future Work)
- Trait-based memory models (breaking change, defer until needed)
- Complete rewrite of interpreter architecture
- Performance optimizations (covered in separate PRD)

## Success Metrics

- ✅ Can run interpreter tests with string-based I/O [COMPLETED]
- ⏳ All public modules have module-level documentation [IN PROGRESS]
- ❌ At least 3 runnable code examples in `examples/` [NOT STARTED]
- ⏳ Generated docs (`cargo doc`) are comprehensive and clear [PARTIALLY DONE]
- ✅ Can capture and verify interpreter output in tests [COMPLETED]
- ✅ Zero breaking changes to existing CLI behavior [COMPLETED]

## Overall Status

**Phase 1: Documentation** ⏳ NOT STARTED
**Phase 2: I/O Abstraction** ✅ COMPLETED
**Phase 3: Code Examples** ❌ NOT STARTED
**Phase 4: Testing Infrastructure** ❌ NOT STARTED
**Phase 5: Execution Hooks** ❌ NOT STARTED (Future work)

## Detailed Implementation Steps

### Phase 1: Documentation (Priority: HIGH | Effort: 1-2h | Risk: NONE)

#### 1.1 Add Crate-Level Documentation
**File**: `crates/ferrous-cortex/src/lib.rs`

**Changes**:
```rust
//! # FerrousCortex
//!
//! A production-grade BrainFuck interpreter and debugger written in Rust.
//!
//! FerrousCortex provides a fast, safe, and extensible BrainFuck interpreter
//! with rich error messages, configurable memory models, and comprehensive
//! validation.
//!
//! ## Features
//!
//! - **Multiple memory models**: Fixed, wrapping, and unbounded memory
//! - **Configurable EOF behavior**: Zero, -1, no-change, or error
//! - **Rich error messages**: Memory dumps, hints, and error chaining
//! - **Validation**: Detect suspicious patterns and potential issues
//! - **Type safety**: Newtype pattern prevents type confusion
//!
//! ## Quick Start
//!
//! ```rust
//! use ferrous_cortex::{parse, interpret_with_config, ExecutionConfigBuilder};
//!
//! let source = "++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.>>.<-.<.+++.------.--------.>>+.>++.";
//! let instructions = parse(source)?;
//!
//! let config = ExecutionConfigBuilder::new()
//!     .with_memory_size(30000)
//!     .build();
//!
//! let stats = interpret_with_config(&instructions, config)?;
//! println!("Executed {} steps", stats.total_steps);
//! # Ok::<(), ferrous_cortex::BfError>(())
//! ```
//!
//! ## Examples
//!
//! See the `examples/` directory for complete usage examples.

// ... existing module declarations
```

#### 1.2 Add Module-Level Documentation
**Files**: All 11 modules in `crates/ferrous-cortex/src/`

**config.rs**:
```rust
//! Memory management and execution configuration.
//!
//! This module provides the [`ExecutionConfig`] and [`MemoryModel`] types
//! for configuring interpreter behavior, along with a type-safe builder
//! pattern for constructing configurations.
//!
//! # Examples
//!
//! ```rust
//! use ferrous_cortex::{ExecutionConfigBuilder, EofBehavior};
//!
//! // Create a configuration with wrapping memory
//! let config = ExecutionConfigBuilder::new()
//!     .with_wrapping_memory(30000)
//!     .with_eof_behavior(EofBehavior::SetZero)
//!     .with_max_steps(1_000_000)
//!     .build();
//! ```
```

**error.rs**:
```rust
//! Error types and error handling utilities.
//!
//! This module defines [`BfError`] for runtime errors and [`BfWarning`]
//! for validation warnings. Errors include rich context like memory dumps,
//! actionable hints, and source error chains.
//!
//! # Examples
//!
//! ```rust
//! use ferrous_cortex::BfError;
//!
//! fn handle_error(err: BfError) {
//!     // Print detailed error with hints and context
//!     eprintln!("{}", err.format_detailed());
//!
//!     // Access specific error information
//!     if let Some(hint) = err.hint() {
//!         eprintln!("Hint: {}", hint);
//!     }
//! }
//! ```
```

**interpreter.rs**:
```rust
//! BrainFuck instruction execution engine.
//!
//! This module provides the core interpreter that executes BrainFuck
//! instructions with configurable memory models, EOF behavior, and
//! resource limits.
//!
//! # Examples
//!
//! ```rust
//! use ferrous_cortex::{parse, interpret_with_config, ExecutionConfig};
//!
//! let instructions = parse("+[>+]")?;
//! let config = ExecutionConfig::default();
//! let stats = interpret_with_config(&instructions, config)?;
//! # Ok::<(), ferrous_cortex::BfError>(())
//! ```
```

**parser.rs**:
```rust
//! BrainFuck source code parser.
//!
//! Parses BrainFuck source code into an Abstract Syntax Tree (AST)
//! of [`Instruction`] nodes. Validates bracket matching and provides
//! detailed error messages with source context.
//!
//! # Examples
//!
//! ```rust
//! use ferrous_cortex::parse;
//!
//! let instructions = parse("++[>++<-]")?;
//! # Ok::<(), ferrous_cortex::BfError>(())
//! ```
```

**validator.rs**:
```rust
//! Program validation and warning detection.
//!
//! Analyzes parsed BrainFuck programs for potentially problematic
//! patterns like empty loops, infinite loops, and extreme nesting.
//!
//! # Examples
//!
//! ```rust
//! use ferrous_cortex::{parse, validate};
//!
//! let instructions = parse("[+]")?;  // Infinite loop
//! let warnings = validate(&instructions);
//! for warning in warnings {
//!     eprintln!("{}", warning);
//! }
//! # Ok::<(), ferrous_cortex::BfError>(())
//! ```
```

**types.rs**:
```rust
//! Type-safe wrappers for interpreter primitives.
//!
//! Provides newtype wrappers ([`MemoryAddress`], [`MemorySize`],
//! [`StepCount`], [`InstructionIndex`]) that prevent mixing up
//! logically distinct concepts at compile time.
```

**stats.rs**:
```rust
//! Execution statistics and performance metrics.
//!
//! Tracks interpreter execution statistics like step count,
//! memory usage, and I/O operations.
```

**instruction.rs**:
```rust
//! BrainFuck instruction types.
//!
//! Defines the Abstract Syntax Tree (AST) node type [`Instruction`]
//! representing parsed BrainFuck commands.
```

**location.rs**:
```rust
//! Source code location tracking.
//!
//! Provides [`SourceLocation`] for tracking line/column positions
//! in source code for error reporting.
```

**minify.rs**:
```rust
//! BrainFuck code minification.
//!
//! Removes comments and non-instruction characters from BrainFuck
//! source code, producing minimal valid BrainFuck output.
```

#### 1.3 Testing
- ✅ Run `cargo doc --open` and verify all modules are documented
- ✅ Check that examples compile and display correctly
- ✅ Verify navigation between modules works

**Success Criteria**:
- All 11 modules have module-level documentation
- `cargo doc` generates comprehensive documentation
- Examples in docs are tested via doc tests

---

### Phase 2: I/O Abstraction (Priority: CRITICAL | Effort: 2-4h | Risk: MEDIUM)

#### 2.1 Define I/O Traits
**New file**: `crates/ferrous-cortex/src/io.rs`

**Changes**:
```rust
//! I/O abstraction for BrainFuck interpreter.
//!
//! Provides traits for abstracting input and output operations,
//! enabling custom I/O implementations for testing, GUI integration,
//! file operations, and more.

use std::io;

/// Input source for BrainFuck `,` (input) instruction.
///
/// Implementations can provide input from stdin, strings, files,
/// network sockets, or any other source.
pub trait BfInput {
    /// Read a single byte.
    ///
    /// Returns `Ok(Some(byte))` if a byte is available,
    /// `Ok(None)` if EOF is reached,
    /// or `Err(e)` on I/O errors.
    fn read_byte(&mut self) -> io::Result<Option<u8>>;
}

/// Output destination for BrainFuck `.` (output) instruction.
///
/// Implementations can write output to stdout, strings, files,
/// network sockets, or any other destination.
pub trait BfOutput {
    /// Write a single byte.
    fn write_byte(&mut self, byte: u8) -> io::Result<()>;

    /// Flush output buffer (optional, default is no-op).
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Standard input from stdin.
#[derive(Debug, Default)]
pub struct StdInput;

impl BfInput for StdInput {
    fn read_byte(&mut self) -> io::Result<Option<u8>> {
        use std::io::Read;
        let mut buf = [0u8; 1];
        match io::stdin().read_exact(&mut buf) {
            Ok(_) => Ok(Some(buf[0])),
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// Standard output to stdout.
#[derive(Debug, Default)]
pub struct StdOutput;

impl BfOutput for StdOutput {
    fn write_byte(&mut self, byte: u8) -> io::Result<()> {
        use std::io::Write;
        io::stdout().write_all(&[byte])
    }

    fn flush(&mut self) -> io::Result<()> {
        use std::io::Write;
        io::stdout().flush()
    }
}

/// String-based I/O for testing and library usage.
///
/// # Examples
///
/// ```rust
/// use ferrous_cortex::io::{StringIo, BfInput, BfOutput};
///
/// let mut io = StringIo::new("ABC");
/// assert_eq!(io.read_byte().unwrap(), Some(b'A'));
/// io.write_byte(b'X').unwrap();
/// assert_eq!(io.output_string(), "X");
/// ```
#[derive(Debug, Clone)]
pub struct StringIo {
    input: Vec<u8>,
    input_pos: usize,
    output: Vec<u8>,
}

impl StringIo {
    /// Create new string-based I/O with given input.
    pub fn new(input: &str) -> Self {
        Self {
            input: input.as_bytes().to_vec(),
            input_pos: 0,
            output: Vec::new(),
        }
    }

    /// Create with empty input.
    pub fn empty() -> Self {
        Self::new("")
    }

    /// Get output as string (lossy UTF-8 conversion).
    pub fn output_string(&self) -> String {
        String::from_utf8_lossy(&self.output).into_owned()
    }

    /// Get output as raw bytes.
    pub fn output_bytes(&self) -> &[u8] {
        &self.output
    }
}

impl BfInput for StringIo {
    fn read_byte(&mut self) -> io::Result<Option<u8>> {
        if self.input_pos < self.input.len() {
            let byte = self.input[self.input_pos];
            self.input_pos += 1;
            Ok(Some(byte))
        } else {
            Ok(None)
        }
    }
}

impl BfOutput for StringIo {
    fn write_byte(&mut self, byte: u8) -> io::Result<()> {
        self.output.push(byte);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_io_input() {
        let mut io = StringIo::new("ABC");
        assert_eq!(io.read_byte().unwrap(), Some(b'A'));
        assert_eq!(io.read_byte().unwrap(), Some(b'B'));
        assert_eq!(io.read_byte().unwrap(), Some(b'C'));
        assert_eq!(io.read_byte().unwrap(), None);
    }

    #[test]
    fn test_string_io_output() {
        let mut io = StringIo::empty();
        io.write_byte(b'H').unwrap();
        io.write_byte(b'i').unwrap();
        assert_eq!(io.output_string(), "Hi");
    }
}
```

#### 2.2 Update Interpreter Signature
**File**: `crates/ferrous-cortex/src/interpreter.rs`

**Changes**:
```rust
use crate::io::{BfInput, BfOutput, StdInput, StdOutput};

/// Interpret with custom I/O (primary function).
///
/// # Examples
///
/// ```rust
/// use ferrous_cortex::{parse, interpret_with_io, io::StringIo, ExecutionConfig};
///
/// let instructions = parse(",[.,]")?;
/// let mut io = StringIo::new("Hi");
/// let stats = interpret_with_io(&instructions, ExecutionConfig::default(), &mut io)?;
/// assert_eq!(io.output_string(), "Hi");
/// # Ok::<(), ferrous_cortex::BfError>(())
/// ```
pub fn interpret_with_io<I: BfInput, O: BfOutput>(
    instructions: &[Instruction],
    config: ExecutionConfig,
    io: &mut impl BfIo,  // Combined trait or separate I and O
) -> Result<ExecutionStats> {
    // ... existing logic, but use io.read_byte() and io.write_byte()
}

/// Convenience function using stdin/stdout (backward compatible).
pub fn interpret_with_config(
    instructions: &[Instruction],
    config: ExecutionConfig,
) -> Result<ExecutionStats> {
    let mut io = StdIo::default();
    interpret_with_io(instructions, config, &mut io)
}

// Alternative: Split I/O into separate parameters
pub fn interpret_with_io<I: BfInput, O: BfOutput>(
    instructions: &[Instruction],
    config: ExecutionConfig,
    input: &mut I,
    output: &mut O,
) -> Result<ExecutionStats>
```

**Implementation details**:
```rust
// Replace hardcoded stdin/stdout:
Instruction::Output => {
    // OLD: io::stdout().write_all(&[memory[pointer.get()]])?;
    // NEW:
    output.write_byte(memory[pointer.get()]).map_err(|source| {
        BfError::IoError {
            operation: "writing output".to_string(),
            instruction_index: Some((*step_count).into()),
            source,
        }
    })?;
    stats.bytes_written += 1;
}

Instruction::Input => {
    // OLD: io::stdin().read_exact(&mut buf)?;
    // NEW:
    match input.read_byte() {
        Ok(Some(byte)) => {
            memory[pointer.get()] = byte;
            stats.bytes_read += 1;
        }
        Ok(None) => {
            // Handle EOF based on configuration
            match config.eof_behavior() {
                EofBehavior::SetZero => memory[pointer.get()] = 0,
                EofBehavior::SetNegOne => memory[pointer.get()] = 255,
                EofBehavior::NoChange => { /* do nothing */ }
                EofBehavior::Error => {
                    return Err(BfError::IoError {
                        operation: "reading input (EOF reached)".to_string(),
                        instruction_index: Some((*step_count).into()),
                        source: io::Error::new(io::ErrorKind::UnexpectedEof, "EOF"),
                    });
                }
            }
        }
        Err(source) => {
            return Err(BfError::IoError {
                operation: "reading input".to_string(),
                instruction_index: Some((*step_count).into()),
                source,
            });
        }
    }
}
```

#### 2.3 Update lib.rs Exports
**File**: `crates/ferrous-cortex/src/lib.rs`

**Changes**:
```rust
mod io;  // Add module

pub use io::{BfInput, BfOutput, StdInput, StdOutput, StringIo};  // Export types
pub use interpreter::{interpret, interpret_with_config, interpret_with_io};  // Add new function
```

#### 2.4 Update CLI
**File**: `crates/ferrous-cortex-cli/src/main.rs`

**Changes**:
```rust
// No changes needed! interpret_with_config still uses stdin/stdout
let stats = interpret_with_config(&instructions, config)?;
```

#### 2.5 Update Tests
**Files**: All test modules

**Changes**:
```rust
// OLD:
let stats = interpret_with_config(&instructions, config)?;

// NEW (for tests that need to verify output):
use ferrous_cortex::io::StringIo;

let mut io = StringIo::new("input data");
let stats = interpret_with_io(&instructions, config, &mut io)?;
assert_eq!(io.output_string(), "expected output");
```

#### 2.6 Testing
- ✅ Convert all existing tests to use `StringIo`
- ✅ Add tests for `StdInput`/`StdOutput` (manual verification)
- ✅ Test EOF behavior with custom I/O
- ✅ Verify CLI still works with stdin/stdout
- ✅ All 57+ tests pass

**Success Criteria**:
- All tests use `StringIo` and can verify output
- CLI behavior unchanged (uses stdin/stdout)
- Can capture and verify interpreter output in tests
- Zero breaking changes to public API (new functions added)

---

### Phase 3: Code Examples (Priority: HIGH | Effort: 1-2h | Risk: NONE)

#### 3.1 Create Basic Usage Example
**New file**: `examples/basic_usage.rs`

```rust
//! Basic BrainFuck interpreter usage.

use ferrous_cortex::{parse, interpret_with_io, io::StringIo, ExecutionConfigBuilder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse a simple "Hello World" program
    let source = r#"
        ++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.
        +++++++..+++.>>.<-.<.+++.------.--------.>>+.>++.
    "#;

    let instructions = parse(source)?;
    println!("Parsed {} instructions", instructions.len());

    // Create a configuration
    let config = ExecutionConfigBuilder::new()
        .with_memory_size(30000)
        .build();

    // Run with string I/O (no actual input needed for this program)
    let mut io = StringIo::empty();
    let stats = interpret_with_io(&instructions, &config, &mut io)?;

    println!("Output: {}", io.output_string());
    println!("\nExecution statistics:");
    println!("  Steps: {}", stats.total_steps);
    println!("  Peak memory: {} cells", stats.peak_memory_used);
    println!("  Bytes written: {}", stats.bytes_written);

    Ok(())
}
```

#### 3.2 Create Custom Configuration Example
**New file**: `examples/custom_config.rs`

```rust
//! Demonstrating different memory models and configurations.

use ferrous_cortex::{
    parse, interpret_with_io, io::StringIo,
    ExecutionConfigBuilder, EofBehavior,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = "+[>+]";  // Infinite loop, will hit step limit
    let instructions = parse(source)?;

    // Example 1: Fixed memory with step limit
    println!("=== Fixed Memory with Step Limit ===");
    let config = ExecutionConfigBuilder::new()
        .with_memory_size(100)
        .with_max_steps(1000)
        .build();

    let mut io = StringIo::empty();
    match interpret_with_io(&instructions, &config, &mut io) {
        Ok(stats) => println!("Completed in {} steps", stats.total_steps),
        Err(e) => println!("Error: {}", e),
    }

    // Example 2: Wrapping memory
    println!("\n=== Wrapping Memory ===");
    let config = ExecutionConfigBuilder::new()
        .with_wrapping_memory(10)
        .with_max_steps(100)
        .build();

    let mut io = StringIo::empty();
    match interpret_with_io(&instructions, &config, &mut io) {
        Ok(stats) => println!("Completed in {} steps", stats.total_steps),
        Err(e) => println!("Error: {}", e),
    }

    // Example 3: Custom EOF behavior
    println!("\n=== Custom EOF Behavior ===");
    let source = ",.";  // Read and echo
    let instructions = parse(source)?;

    let config = ExecutionConfigBuilder::new()
        .with_memory_size(100)
        .with_eof_behavior(EofBehavior::SetZero)
        .build();

    let mut io = StringIo::empty();  // No input, will hit EOF
    let stats = interpret_with_io(&instructions, &config, &mut io)?;
    println!("EOF set cell to 0, output byte: {:?}", io.output_bytes());

    Ok(())
}
```

#### 3.3 Create Error Handling Example
**New file**: `examples/error_handling.rs`

```rust
//! Demonstrating comprehensive error handling.

use ferrous_cortex::{parse, interpret_with_io, io::StringIo, ExecutionConfigBuilder, BfError};

fn main() {
    demonstrate_parse_error();
    demonstrate_memory_error();
    demonstrate_step_limit();
}

fn demonstrate_parse_error() {
    println!("=== Parse Error ===");
    let source = "++[>++";  // Unmatched bracket
    match parse(source) {
        Ok(_) => println!("Parsed successfully"),
        Err(e) => println!("{}", e.format_detailed()),
    }
}

fn demonstrate_memory_error() {
    println!("\n=== Memory Out of Bounds ===");
    let source = ">>>>>>>>>>>>>>>>>";  // Try to move beyond memory
    let instructions = parse(source).unwrap();

    let config = ExecutionConfigBuilder::new()
        .with_memory_size(10)  // Small memory
        .build();

    let mut io = StringIo::empty();
    match interpret_with_io(&instructions, &config, &mut io) {
        Ok(_) => println!("Executed successfully"),
        Err(e) => println!("{}", e.format_detailed()),
    }
}

fn demonstrate_step_limit() {
    println!("\n=== Step Limit Exceeded ===");
    let source = "+[>+]";  // Infinite loop
    let instructions = parse(source).unwrap();

    let config = ExecutionConfigBuilder::new()
        .with_memory_size(1000)
        .with_max_steps(100)
        .build();

    let mut io = StringIo::empty();
    match interpret_with_io(&instructions, &config, &mut io) {
        Ok(_) => println!("Executed successfully"),
        Err(e) => {
            println!("{}", e.format_detailed());

            // Access specific error information
            if let Some(hint) = e.hint() {
                println!("\nProgrammatic hint access: {}", hint);
            }
        }
    }
}
```

#### 3.4 Create String I/O Example
**New file**: `examples/string_io.rs`

```rust
//! Using the interpreter as a library with string I/O.

use ferrous_cortex::{parse, interpret_with_io, io::StringIo, ExecutionConfigBuilder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Example: Echo program that reads and outputs characters
    let echo_program = ",[.,]";  // Read char, output char, loop
    let instructions = parse(echo_program)?;

    let config = ExecutionConfigBuilder::new()
        .with_memory_size(100)
        .build();

    // Test with different inputs
    for input in &["Hello", "World", "BrainFuck!"] {
        let mut io = StringIo::new(input);
        interpret_with_io(&instructions, &config, &mut io)?;
        println!("Input: '{}' -> Output: '{}'", input, io.output_string());
        assert_eq!(io.output_string(), *input);
    }

    println!("\nAll tests passed!");
    Ok(())
}
```

#### 3.5 Update Cargo.toml
**File**: `Cargo.toml` (workspace root)

Add example metadata if needed:
```toml
[[example]]
name = "basic_usage"
path = "examples/basic_usage.rs"

[[example]]
name = "custom_config"
path = "examples/custom_config.rs"

[[example]]
name = "error_handling"
path = "examples/error_handling.rs"

[[example]]
name = "string_io"
path = "examples/string_io.rs"
```

#### 3.6 Testing
- ✅ Run `cargo run --example basic_usage`
- ✅ Run `cargo run --example custom_config`
- ✅ Run `cargo run --example error_handling`
- ✅ Run `cargo run --example string_io`
- ✅ Verify all examples compile and run successfully

**Success Criteria**:
- At least 4 runnable code examples
- Examples cover basic usage, configuration, error handling, and I/O
- All examples compile and run successfully
- Examples are referenced in documentation

---

### Phase 4: Testing Infrastructure (Priority: MEDIUM | Effort: 2-3h | Risk: NONE)

#### 4.1 Add Development Dependencies
**File**: `crates/ferrous-cortex/Cargo.toml`

**Changes**:
```toml
[dev-dependencies]
proptest = "1.0"      # Property-based testing
criterion = "0.5"     # Benchmarking

[[bench]]
name = "interpreter"
harness = false
```

#### 4.2 Create Property-Based Tests
**New file**: `crates/ferrous-cortex/src/parser.rs` (add to test module)

```rust
#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    // Property: Parsing never panics
    proptest! {
        #[test]
        fn parse_never_panics(source in ".*") {
            let _ = parse(&source);  // Should never panic
        }
    }

    // Property: Valid BrainFuck always parses
    proptest! {
        #[test]
        fn valid_bf_always_parses(instructions in valid_bf_source()) {
            parse(&instructions).unwrap();
        }
    }

    // Generate valid BrainFuck programs
    fn valid_bf_source() -> impl Strategy<Value = String> {
        let instructions = prop::sample::select(vec!['+', '-', '<', '>', '.', ',']);
        let balanced_brackets = prop::collection::vec(instructions, 0..100)
            .prop_map(|chars| {
                let mut result = String::new();
                let mut depth = 0;
                for ch in chars {
                    result.push(ch);
                    if rand::random::<bool>() && depth < 5 {
                        result.push('[');
                        depth += 1;
                    }
                }
                // Close all brackets
                for _ in 0..depth {
                    result.push(']');
                }
                result
            });
        balanced_brackets
    }
}
```

#### 4.3 Create Benchmarks
**New file**: `crates/ferrous-cortex/benches/interpreter.rs`

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use ferrous_cortex::{parse, interpret_with_io, io::StringIo, ExecutionConfigBuilder};

fn bench_simple_loop(c: &mut Criterion) {
    let source = "+++++[>++++[>++<-]<-]";
    let instructions = parse(source).unwrap();
    let config = ExecutionConfigBuilder::new().with_memory_size(1000).build();

    c.bench_function("simple_loop", |b| {
        b.iter(|| {
            let mut io = StringIo::empty();
            interpret_with_io(black_box(&instructions), &config, &mut io).unwrap();
        });
    });
}

fn bench_hello_world(c: &mut Criterion) {
    let source = "++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.";
    let instructions = parse(source).unwrap();
    let config = ExecutionConfigBuilder::new().with_memory_size(1000).build();

    c.bench_function("hello_world", |b| {
        b.iter(|| {
            let mut io = StringIo::empty();
            interpret_with_io(black_box(&instructions), &config, &mut io).unwrap();
        });
    });
}

fn bench_parse(c: &mut Criterion) {
    let source = "+++++[>++++[>++<-]<-]".repeat(10);

    c.bench_function("parse", |b| {
        b.iter(|| {
            parse(black_box(&source)).unwrap();
        });
    });
}

criterion_group!(benches, bench_simple_loop, bench_hello_world, bench_parse);
criterion_main!(benches);
```

#### 4.4 Testing
- ✅ Run `cargo test` - proptest runs automatically
- ✅ Run `cargo bench` - generates benchmark reports
- ✅ Review criterion HTML reports in `target/criterion/`

**Success Criteria**:
- Property-based tests run on every `cargo test`
- Benchmarks can be run with `cargo bench`
- Baseline performance metrics established

---

### Phase 5: Execution Hooks (Priority: LOW | Effort: 3-4h | Risk: MEDIUM)

**Note**: This is for future debugger/REPL features. Can be deferred.

#### 5.1 Define Hook Trait
**New file**: `crates/ferrous-cortex/src/hooks.rs`

```rust
//! Execution hooks for debugging and instrumentation.

use crate::instruction::Instruction;
use crate::types::{MemoryAddress, StepCount};

/// State snapshot for hooks.
#[derive(Debug, Clone)]
pub struct InterpreterState<'a> {
    pub memory: &'a [u8],
    pub pointer: MemoryAddress,
    pub step_count: StepCount,
}

/// Hook that runs before/after each instruction.
pub trait ExecutionHook {
    /// Called before executing an instruction.
    ///
    /// Return `false` to halt execution (breakpoint).
    fn before_instruction(&mut self, instruction: &Instruction, state: &InterpreterState) -> bool {
        let _ = (instruction, state);
        true  // Continue execution
    }

    /// Called after executing an instruction.
    fn after_instruction(&mut self, instruction: &Instruction, state: &InterpreterState) {
        let _ = (instruction, state);
    }
}

/// Example: Step counter hook
#[derive(Debug, Default)]
pub struct StepCounter {
    pub count: usize,
}

impl ExecutionHook for StepCounter {
    fn after_instruction(&mut self, _instruction: &Instruction, _state: &InterpreterState) {
        self.count += 1;
    }
}

/// Example: Memory watch hook
#[derive(Debug)]
pub struct MemoryWatch {
    pub address: usize,
    pub values: Vec<u8>,
}

impl MemoryWatch {
    pub fn new(address: usize) -> Self {
        Self { address, values: Vec::new() }
    }
}

impl ExecutionHook for MemoryWatch {
    fn after_instruction(&mut self, _instruction: &Instruction, state: &InterpreterState) {
        if self.address < state.memory.len() {
            self.values.push(state.memory[self.address]);
        }
    }
}

/// Example: Breakpoint hook
#[derive(Debug)]
pub struct Breakpoint {
    pub step: usize,
}

impl ExecutionHook for Breakpoint {
    fn before_instruction(&mut self, _instruction: &Instruction, state: &InterpreterState) -> bool {
        state.step_count.get() < self.step as u64
    }
}
```

#### 5.2 Update ExecutionConfig
**File**: `crates/ferrous-cortex/src/config.rs`

```rust
use crate::hooks::ExecutionHook;

pub struct ExecutionConfig {
    // ... existing fields
    hooks: Vec<Box<dyn ExecutionHook>>,
}

impl ExecutionConfigBuilder<ReadyToBuild> {
    pub fn with_hook(mut self, hook: Box<dyn ExecutionHook>) -> Self {
        self.hooks.push(hook);
        self
    }
}
```

#### 5.3 Update Interpreter
**File**: `crates/ferrous-cortex/src/interpreter.rs`

```rust
// In execute_block, before each instruction:
for hook in &mut config.hooks {
    if !hook.before_instruction(instruction, &state) {
        // Breakpoint triggered, halt execution
        return Ok(());
    }
}

// After each instruction:
for hook in &mut config.hooks {
    hook.after_instruction(instruction, &state);
}
```

**Success Criteria**:
- Can add hooks to configuration
- Hooks are called before/after each instruction
- Breakpoint hook can halt execution
- Memory watch hook can track cell values

---

## Migration Path

### For Existing Code
- ✅ No breaking changes to CLI
- ✅ `interpret_with_config()` continues to work with stdin/stdout
- ✅ New `interpret_with_io()` function available for custom I/O
- ✅ All existing tests continue to pass

### For Library Users
- **Before**: Limited to stdin/stdout
- **After**: Can use `StringIo` or custom I/O implementations

### Documentation Migration
- Add migration guide in crate-level docs
- Document all new APIs with examples
- Keep backward compatibility for at least 2 versions

## Dependencies

### New Dependencies
```toml
# Dev dependencies only (no new runtime dependencies)
[dev-dependencies]
proptest = "1.0"   # Phase 4
criterion = "0.5"  # Phase 4
```

## Timeline Estimate

- **Phase 1 (Documentation)**: 1-2 hours
- **Phase 2 (I/O Abstraction)**: 2-4 hours
- **Phase 3 (Code Examples)**: 1-2 hours
- **Phase 4 (Testing Infrastructure)**: 2-3 hours (optional)
- **Phase 5 (Execution Hooks)**: 3-4 hours (future work)

**Total (Phases 1-3)**: 4-8 hours
**Total (All phases)**: 9-15 hours

## Risks and Mitigations

### Risk: Breaking Changes
**Mitigation**:
- Keep `interpret_with_config()` unchanged
- Add new functions alongside existing ones
- Extensive testing before release

### Risk: Performance Regression
**Mitigation**:
- Trait objects are zero-cost for monomorphization
- Benchmark before/after (Phase 4)
- Generic parameters avoid vtable overhead

### Risk: API Complexity
**Mitigation**:
- Provide simple defaults (`StringIo`, `StdIo`)
- Extensive documentation and examples
- Keep backward-compatible convenience functions

## Success Criteria

### Phase 1 (Documentation)
- ✅ All 11 modules have module-level docs
- ✅ `cargo doc --open` shows comprehensive documentation
- ✅ Doc examples compile and pass

### Phase 2 (I/O Abstraction)
- ✅ Can run tests with `StringIo`
- ✅ Can capture and verify output
- ✅ CLI behavior unchanged
- ✅ All tests pass

### Phase 3 (Code Examples)
- ✅ At least 4 runnable examples
- ✅ Examples cover common use cases
- ✅ All examples compile and run

### Phase 4 (Testing)
- ✅ Property-based tests run on `cargo test`
- ✅ Benchmarks available via `cargo bench`
- ✅ Performance baseline established

### Phase 5 (Hooks)
- ✅ Can add execution hooks
- ✅ Breakpoint hook works
- ✅ Memory watch hook works

## Related PRDs

- **Error Handling and Reliability** - Already implemented (rich errors, validation)
- **Performance Optimizations** - Benchmarks from Phase 4 enable this
- **Debug Symbols and Runtime Diagnostics** - Enabled by Phase 2 (I/O) and Phase 5 (hooks)

## Questions and Open Issues

1. **Should we make `BfInput` and `BfOutput` separate parameters or combine into a single `BfIo` trait?**
   - Recommendation: Separate for flexibility (some use cases need different input/output)

2. **Should hooks be `&mut dyn` or generic parameters?**
   - Recommendation: `Box<dyn>` for flexibility, generic for performance (provide both)

3. **Should we add async I/O support?**
   - Recommendation: Defer to future PRD, current sync I/O is sufficient

## Conclusion

These architectural improvements address critical gaps in maintainability and extensibility:

1. **Documentation** removes onboarding friction
2. **I/O Abstraction** unblocks REPL, debugger, GUI, and library usage
3. **Code Examples** dramatically improve developer experience
4. **Testing Infrastructure** increases confidence in changes
5. **Execution Hooks** enable advanced debugging features

Phases 1-3 are **recommended before continuing with other PRD items**, especially Phase 2 (I/O Abstraction) which is blocking multiple features.
