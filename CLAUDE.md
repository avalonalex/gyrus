# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

FerrousCortex is an industry-strength BrainFuck interpreter/compiler and visual debugger written in Rust. The project uses Rust edition 2024.

## Core Architecture

The codebase follows a simple pipeline architecture:

1. **Parse** (`src/bf.rs`): BrainFuck source → AST (Abstract Syntax Tree)
   - Recursive descent parser that converts source text into `Vec<Instruction>`
   - Loops are represented as `Instruction::Loop(Vec<Instruction>)` creating a nested tree structure
   - Non-BF characters are treated as comments and ignored

2. **Interpret** (`src/bf.rs`): AST → Execution
   - Tree-walking interpreter with 30,000 bytes of memory (standard BF spec)
   - Recursive execution for nested loops via `execute_block()`
   - Direct I/O to stdin/stdout

3. **CLI** (`src/main.rs`): Entry point using clap for argument parsing
   - Simple flow: read file → parse → interpret

### Key Design Decisions

- **Error handling**: Uses `thiserror` for custom error types (`BfError`)
- **Memory model**: Fixed 30KB array (`Vec<u8>`) with bounds checking
- **Loop representation**: Nested `Vec<Instruction>` rather than jump tables
- **Parsing approach**: Single-pass recursive descent, position tracking for error reporting

## Common Commands

### Build and Run
```bash
cargo build                           # Development build
cargo build --release                 # Optimized build
cargo run -- <file.bf>                # Run a BF program
cargo run -- examples/hello_world.bf  # Run example
```

### Testing
```bash
cargo test                            # Run all tests
cargo test test_parse_simple          # Run specific test
cargo test --lib                      # Run only library tests
```

### Development
```bash
cargo check                           # Fast syntax check
cargo clippy                          # Linting
cargo fmt                             # Format code
```

## Testing BF Programs

Example programs are in `examples/`:
- `simple.bf` - Prints 'H' (simple test case)
- `hello_world.bf` - Prints "Hello World!" (classic BF program)

## Future Architecture Notes

The roadmap includes:
- **Debugger**: Will need to extend interpreter with debug hooks (breakpoints, step execution)
- **Compiler**: Planned JIT/AOT backend - consider IR layer between parser and execution
- **Optimizations**: Instruction fusion (e.g., `+++` → IncrementValue(3)) will require AST transformation pass

When adding the debugger, the interpreter state (memory, pointer, instruction counter) should be extracted into a struct that can be inspected and controlled externally.
