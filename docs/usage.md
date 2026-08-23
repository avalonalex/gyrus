# Usage

Running programs with the `gyrus` CLI: options, execution modes, statistics,
and the language itself.

Back to the [README](../README.md).

## Usage

gyrus provides two command-line tools:

1. **`gyrus`** - BrainFuck interpreter for executing programs
2. **`gyrus-tool`** - Development tools for analyzing and processing BF code

### Running a BrainFuck Program

```bash
cargo run -p gyrus-cli -- <path-to-bf-file>

# Or using the compiled binary
./target/release/gyrus <path-to-bf-file>
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
cargo run -p gyrus-cli -- programs/errors/unmatched_bracket.bf

# Memory bounds error
cargo run -p gyrus-cli -- programs/errors/memory_overflow.bf --memory-size 100

# Infinite loop with step limit
cargo run -p gyrus-cli -- programs/errors/infinite_loop.bf --max-steps 10000
```

See [`programs/errors/README.md`](../programs/errors/README.md) for detailed error examples documentation.

#### Using gyrus as a Library

The `crates/gyrus/examples/` directory contains Rust examples showing library usage:

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

See [`crates/gyrus/examples/README.md`](../crates/gyrus/examples/README.md) for detailed library examples.

### Command-Line Options

#### gyrus (Interpreter)

Execute BrainFuck programs with configurable runtime options:

```bash
# Show all available options
gyrus --help

# Run with verbose output
gyrus program.bf --verbose

# Limit execution to 10000 steps
gyrus program.bf --max-steps 10000

# Set execution timeout to 5 seconds
gyrus program.bf --timeout 5000

# Use custom memory size (1MB)
gyrus program.bf --memory-size 1000000

# Combine multiple options
gyrus program.bf --verbose --max-steps 100000 --timeout 10000
```

**Available Options:**

| Flag | Description | Default |
|------|-------------|---------|
| `-v, --verbose` | Show detailed execution information and statistics | false |
| `-q, --quiet` | Suppress runtime warnings and non-program output | false |
| `--debug` | Enable debug symbols for source location tracking (slower) | false |
| `--max-steps <N>` | Maximum number of execution steps (0 = unlimited) | 0 |
| `--timeout <MS>` | Execution timeout in milliseconds (0 = unlimited) | 0 |
| `--memory-size <BYTES>` | Memory size in bytes (for fixed model) | 30000 |
| `--memory-model <MODEL>` | Memory model: fixed or unbounded | fixed |
| `--cell-model <MODEL>` | Cell model: wrapping (production) or checked (debugging) | wrapping |
| `--unbounded-initial <BYTES>` | Initial size for unbounded memory model | 1000 |
| `--unbounded-max <BYTES>` | Maximum size for unbounded memory model | 1000000 |
| `--eof-behavior <BEHAVIOR>` | EOF behavior: zero, neg-one, no-change, or error | zero |

#### gyrus-tool (Development Tools)

Analyze, validate, and process BrainFuck programs:

```bash
# Show all available commands
gyrus-tool --help

# Minify a program (strip comments)
gyrus-tool minify program.bf

# Save minified output to file
gyrus-tool minify program.bf -o program.min.bf --verbose

# Validate a program and show warnings
gyrus-tool validate program.bf

# Validate in strict mode (exit with error if warnings found)
gyrus-tool validate program.bf --strict

# Inspect debug symbols
gyrus-tool debug-info program.bf

# Output debug info as JSON
gyrus-tool debug-info program.bf --format json

# View program with syntax highlighting
gyrus-tool view program.bf --line-numbers

# Plain output (no colors)
gyrus-tool view program.bf --plain
```

**Available Commands:**

| Command | Description |
|---------|-------------|
| `minify` | Strip comments and whitespace from BF programs |
| `validate` | Validate programs and show static analysis warnings |
| `debug-info` | Inspect debug symbols and source location mappings |
| `view` | Display BF programs with syntax highlighting |

## Execution Statistics

gyrus can track and display detailed execution statistics using the `--verbose` flag:

```bash
gyrus program.bf --verbose
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
$ gyrus programs/basic/hello_world.bf --verbose
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

gyrus supports two types of comments:

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
