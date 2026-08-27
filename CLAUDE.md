# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

gyrus is a BrainFuck interpreter, optimizer, JIT, terminal debugger, and
interactive tutorial written in Rust, built as a learning project. Rust edition
2024, MSRV 1.95 (Cranelift's floor;
the library alone would need 1.88 for let-chains); `rust-toolchain.toml` pins
the development compiler.

**Project Structure**: Cargo workspace with multiple crates

- **gyrus** (`crates/gyrus/`): Core library crate
- **gyrus-cli** (`crates/gyrus-cli/`): `gyrus` binary — program execution
- **gyrus-tool** (`crates/gyrus-tool/`): `gyrus-tool` binary — development tools
- **gyrus-jit** (`crates/gyrus-jit/`): Cranelift JIT for the optimized IR
  (`gyrus --jit`); see `docs/execution-models.md` and the crate's module docs
- **gyrus-tui** (`crates/gyrus-tui/`): shared terminal widgets — source panel,
  hex memory dump, labelled tape strip, output, watches, status, help and
  overlay popups, plus the terminal guard and `cells.rs` for reading a tape.
  A widget may name what it draws (`SourceView::breakpoints`); it may not name
  a **key**, because the two binaries bind them differently — `?` opens help in
  the debugger, and in the tutorial `?` is a character you might be typing. Any
  text naming a key is a parameter (`HelpOverlay::dismiss`,
  `WatchList::empty_hint`). No application state lives here
- **gyrus-debug** (`crates/gyrus-debug/`): `gyrus-debug` binary — the terminal
  debugger; see `docs/debugger.md`
- **gyrus-tutorial** (`crates/gyrus-tutorial/`): `gyrus-tutorial` binary —
  thirteen lessons in BrainFuck; see `docs/tutorial.md`
- **gyrus-macro** (`crates/gyrus-macro/`): the `.bfm` macro preprocessor —
  expands macro source to pure BrainFuck and carries an origin map, so runtime
  errors name the `.bfm` rather than the expansion. Written entirely against
  `gyrus`'s public API, as the debugger was. `@define`, `OP{N}`,
  `@var`/`@to`/`@here` with static cursor tracking, `@stride`/`@field` for
  record-relative addressing, `@macro` with parameters,
  `@ifdef`/`@ifndef`/`@endif`, and `@include` (a library declares; it does not
  emit). `gyrus` runs a `.bfm` and `gyrus-tool expand` produces the BrainFuck.
  See `docs/macro-language.md` for the language and `docs/architecture.md` for
  the crate
- **gyrus-corpus** (`crates/gyrus-corpus/`): test support only — parses
  `programs/test_manifest.toml` so the tree-walker's corpus test and the JIT's
  read the same cases. Not a product crate; nothing depends on it outside
  `[dev-dependencies]`
- Future: REPL as a separate crate

## Documentation Organization

- **IMPORTANT** — Do not create markdown files unless the user explicitly asks.
  You may offer. Prefer succinct comments in the relevant code file.
- **`README.md`** — the landing page, kept short on purpose. It says what gyrus
  is, shows one error message, gives a quick start, and links onward.
- **`docs/`** — user-facing documentation: `manual.md` is the task-oriented
  front door, and the rest are reference — usage, errors, execution models,
  tooling, debugger, tutorial, macro language, development, testing,
  architecture, performance.
  Anything describing what *exists* goes here, and the manual links rather than
  restates. `performance.md` is also where the optimization work's
  negative results live: it concluded in August 2026, and the experiments that
  did not pay are recorded there so they are not repeated.
- **`PRD/`** — design documents for what does **not** exist yet. When a feature
  ships, delete its PRD rather than archiving it: the code and `docs/` describe
  what was built, and git history keeps the reasoning.
- There is no `internal/` directory and no `PRD/archived/`. Both were deleted in
  August 2026 — together about 12,000 lines of completed-milestone records,
  progress logs, and superseded status docs that no reader needed and that
  quietly contradicted the code.

The bar for a new document: would someone who has not read this conversation
need it? Notes to self, "phase N complete" records, and status snapshots fail
that test. Prefer one focused document over a section in a large one.

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
   - **The tape contract**: reading or writing a cell outside the tape is an
     error; moving the cursor outside it is not. Enforced at the access, in
     both interpreters — `VmState::cell_at` is the only place a position can be
     wrong, and pointer movement never fails.
   - **Multiple memory models**:
     - Fixed: Traditional fixed-size array (default 30,000 bytes)
     - Unbounded: Grows to cover cells that are *used* beyond its size
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
- **random** (`random.rs`): random BrainFuck program generation for fuzzing
  and benchmark inputs. Behind the off-by-default `random` feature — the
  crate's only optional dependency (`rand`)
- **test_utils** (`test_utils.rs`): unit-test helpers. `#[cfg(test)]` and
  private: not public API, not shipped to consumers
- **lib** (`lib.rs`): Pure module interface with re-exports

**CLI** (`crates/gyrus-cli/src/main.rs`) — execution only
   - Flow: read file → parse → optimize (unless `--debug`) → interpret, or
     JIT-compile and run (`--jit`) → (stats)
   - Accepts `.bfm` macro source as well as `.bf`: expanded first, with its
     debug symbols rewritten so errors name the macro source. A `.bfm` defaults
     to `--debug` rather than the optimized interpreter, which is the one
     engine that cannot name a source position
   - Flags: `--verbose`, `--quiet`, `--debug`, `--trace`, `--jit`, `--max-steps`,
     `--timeout`, `--memory-size`, `--memory-model`, `--cell-model`,
     `--unbounded-initial`, `--unbounded-max`, `--eof-behavior`
   - Configuration via `ExecutionConfig` (builder pattern)

**Tool** (`crates/gyrus-tool/src/main.rs`) — development workflows
   - Subcommands: `expand`, `minify`, `validate`, `debug-info`, `view`,
     `generate`, `compile`, `optimize`
   - **Note**: minify and validate are subcommands here, NOT flags on `gyrus`.
     `gyrus --minify` and `gyrus --validate` do not exist.
   - **Runtime warnings**: Only shown with `--verbose` flag (cell wrapping is common in BF programs)
   - **Debug symbols**: Opt-in with `--debug` flag (default: fast mode without source locations)

**Debugger** (`crates/gyrus-debug/`) — step through a program
   - Runs the tree-walking interpreter: it needs a source location per
     instruction and a hook per step, and the optimized path has neither
   - Stops **before** each instruction. `]` is not a stopping point because it
     is not an instruction — `[` is the `LoopCheck` at the head of the body,
     and `]` is the loop's structure
   - `before_instruction` covers every instruction except `[`;
     `after_instruction` covers `[`, and only `[`, because the interpreter
     dispatches that hook point for `LoopCheck` *before* the check runs
   - Breakpoints are source positions (line **and** column), not lines: a
     BrainFuck program is often one line of a hundred instructions
   - "Step over" and "step out" are instruction *ranges*, not loop depths — at
     a `[` the depth is the same before the loop and after it
   - Same runtime flags as `gyrus`, plus `--break LINE[:COL]`, `--run`,
     `--input`, `--input-file`, `--display`

**Tutorial** (`crates/gyrus-tutorial/`) — thirteen lessons, numbered 0 to 12
   - Records every step of a run and lets the learner scrub through it in both
     directions. Affordable because a lesson tape is 16 cells and runs are
     capped at 20,000 steps; the debugger cannot do this on a 30,000-cell tape
   - Lessons are one table in `src/lesson.rs`: prose, starter, answer, hints,
     and a check. Three tests hold it together — every answer solves its own
     lesson, every starter parses and runs, and no starter is already the answer

### Key Design Decisions

- **Error handling**: Uses `thiserror` for custom error types (`BfError`)
  - All errors include context (location, source snippet, details)
  - Structured error types (not strings) for better error handling
- **Memory models**: Two configurable models via `MemoryModel` enum
  - Fixed: Traditional bounds-checked array
  - Unbounded: Vec that grows on-demand up to max limit
  - Models govern *access*, not movement: `MemoryModel::cell` resolves a cursor
    to a cell, growing or reporting as the model decides. Movement is plain
    arithmetic on a signed cursor and cannot fail.
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

2. **Unbounded**:
   - Initial size: Configurable (default 30,000)
   - Maximum size: Configurable (default 1,000,000)
   - Overflow behavior: **GROW** - memory expands dynamically up to max limit
   - Negative movement: Still errors (no negative indices)
   - Use case: Programs with dynamic memory needs

**Code location**: `config.rs` defines `MemoryModel` enum and `MemoryBehavior` trait

### Cell-Model-Aware Validation

The validator (`validator.rs`) provides **cell-model-aware** warnings via
`validate_with_cell_model()`; `validate()` is that with the default model.
Warnings carry the position of the loop's `[`, resolved through `DebugInfo`,
and `BfWarning::format_with_source` renders them with a caret the way errors
are rendered.

**Pattern**: `[+]` (increment loop behavior)
- **With U8Wrapping**: Inefficient (~256 iterations), but terminates by wrapping through 0
  - Warning: "Inefficient pattern: loops ~256 times. Use [-] to clear a cell."
- **With U8Checked**: Will error on overflow when reaching 255+1
  - Warning: "Will fail under checked cells: incrementing by N never reaches
    zero, and the cell overflows at 255. Use [-] to clear a cell."

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
- Cell arithmetic delegation is in `interpreter/execution.rs` (tree-walker) and
  `interpreter/optimized.rs` (optimized path), both calling `try_increment` /
  `try_decrement`
- CellModel trait and implementations are in `config/cell_model.rs`
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
    ├── random.rs         - Random program generation (`random` feature)
    ├── test_utils.rs     - Unit-test helpers (private, cfg(test))
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

**Why the workspace split**: the library is usable on its own as a path or git
dependency; development tooling does not clutter the execution CLI; module
boundaries stay honest because crossing one requires a manifest edit; and a new
binary (debugger, REPL) is a new crate rather than another flag.

## Common Commands

### Build and Run
```bash
cargo build                           # Build entire workspace
cargo build --release                 # Optimized build

# Run interpreter
cargo run -p gyrus-cli -- programs/basic/hello_world.bf

# Run the debugger and the tutorial (both take over the terminal)
cargo run -p gyrus-debug -- programs/basic/hello_world.bf
cargo run -p gyrus-tutorial

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
cargo update                          # Refresh Cargo.lock within the declared ranges
```

### Verifying claims that rot

Some facts in this repo are claims about things nobody exercises day to day,
and they go stale silently. Both of these have already been wrong once, so both
are now scripts rather than good intentions:

```bash
scripts/check-msrv.sh                 # workspace really builds on its declared MSRV
scripts/check-readme-commands.py      # every flag README.md and docs/ use exists
scripts/check-doc-links.py            # every relative Markdown link resolves
scripts/check-examples.sh             # every example still runs, not just compiles
scripts/check-tape-access.py          # the tape is only indexed where the contract is enforced
scripts/check-bfm-pseudocode.py       # every .bfm with a loop says what the loop is for
scripts/check-macro-language.py       # every example in the .bfm reference expands to what it says
```

### Benchmarking and profiling

```bash
scripts/benchmark.sh                  # time each mode, diff output vs benchmarks/expected/
scripts/benchmark.sh --full           # include the --debug runs for hanoi and mandelbrot
scripts/benchmark.sh --profile PROG   # loop profile via --trace
cargo bench                           # criterion micro-benchmarks
```

`benchmark.sh` diffs every run against a golden output in `benchmarks/expected/`
and, for the fast programs, checks that the optimized and `--debug`
interpreters agree byte for byte — so it doubles as a differential test of the
optimizer. A timing that improves while the output moves is a bug, and the
script fails rather than printing the number. Only re-record with `--record`
after confirming the new output is correct.

`check-msrv.sh` reads `rust-version` out of `Cargo.toml` instead of restating
it, and installs that toolchain if it is missing. Run it after touching
dependencies or using a new language feature — the declared MSRV was already
wrong once (1.85 by inference; 1.88 in fact, because of let-chains).

`check-readme-commands.py` needs `cargo build --release --workspace` first. It
exists because the README documented `gyrus --validate` and `gyrus --minify`
long after both became `gyrus-tool` subcommands.

`check-tape-access.py` enforces the tape contract's one structural requirement:
every read and write goes through `VmState::cell`/`cell_at`, because that is
where the bound lives. `docs/architecture.md` states it as an imperative --
"Never index `state.memory` by the cursor" -- and a claim in prose is one that
erodes. A site that genuinely needs a direct index says why with a
`// tape-access-ok:` note.

`check-examples.sh` runs each example rather than only building it. Building is
already covered by clippy, and it is not enough: when `MemoryAddress` became
signed, `hooks_execution_tracer` still compiled and panicked on its first
instruction, and nothing noticed because nothing ran it.

**When adding a claim to the docs, ask whether a script could check it.** If it
could, write the script — an unexecuted claim is one that will eventually be
false.

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

CLI integration (`gyrus-tool validate FILE`, not a `gyrus` flag):
- `--cell-model`: model to assume while validating (default: wrapping)
- `--strict`: exit non-zero if any warning is found, for CI
- `--verbose`: extra validation context
- Independent of the runtime cell model used to execute

## Minification System

Converts parsed instructions back to minimal BrainFuck source:

- **minify()**: Entry point that returns String
- **minify_instructions()**: Recursive conversion of AST to BF code
- Strips all comments (line and implicit)
- Removes all whitespace and formatting
- Size reduction depends entirely on how commented the source is: 94.9% on
  `programs/basic/line_comments.bf` (514 bytes to 26), 49.6% on the dense
  `life.bf` (1,479 to 745). Do not quote a single headline number
- Round-trip property: parse → minify → parse yields identical AST

CLI integration (`gyrus-tool minify FILE`, not a `gyrus` flag):
- `-o/--output FILE`: Write to file instead of stdout
- `-v/--verbose`: Show compression stats

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
  programs run end to end, every case read from `programs/test_manifest.toml`
  via `gyrus-corpus`. The JIT's `corpus.rs` reads the same cases, so a manifest
  entry tests both engines — add programs there, not as hand-written tests
- **Generated differential** (`crates/gyrus/tests/generated_differential.rs`):
  the optimizer against the tree-walker on generated programs, under both
  memory models and both cell models
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

## What exists, and what does not

The ✅/⏳ status tables that used to end this file are gone. They were a
snapshot, they were wrong (they still listed the JIT as unbuilt after it
shipped), and the repository has a rule against exactly that kind of record.
What follows is the shape of the thing, which changes far more slowly.

**Built and shipped**: the parser with full source locations, four execution
paths (tree-walking, optimized, JIT, tracing), the optimizer, the hook system
with its built-in hooks, static validation, minification, syntax highlighting,
debug symbols, the string-to-BrainFuck compiler, the program generator, the
`gyrus-tool` subcommands, the terminal debugger, and the tutorial.

**Not built**: a REPL and an AOT backend on the JIT's translator. Neither has
code, or anything beyond the idea.

**The macro preprocessor is finished** (`gyrus-macro`). The language is
`@define`, repeat counts, `@var`/`@to`/`@here` with cursor tracking,
`@stride`/`@field` for arrays of records walked by scan loops, `@macro` with
parameters, `@ifdef`/`@ifndef`/`@endif`, and `@include` — and so is the source
map, so a runtime error in a `.bfm` reports the line and column somebody wrote.
Its oracle generator (`tests/oracle.rs`) is the second thing in the repository
that proves correctness rather than agreement between engines. `gyrus` runs a
`.bfm` and `gyrus-tool expand` produces the BrainFuck. Its PRD was deleted when
it shipped, per the rule above; `docs/macro-language.md` is the language
reference and `docs/architecture.md` describes what the crate does.

`programs/macros/` is where to look for what the language is *for*:
`99bottles.bfm` prints 11,354 bytes that match a hand-written program byte for
byte, and `factor.bfm` factors 13911 using wide arithmetic that turned out to
be a library (`lib/wide.bfm`) rather than the language feature the design
assumed it needed.

**One rule of that crate is worth knowing before reading it**: an `@include`d
file *declares* — it does not emit, and does not move the cursor. The source map holds one position per
emitted byte against one text, and a second file cannot be written in it — so
an instruction from a library would otherwise report either a line of the file
that included it or a line number belonging to a file the reader is not looking
at. Refusing to emit is the third option, and it costs a library nothing,
because a macro's bytes already name the invocation.

**The hook system was the debugger's foundation**, and the claim that it needed
no API change to support one held up — `gyrus-debug` and `gyrus-tutorial` are
written entirely against the library's public surface and changed nothing in
`gyrus`:
- `HookContext` gives memory, pointer, step count, source location, loop depth
- `before_instruction` is where the debugger stops; `HookDecision::Break`
  unwinds with `BfError::ExecutionPaused`
- `on_loop_enter`/`on_loop_exit` give the stack that "step out" needs
- `on_complete` is the only chance to capture the final tape
- `Option<HookManager>` means zero overhead when nothing is registered
- The JIT declines hooks outright rather than half-supporting them: a
  configuration carrying hooks is refused

Two things a debugger might want still do not exist: a `HookDecision` variant
that substitutes an instruction (which is what evaluating an expression in a
paused context would need), and any way to write to the tape from a hook —
`HookContext` is immutable by design.

For anything more specific than this, read the code or `docs/`. For why a
decision was made, read git history.
