# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

gyrus is a BrainFuck interpreter, optimizer, and debugger written in Rust,
built as a learning project. Rust edition 2024, MSRV 1.88 (the code uses
let-chains); `rust-toolchain.toml` pins the development compiler.

**Project Structure**: Cargo workspace with multiple crates

- **gyrus** (`crates/gyrus/`): Core library crate
- **gyrus-cli** (`crates/gyrus-cli/`): `gyrus` binary — program execution
- **gyrus-tool** (`crates/gyrus-tool/`): `gyrus-tool` binary — development tools
- Future: TUI debugger, REPL, JIT compiler as separate crates

## Documentation Organization

**Important**: Use the following directories for different types of documentation:

- **IMPORTANT** - Do not create markdown files unless the user explicitly states to do so. You may offer to create markdown files, but only do so with explicit user approval. Integrate any necessary notes as comments within the relevant code files, and keep comments succinct and on point.
- **`PRD/`** - Product Requirements Documents, project proposals, and high-level design documents. We should be aggressive in terms of purge outdated PRDs as they have high cognative overhead.
- **`internal/`** - Internal documentation, implementation notes, test results, and milestone records

## Core Architecture

### Module Structure (Idiomatic Rust)

The library uses a clean module structure with `lib.rs` as a pure interface of
re-exports and crate docs.

**Core Modules:**

1. **parser** (`crates/gyrus/src/parser.rs`)
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

2. **interpreter** (`crates/gyrus/src/interpreter/`): `mod.rs` (entry points), `state.rs`
   (VM state), `dispatch.rs` (instruction dispatch), `execution.rs` (the tree
   walker), `optimized.rs` (the optimized executor)
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

3. **validator** (`crates/gyrus/src/validator.rs`)
   - AST → Warnings for suspicious patterns
   - Detects: empty loops, infinite loops, extreme nesting, inefficient patterns
   - Public API: `validate(instructions: &[Instruction]) -> Vec<BfWarning>`

4. **minify** (`crates/gyrus/src/minify.rs`)
   - AST → Minimal BrainFuck source (removes comments)
   - Public API: `minify(instructions: &[Instruction]) -> String`

**Supporting Modules:**

- **error** (`error.rs`): `BfError`, `BfWarning` types with rich formatting and syntax highlighting
  - `extract_source_context_highlighted()`: Generates syntax-highlighted error/warning messages with ANSI colors
  - Caret positioning formula: `column + 6` (7-char line prefix, column is 1-indexed)
  - All runtime errors and warnings include source location and highlighted code
- **syntax** (`syntax.rs`): Syntax highlighting for BrainFuck source code
  - Color scheme: cyan for pointer ops, green for cell ops, orange for loops, gray for comments
  - Line numbers and loop nesting depth visualization
- **config** (`config/`): `ExecutionConfig`, `MemoryModel`, `CellModel`, `EofBehavior`
- **instruction** (`instruction.rs`): AST node definition `Instruction` enum
- **location** (`location.rs`): Source position tracking `SourceLocation`
- **stats** (`stats.rs`): Execution statistics `ExecutionStats`
- **optimizer** (`optimizer.rs`): AST → `OptimizedProgram`.
  Fuses instruction runs into `Add`/`Sub`/`Right`/`Left`, and recognizes clear
  loops (`Zero`), scan loops (`SeekRight`/`SeekLeft`), and multiply loops
  (`MultiplyAdd`)
- **types** (`types.rs`): newtypes shared by the optimizer
  and optimized interpreter
- **hooks** (`hooks/mod.rs` + `hooks/builtin.rs`):
  `ExecutionHook` trait with five hook points, `HookManager`, `HookContext`,
  `HookDecision`, plus built-in hooks
- **io** (`io.rs`): `BfInput`/`BfOutput` abstraction —
  `StdIo`, `StringIo`, `DebugIo`
- **codegen** (`codegen.rs`): string → BrainFuck compiler
  (dynamic programming, ~12.3 ops/byte)
- **debug** (`debug.rs`): debug symbol tables
- **test_utils** (`test_utils.rs`): test helpers and
  random program generation
- **lib** (`lib.rs`): Pure module interface with re-exports

**CLI** (`crates/gyrus-cli/src/main.rs`) — execution only
   - Flow: read file → parse → optimize (unless `--debug`) → interpret → (stats)
   - Flags: `--verbose`, `--quiet`, `--debug`, `--trace`, `--max-steps`,
     `--timeout`, `--memory-size`, `--memory-model`, `--cell-model`,
     `--unbounded-initial`, `--unbounded-max`, `--eof-behavior`
   - Configuration via `ExecutionConfig` (builder pattern)

**Tool** (`crates/gyrus-tool/src/main.rs`) — development workflows
   - Subcommands: `minify`, `validate`, `debug-info`, `view`, `generate`,
     `compile`, `optimize`
   - **Note**: minify and validate are subcommands here, NOT flags on `gyrus`.
     `gyrus --minify` and `gyrus --validate` do not exist.
   - **Runtime warnings**: Only shown with `--verbose` flag (cell wrapping is common in BF programs)
   - **Debug symbols**: Opt-in with `--debug` flag (default: fast mode without source locations)

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

## Memory and Cell Models: Orthogonal Configuration

gyrus separates two independent concerns for maximum flexibility:

1. **MemoryModel**: Controls pointer movement (`>`, `<` instructions)
2. **CellModel**: Controls cell arithmetic (`+`, `-` instructions)

These can be mixed independently (e.g., Fixed memory + Checked cells, or Unbounded memory + Wrapping cells).

### CellModel: Cell Arithmetic Behavior

**CellModel** controls how `+` and `-` instructions behave at boundaries:

1. **U8Wrapping** (default - production use):
   - Cell type: `u8` (0-255)
   - Overflow: `255 + 1 = 0` (wraps)
   - Underflow: `0 - 1 = 255` (wraps)
   - **Use case**: Standard BrainFuck behavior, maximum compatibility
   - **Aligns with JIT/AOT**: Compiled code will use wrapping arithmetic

2. **U8Checked** (debugging mode):
   - Cell type: `u8` (0-255)
   - Overflow: `255 + 1` → `BfError::CellOverflow`
   - Underflow: `0 - 1` → `BfError::CellUnderflow`
   - **Use case**: Catch arithmetic bugs during development
   - **Strict mode**: Errors immediately on boundary violations

**Code location**: `config.rs` defines `CellModel` enum and `CellBehavior` trait

**Example usage**:
```rust
// Production: wrapping cells (compatible)
let config = ExecutionConfig::builder()
    .with_memory_size(30000)
    .with_wrapping_cells()  // Default
    .build();

// Debugging: checked cells (strict)
let config = ExecutionConfig::builder()
    .with_memory_size(30000)
    .with_checked_cells()  // Catches bugs
    .build();
```

### MemoryModel: Pointer Movement Behavior

**MemoryModel** controls how pointer movement behaves at boundaries:

1. **Fixed** (default):
   - Memory size: Configurable (default 30,000 bytes)
   - Overflow behavior: **ERROR** - moving beyond bounds raises `BfError::MemoryOutOfBounds`
   - Use case: Strict BF compliance, catching bugs

2. **Wrapping**:
   - Memory size: Configurable
   - Overflow behavior: **WRAP** - pointer wraps to opposite end (modulo arithmetic)
   - Example: With 1000 cells, pointer 999 + 1 = 0, pointer 0 - 1 = 999
   - Use case: Circular buffer programs

3. **Unbounded**:
   - Initial size: Configurable (default 30,000)
   - Maximum size: Configurable (default 1,000,000)
   - Overflow behavior: **GROW** - memory expands dynamically up to max limit
   - Negative movement: Still errors (no negative indices)
   - Use case: Programs with dynamic memory needs

**Code location**: `config.rs` defines `MemoryModel` enum and `MemoryBehavior` trait

### Cell-Model-Aware Validation

The validator (`validator.rs`) provides **cell-model-aware** warnings via `validate_with_cell_model()`:

**Pattern**: `[+]` (increment loop behavior)
- **With U8Wrapping**: Inefficient (~256 iterations), but terminates by wrapping through 0
  - Warning: "Inefficient pattern: loops ~256 times. Use [-] to clear a cell."
- **With U8Checked**: Will error on overflow when reaching 255+1
  - Warning: "Will error on overflow with checked arithmetic."

**Pattern**: `[-]` (cell clear)
- **All models**: This is idiomatic BrainFuck
- **Warning**: None - this is the recommended way to clear cells

**Pattern**: `[--]` (multiple decrements)
- **All models**: Inefficient compared to `[-]`
- **Warning**: "Multiple decrements in a loop is inefficient. Consider using [-]."

**Pattern**: `[>]` or `[<]` (pointer seeking)
- **All models**: Idiomatic BrainFuck for seeking non-zero cells
- **Warning**: None - standard pattern

### Orthogonality: Mixing Models

**Important**: MemoryModel and CellModel are completely independent:

Any combination is valid:
```rust
// Fixed memory + Wrapping cells (default - traditional BF, production/JIT)
ExecutionConfig::builder()
    .with_memory_size(30000)
    .with_wrapping_cells()
    .build()

// Fixed memory + Checked cells (catch overflow bugs)
ExecutionConfig::builder()
    .with_memory_size(30000)
    .with_checked_cells()
    .build()

// Unbounded memory + Wrapping cells (dynamic growth + compatibility)
ExecutionConfig::builder()
    .with_unbounded_memory(1000, 100000)?
    .with_wrapping_cells()
    .build()
```

### Design Rationale

**Why only 2 cell models?**
- **U8Wrapping**: Standard BrainFuck behavior, aligns with JIT/AOT compilation
- **U8Checked**: Debugging mode to catch arithmetic bugs

**Why not saturating?**
- Creates infinite loops (`[+]` gets stuck at 255 forever)
- Not useful for debugging (just hangs, no useful error)
- Not used in production (non-standard behavior)
- Removed in favor of simpler, clearer API

**Future extensions**:
- **I8Wrapping**: Signed 8-bit cells (-128 to 127) for specific algorithms
- **U16Wrapping**: 16-bit cells (0-65535) for larger value ranges
- Both would follow the same pattern: wrapping (production) vs checked (debugging)

### Where This Matters

**For library users**:
- Use `with_wrapping_cells()` for compatibility with standard BrainFuck
- Use `with_checked_cells()` during development to catch bugs
- Mix with any MemoryModel for your specific needs

**For contributors**:
- Cell arithmetic delegation is in `interpreter.rs:205-214`
- CellModel trait and implementations are in `config.rs`
- Cell-model-aware validation is in `validator.rs`
- Trait-based design allows easy extension (I8Wrapping, U16Wrapping, etc.)

**For BrainFuck programs**:
- `[-]` is idiomatic for clearing cells
- `[+]` is inefficient (~256 iterations) - use `[-]` instead
- `[>]` and `[<]` are standard for seeking non-zero cells
- Cell and memory behaviors are independent

## Workspace Structure

This is a Cargo workspace with the following crates:

- **`gyrus`** (library): Core BrainFuck interpreter, parser, and runtime
  - Location: `crates/gyrus/`
  - **Module breakdown**:
    ```
    src/
    ├── lib.rs            - Module interface and crate docs
    ├── parser.rs         - Source → AST
    ├── interpreter/      - AST → Execution
    │   ├── mod.rs        -   entry points
    │   ├── state.rs      -   VM state
    │   ├── dispatch.rs   -   instruction dispatch
    │   ├── execution.rs  -   tree-walking executor
    │   ├── optimized.rs  -   optimized executor
    │   └── tests.rs      -   interpreter test suite
    ├── optimizer.rs      - AST → OptimizedProgram
    ├── types.rs          - optimizer newtypes
    ├── hooks/            - execution hooks
    │   ├── mod.rs        -   trait, manager, context
    │   └── builtin.rs    -   built-in hooks
    ├── error.rs          - Error types and formatting
    ├── syntax.rs         - Syntax highlighting
    ├── io.rs             - I/O abstraction
    ├── test_utils.rs     - Test helpers, random programs
    ├── debug.rs          - Debug symbol tables
    ├── validator.rs      - AST validation
    ├── codegen.rs        - String → BrainFuck compiler
    ├── config/           - ExecutionConfig and models
    ├── minify.rs         - AST → Source
    ├── stats.rs          - Statistics
    ├── instruction.rs    - AST nodes
    └── location.rs       - Source tracking
    ```
  - Can be used as a library by other Rust projects
  - **Not published to crates.io, by decision.** `publish = false` is set in
    `[workspace.package]`; use it from a path or git dependency instead.

- **`gyrus-cli`** (binary): Command-line interpreter
  - Location: `crates/gyrus-cli/`
  - Focused on **program execution** only
  - Handles runtime configuration (memory models, limits, timeouts)
  - Binary name: `gyrus`

- **`gyrus-tool`** (binary): Development and analysis tools
  - Location: `crates/gyrus-tool/`
  - Focused on **development workflows**
  - Subcommand-based CLI (minify, validate, debug-info)
  - Binary name: `gyrus-tool`

**Benefits of workspace structure**:
- ✅ Clear separation between library, execution CLI, and development tools
- ✅ Easy to add new binaries (debugger, REPL, JIT compiler)
- ✅ Library is usable on its own as a path or git dependency
- ✅ Each crate can have its own version
- ✅ Faster incremental compilation
- ✅ Clean module boundaries prevent coupling
- ✅ Tool features don't clutter the execution CLI

## Common Commands

### Build and Run
```bash
cargo build                           # Build entire workspace
cargo build --release                 # Optimized build

# Run interpreter
cargo run -p gyrus-cli -- programs/basic/hello_world.bf

# Run development tools
cargo run -p gyrus-tool -- minify programs/basic/hello_world.bf
cargo run -p gyrus-tool -- validate programs/tests/warnings_test.bf
cargo run -p gyrus-tool -- debug-info programs/basic/simple.bf
cargo run -p gyrus-tool -- view programs/basic/simple.bf --line-numbers

# Build specific crate
cargo build -p gyrus         # Build library only
cargo build -p gyrus-cli     # Build interpreter only
cargo build -p gyrus-tool    # Build tool only
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

Rust examples demonstrating library usage are in `crates/gyrus/examples/`:
- `basic_usage.rs` - Core parsing, execution, error handling
- `custom_io.rs` - Implementing custom I/O traits
- `memory_models.rs` - Different memory model configurations
- `validation.rs` - Program validation
- `minify.rs` - Code minification

Run with: `cargo run --example basic_usage`

## Error Handling Architecture

The error system is comprehensive and production-ready with **syntax-highlighted output**:

- **SourceLocation**: Tracks line, column, offset for every parse position
- **extract_source_context_highlighted()**: Generates syntax-highlighted error messages with ANSI colors
  - Uses 24-bit RGB colors for terminal output
  - Color scheme: cyan (pointer ops), green (cell ops), orange (loops), gray (comments/line numbers)
  - Caret positioning: `column + 6` spaces (accounts for 7-char line prefix "   N │ ")
- **extract_source_context()**: Plain text version (for non-terminal output)
- **validate_brackets()**: Pre-parse validation that finds ALL bracket errors in one pass
- **BfError variants**: Structured errors with relevant context
  - Parse errors: Include location and syntax-highlighted source context
  - Runtime errors: Include instruction index, source location, and syntax-highlighted context
  - Limit errors: Include the limit that was exceeded
- **RuntimeWarning variants**: All warnings include source location and syntax highlighting
  - CellOverflow: Shows instruction that caused 255→0 wrap
  - CellUnderflow: Shows instruction that caused 0→255 wrap
  - MemoryExpanded: Shows instruction that triggered memory growth

Example bracket validation flow:
1. `parse()` calls `validate_brackets()` before parsing
2. `validate_brackets()` scans entire source with a stack
3. Collects ALL unmatched `[` and `]` errors
4. If multiple errors found, reports all to stderr then returns first
5. This saves time by showing all bracket issues at once

Example runtime error flow:
1. Interpreter detects cell overflow at instruction N
2. Looks up source location using debug_info (if available)
3. Calls `extract_source_context_highlighted()` with source and location
4. Returns `BfError::CellOverflow { location, ... }`
5. Error Display shows formatted message with syntax-highlighted code and red caret

Example runtime warning flow (wrapping mode):
1. Cell value is 255, about to increment
2. Looks up source location: `debug_info.lookup(step_count - 1)`
3. Creates `RuntimeWarning::CellOverflow { source_location, ... }`
4. Warning added to stats.warnings vector
5. CLI displays warning with `warning.format_with_source(&source)` (syntax-highlighted)

## Validation Architecture

The validation system performs static analysis on parsed instructions:

- **BfWarning enum**: Structured warning types (EmptyLoop, ExtremeNesting, SuspiciousPattern, DeadCode)
- **validate()**: Entry point that returns `Vec<BfWarning>`
- **validate_instructions()**: Recursive traversal checking nesting depth
- **check_suspicious_loop_patterns()**: Pattern matching for common issues

Warnings detected:
1. **Empty loops** `[]` - No-op that can be removed
2. **Inefficient increment loops** `[+]`, `[++]` - Loop many times (~256, ~128 iterations)
3. **Extreme nesting** - Depth > 10 levels (performance impact)
4. **Inefficient patterns** - `[--]` instead of `[-]`

Note: Common patterns like `[>]`, `[<]`, and `[-]` are NOT flagged as they're standard BF idioms.

CLI integration:
- `--validate`: Show warnings and exit (does not execute)
- Validation always assumes u8 wrapping (production/JIT target)
- Independent of runtime cell model

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
- **Parse tests**: All instruction types, nested loops, comments
- **Error tests**: All error types with proper context
- **Limit tests**: Step limits, timeouts, memory bounds
- **Location tests**: Multiline programs, error positions
- **Validation tests**: Empty loops, infinite loops, nesting, clean programs
- **Comment tests**: Line comments, BF commands in comments, multiline
- **Minify tests**: Simple, line comments, nested loops, round-trip
- **Bracket matching tests**: Multiple errors, single errors, location tracking
- **Memory model tests**: Fixed, unbounded behaviors
- **Statistics tests**: Step counting, loop iterations, I/O tracking, memory tracking
- **Error formatting tests**: Syntax highlighting, source location, caret positioning
- **Hook system tests**: Hook infrastructure and manager behavior
- **Optimizer tests**: Run fusion, clear/scan/multiply loop recognition
- **Program corpus** (`crates/gyrus/tests/program_corpus.rs`): real BrainFuck
  programs run end to end, expectations declared in `programs/test_manifest.toml`
- **Property tests** (`crates/gyrus/tests/property_debug_symbols.rs`): proptest
  over debug symbol invariants

Test and line counts are deliberately not recorded here — they change with
every commit, and a stale number is worse than no number. Run
`cargo test --workspace` for the current picture.

To add new tests:
1. Test parsing with `parse(source).unwrap()` or `parse_with_debug(source).unwrap()`
2. Test execution with `interpret_with_config(&instructions, config, Some(&debug_info))`
3. Test validation with `validate(&instructions)`
4. Use `matches!` macro for error/warning pattern matching
5. Check error details (location, context, values)
6. For bracket errors, test both single and multiple error scenarios
7. For runtime errors, test both with and without debug info
8. For error formatting, use `format!("{}", error)` to get full output

## Future Architecture Notes

The roadmap includes:
- **Debugger**: ✅ Hook infrastructure complete! Build on top of `ExecutionHook` trait
  - Breakpoints: Use `before_instruction` hook with step count or source location matching
  - Step execution: Return `HookDecision::Break` to pause, resume by calling interpret again
  - Watchpoints: Use `after_instruction` hook to monitor memory changes
  - Time-travel debugging: Capture state snapshots in hooks, allow forward/backward navigation
- **Compiler**: Planned JIT/AOT backend - consider IR layer between parser and execution
- **Optimizations**: Instruction fusion (e.g., `+++` → IncrementValue(3)) will require AST transformation pass
- **Validation pass**: ✅ Complete - static analysis for warnings (dead loops, suspicious patterns)

**Hook System Integration (Complete):**
- `HookContext` provides full state access: memory, pointer, step count, source location, loop depth
- `HookDecision::Break` pauses execution with `BfError::ExecutionPaused`
- `HookDecision::Skip` allows instruction filtering/modification
- Zero overhead when hooks not used (`Option<HookManager>`)

## Implementation Status

**Completed (Phase 1, 2.1, 2.2, 3.1, 3.2, 4.1 from PRD + Community features)**:
- ✅ Source location tracking (Phase 1)
  - Parse errors include line/column
  - Runtime errors include source location via debug symbols
  - Runtime warnings include source location
- ✅ Rich error messages with context (Phase 1)
  - **Syntax-highlighted error and warning messages** (NEW)
  - ANSI 24-bit RGB color support
  - Color-coded BrainFuck commands by type
  - Red caret pointing at exact instruction
  - Line numbers and loop nesting visualization
- ✅ Execution limits (step count, timeout) (Phase 3.1)
- ✅ Configurable memory size (Phase 3.1)
- ✅ Verbose mode (Phase 1)
- ✅ Comprehensive test suite (unit, integration, property, and doc tests)
  - Added 8 comprehensive tests for error formatting and source location tracking
  - Added 8 tests for hook infrastructure (context creation, manager dispatch, early exit)
  - Tests cover single-char programs, multiline, nested loops, comments, hooks
- ✅ Validation pass with warnings (Phase 2.1)
- ✅ Line comments using `*` (Community feature)
- ✅ Code minification (Phase 4.1)
- ✅ Better bracket matching - multiple errors (Phase 2.2)
- ✅ Multiple memory models (Phase 3.2)
  - Fixed and Unbounded models (aligned with JIT/AOT goals)
  - CLI flags for model selection
  - Comprehensive testing
- ✅ Execution statistics tracking (Community feature)
  - Steps, loop iterations, memory usage
  - I/O tracking
  - Stats and warnings shown with `--verbose` flag
- ✅ Advanced I/O error handling (Phase 3.3)
  - EOF behavior configuration (SetZero, SetNegOne, NoChange, Error)
  - `--eof-behavior` CLI flag
  - Graceful EOF handling in Input instruction
- ✅ Configurable cell arithmetic (CellModel)
  - U8Wrapping: Standard BrainFuck (production, aligns with JIT/AOT)
  - U8Checked: Strict debugging mode (catches overflow/underflow bugs)
  - Cell-model-aware validation
  - `--cell-model` CLI flag
  - Fully orthogonal with MemoryModel (any combination supported)
- ✅ Runtime warnings with source location
  - MemoryExpanded: Shows unbounded memory growth with syntax highlighting
  - **Display behavior**: Only shown with `--verbose` flag
  - **Note**: Cell overflow/underflow warnings removed - wrapping is standard BF behavior
- ✅ **Plugin/Hook Architecture** (Foundation for debugger, profiler, tracer)
  - Complete hook system with 5 hook points: before/after instruction, loop enter/exit, completion
  - `ExecutionHook` trait for custom execution observers
  - `HookManager` for efficient hook dispatch
  - `HookContext` provides immutable state snapshots (memory, pointer, step count, source location, loop depth)
  - `HookDecision` enum for execution control (Continue, Break, Skip)
  - Zero-cost abstraction when hooks disabled (Option<HookManager>)
  - Builder pattern integration: `with_hook()`, `with_hooks_enabled()`
  - 8 unit tests for hook infrastructure
  - All tests passing
  - **Enables**: Interactive debuggers, profilers, execution tracers, time-travel debugging

**Remaining (from PRD)**:
- ⏳ Built-in hooks (StepBreakpoint, InstructionCounter, MemoryWatcher)
- ⏳ Hook usage examples and documentation
- ⏳ Debug symbols and runtime diagnostics (Phase 4.2) - partially complete with Phase 2
- ⏳ Visual TUI debugger (foundation ready via hooks)
- ⏳ Performance optimizations (instruction fusion, I/O buffering)
- ⏳ JIT/AOT compiler backend

**Project Structure Improvements**:
- ✅ Workspace migration (v0.2.0)
  - Separated core library from CLI binary
  - Foundation for future crates (debugger, REPL, JIT)
