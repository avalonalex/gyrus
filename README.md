# FerrousCortex

An industry-strength BrainFuck interpreter/compiler and visual debugger written in Rust.

## Features

- Fast and efficient BrainFuck interpreter
- **Rich error handling** with source location tracking and context
  - Line and column numbers for parse errors
  - Visual error context with caret (^) pointing to issues
  - Detailed error messages for debugging
- Support for all 8 BrainFuck commands: `><+-.,[]`
- Nested loop support
- **Configurable execution limits**
  - Maximum step count to prevent infinite loops
  - Execution timeout in milliseconds
  - Customizable memory size
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
| `--max-steps <N>` | Maximum number of execution steps (0 = unlimited) | 0 |
| `--timeout <MS>` | Execution timeout in milliseconds (0 = unlimited) | 0 |
| `--memory-size <BYTES>` | Memory size in bytes | 30000 |

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

Any other characters are treated as comments and ignored.

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

### Planned
- [ ] Visual TUI debugger with breakpoints
- [ ] Step-by-step execution
- [ ] Memory visualization
- [ ] Validation pass with warnings
- [ ] Performance optimizations (instruction fusion, loop detection)
- [ ] Multiple memory models (bounded, unbounded, wrapping)
- [ ] Advanced I/O error handling (EOF behavior)
- [ ] JIT/AOT compiler backend
- [ ] REPL mode

## References

- [The BrainFuck Programming Language](https://www.muppetlabs.com/~breadbox/bf/) - Comprehensive guide and reference by Brian Raiter

## License

[Add your license here]

## Contributing

[Add contribution guidelines here]
