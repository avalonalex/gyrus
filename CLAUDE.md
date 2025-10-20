# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

FerrousCortex is an industry-strength BrainFuck interpreter/compiler and visual debugger written in Rust. The project uses Rust edition 2024.

## Core Architecture

The codebase follows a pipeline architecture with comprehensive error handling:

1. **Parse** (`src/bf.rs`): BrainFuck source → AST (Abstract Syntax Tree)
   - Recursive descent parser that converts source text into `Vec<Instruction>`
   - **Location tracking**: Maintains line, column, and offset for every position
   - Loops are represented as `Instruction::Loop(Vec<Instruction>)` creating a nested tree structure
   - Non-BF characters are treated as comments and ignored
   - Rich error messages with source context (shows 2 lines before/after with caret)

2. **Interpret** (`src/bf.rs`): AST → Execution with safety limits
   - Tree-walking interpreter with configurable memory (default 30,000 bytes)
   - **Execution limits**: Step counting and timeout support
   - Recursive execution for nested loops via `execute_block()`
   - Direct I/O to stdin/stdout
   - Instruction counting for error reporting

3. **CLI** (`src/main.rs`): Entry point with extensive configuration
   - Flow: read file → parse → configure → interpret
   - Flags: `--verbose`, `--max-steps`, `--timeout`, `--memory-size`
   - Configuration via `ExecutionConfig` (builder pattern)

### Key Design Decisions

- **Error handling**: Uses `thiserror` for custom error types (`BfError`)
  - All errors include context (location, source snippet, details)
  - Structured error types (not strings) for better error handling
- **Memory model**: Configurable size with bounds checking on every pointer operation
- **Loop representation**: Nested `Vec<Instruction>` rather than jump tables
- **Parsing approach**: Single-pass recursive descent with full location tracking
- **Safety**: Multiple layers of protection (step limits, timeouts, bounds checks)
- **Configuration**: Builder pattern for `ExecutionConfig` (fluent API)

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

## Error Handling Architecture

The error system is comprehensive and production-ready:

- **SourceLocation**: Tracks line, column, offset for every parse position
- **extract_source_context()**: Generates visual error messages with surrounding code
- **BfError variants**: Structured errors with relevant context
  - Parse errors: Include location and source context
  - Runtime errors: Include instruction index and attempted values
  - Limit errors: Include the limit that was exceeded

Example error flow:
1. Parser detects unmatched `[` at position
2. Creates `SourceLocation` with line/column
3. Calls `extract_source_context()` to get surrounding lines
4. Returns `BfError::UnmatchedOpenBracket { location, context }`
5. Error Display shows formatted message with visual caret

## Validation Architecture

The validation system performs static analysis on parsed instructions:

- **BfWarning enum**: Structured warning types (EmptyLoop, ExtremeNesting, SuspiciousPattern, DeadCode)
- **validate()**: Entry point that returns `Vec<BfWarning>`
- **validate_instructions()**: Recursive traversal checking nesting depth
- **check_suspicious_loop_patterns()**: Pattern matching for common issues

Warnings detected:
1. **Empty loops** `[]` - No-op that can be removed
2. **Infinite increment loops** `[+]`, `[++]` - Cell never reaches zero
3. **Extreme nesting** - Depth > 10 levels (performance impact)
4. **Inefficient patterns** - `[--]` instead of `[-]`

Note: Common patterns like `[>]`, `[<]`, and `[-]` are NOT flagged as they're standard BF idioms.

CLI integration:
- `--validate`: Show warnings but continue execution
- `--strict`: Treat warnings as errors (exit code 1)

## Testing

The test suite covers:
- **Parse tests**: All instruction types, nested loops, comments
- **Error tests**: All error types with proper context
- **Limit tests**: Step limits, timeouts, memory bounds
- **Location tests**: Multiline programs, error positions
- **Validation tests**: Empty loops, infinite loops, nesting, clean programs (5 tests)

To add new tests:
1. Test parsing with `parse(source).unwrap()`
2. Test execution with `interpret_with_config(&instructions, config)`
3. Test validation with `validate(&instructions)`
4. Use `matches!` macro for error/warning pattern matching
5. Check error details (location, context, values)

## Future Architecture Notes

The roadmap includes:
- **Debugger**: Will need to extend interpreter with debug hooks (breakpoints, step execution)
- **Compiler**: Planned JIT/AOT backend - consider IR layer between parser and execution
- **Optimizations**: Instruction fusion (e.g., `+++` → IncrementValue(3)) will require AST transformation pass
- **Validation pass**: Static analysis for warnings (dead loops, suspicious patterns)

When adding the debugger, the interpreter state (memory, pointer, instruction counter) should be extracted into a struct that can be inspected and controlled externally.

## Implementation Status

**Completed (Phase 1, 2.1, 3.1, 4.1 from PRD)**:
- ✅ Source location tracking
- ✅ Rich error messages with context
- ✅ Execution limits (step count, timeout)
- ✅ Configurable memory size
- ✅ Verbose mode
- ✅ Comprehensive test suite (19 tests)
- ✅ Validation pass with warnings
- ✅ Strict mode for CI/CD

**Remaining (from PRD)**:
- ⏳ Better bracket matching (Phase 2.2)
- ⏳ Multiple memory models (Phase 3.2)
- ⏳ Advanced I/O error handling (Phase 3.3)
