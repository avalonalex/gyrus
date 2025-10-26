# FerrousCortex

An industry-strength BrainFuck interpreter/compiler and visual debugger written in Rust.

## Features

- Fast and efficient BrainFuck interpreter
- **Rich error handling** with source location tracking and context
  - Line and column numbers for parse errors
  - Visual error context with caret (^) pointing to issues
  - **Multiple bracket error reporting** - shows ALL errors at once
  - Detailed error messages for debugging
- Support for all 8 BrainFuck commands: `><+-.,[]`
- **Line comments** using `*` - makes documentation safe and easy
- Nested loop support
- **Multiple memory models** for different use cases
  - Fixed: Traditional fixed-size array with bounds checking (production/JIT target)
  - Unbounded: Dynamic growth up to configurable limit (development/prototyping)
- **Configurable execution limits**
  - Maximum step count to prevent infinite loops
  - Execution timeout in milliseconds
  - Customizable memory size
- **Development tools** (`ferrous-cortex-tool`)
  - Program validation with static analysis
  - Code minification (95%+ size reduction)
  - Debug symbol inspection with JSON/CSV/table output
- **Verbose mode** with execution diagnostics
  - Configuration details (memory model, limits, timeout)
  - Execution statistics (step count, loop iterations, memory usage)
  - I/O statistics (bytes read/written)
  - Useful for performance analysis and debugging
- **Configurable EOF handling** for input operations
  - SetZero: Set cell to 0 on EOF (default)
  - SetNegOne: Set cell to 255 (-1) on EOF
  - NoChange: Leave cell unchanged on EOF
  - Error: Fail on EOF with error message
- Production-grade reliability with comprehensive error checking
- Command-line interface with extensive options

## Installation

### Prerequisites

- Rust 1.85+ with edition 2024 support
- Cargo (comes with Rust)

### Building from Source

```bash
git clone <repository-url>
cd FerrousCortex
cargo build --release
```

The compiled binary will be available at `target/release/ferrous-cortex`.

## Usage

FerrousCortex provides two command-line tools:

1. **`ferrous-cortex`** - BrainFuck interpreter for executing programs
2. **`ferrous-cortex-tool`** - Development tools for analyzing and processing BF code

### Running a BrainFuck Program

```bash
cargo run -p ferrous-cortex-cli -- <path-to-bf-file>

# Or using the compiled binary
./target/release/ferrous-cortex <path-to-bf-file>
```

### Examples

#### Running BrainFuck Programs

Run the included example programs from the `programs/` directory:

```bash
# Simple program that prints 'H'
cargo run -- programs/basic/simple.bf

# Classic Hello World
cargo run -- programs/basic/hello_world.bf

# Line comments demonstration
cargo run -- programs/basic/line_comments.bf
```

**Error Handling Examples:**

See the `programs/errors/` directory for comprehensive error handling demonstrations:

```bash
# Parse error with detailed context
cargo run -p ferrous-cortex-cli -- programs/errors/unmatched_bracket.bf

# Memory bounds error
cargo run -p ferrous-cortex-cli -- programs/errors/memory_overflow.bf --memory-size 100

# Infinite loop with step limit
cargo run -p ferrous-cortex-cli -- programs/errors/infinite_loop.bf --max-steps 10000
```

See [`programs/errors/README.md`](programs/errors/README.md) for detailed error examples documentation.

#### Using FerrousCortex as a Library

The `crates/ferrous-cortex/examples/` directory contains Rust examples showing library usage:

```bash
# Basic usage - parsing, execution, error handling
cargo run --example basic_usage

# Custom I/O implementations
cargo run --example custom_io

# Memory model configuration
cargo run --example memory_models

# Program validation
cargo run --example validation

# Code minification
cargo run --example minify
```

See [`crates/ferrous-cortex/examples/README.md`](crates/ferrous-cortex/examples/README.md) for detailed library examples.

### Command-Line Options

#### ferrous-cortex (Interpreter)

Execute BrainFuck programs with configurable runtime options:

```bash
# Show all available options
ferrous-cortex --help

# Run with verbose output
ferrous-cortex program.bf --verbose

# Limit execution to 10000 steps
ferrous-cortex program.bf --max-steps 10000

# Set execution timeout to 5 seconds
ferrous-cortex program.bf --timeout 5000

# Use custom memory size (1MB)
ferrous-cortex program.bf --memory-size 1000000

# Combine multiple options
ferrous-cortex program.bf --verbose --max-steps 100000 --timeout 10000
```

**Available Options:**

| Flag | Description | Default |
|------|-------------|---------|
| `-v, --verbose` | Show detailed execution information and statistics | false |
| `-q, --quiet` | Suppress runtime warnings and non-program output | false |
| `--max-steps <N>` | Maximum number of execution steps (0 = unlimited) | 0 |
| `--timeout <MS>` | Execution timeout in milliseconds (0 = unlimited) | 0 |
| `--memory-size <BYTES>` | Memory size in bytes (for fixed model) | 30000 |
| `--memory-model <MODEL>` | Memory model: fixed or unbounded | fixed |
| `--cell-model <MODEL>` | Cell model: wrapping (production) or checked (debugging) | wrapping |
| `--unbounded-initial <BYTES>` | Initial size for unbounded memory model | 1000 |
| `--unbounded-max <BYTES>` | Maximum size for unbounded memory model | 1000000 |
| `--eof-behavior <BEHAVIOR>` | EOF behavior: zero, neg-one, no-change, or error | zero |

#### ferrous-cortex-tool (Development Tools)

Analyze, validate, and process BrainFuck programs:

```bash
# Show all available commands
ferrous-cortex-tool --help

# Minify a program (strip comments)
ferrous-cortex-tool minify program.bf

# Save minified output to file
ferrous-cortex-tool minify program.bf -o program.min.bf --verbose

# Validate a program and show warnings
ferrous-cortex-tool validate program.bf

# Validate in strict mode (exit with error if warnings found)
ferrous-cortex-tool validate program.bf --strict

# Inspect debug symbols
ferrous-cortex-tool debug-info program.bf

# Output debug info as JSON
ferrous-cortex-tool debug-info program.bf --format json

# View program with syntax highlighting
ferrous-cortex-tool view program.bf --line-numbers

# Plain output (no colors)
ferrous-cortex-tool view program.bf --plain
```

**Available Commands:**

| Command | Description |
|---------|-------------|
| `minify` | Strip comments and whitespace from BF programs |
| `validate` | Validate programs and show static analysis warnings |
| `debug-info` | Inspect debug symbols and source location mappings |
| `view` | Display BF programs with syntax highlighting |

## BrainFuck Language Reference

| Command | Description |
|---------|-------------|
| `>`     | Move pointer right |
| `<`     | Move pointer left |
| `+`     | Increment value at pointer |
| `-`     | Decrement value at pointer |
| `.`     | Output byte at pointer |
| `,`     | Input byte to pointer |
| `[`     | Jump forward past matching `]` if value at pointer is 0 |
| `]`     | Jump back to matching `[` if value at pointer is non-zero |

### Comments

FerrousCortex supports two types of comments:

1. **Implicit comments**: Any character that isn't one of the 8 BF commands is ignored
2. **Line comments**: Use `*` to start a line comment - everything after `*` on that line is ignored

**Example with line comments:**
```brainfuck
* This entire line is a comment
+++      * Set cell 0 to 3
[        * Start loop
  >++    * Cell 1 += 2
  <-     * Cell 0 -= 1
]        * End loop
>.       * Print cell 1

* You can safely use BF commands in comments after *
* Example: >++<-- These won't execute!
```

Line comments make it safe to write documentation without accidentally including BF commands.

## Error Handling

FerrousCortex provides detailed error messages to help debug BrainFuck programs:

### Parse Errors

When your program has syntax errors, you'll see the exact location:

```
Error: Unmatched '[' at line 3, column 12
    1 | +++
    2 | [->+++<]
    3 | Some code [
      |            ^
    4 | More code
```

#### Multiple Bracket Errors

FerrousCortex detects **all** bracket matching errors in a single pass, saving you time by showing all issues at once:

```
Found 3 bracket matching error(s):

Error 1:
Unmatched '[' at line 3, column 1
    1 | * Test file
    2 |
    3 | [>++
      | ^
    4 | [<--
    5 | [+++

Error 2:
Unmatched '[' at line 4, column 1
    3 | [>++
    4 | [<--
      | ^
    5 | [+++
    6 |

Error 3:
Unmatched '[' at line 5, column 1
    4 | [<--
    5 | [+++
      | ^
    6 |
```

This comprehensive error reporting helps you fix all bracket issues in one go instead of fixing them one at a time.

### Runtime Errors

Runtime errors include:
- **Memory out of bounds**: Attempting to access memory outside valid range
- **Step limit exceeded**: Program exceeded maximum allowed instructions
- **Execution timeout**: Program took too long to execute
- **I/O errors**: Problems reading input or writing output

Example:
```
Error: Memory pointer out of bounds at instruction 30001
Attempted to access cell 30000, valid range: 0-29999
```

### Preventing Infinite Loops

Use `--max-steps` or `--timeout` to prevent runaway programs:

```bash
# Prevent infinite loops with step limit
ferrous-cortex suspicious_program.bf --max-steps 1000000

# Or use a timeout
ferrous-cortex suspicious_program.bf --timeout 5000
```

## Memory Models

FerrousCortex supports three different memory models to handle different BrainFuck variants and use cases:

### Fixed Memory (Default)

Traditional BrainFuck behavior with a fixed-size memory array.

```bash
ferrous-cortex program.bf --memory-model fixed --memory-size 30000
```

**Characteristics:**
- Memory size is fixed at startup
- Out-of-bounds access (< 0 or >= size) returns an error
- Most compatible with standard BrainFuck programs
- Best for production use and debugging

### Unbounded Memory

Memory grows dynamically as needed, up to a maximum limit.

```bash
ferrous-cortex program.bf --memory-model unbounded \
  --unbounded-initial 1000 \
  --unbounded-max 1000000
```

**Characteristics:**
- Starts with small initial allocation (default: 1000 bytes)
- Automatically grows when accessing beyond current size
- Maximum size limit prevents runaway memory usage
- Efficient for programs with unpredictable memory needs

**Example:**
```bash
# Start with 100 bytes, allow growth up to 10MB
ferrous-cortex program.bf --memory-model unbounded \
  --unbounded-initial 100 \
  --unbounded-max 10000000
```

### Choosing a Memory Model

- **Fixed**: Use for standard BrainFuck programs, strict bounds checking, and production/JIT targets (default)
- **Unbounded**: Use for programs with unknown memory requirements or when prototyping

## Cell Models and Arithmetic Behavior

FerrousCortex provides **configurable cell arithmetic** to support different use cases. Cell arithmetic is completely independent from memory models - you can mix any cell model with any memory model.

### Understanding Memory vs Cell Models

FerrousCortex distinguishes between two orthogonal (independent) configuration axes:

| Aspect | Controlled By | What It Affects |
|--------|--------------|-----------------|
| **Pointer movement** (`>`, `<`) | `--memory-model` | How pointer moves between cells |
| **Cell arithmetic** (`+`, `-`) | `--cell-model` | How cell values increment/decrement |

These are **completely independent** - you can combine any memory model with any cell model.

### Available Cell Models

Configure cell arithmetic with the `--cell-model` flag:

#### U8 Wrapping (Default - Production)

Standard BrainFuck behavior with wrapping arithmetic. This is the **default** and aligns with traditional BrainFuck semantics and future JIT/AOT compilation.

```bash
ferrous-cortex program.bf --cell-model wrapping
```

**Characteristics:**
- Cell type: `u8` (unsigned 8-bit integer, range 0-255)
- Increment overflow: `255 + 1 = 0` (wraps to zero)
- Decrement underflow: `0 - 1 = 255` (wraps to 255)
- Use case: **Production use, standard BrainFuck programs**

**Example:**
```brainfuck
+++      * Increment 3 times
[-]      * Decrement until zero (standard clear pattern)
```

**Validation behavior:**
- `[+]` → Warning: "Inefficient pattern: loops ~256 times" (NOT infinite, just slow!)
- `[-]` → No warning (idiomatic pattern)

#### U8 Checked (Debugging)

Strict overflow detection mode that raises errors on overflow/underflow. Use this to catch bugs where your program unexpectedly reaches cell boundaries.

```bash
ferrous-cortex program.bf --cell-model checked
```

**Characteristics:**
- Cell type: `u8` (unsigned 8-bit integer, range 0-255)
- Increment overflow: `255 + 1` → **ERROR** (execution stops)
- Decrement underflow: `0 - 1` → **ERROR** (execution stops)
- Use case: **Debugging, finding arithmetic bugs**

**Example error:**
```
Error: Cell overflow at instruction 42: attempted to increment cell with value 255
```

**Validation behavior:**
- `[+]` → Warning: "Will error on overflow with checked arithmetic"
- `[-]` → No warning (will terminate at zero before underflow)

### Combining Models

Since CellModel and MemoryModel are orthogonal, all combinations are valid:

```bash
# Fixed memory + Wrapping cells (traditional BrainFuck, default)
ferrous-cortex program.bf --memory-model fixed --cell-model wrapping

# Fixed memory + Checked cells (strict debugging)
ferrous-cortex program.bf --memory-model fixed --cell-model checked

# Unbounded memory + Wrapping cells (dynamic memory, standard arithmetic)
ferrous-cortex program.bf --memory-model unbounded --cell-model wrapping
```

**Example combinations:**

| Memory Model | Cell Model | Pointer at boundary | Cell at 255, execute `+` |
|--------------|-----------|---------------------|--------------------------|
| Fixed | Wrapping | Error (out of bounds) | Wraps to 0 |
| Fixed | Checked | Error (out of bounds) | Error (overflow) |
| Unbounded | Wrapping | Grows memory | Wraps to 0 |
| Unbounded | Checked | Grows memory | Error (overflow) |

### When to Use Each Cell Model

**Use Wrapping (default) when:**
- Running standard BrainFuck programs
- In production environments
- When you want traditional BrainFuck semantics
- When preparing for JIT/AOT compilation (uses u8 wrapping)

**Use Checked when:**
- Debugging your BrainFuck programs
- Finding arithmetic overflow bugs
- Verifying your program doesn't unexpectedly hit cell boundaries
- Learning BrainFuck and want strict error checking

### Cell-Model-Aware Validation

The validator provides different warnings based on your cell model:

```bash
# Validate with wrapping model
ferrous-cortex program.bf --validate --cell-model wrapping

# Validate with checked model
ferrous-cortex program.bf --validate --cell-model checked
```

**Example - `[+]` pattern:**

With `--cell-model wrapping`:
```
Warning: Inefficient pattern [+]
Inefficient pattern: loops ~256 times before reaching zero. Use [-] to clear a cell.
```

With `--cell-model checked`:
```
Warning: Suspicious pattern [+]
Suspicious pattern: will error on overflow with checked arithmetic.
Cell will reach 255 and then increment will panic.
```

### Practical Examples

**Production execution with wrapping:**
```bash
ferrous-cortex programs/basic/hello_world.bf --verbose
# Configuration:
#   Memory model: Fixed(30000 bytes)
#   Cell model: U8Wrapping
```

**Debug mode with overflow checking:**
```bash
ferrous-cortex my_program.bf --cell-model checked
# Will catch runtime overflow/underflow errors during execution
```

**Testing with different models:**
```bash
# Test with standard wrapping (production)
ferrous-cortex program.bf --cell-model wrapping

# Test with checked mode to find overflow bugs
ferrous-cortex program.bf --cell-model checked
```

## EOF Handling

FerrousCortex provides configurable end-of-file (EOF) handling for the input command (`,`). Different BrainFuck implementations handle EOF differently, so you can choose the behavior that matches your needs.

### EOF Behaviors

Configure EOF handling with the `--eof-behavior` flag:

#### SetZero (Default)

Sets the current cell to 0 when EOF is reached.

```bash
ferrous-cortex program.bf --eof-behavior zero
```

This is the most common behavior and matches many BrainFuck implementations. It's useful for programs that need to detect end of input by checking for a zero value.

**Example:**
```brainfuck
,           * Read input (becomes 0 on EOF)
[           * Loop while not zero (skip if EOF)
  .         * Process the character
  ,         * Read next character
]
```

#### SetNegOne

Sets the current cell to 255 (-1 as unsigned byte) when EOF is reached.

```bash
ferrous-cortex program.bf --eof-behavior neg-one
# Alternatives: negone, -1, 255
```

Some BrainFuck programs use 255 (which represents -1 in two's complement) as an EOF marker.

#### NoChange

Leaves the cell value unchanged when EOF is reached.

```bash
ferrous-cortex program.bf --eof-behavior no-change
# Alternatives: nochange, unchanged
```

This behavior is useful when you want to preserve the previous cell value or have pre-initialized sentinel values.

#### Error

Returns an error and stops execution when EOF is reached.

```bash
ferrous-cortex program.bf --eof-behavior error
```

This is the strictest mode - use it when your program requires valid input and EOF should be treated as an exceptional condition.

**Example error:**
```
Error: End of input reached
```

### Choosing an EOF Behavior

- **SetZero**: Best for most programs, standard behavior
- **SetNegOne**: Use when porting code that expects -1 for EOF
- **NoChange**: Use when you want to preserve cell values across EOF
- **Error**: Use when EOF should terminate execution (strict mode)

## Execution Statistics

FerrousCortex can track and display detailed execution statistics using the `--verbose` flag:

```bash
ferrous-cortex program.bf --verbose
```

Verbose mode shows both the configuration and execution statistics.

**Statistics Collected:**
- **Total steps executed**: Number of instructions executed
- **Loop iterations**: Number of times loop bodies were entered
- **Peak memory used**: Highest memory cell accessed (cells)
- **Memory allocated**: Actual memory allocated (bytes)
- **Cells modified**: Number of memory cells with non-zero values
- **Bytes read**: Total bytes read from input
- **Bytes written**: Total bytes written to output

**Example Output:**
```bash
$ ferrous-cortex programs/basic/hello_world.bf --verbose
Configuration:
  Memory model: Fixed(30000 bytes)
  Max steps: unlimited
  Timeout: unlimitedms

Hello World!

=== Execution Statistics ===
Total steps executed: 826
Loop iterations: 80
Peak memory used: 7 cells
Memory allocated: 30000 bytes
Cells modified: 5
Bytes read: 0
Bytes written: 13
```

**Use Cases:**
- **Performance analysis**: Understand program behavior and complexity
- **Debugging**: Track memory usage and loop execution
- **Optimization**: Identify inefficient code patterns
- **Learning**: See how BrainFuck programs execute internally

## Program Validation

FerrousCortex can validate your BrainFuck programs and warn about potential issues:

```bash
# Validate only (does not execute)
ferrous-cortex program.bf --validate
```

### What Validation Does

**`--validate` (Lint Mode)**
- Parses and analyzes the code for issues
- Shows all warnings (or "No warnings found")
- Never executes the program
- Useful for checking code quality without running

**Validation Target: U8 Wrapping**
- Validation ALWAYS assumes u8 wrapping (production/JIT target)
- Warns about inefficient patterns for standard BrainFuck
- Independent of runtime cell model (`--cell-model` is for runtime only)

### Warning Types

The validator checks for:

- **Empty loops**: `[]` - Does nothing and can be removed
- **Inefficient increment loops**: `[+]` or `[++]` - Loop many times (~256, ~128 iterations) to reach zero via wrapping
- **Extreme nesting**: Loops nested more than 10 levels deep (performance impact)
- **Inefficient patterns**: Multiple operations that could be optimized (e.g., `[--]` instead of `[-]`)

### Example Workflows

```bash
# Development: Check for issues without running
ferrous-cortex program.bf --validate

# CI/CD: Validate, then run if clean
ferrous-cortex program.bf --validate && ferrous-cortex program.bf

# CI/CD with verbose output
ferrous-cortex program.bf --validate && ferrous-cortex program.bf --verbose
```

## Code Minification

Strip all comments and whitespace to create compact BrainFuck programs:

```bash
# Output to stdout
ferrous-cortex program.bf --minify

# Save to file
ferrous-cortex program.bf --minify -o program.min.bf

# With verbose stats
ferrous-cortex program.bf --minify -o program.min.bf --verbose
```

**Example:**
```bash
$ cat programs/basic/line_comments.bf
* Line Comment Demo
* Everything after * is completely ignored!

++++++++++  * Cell 0 = 10
[           * Loop 10 times
  >+++++++  * Cell 1 += 7
  <-        * Cell 0 -= 1
]           * Result: Cell 1 = 70
>++.        * Add 2, print 'H'

$ ferrous-cortex programs/basic/line_comments.bf --minify
++++++++++[>+++++++<-]>++.

$ ferrous-cortex programs/basic/line_comments.bf --minify --verbose -o min.bf
Minified 514 bytes to 26 bytes (saved to min.bf)
```

Minification achieves **95%+ size reduction** by removing:
- All line comments (after `*`)
- All implicit comments (non-BF characters)
- All whitespace and formatting

The minified code is functionally identical to the original.

## Development

### Running Tests

FerrousCortex has a comprehensive testing infrastructure with **137 tests** including unit tests, property-based tests, and benchmarks.

#### Run All Tests

```bash
# Run all unit tests (137 tests)
cargo test

# Run with output
cargo test -- --nocapture
```

#### Run Property-Based Tests

Property-based tests use [proptest](https://github.com/proptest-rs/proptest) to verify properties hold across thousands of randomly generated inputs:

```bash
# Run only property tests
cargo test proptest

# Run property tests with more cases (default is 100)
PROPTEST_CASES=1000 cargo test proptest
```

**What property tests verify:**
- Parsing never panics on any input
- Valid BrainFuck programs always parse successfully
- Parsing is deterministic (same input = same output)
- Balanced brackets always parse correctly
- Comments don't affect program validity

#### Run Benchmarks

Benchmarks use [criterion](https://github.com/bheisler/criterion.rs) to measure performance with statistical analysis:

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark suite
cargo bench --bench interpreter
cargo bench --bench parser

# Run specific benchmark
cargo bench -- simple_arithmetic
```

**Benchmark suites:**
- **Interpreter benchmarks**: Arithmetic, loops, pointer movement, I/O, Hello World
- **Parser benchmarks**: Simple programs, nested loops, long programs, comments

Benchmark results are saved in `target/criterion/` with detailed HTML reports including:
- Performance graphs
- Regression analysis
- Statistical comparisons

**View HTML reports:**
```bash
# After running benchmarks
open target/criterion/report/index.html
```

#### Test Organization

```
crates/ferrous-cortex/
├── src/
│   ├── test_utils.rs        # Test helper functions
│   ├── parser.rs            # Unit tests + property tests
│   ├── interpreter.rs       # Unit tests
│   └── ...
└── benches/
    ├── interpreter.rs       # Interpreter benchmarks
    └── parser.rs            # Parser benchmarks
```

### Development Build

```bash
cargo build
```

### Running with Cargo

```bash
cargo run -- path/to/your/program.bf
```

## Project Structure

```
FerrousCortex/
├── crates/
│   ├── ferrous-cortex/      # Core library crate
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs           # Module interface (21 lines)
│   │   │   ├── parser.rs        # Source → AST parsing (+ 22 tests + 5 property tests)
│   │   │   ├── interpreter.rs   # AST → Execution (+ 20 tests)
│   │   │   ├── validator.rs     # AST validation (+ 5 tests)
│   │   │   ├── minify.rs        # AST → Source (+ 5 tests)
│   │   │   ├── test_utils.rs    # Test helper functions (+ 12 tests)
│   │   │   ├── io.rs            # I/O abstraction traits
│   │   │   ├── error.rs         # Error types and formatting
│   │   │   ├── config.rs        # Configuration types
│   │   │   ├── instruction.rs   # AST node definition
│   │   │   ├── location.rs      # Source position tracking
│   │   │   ├── types.rs         # Type-safe wrappers
│   │   │   └── stats.rs         # Execution statistics
│   │   ├── benches/
│   │   │   ├── interpreter.rs   # Interpreter benchmarks (5 benchmarks)
│   │   │   └── parser.rs        # Parser benchmarks (5 benchmarks)
│   │   └── examples/            # Rust library usage examples
│   │       ├── README.md        # Library examples documentation
│   │       ├── basic_usage.rs   # Basic parsing & execution
│   │       ├── custom_io.rs     # Custom I/O implementations
│   │       ├── memory_models.rs # Memory model configuration
│   │       ├── validation.rs    # Program validation
│   │       └── minify.rs        # Code minification
│   └── ferrous-cortex-cli/  # CLI binary crate
│       ├── Cargo.toml
│       └── src/
│           └── main.rs      # CLI interface and entry point
├── programs/                # BrainFuck programs for testing
│   ├── README.md            # Programs documentation
│   ├── basic/               # Simple demonstration programs
│   │   ├── hello_world.bf
│   │   ├── simple.bf
│   │   └── line_comments.bf
│   ├── advanced/            # Complex programs
│   │   ├── quine.bf
│   │   └── factor.bf
│   ├── tests/               # Feature testing programs
│   │   ├── test_eof.bf
│   │   └── warnings_test.bf
│   └── errors/              # Error handling demonstrations
│       ├── README.md        # Error examples documentation
│       ├── unmatched_bracket.bf
│       ├── memory_overflow.bf
│       ├── infinite_loop.bf
│       └── validation_warnings.bf
├── PRD/                     # Product requirement documents
│   └── TESTING_STATUS.md    # Testing infrastructure status
├── ARCHITECTURE.md          # Architecture and design decisions
├── Cargo.toml               # Workspace root
└── README.md
```

### Module Organization

The core library follows idiomatic Rust structure with clear separation of concerns:

- **lib.rs** (21 lines): Pure module interface with re-exports
- **parser.rs** (431 lines): Converts BrainFuck source code to AST
- **interpreter.rs** (484 lines): Executes AST with configurable runtime
- **validator.rs** (145 lines): Analyzes AST for warnings and best practices
- **minify.rs** (75 lines): Converts AST back to minimal source code
- **test_utils.rs**: Test helper functions and utilities
- **io.rs**: I/O abstraction traits (BfInput, BfOutput, StringIo)
- **Supporting modules**: error, config, instruction, location, stats, types

All modules include comprehensive tests (**137 total** including unit tests, property tests, and cell model tests) with co-located implementation.

## Roadmap

### Completed
- [x] Rich error messages with source context
- [x] Source location tracking (line/column numbers)
- [x] Execution limits (step count, timeout)
- [x] Configurable memory size
- [x] Verbose mode for diagnostics
- [x] Comprehensive error handling
- [x] Validation pass with warnings
- [x] Line comments using `*` for safer documentation
- [x] Code minification with comment stripping
- [x] Better bracket matching (report multiple errors)
- [x] Multiple memory models (fixed, unbounded)
- [x] Configurable cell arithmetic (wrapping, checked)
- [x] Cell-model-aware validation
- [x] Execution statistics tracking
- [x] Advanced I/O error handling (EOF behavior)
- [x] I/O abstraction for library usage and testing
- [x] Comprehensive testing infrastructure (137 tests)
- [x] Property-based testing with proptest
- [x] Performance benchmarking with criterion

### Planned
- [ ] Visual TUI debugger with breakpoints
- [ ] Step-by-step execution
- [ ] Memory visualization
- [ ] Performance optimizations (instruction fusion, loop detection)
- [ ] JIT/AOT compiler backend
- [ ] REPL mode

## References

- [The BrainFuck Programming Language](https://www.muppetlabs.com/~breadbox/bf/) - Comprehensive guide and reference by Brian Raiter

## License

[Add your license here]

## Contributing

[Add contribution guidelines here]
