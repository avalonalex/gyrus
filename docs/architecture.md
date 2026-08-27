# gyrus Architecture

How the pieces fit together: what each crate is responsible for, how the four
execution paths relate, and why the boundaries fall where they do.

Back to the [README](../README.md).

---

## Architecture

### Core Design Principles

1. **Separation of Concerns**: Parser, interpreter, and instrumentation are independent modules
2. **Opt-in Complexity**: Advanced features (debugging, profiling) are zero-cost when not used
3. **Extensibility via Hooks**: All instrumentation goes through the hook system
4. **Type Safety**: Rust's type system enforces correctness (no pointer arithmetic bugs)

### The crates

| Crate | Responsibility |
|---|---|
| `gyrus` | The library: parser, optimizer, both interpreters, hooks, diagnostics |
| `gyrus-cli` | The `gyrus` binary — run a program, in any of the four modes |
| `gyrus-tool` | The `gyrus-tool` binary — minify, validate, view, inspect, generate |
| `gyrus-jit` | Cranelift JIT over the optimized IR, behind `gyrus --jit` |
| `gyrus-tui` | Shared terminal widgets: source, memory, tape, output, status, help |
| `gyrus-debug` | The `gyrus-debug` binary — step through a program with the tape in view |
| `gyrus-tutorial` | The `gyrus-tutorial` binary — thirteen lessons in BrainFuck |
| `gyrus-macro` | The `.bfm` preprocessor: expansion, and the map back to macro source |
| `gyrus-corpus` | Test support: the program manifest, parsed once for both corpus suites |

The file-by-file layout is in [Project Structure](#project-structure) below;
this section covers the parts whose design is worth explaining.

### The Hook System

**Design**: All execution monitoring goes through `ExecutionHook` trait implementations.

**Built-in Hooks** (automatically registered):
- `StatsTrackerHook`: Collects execution statistics
- `WarningCollectorHook`: Tracks runtime warnings
- `LimitEnforcerHook`: Enforces step/time limits
- `DebugTrackingHook`: Maintains debug symbols and loop call stacks (opt-in)

**Hook Integration Points**:
```rust
// In execute_block():
before_instruction(&mut self, instruction: &Instruction, context: &HookContext)
after_instruction(&mut self, instruction: &Instruction, context: &HookContext)
on_loop_enter(&mut self, context: &HookContext, loop_info: Option<&LoopInfo>)
on_loop_exit(&mut self, context: &HookContext)
on_complete(&mut self, context: &HookContext)
```

There are five, not six: an `on_error` point is a reasonable thing to want and
does not exist. This list said otherwise for a year, which is the hazard of
writing a trait's shape in prose beside the trait.

**HookContext**: Immutable snapshot of interpreter state
- Memory snapshot (`&[u8]`)
- Pointer position
- Step count
- Loop depth
- Current instruction index

**Key Design Decision**: Hooks receive **immutable state snapshots** via `HookContext`. They cannot modify execution directly, ensuring hooks are safe and composable.

### Error Enrichment Pattern

Errors are created deep in memory/cell models without access to hooks. The interpreter enriches them with hook data after catching:

```rust
// Memory model creates basic error
Err(BfError::MemoryOutOfBounds {
    loop_call_stack: None,  // Will be enriched
    ...
})

// Interpreter enriches with hook data
if let Err(error) = execute_result {
    let loop_stack = debug_hook.lock().unwrap().loop_stack();
    return Err(error.with_loop_call_stack(loop_stack));
}
```

This keeps concerns separated while enabling rich error messages.

### Memory Management

**VmState** (private to interpreter):
- `memory: Vec<u8>` - The tape
- `pointer: MemoryAddress` - Cursor position. Signed: it may sit off either end
  of the tape, which is legal until something uses it
- `step_count: StepCount` - Instructions executed
- `loop_depth: usize` - Nesting level
- `memory_model: MemoryModel` - Behavior strategy
- `peak_used: usize` - Highest cell actually used, maintained at the access

**Memory Models** (via trait `MemoryBehavior`):
- **Fixed**: errors when a cell outside the tape is used (default)
- **Unbounded**: grows to cover cells that are used, up to a max limit

Models govern *access*, not movement. `VmState::cell`/`cell_at` is the only
place a cursor position can be wrong; pointer movement is plain arithmetic and
cannot fail.

**Cell Models** (via trait `CellBehavior`):
- **U8Wrapping**: Standard BF (255+1=0, production/JIT target)
- **U8Checked**: Strict debugging mode (overflow=error)

---

## How the debugger attaches

The hook system was built for this, and the claim that it needed no API change
to support a debugger turned out to be true: `gyrus-debug` and
`gyrus-tutorial` are written entirely against the library's public surface, and
adding them changed nothing in `gyrus`.

- `before_instruction` is the stop point, and `HookDecision::Break` unwinds the
  interpreter with `BfError::ExecutionPaused` when the user quits or restarts.
- `after_instruction` is the stop point for `[`, and only for `[`. The
  interpreter runs the `LoopCheck` at the head of a loop body itself and
  dispatches only `after_instruction` for it — before the check executes. For
  that one instruction this hook point means "about to run".
- `on_loop_enter` and `on_loop_exit` give the stack of enclosing loops that
  "step out" needs. Loop depth alone cannot express either "step over" or "step
  out": at a `[`, the depth is the same on the iteration about to start as it
  is once the loop has finished. Both are expressed as instruction ranges
  instead, taken from the loop metadata the parser records.
- `on_complete` is the only chance to capture the final tape; the VM state is
  gone by the time the interpreter returns.
- `BfInput` and `BfOutput` keep the program's bytes out of the interface that
  is already drawing on the terminal.

Two things a debugger might want still do not exist. There is no `HookDecision`
variant that replaces an instruction, which is what evaluating an expression in
a paused context would need. And `HookContext` is immutable by design, so
nothing can edit the tape from a hook.

`gyrus-debug` runs the tree-walking interpreter. Debugging needs a source
location per instruction and a hook per step, and the optimized path
deliberately has neither. See [the debugger](debugger.md).

---

## The macro expander (`crates/gyrus-macro`)

`.bfm` source in, pure BrainFuck out, plus a map from every emitted byte back
to the position in the macro source that wrote it. The map is the reason the
crate exists: a program that expands to a wall of BrainFuck and then reports a
cell overflow at column 3,847 of the *expansion* is the experience gyrus was
built to replace.

It understands `@define` (named constants), `OP{N}` (repeat counts), `@var`
and `@to` (named cells, with the cursor tracked and the movement emitted),
`@here` (assert a position without moving), `@macro` with parameters, and
`@ifdef`/`@ifndef`/`@endif` (a branch not taken is never expanded, so it may
hold names and brackets that would be errors in one that is, and a body's test
is made against the scope it is expanded into), and `@include`. A `@var` without
a cell has one chosen, and anywhere a number may be written so may an ASCII
character or a hexadecimal one — `'A'`, `'\n'` and `0x41` are all 65. A
directive must start its line and
owns the rest of it; `{` and `}` are reserved everywhere, `@` only at the start
of a line, so BrainFuck's free-form prose comments survive.

Three things are worth knowing about the design, all documented in full in the
crate's module documentation:

**Numbers wider than a cell are a library, not a language.** The design
recorded multi-cell variables as something the expander would have to grow, on
the assumption that a `size` on a declaration was the only way to get them.
`programs/macros/lib/wide.bfm` does it without one, in a hundred lines of which
two thirds are prose, because a macro parameter can name a cell: `@wide_dec(n_hi, n_lo, flag, held)` says
which four, and the carry is the same "empty a cell into a loop" idiom every
other branch in BrainFuck is. `programs/macros/factor.bfm` factors 13911 with
it.

**An included file declares; it does not emit.** `@include "lib.bfm"` reads
another file's `@define`s, `@var`s and `@macro`s, resolved relative to the file
that wrote the path, and reads a file named twice only once — so two libraries
can share a third and a cycle terminates instead of needing to be detected. It
may not emit BrainFuck -- nor move the cursor with `@here`, the one way to do
that without emitting -- and that is what the map below rests on: one position
per emitted byte, against one text, is not a thing a second file can be written
in. An instruction from a library would have to report either a line of the
file that included it or a line number belonging to a file the reader is not
looking at, and both are what this crate exists to prevent. Refusing costs a
library nothing, because a macro's bytes already name the invocation rather
than the definition.

**Located errors need no change to `gyrus`.** `Expansion::remap` rewrites
`parse_with_debug`'s `DebugInfo` using only public API. What it cannot carry is
loop metadata, which no foreign crate can construct — nothing in the error path
reads it, but `gyrus-debug` does, which is why stepping through a `.bfm` is not
yet possible.

**Four readers, one set of lexical rules.** The expander walks the source; a
macro body is walked again to find its closing brace; the "defined below this
line" hint walks ahead of the cursor; and a branch of a conditional that is not
taken is walked to its `@endif` without being expanded. They must agree about
where a `*` comment ends, how far a `'x'` literal reaches, and whether a `{` is a repeat
count — and each time they did not, it cost a bug. `lex.rs` holds one copy of
each rule; `lex::step` holds the classification the two skipping walks share.
The fourth reader arrived after that and was a caller rather than a copy, which
is what the unification was for.

**A macro is expanded in place, from a span of the source.** A body is not
copied — invoking one moves the cursor and scans to the closing brace — so
every position inside a body is a real position in the file, and the cursor
tracking, loop rules and expansion budget apply to macro output with no special
case. Emitted bytes name the *invocation* rather than the definition, since the
map holds one position per byte.

**Cursor tracking is measured in movement, not position.** A loop body that
does not return the cursor leaves the position unknown rather than being
refused, because `[>]` is ordinary BrainFuck.

**A record's stride is what a scan preserves.** With `@stride` declared, a loop
whose body moves by a whole number of records loses the cursor's *cell* but
keeps which *field* of a record it is on — so `@field` names stay reachable
across a scan, which is how an array is walked in every large BrainFuck
program. `scripts/check-mandelbrot-claims.py` measures how much of one.
See `expand.rs`'s module documentation for the rules.

## The optimized interpreter

`Source → AST → OptimizedProgram → execution` is the default path. `--debug`
keeps the AST and walks it directly, trading speed for a source location on
every instruction.

`optimizer.rs` fuses runs of identical instructions into single operations
(`Add`, `Sub`, `Right`, `Left`) and recognizes loop idioms: clear loops
(`Zero`), scan loops (`SeekRight`/`SeekLeft`), and multiply loops
(`MultiplyAdd`). The Optimizer Design section below covers what it recognizes
and why; [Performance](performance.md)
covers what it does not, including the folds that were tried and measured as
losses.

`OptimizedProgram` is the interface between the optimizer and both fast
engines. The optimized interpreter (`interpreter/optimized.rs`) and the JIT
consume the same structure, so every fold the optimizer learns serves both.

### The JIT (`crates/gyrus-jit`)

`gyrus --jit` compiles the whole optimized program to one native function
with Cranelift and runs it. The design is in the crate's module docs; the
points that matter to the rest of the architecture:

- It consumes `OptimizedProgram` unchanged, so every optimizer fold serves
  both engines, and honours the program's cell model as the interpreter does.
- The tape contract is enforced at every access with one compare; a run
  touching several cells, or a balanced loop nest, gets one guard for all of
  them and compiles check-free behind it. A failed guard falls back to a small
  interpreter of the same IR inside the runtime, which reproduces every effect
  up to the failing access and then the interpreter's own error.
- Errors are never traps. Each failure site is a cold exit block; the runtime
  rebuilds the same `BfError` the interpreter would, through the same
  constructors, and -- because every site knows its instruction -- with a
  source location, which the optimized interpreter cannot give.
- Hooks are not run (the JIT refuses a configuration that carries them);
  statistics are counted only on request (`--verbose`), because counting
  costs; `--max-steps` counts loop iterations.

This section used to sketch a different JIT: hot loops detected at runtime and
compiled individually, source locations via DWARF, and hook callbacks from
compiled code. All three were dropped. Compilation became whole-program,
locations became a per-site instruction index, and hooks were declined outright
rather than half-supported. Git history has the sketch if the reasoning is ever
wanted.

---

## Performance

Ratios rather than absolute times, because absolute times belong to whichever
laptop measured them. `scripts/benchmark.sh` re-measures everything, and
verifies the output while it does.

- **The optimizer is worth more than the JIT.** Fusing runs and folding loop
  idioms is the single largest win. By the time compilation shipped, most of
  the speedup the original plan attributed to it had already been taken by the
  optimizer.
- **The JIT is roughly 3.5x over the optimized interpreter on mandelbrot and
  1.4x on hanoi**, and *loses* on programs that finish in a few milliseconds,
  because compile time is part of the run — tens of milliseconds for the
  largest programs in the corpus.
- **The JIT is within about 1.6x of native** on mandelbrot. That gap is codegen
  quality, not bounds checking: removing every bounds check was measured at
  4.5%, which is why the next round is aimed at the generated code rather than
  at the guards.
- **Tree-walking (`--debug`) is the slow path on purpose.** It keeps the AST
  and a source location for every instruction, which is exactly what makes its
  errors precise.

Not bottlenecks: parsing, which is a fast one-time cost, and error
construction, which is a cold path.

[Performance](performance.md) records
what is left, including the experiments that were tried and measured as losses.

---

## Testing

[Testing](testing.md) describes the suite itself. The architectural point is
that four execution paths have to agree, so the primary defense is differential
rather than example-based: the JIT is held to the optimized interpreter, which
is held to the tree-walker, across both the bundled corpus and generated
programs under every memory and cell model combination.

An optimizer that is subtly wrong still produces plausible output, which is why
agreement between engines carries more weight here than any hand-written
expectation.

## Project Structure

```
gyrus/
├── crates/
│   ├── gyrus/      # Core library crate
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs           # Module interface
│   │   │   ├── parser.rs        # Source → AST parsing
│   │   │   ├── interpreter/     # AST → Execution
│   │   │   ├── optimizer.rs     # AST → OptimizedProgram
│   │   │   ├── hooks/           # ExecutionHook trait and built-in hooks
│   │   │   ├── validator.rs     # AST validation
│   │   │   ├── minify.rs        # AST → Source
│   │   │   ├── codegen.rs       # String → BrainFuck compiler
│   │   │   ├── syntax.rs        # Syntax highlighting
│   │   │   ├── debug.rs         # Debug symbol tables
│   │   │   ├── random.rs        # Random programs (`random` feature)
│   │   │   ├── test_utils.rs    # Unit-test helpers (private)
│   │   │   ├── io.rs            # I/O abstraction traits
│   │   │   ├── error.rs         # Error types and formatting
│   │   │   ├── config/          # Configuration types
│   │   │   ├── instruction.rs   # AST node definition
│   │   │   ├── location.rs      # Source position tracking
│   │   │   ├── types.rs         # Type-safe wrappers
│   │   │   └── stats.rs         # Execution statistics
│   │   ├── tests/
│   │   │   ├── program_corpus.rs        # Real programs, end to end
│   │   │   └── property_debug_symbols.rs # Proptest over debug symbols
│   │   ├── benches/
│   │   │   ├── interpreter.rs   # Interpreter benchmarks (criterion)
│   │   │   └── parser.rs        # Parser benchmarks (criterion)
│   │   └── examples/            # Rust library usage examples
│   │       ├── README.md        # Library examples documentation
│   │       ├── basic_usage.rs   # Basic parsing & execution
│   │       ├── custom_io.rs     # Custom I/O implementations
│   │       ├── memory_models.rs # Memory model configuration
│   │       ├── validation.rs    # Program validation
│   │       └── minify.rs        # Code minification
│   ├── gyrus-cli/  # `gyrus` binary — program execution
│   │   └── src/main.rs      # CLI interface and entry point
│   ├── gyrus-tool/ # `gyrus-tool` binary — development workflows
│   │   └── src/main.rs      # Subcommands: minify, validate, view, ...
│   ├── gyrus-jit/  # Cranelift JIT over OptimizedProgram
│   │   ├── src/             # Translator, runtime, slow-path interpreter
│   │   └── tests/           # Corpus, differential, and generated-program tests
│   ├── gyrus-tui/  # Shared terminal widgets, no application logic
│   │   └── src/
│   │       ├── cells.rs     # Reading a tape: what is under the cursor, what changed
│   │       ├── source.rs    # Code with line numbers, breakpoints, a cursor
│   │       ├── memory.rs    # The tape as a hex dump
│   │       ├── tape.rs      # A few cells, labelled, for teaching
│   │       ├── output.rs    # What the program printed
│   │       ├── watch.rs     # Watched cells and whether they changed
│   │       ├── status.rs    # Header and the two status rows
│   │       ├── help.rs      # The key-binding overlay
│   │       ├── overlay.rs   # A centered popup for results and explanations
│   │       ├── layout.rs    # The two screen shapes
│   │       ├── terminal.rs  # Entering and leaving the alternate screen
│   │       ├── test_utils.rs # Render a widget and read it back as text (private)
│   │       └── theme.rs     # Colors, matching gyrus::syntax
│   ├── gyrus-debug/ # `gyrus-debug` binary — the debugger
│   │   └── src/
│   │       ├── program.rs   # Source positions to instruction indices, both ways
│   │       ├── state.rs     # Session state and the rule for when to stop
│   │       ├── hook.rs      # The ExecutionHook and the I/O adapters
│   │       └── ui.rs        # Drawing and key handling
│   ├── gyrus-macro/ # `.bfm` preprocessor — expansion and its source map
│   │   ├── src/
│   │   │   ├── lex.rs       # The lexical rules, in one place: comments, literals, counts
│   │   │   ├── directive.rs # The directive vocabulary, built and planned
│   │   │   ├── expand.rs    # One pass: @define, repeat counts, brackets, macros
│   │   │   ├── source_map.rs # Origin per emitted byte, and the DebugInfo remap
│   │   │   └── error.rs     # Located errors, rendered like a parse error
│   │   └── tests/           # Round trip, source locations, and the oracle
   ├── gyrus-tutorial/ # `gyrus-tutorial` binary — the course
│   │   └── src/
│   │       ├── lesson.rs    # The thirteen lessons and their criteria
│   │       ├── trace.rs     # Recording every step of a run, for scrubbing
│   │       ├── editor.rs    # A very small text editor
│   │       ├── app.rs       # Lesson progress and the current attempt
│   │       └── ui.rs        # Drawing and key handling
│   └── gyrus-corpus/        # The test manifest, parsed; shared by both
│       └── src/lib.rs       #   corpus suites. Test support, not a product.
├── programs/                # BrainFuck programs for testing
│   ├── README.md            # Programs documentation
│   ├── basic/               # Simple demonstration programs
│   │   ├── hello_world.bf
│   │   ├── simple.bf
│   │   └── line_comments.bf
│   ├── tests/               # Feature testing programs
│   │   ├── test_eof.bf
│   │   ├── deep_nesting.bf
│   │   └── warnings_test.bf
│   ├── errors/              # Error handling demonstrations
│   │   ├── README.md        # Error examples documentation
│   │   ├── unmatched_bracket.bf
│   │   ├── memory_overflow.bf
│   │   ├── infinite_loop.bf
│   │   └── validation_warnings.bf
│   └── third-party/         # Programs by other authors (NOT MIT)
│       ├── CREDITS.md       # Per-file attribution and licenses
│       ├── advanced/        # Complex programs (quine, factor, mandelbrot, ...)
│       └── utilities/       # Small utilities from Cristofani's collection
├── benchmarks/expected/     # Golden outputs, diffed by scripts/benchmark.sh
├── scripts/                 # Gates: MSRV, doc links, examples, tape access, benchmarks
├── PRD/                     # Designs for things not yet built
├── docs/                    # User-facing documentation (this file lives here)
├── Cargo.toml               # Workspace root
├── rust-toolchain.toml      # The pinned development compiler
└── README.md
```

### Module Organization

The core library follows idiomatic Rust structure with clear separation of concerns:

- **lib.rs**: Pure module interface with re-exports
- **parser.rs**: Converts BrainFuck source code to AST
- **interpreter/**: Executes AST with configurable runtime — tree-walking,
  optimized, and tracing paths
- **optimizer.rs**: Rewrites the AST into fused, pattern-recognized instructions
- **hooks/**: `ExecutionHook` trait, manager, and built-in hooks
- **validator.rs**: Analyzes AST for warnings and best practices
- **minify.rs**: Converts AST back to minimal source code
- **codegen.rs**: Compiles a string into a BrainFuck program that prints it
- **random.rs**: Generates syntactically valid random programs for fuzzing and
  benchmark inputs, behind the off-by-default `random` feature
- **test_utils.rs**: Unit-test helpers. Private and `#[cfg(test)]`, so it is
  neither public API nor compiled into what consumers link
- **io.rs**: I/O abstraction traits (BfInput, BfOutput, StringIo)
- **Supporting modules**: error, syntax, debug, config, instruction, location,
  stats, types

Tests are co-located with the implementation, plus integration tests over a
corpus of real programs, property tests, and doc tests. Run `cargo test
--workspace` to see where things stand.

## Optimizer Design

### Overview

The optimizer transforms BrainFuck AST into an optimized intermediate representation (IR) that:
1. **Fuses repeated instructions** (e.g., `+++` → `Add(3)`)
2. **Recognizes loop patterns** (e.g., `[-]` → `Zero`)
3. **Preserves source location ranges** for debugging and profiling

### Architecture

### Module: `src/optimizer.rs`

**Key Types:**

```rust
/// Source location range for tracking optimizations
pub struct SourceRange {
    pub start: usize,  // Original instruction index (inclusive)
    pub end: usize,    // Original instruction index (exclusive)
}

/// Optimized IR instruction
pub enum OptimizedInstruction {
    // Fused operations
    Add(u8, SourceRange),           // +++ → Add(3)
    Sub(u8, SourceRange),           // --- → Sub(3)
    Right(usize, SourceRange),      // >>> → Right(3)
    Left(usize, SourceRange),       // <<< → Left(3)

    // I/O (not fused)
    Output(SourceRange),            // .
    Input(SourceRange),             // ,

    // Loop patterns
    Zero(SourceRange),              // [-]
    Set(u8, SourceRange),           // [-]+++ → Set(3)
    SeekRight(usize, SourceRange),  // [>], [>>], ... (stride)
    SeekLeft(usize, SourceRange),   // [<], [<<], ...
    // [->+++>+<<] → cell[ptr+offset] += cell[ptr] * mul, then cell[ptr] = 0
    MultiplyAdd(Vec<(isize, u8)>, SourceRange),

    // General loops (recursively optimized body)
    Loop(Vec<OptimizedInstruction>, SourceRange),
}

/// Optimized program with metadata
pub struct OptimizedProgram {
    pub instructions: Vec<OptimizedInstruction>,
    pub original_count: usize,
    pub optimized_count: usize,
    /// Not every fold is valid under every cell model, so a program is only
    /// meaningful under the one it was built for. `interpret_optimized`
    /// rejects a mismatch rather than running folds that do not hold.
    pub cell_model: CellModel,
}
```

**API:**

```rust
/// Optimize for the default cell model (u8 wrapping)
pub fn optimize(instructions: &[Instruction]) -> OptimizedProgram

/// Optimize for a specific cell model; checked cells disable the folds
/// that would swallow an overflow the program should have reported
pub fn optimize_with_cell_model(
    instructions: &[Instruction],
    cell_model: CellModel,
) -> OptimizedProgram
```

### Implemented Optimizations

#### 1. Instruction Fusion

Combines sequential operations of the same type:

| Pattern | Before | After | Speedup |
|---------|--------|-------|---------|
| Increment | `++++` (4 ops) | `Add(4)` (1 op) | 4× |
| Decrement | `----` (4 ops) | `Sub(4)` (1 op) | 4× |
| Move Right | `>>>>` (4 ops) | `Right(4)` (1 op) | 4× |
| Move Left | `<<<<` (4 ops) | `Left(4)` (1 op) | 4× |

**Implementation:** `optimize_block()` function uses a sliding window to count consecutive operations.

**Saturation:** Counts saturate at 255 for Add/Sub (u8 limit), unlimited for Right/Left (usize).

#### 2. Loop Pattern Recognition

Detects common idioms and converts to single operations:

| Pattern | BF Code | Optimized | Description |
|---------|---------|-----------|-------------|
| Clear cell | `[-]` | `Zero` | Set current cell to 0 (not `[+]`: checked cells reject the wrap) |
| Set | `[-]+++` | `Set(3)` | Clear, then store a constant |
| Seek right | `[>]`, `[>>]`, ... | `SeekRight(stride)` | Find next zero cell (right), `stride` cells at a time |
| Seek left | `[<]`, `[<<]`, ... | `SeekLeft(stride)` | Find previous zero cell (left), `stride` cells at a time |
| Move | `[->+<]` | `MultiplyAdd([(1, 1)])` | Add to offset 1, zero source |
| Copy | `[->+>+<<]` | `MultiplyAdd([(1, 1), (2, 1)])` | Add to two offsets, zero source |
| Multiply | `[->+++<]` | `MultiplyAdd([(1, 3)])` | Add 3x to offset 1, zero source |

**Implementation:** `recognize_loop_pattern()` function pattern-matches on loop body.

**Filter:** LoopCheck instructions are filtered out before pattern matching.

#### 3. Source Location Tracking

Every optimized instruction tracks its origin:

```rust
// Example: "+++>---" optimizes to:
[
    Add(3, SourceRange { start: 0, end: 3 }),    // Instructions 0-2
    Right(1, SourceRange { start: 3, end: 4 }),  // Instruction 3
    Sub(3, SourceRange { start: 4, end: 7 }),    // Instructions 4-6
]
```

**Benefits:**
- Runtime errors map back to original source
- Profiler can attribute time to original instructions
- Debugger can set breakpoints on original code

#### 4. Recursive Loop Optimization

Nested loops are optimized recursively:

```rust
// [++[-]]
Loop([
    LoopCheck,
    IncrementValue,
    IncrementValue,
    Loop([LoopCheck, DecrementValue])
])

// Optimizes to:
Loop([
    Add(2, range=1..3),
    Zero(range=3..5)
], range=0..5)
```



### Not implemented

- **Dead code elimination** — unreachable code after an infinite loop, no-op
  sequences.
- **Constant propagation** — track known cell values, drop the operations that
  are then redundant.

Copy, multi-cell move, and multiplication patterns used to be listed here. They
are all `MultiplyAdd` now: it takes a list of `(offset, multiplier)` pairs, so
one variant covers all three.

[Performance](performance.md) is the
catalogue — what is missing, what was tried, and what was measured as a loss.

### Integration Points

#### Parser Integration
```rust
let instructions = parse(source)?;
let optimized = optimize(&instructions);
```

#### Interpreter Integration
```rust
let optimized = optimize(&instructions);
interpret_optimized(&optimized, config, debug_info.as_ref())?;
```

#### Profiler Integration
```rust
// Map profiling data back to original source using SourceRange
for inst in &optimized.instructions {
    let range = inst.source_range();
    println!("Optimized instruction maps to original [{}, {})", range.start, range.end);
}
```

#### Debugger Integration

`gyrus-debug` does not use any of this: it runs the tree-walking interpreter,
where every instruction has one source location rather than a range. `SourceRange`
exists for the tools that report on optimized code — `gyrus-tool optimize` and
the JIT's error locations — not for stepping.

### Design Decisions

### Why SourceRange instead of single SourceLocation?

**Fused instructions span multiple source locations:**
- `+++` at line 1, columns 1-3 becomes `Add(3, range=0..3)`
- Single location would lose precision
- Range preserves full mapping for debugging

### Why separate OptimizedInstruction enum?

**Clean separation of concerns:**
- Original `Instruction` remains simple AST
- `OptimizedInstruction` carries optimization metadata
- Different execution paths (unoptimized vs optimized)
- Future: Could compile to native code from OptimizedInstruction

### Why not optimize in-place?

**Preservation of original AST:**
- Parser output remains unchanged
- Can validate unoptimized code
- Can compare optimized vs unoptimized execution
- Debugging unoptimized code is easier

### Why saturating_add for fusion?

**Safety against overflow:**
- `Add(255)` + `Add(1)` = `Add(255)`, not overflow
- Alternative: Split into multiple Add instructions
- Current: Conservative (may miss some fusion opportunities)

### Performance Characteristics

#### Optimization Pass
- **Time Complexity:** O(n) where n = instruction count
- **Space Complexity:** O(n) for optimized program
- **Fast enough to run on every execution**

#### Runtime speedup

Measured rather than estimated: see the [Performance](#performance) section
above, and re-measure with `scripts/benchmark.sh`.

### References

- Original AST: `src/instruction.rs`
- Parser: `src/parser.rs`
- Interpreter: `src/interpreter/`
- Benchmarks: `scripts/benchmark.sh`, golden outputs in `benchmarks/expected/`
