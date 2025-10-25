# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

FerrousCortex is an industry-strength BrainFuck interpreter/compiler and visual debugger written in Rust. The project uses Rust edition 2024.

**Project Structure**: Cargo workspace with multiple crates
- **ferrous-cortex** (`crates/ferrous-cortex/`): Core library crate
- **ferrous-cortex-cli** (`crates/ferrous-cortex-cli/`): CLI binary crate
- Future: debugger, REPL, JIT compiler as separate crates

## Core Architecture

### Module Structure (Idiomatic Rust)

The library uses a clean module structure with `lib.rs` as a pure interface (21 lines):

**Core Modules:**

1. **parser** (`crates/ferrous-cortex/src/parser.rs` - 431 lines + 22 tests)
   - BrainFuck source → AST (Abstract Syntax Tree)
   - Recursive descent parser that converts source text into `Vec<Instruction>`
   - **Location tracking**: Maintains line, column, and offset for every position
   - **Bracket validation**: Pre-parse phase validates ALL bracket matching errors at once
   - Loops are represented as `Instruction::Loop(Vec<Instruction>)` creating a nested tree structure
   - **Comments**:
     - Non-BF characters are ignored (implicit comments)
     - `*` starts line comments - everything after `*` on that line is ignored
   - Rich error messages with source context (shows 2 lines before/after with caret)
   - **Multiple error reporting**: Shows all bracket errors in a single pass
   - Public API: `parse(source: &str) -> Result<Vec<Instruction>, BfError>`

2. **interpreter** (`crates/ferrous-cortex/src/interpreter.rs` - 484 lines + 20 tests)
   - AST → Execution with safety limits and statistics
   - Tree-walking interpreter with configurable memory
   - **Multiple memory models**:
     - Fixed: Traditional fixed-size array (default 30,000 bytes)
     - Wrapping: Circular buffer that wraps at boundaries
     - Unbounded: Dynamic growth from initial size up to max limit
   - **Execution limits**: Step counting and timeout support
   - **Statistics tracking** via `ExecutionStats`:
     - Total steps, loop iterations, peak memory usage
     - Memory allocation (useful for unbounded model)
     - I/O statistics (bytes read/written)
     - Modified cell count
   - Recursive execution for nested loops via `execute_block()`
   - Direct I/O to stdin/stdout
   - Public API: `interpret()`, `interpret_with_config()`

3. **validator** (`crates/ferrous-cortex/src/validator.rs` - 145 lines + 5 tests)
   - AST → Warnings for suspicious patterns
   - Detects: empty loops, infinite loops, extreme nesting, inefficient patterns
   - Public API: `validate(instructions: &[Instruction]) -> Vec<BfWarning>`

4. **minify** (`crates/ferrous-cortex/src/minify.rs` - 75 lines + 5 tests)
   - AST → Minimal BrainFuck source (removes comments)
   - Public API: `minify(instructions: &[Instruction]) -> String`

**Supporting Modules:**
- **error** (`error.rs`): `BfError`, `BfWarning` types with rich formatting
- **config** (`config.rs`): `ExecutionConfig`, `MemoryModel`, `EofBehavior`
- **instruction** (`instruction.rs`): AST node definition `Instruction` enum
- **location** (`location.rs`): Source position tracking `SourceLocation`
- **stats** (`stats.rs`): Execution statistics `ExecutionStats`
- **lib** (`lib.rs` - 21 lines): Pure module interface with re-exports

**CLI** (`crates/ferrous-cortex-cli/src/main.rs`)
   - Flow: read file → parse → (minify OR validate) → configure → interpret → (stats)
   - Flags: `--verbose`, `--max-steps`, `--timeout`, `--memory-size`, `--memory-model`, `--unbounded-initial`, `--unbounded-max`, `--validate`, `--strict`, `--minify`, `-o/--output`, `--eof-behavior`
   - Configuration via `ExecutionConfig` (builder pattern)
   - Minify mode: Parse → minify → output (no execution)
   - Validate mode: Parse → validate → show warnings (no execution)
   - Strict mode: Parse → validate → execute if clean (exits on warnings)

### Key Design Decisions

- **Error handling**: Uses `thiserror` for custom error types (`BfError`)
  - All errors include context (location, source snippet, details)
  - Structured error types (not strings) for better error handling
- **Memory models**: Three configurable models via `MemoryModel` enum
  - Fixed: Traditional bounds-checked array
  - Wrapping: Circular buffer (modulo arithmetic on pointer)
  - Unbounded: Vec that grows on-demand up to max limit
  - Pointer movement handled by `increment_pointer()` and `decrement_pointer()` helpers
- **Loop representation**: Nested `Vec<Instruction>` rather than jump tables
- **Parsing approach**: Single-pass recursive descent with full location tracking
- **Safety**: Multiple layers of protection (step limits, timeouts, bounds checks)
- **Configuration**: Builder pattern for `ExecutionConfig` (fluent API)

## Workspace Structure

This is a Cargo workspace with the following crates:

- **`ferrous-cortex`** (library): Core BrainFuck interpreter, parser, and runtime
  - Location: `crates/ferrous-cortex/`
  - **Structure**: 10 modules, 1,502 lines total
  - **Tests**: 52 tests co-located with implementation
    - Parser: 22 tests
    - Interpreter: 20 tests
    - Validator: 5 tests
    - Minify: 5 tests
  - **Module breakdown**:
    ```
    src/
    ├── lib.rs           (21 lines)   - Module interface
    ├── parser.rs        (431 lines)  - Source → AST
    ├── interpreter.rs   (484 lines)  - AST → Execution
    ├── validator.rs     (145 lines)  - AST validation
    ├── minify.rs        (75 lines)   - AST → Source
    ├── error.rs         (127 lines)  - Error types
    ├── config.rs        (137 lines)  - Configuration
    ├── instruction.rs   (11 lines)   - AST nodes
    ├── location.rs      (35 lines)   - Source tracking
    └── stats.rs         (36 lines)   - Statistics
    ```
  - Can be used as a library by other Rust projects
  - Ready for publication to crates.io

- **`ferrous-cortex-cli`** (binary): Command-line interface
  - Location: `crates/ferrous-cortex-cli/`
  - Thin wrapper around the library
  - Handles argument parsing and output formatting
  - Binary name: `ferrous-cortex`

**Benefits of workspace structure**:
- ✅ Clear separation between library and CLI
- ✅ Easy to add new binaries (debugger, REPL, etc.)
- ✅ Library can be published to crates.io independently
- ✅ Each crate can have its own version
- ✅ Faster incremental compilation
- ✅ Clean module boundaries prevent coupling

## Common Commands

### Build and Run
```bash
cargo build                           # Build entire workspace
cargo build --release                 # Optimized build
cargo run -- <file.bf>                # Run CLI (auto-detects binary)
cargo run -- programs/basic/hello_world.bf  # Run example

# Build specific crate
cargo build -p ferrous-cortex         # Build library only
cargo build -p ferrous-cortex-cli     # Build CLI only
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

Example BrainFuck programs are in `programs/`:
- `programs/basic/simple.bf` - Prints 'H' (simple test case)
- `programs/basic/hello_world.bf` - Prints "Hello World!" (classic BF program)
- `programs/basic/line_comments.bf` - Demonstrates line comment syntax
- `programs/errors/` - Error handling demonstrations
- See `programs/README.md` for full list and documentation

## Library Usage Examples

Rust examples demonstrating library usage are in `crates/ferrous-cortex/examples/`:
- `basic_usage.rs` - Core parsing, execution, error handling
- `custom_io.rs` - Implementing custom I/O traits
- `memory_models.rs` - Different memory model configurations
- `validation.rs` - Program validation
- `minify.rs` - Code minification

Run with: `cargo run --example basic_usage`

## Error Handling Architecture

The error system is comprehensive and production-ready:

- **SourceLocation**: Tracks line, column, offset for every parse position
- **extract_source_context()**: Generates visual error messages with surrounding code
- **validate_brackets()**: Pre-parse validation that finds ALL bracket errors in one pass
- **BfError variants**: Structured errors with relevant context
  - Parse errors: Include location and source context
  - Runtime errors: Include instruction index and attempted values
  - Limit errors: Include the limit that was exceeded

Example bracket validation flow:
1. `parse()` calls `validate_brackets()` before parsing
2. `validate_brackets()` scans entire source with a stack
3. Collects ALL unmatched `[` and `]` errors
4. If multiple errors found, reports all to stderr then returns first
5. This saves time by showing all bracket issues at once

Example single error flow:
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

## Minification System

Converts parsed instructions back to minimal BrainFuck source:

- **minify()**: Entry point that returns String
- **minify_instructions()**: Recursive conversion of AST to BF code
- Strips all comments (line and implicit)
- Removes all whitespace and formatting
- Typical size reduction: 95%+
- Round-trip property: parse → minify → parse yields identical AST

CLI integration:
- `--minify`: Output minified code
- `-o/--output FILE`: Write to file instead of stdout
- `--verbose` with minify: Show compression stats

## Testing

The test suite covers:
- **Parse tests**: All instruction types, nested loops, comments (5 tests)
- **Error tests**: All error types with proper context (4 tests)
- **Limit tests**: Step limits, timeouts, memory bounds (5 tests)
- **Location tests**: Multiline programs, error positions (included above)
- **Validation tests**: Empty loops, infinite loops, nesting, clean programs (5 tests)
- **Comment tests**: Line comments, BF commands in comments, multiline (4 tests)
- **Minify tests**: Simple, line comments, nested loops, round-trip (5 tests)
- **Bracket matching tests**: Multiple errors, single errors, location tracking (9 tests)
- **Memory model tests**: Fixed, wrapping, unbounded behaviors (7 tests)
- **Statistics tests**: Step counting, loop iterations, I/O tracking, memory tracking (6 tests)
- **Total**: 50 tests

To add new tests:
1. Test parsing with `parse(source).unwrap()`
2. Test execution with `interpret_with_config(&instructions, config)`
3. Test validation with `validate(&instructions)`
4. Use `matches!` macro for error/warning pattern matching
5. Check error details (location, context, values)
6. For bracket errors, test both single and multiple error scenarios

## Future Architecture Notes

The roadmap includes:
- **Debugger**: Will need to extend interpreter with debug hooks (breakpoints, step execution)
- **Compiler**: Planned JIT/AOT backend - consider IR layer between parser and execution
- **Optimizations**: Instruction fusion (e.g., `+++` → IncrementValue(3)) will require AST transformation pass
- **Validation pass**: Static analysis for warnings (dead loops, suspicious patterns)

When adding the debugger, the interpreter state (memory, pointer, instruction counter) should be extracted into a struct that can be inspected and controlled externally.

## Implementation Status

**Completed (Phase 1, 2.1, 2.2, 3.1, 3.2, 4.1 from PRD + Community features)**:
- ✅ Source location tracking (Phase 1)
- ✅ Rich error messages with context (Phase 1)
- ✅ Execution limits (step count, timeout) (Phase 3.1)
- ✅ Configurable memory size (Phase 3.1)
- ✅ Verbose mode (Phase 1)
- ✅ Comprehensive test suite (50 tests)
- ✅ Validation pass with warnings (Phase 2.1)
- ✅ Strict mode for CI/CD (Phase 2.1)
- ✅ Line comments using `*` (Community feature)
- ✅ Code minification (Phase 4.1)
- ✅ Better bracket matching - multiple errors (Phase 2.2)
- ✅ Multiple memory models (Phase 3.2)
  - Fixed, Wrapping, and Unbounded models
  - CLI flags for model selection
  - Comprehensive testing
- ✅ Execution statistics tracking (Community feature)
  - Steps, loop iterations, memory usage
  - I/O tracking
  - `--stats` CLI flag

- ✅ Advanced I/O error handling (Phase 3.3)
  - EOF behavior configuration (SetZero, SetNegOne, NoChange, Error)
  - `--eof-behavior` CLI flag
  - Graceful EOF handling in Input instruction

**Remaining (from PRD)**:
- ⏳ Debug symbols and runtime diagnostics (Phase 4.2)
- ⏳ Visual TUI debugger
- ⏳ Performance optimizations (instruction fusion, I/O buffering)
- ⏳ JIT/AOT compiler backend

**Project Structure Improvements**:
- ✅ Workspace migration (v0.2.0)
  - Separated core library from CLI binary
  - Foundation for future crates (debugger, REPL, JIT)
