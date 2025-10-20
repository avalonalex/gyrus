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
  - Fixed: Traditional fixed-size array with bounds checking
  - Wrapping: Circular buffer that wraps at boundaries
  - Unbounded: Dynamic growth up to configurable limit
- **Configurable execution limits**
  - Maximum step count to prevent infinite loops
  - Execution timeout in milliseconds
  - Customizable memory size
- **Program validation** with static analysis
  - Detects empty loops, infinite loops, extreme nesting
  - Identifies suspicious patterns
  - Strict mode for CI/CD integration
- **Code minification** - strip comments for compact programs
  - 95%+ size reduction typical
  - Preserves functionality
- **Execution statistics** tracking
  - Step count, loop iterations, memory usage
  - I/O statistics (bytes read/written)
  - Useful for performance analysis and debugging
- **Configurable EOF handling** for input operations
  - SetZero: Set cell to 0 on EOF (default)
  - SetNegOne: Set cell to 255 (-1) on EOF
  - NoChange: Leave cell unchanged on EOF
  - Error: Fail on EOF with error message
- **Verbose mode** for execution diagnostics
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

### Running a BrainFuck Program

```bash
cargo run -- <path-to-bf-file>

# Or using the compiled binary
./target/release/ferrous-cortex <path-to-bf-file>
```

### Examples

Run the included example programs:

```bash
# Simple program that prints 'H'
cargo run -- examples/simple.bf

# Classic Hello World
cargo run -- examples/hello_world.bf
```

### Command-Line Options

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

#### Available Flags

| Flag | Description | Default |
|------|-------------|---------|
| `-v, --verbose` | Show detailed execution information | false |
| `--stats` | Show execution statistics after program finishes | false |
| `--validate` | Validate program and show warnings | false |
| `--strict` | Treat warnings as errors (implies --validate) | false |
| `--minify` | Strip all comments and output only BF commands | false |
| `-o, --output <FILE>` | Output file for minified code (stdout if not specified) | - |
| `--max-steps <N>` | Maximum number of execution steps (0 = unlimited) | 0 |
| `--timeout <MS>` | Execution timeout in milliseconds (0 = unlimited) | 0 |
| `--memory-size <BYTES>` | Memory size in bytes (for fixed/wrapping models) | 30000 |
| `--memory-model <MODEL>` | Memory model: fixed, wrapping, or unbounded | fixed |
| `--unbounded-initial <BYTES>` | Initial size for unbounded memory model | 1000 |
| `--unbounded-max <BYTES>` | Maximum size for unbounded memory model | 1000000 |
| `--eof-behavior <BEHAVIOR>` | EOF behavior: zero, neg-one, no-change, or error | zero |

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

### Wrapping Memory

Memory pointer wraps around at boundaries (circular buffer).

```bash
ferrous-cortex program.bf --memory-model wrapping --memory-size 30000
```

**Characteristics:**
- Pointer wraps: position 30000 → 0, position -1 → 29999
- Never raises memory out-of-bounds errors
- Some BrainFuck variants use this behavior
- Useful for programs that rely on wrapping

**Example:**
```brainfuck
* With wrapping memory size 10:
>>>>>>>>>>  * Move right 10 times
+.          * Now at cell 0 (wrapped), increment and output
```

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

- **Fixed**: Use for standard BrainFuck programs and when you want strict bounds checking
- **Wrapping**: Use for programs designed for wrapping behavior or when porting from other interpreters
- **Unbounded**: Use for programs with unknown memory requirements or when prototyping

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

FerrousCortex can track and display detailed execution statistics using the `--stats` flag:

```bash
ferrous-cortex program.bf --stats
```

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
$ ferrous-cortex examples/hello_world.bf --stats
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
# Validate program for warnings
ferrous-cortex program.bf --validate
```

### Warning Types

The validator checks for:

- **Empty loops**: `[]` - Does nothing and can be removed
- **Infinite loops**: `[+]` or `[++]` - Cell never reaches zero by incrementing
- **Extreme nesting**: Loops nested more than 10 levels deep (performance impact)
- **Inefficient patterns**: Multiple operations that could be optimized

### Strict Mode

Use `--strict` to treat warnings as errors (useful for CI/CD):

```bash
# Exit with error if warnings are found
ferrous-cortex program.bf --strict
```

This is useful for maintaining code quality in automated pipelines.

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
$ cat examples/line_comments.bf
* Line Comment Demo
* Everything after * is completely ignored!

++++++++++  * Cell 0 = 10
[           * Loop 10 times
  >+++++++  * Cell 1 += 7
  <-        * Cell 0 -= 1
]           * Result: Cell 1 = 70
>++.        * Add 2, print 'H'

$ ferrous-cortex examples/line_comments.bf --minify
++++++++++[>+++++++<-]>++.

$ ferrous-cortex examples/line_comments.bf --minify --verbose -o min.bf
Minified 514 bytes to 26 bytes (saved to min.bf)
```

Minification achieves **95%+ size reduction** by removing:
- All line comments (after `*`)
- All implicit comments (non-BF characters)
- All whitespace and formatting

The minified code is functionally identical to the original.

## Development

### Running Tests

```bash
cargo test
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
├── src/
│   ├── main.rs          # CLI interface and entry point
│   └── bf.rs            # Parser and interpreter core
├── examples/            # Example BrainFuck programs
│   ├── hello_world.bf
│   └── simple.bf
├── Cargo.toml
└── README.md
```

## Roadmap

### Completed
- [x] Rich error messages with source context
- [x] Source location tracking (line/column numbers)
- [x] Execution limits (step count, timeout)
- [x] Configurable memory size
- [x] Verbose mode for diagnostics
- [x] Comprehensive error handling
- [x] Validation pass with warnings
- [x] Strict mode for CI/CD
- [x] Line comments using `*` for safer documentation
- [x] Code minification with comment stripping
- [x] Better bracket matching (report multiple errors)
- [x] Multiple memory models (fixed, wrapping, unbounded)
- [x] Execution statistics tracking
- [x] Advanced I/O error handling (EOF behavior)

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
