# gyrus Architecture

This document describes the current architecture of gyrus and provides design notes for future development.

**Last updated**: 2025-10-30 (after hook system refactoring)

---

## Current Architecture (v0.3.0)

### Core Design Principles

1. **Separation of Concerns**: Parser, interpreter, and instrumentation are independent modules
2. **Opt-in Complexity**: Advanced features (debugging, profiling) are zero-cost when not used
3. **Extensibility via Hooks**: All instrumentation goes through the hook system
4. **Type Safety**: Rust's type system enforces correctness (no pointer arithmetic bugs)

### Module Structure

```
crates/
├── gyrus/          # Core library
│   ├── parser.rs            # Source → AST (tree structure)
│   ├── interpreter.rs       # AST → Execution (tree-walking)
│   ├── hooks/               # Hook system for instrumentation
│   │   ├── mod.rs           # Hook traits and context
│   │   └── builtin.rs       # Built-in hooks (stats, limits, debug)
│   ├── config/              # Execution configuration
│   │   ├── memory_model.rs  # Fixed/Unbounded memory behaviors
│   │   └── cell_model.rs    # Wrapping/Checked cell behaviors
│   ├── error.rs             # Rich error types with source locations
│   └── ...
├── gyrus-cli/      # CLI binary
├── gyrus-jit/      # Cranelift JIT for the optimized IR (`gyrus --jit`)
└── gyrus-tool/     # Development tools
```

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
on_error(&mut self, error: &BfError, context: &HookContext)
```

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

## Future: Interactive Debugger

### Requirements

An interactive debugger needs:
1. **Breakpoints**: Pause at specific instructions
2. **Step execution**: Execute one instruction at a time
3. **State inspection**: View memory, pointer, loop stack
4. **Reverse execution**: Step backwards (time-travel debugging)
5. **Watchpoints**: Break when memory/pointer changes
6. **Expression evaluation**: Check conditions during execution

### Proposed Design: DebuggerHook

**Concept**: Implement debugger as a special hook with bidirectional communication.

```rust
pub struct DebuggerHook {
    /// Breakpoints (instruction indices)
    breakpoints: HashSet<usize>,

    /// Watchpoints (memory addresses)
    watchpoints: HashSet<usize>,

    /// Command channel from debugger UI
    commands: Receiver<DebugCommand>,

    /// State snapshots for time-travel
    history: VecDeque<StateSnapshot>,
    history_limit: usize,

    /// Current execution mode
    mode: DebugMode,
}

pub enum DebugMode {
    Running,           // Execute normally
    StepInto,          // Pause after each instruction
    StepOver,          // Pause after current loop completes
    StepOut,           // Pause after exiting current loop
    Paused,            // Waiting for user command
}

pub enum DebugCommand {
    Continue,
    StepInto,
    StepOver,
    StepOut,
    SetBreakpoint(usize),
    ClearBreakpoint(usize),
    SetWatchpoint(usize),
    Inspect,           // Dump current state
    Reverse(usize),    // Go back N steps
}
```

**Hook Implementation**:

```rust
impl ExecutionHook for DebuggerHook {
    fn before_instruction(
        &mut self,
        instruction: &Instruction,
        context: &HookContext,
    ) -> HookDecision {
        // Save state snapshot for time-travel
        if self.mode != DebugMode::Running {
            self.history.push_back(StateSnapshot::from(context));
            if self.history.len() > self.history_limit {
                self.history.pop_front();
            }
        }

        // Check breakpoints
        if self.breakpoints.contains(&context.instruction_index()) {
            self.mode = DebugMode::Paused;
        }

        // Check watchpoints
        for &addr in &self.watchpoints {
            if context.pointer().get() == addr {
                self.mode = DebugMode::Paused;
            }
        }

        // Check execution mode
        match self.mode {
            DebugMode::Running => HookDecision::Continue,
            DebugMode::Paused => {
                // Wait for user command
                self.wait_for_command(context)
            }
            DebugMode::StepInto => {
                self.mode = DebugMode::Paused;
                HookDecision::Continue  // Will pause before next instruction
            }
            // ... other modes
        }
    }
}
```

**UI Integration**:

The debugger hook communicates with a TUI/GUI via channels:
- Hook → UI: State snapshots, pause events
- UI → Hook: User commands

**Time-Travel Debugging**:

Store `StateSnapshot` at each step:
```rust
struct StateSnapshot {
    memory: Vec<u8>,          // Full memory copy
    pointer: MemoryAddress,
    step_count: StepCount,
    loop_stack: Vec<LoopContext>,
}
```

For efficiency, use **copy-on-write** or **delta encoding**:
- Only store memory diffs between steps
- Limit history size (e.g., last 1000 steps)

**Challenges**:

1. **Performance**: Copying memory at every step is expensive
   - Solution: Only enable time-travel when explicitly requested
   - Solution: Use CoW (copy-on-write) for memory snapshots

2. **I/O side effects**: Can't truly reverse I/O operations
   - Solution: Mark I/O operations as "irreversible boundaries"
   - Solution: Buffer I/O and only commit on user confirmation

3. **Hook limitations**: Hooks can't modify execution flow arbitrarily
   - Current design: Hooks return `HookDecision::{Continue, Break, Skip}`
   - May need to extend with `HookDecision::Inject(Instruction)` for expression evaluation

### API Changes Needed

**None!** The current hook system supports debuggers without API changes. Just implement `DebuggerHook` and register it.

**Optional enhancement**:
```rust
// Allow hooks to modify state (carefully!)
pub enum HookDecision {
    Continue,
    Break,
    Skip,
    ReplaceInstruction(Instruction),  // NEW: For expression evaluation
}
```

---

## Future: Optimized Interpreter

### Goals

1. **Instruction fusion**: `+++` → `IncrementValue(3)`
2. **Loop optimization**: Recognize common patterns (e.g., `[-]` = clear cell)
3. **JIT compilation**: Compile hot loops to native code

### Proposed Design: IR Layer

**Concept**: Add an intermediate representation (IR) between AST and execution.

```
Source → AST → IR → Execution
              ↓
              JIT (optional)
```

**IR Design**:

```rust
pub enum IrInstruction {
    // Fused operations
    IncrementPointer(usize),     // > repeated N times
    DecrementPointer(usize),     // < repeated N times
    IncrementValue(u8),          // + repeated N times (wrapping)
    DecrementValue(u8),          // - repeated N times (wrapping)

    // Optimized patterns
    ClearCell,                   // [-] or [+] → *ptr = 0
    MoveValue { offset: isize }, // [->+<] → ptr[offset] += ptr[0]; ptr[0] = 0
    ScanRight,                   // [>] → while *ptr != 0 { ptr++; }
    ScanLeft,                    // [<] → while *ptr != 0 { ptr--; }

    // Unoptimized
    Input,
    Output,
    Loop(Vec<IrInstruction>),
}
```

**Optimization Pass**:

```rust
pub fn optimize(ast: &[Instruction]) -> Vec<IrInstruction> {
    let mut ir = Vec::new();
    let mut i = 0;

    while i < ast.len() {
        match &ast[i..] {
            // Pattern: Multiple increments
            [Instruction::IncrementValue, ..] => {
                let count = count_consecutive(ast, i, |inst| {
                    matches!(inst, Instruction::IncrementValue)
                });
                ir.push(IrInstruction::IncrementValue(count as u8));
                i += count;
            }

            // Pattern: Clear cell loop
            [Instruction::Loop(body), ..] if is_clear_loop(body) => {
                ir.push(IrInstruction::ClearCell);
                i += 1;
            }

            // Pattern: Move value loop
            [Instruction::Loop(body), ..] if let Some(offset) = is_move_loop(body) => {
                ir.push(IrInstruction::MoveValue { offset });
                i += 1;
            }

            // Default: No optimization
            _ => {
                ir.push(translate_single(&ast[i]));
                i += 1;
            }
        }
    }

    ir
}
```

**Execution**:

```rust
// IR interpreter (similar to current execute_block)
fn execute_ir(ir: &[IrInstruction], state: &mut VmState, ...) -> Result<()> {
    for instruction in ir {
        match instruction {
            IrInstruction::IncrementValue(n) => {
                // Every access goes through `cell`/`cell_at`: the cursor is
                // signed and may sit off the tape, so this is the one place
                // that can fail. Never index `state.memory` by the cursor.
                let cell = state.cell(None, 0)?;
                *cell = cell.wrapping_add(*n);
            }
            IrInstruction::ClearCell => {
                *state.cell(None, 0)? = 0;
            }
            // ... other optimized ops
        }
    }
}
```

**Benchmarks** (estimated improvements):

- `+++` (3 ops) → `IncrementValue(3)` (1 op): **3x faster**
- `[-]` (256 loop iterations) → `ClearCell` (1 op): **256x faster**
- `[->+<]` (N loop iterations) → `MoveValue` (1 op): **Nx faster**

### JIT Compilation (Future)

**Concept**: For hot loops, compile IR to native code using `cranelift`.

```rust
pub struct JitCompiler {
    builder: FunctionBuilder,
    module: JITModule,
}

impl JitCompiler {
    pub fn compile_loop(&mut self, ir: &[IrInstruction]) -> *const u8 {
        // Generate machine code for IR
        // ...
    }
}
```

**Execution Strategy**:
1. Start with interpreter
2. Track loop execution counts
3. When loop is "hot" (e.g., >1000 iterations), compile it
4. Replace interpreted loop with JIT-compiled version

**Challenges**:
- Safety: Must validate JIT-compiled code doesn't violate memory safety
- Debugging: JIT code can't have source locations (unless we emit DWARF)
- Hooks: JIT code needs to call back to hooks at appropriate points

### API Changes Needed

**Optional optimization flag**:

```rust
pub struct ExecutionConfigBuilder {
    // ... existing fields
    optimization_level: OptLevel,
    jit_enabled: bool,
}

pub enum OptLevel {
    None,         // Direct AST interpretation (current)
    Basic,        // Instruction fusion only
    Aggressive,   // Pattern recognition + fusion
}
```

**Backward compatible**: Default is `OptLevel::None`, maintaining current behavior.

---

## Performance Considerations

### Current Performance (v0.3.0)

**Characteristics**:
- Tree-walking interpreter: ~10-50x slower than native
- Hook overhead: <5% when hooks are enabled
- Memory allocation: One allocation per unbounded memory expansion

**Bottlenecks**:
1. Indirect calls through trait objects (cell/memory models)
2. Loop depth tracking and hook calls
3. Bounds checking on every memory access

**Not a bottleneck**:
- Parsing is fast (one-time cost)
- Error handling is zero-cost (cold path)

### Optimization Strategy

**Phase 1: Instruction Fusion** (10-100x improvement)
- Implement IR layer with basic fusion
- No JIT, still safe Rust
- **Estimated effort**: 2-3 weeks

**Phase 2: Pattern Recognition** (additional 10-100x for specific patterns)
- Recognize common idioms (`[-]`, `[->+<]`, etc.)
- Replace with optimized operations
- **Estimated effort**: 1-2 weeks

**Phase 3: JIT Compilation** (10-100x improvement for hot code)
- Integrate `cranelift` for JIT
- Requires careful safety analysis
- **Estimated effort**: 4-8 weeks

**Total potential speedup**: 1000-100,000x (approaching native speed for optimizable code)

---

## Testing Strategy

### Current Test Coverage

- **Parser**: 22 tests covering all syntax, errors, source locations
- **Interpreter**: 77 tests covering execution, hooks, limits, errors
- **Property tests**: 6 tests using proptest for fuzzing
- **Integration tests**: CLI and tool tests

**Coverage**: ~95% of core functionality

### Future Test Needs

**For Debugger**:
- Breakpoint accuracy
- State snapshot correctness
- Time-travel consistency
- Watchpoint triggering

**For Optimizations**:
- Semantic equivalence: `optimize(ast)` must produce same output as `ast`
- Performance regression tests
- Edge cases: What if optimized pattern appears in comments?

**Strategy**:
- Property-based testing: `forall ast. execute(ast) == execute(optimize(ast))`
- Benchmark suite: Track performance across versions
- Fuzzing: Use `cargo-fuzz` to find optimization bugs

---

## Conclusion

The current architecture (post-hook refactoring) provides excellent foundations for future development:

1. **✅ Clean separation of concerns**: Parser, interpreter, instrumentation
2. **✅ Extensible hook system**: Debuggers can be implemented as hooks
3. **✅ Type-safe abstractions**: Memory/cell models via traits
4. **✅ Rich error handling**: Source locations, loop call stacks
5. **✅ Comprehensive tests**: 179 passing tests

**Next steps**:
1. Implement interactive debugger hook (no API changes needed)
2. Add IR layer with instruction fusion (backward compatible)
3. JIT compilation for hot loops (optional feature flag)

The architecture is production-ready and future-proof.

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
│   └── gyrus-cli/  # CLI binary crate
│       ├── Cargo.toml
│       └── src/
│           └── main.rs      # CLI interface and entry point
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
├── PRD/                     # Designs for things not yet built
├── docs/                    # User-facing documentation (this file lives here)
├── Cargo.toml               # Workspace root
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
    SeekRight(usize, SourceRange),  // [>], [>>], ... (stride)
    SeekLeft(usize, SourceRange),   // [<], [<<], ...
    MoveRight(usize, SourceRange),  // [->+<] move value N cells right
    MoveLeft(usize, SourceRange),   // [-<+>] move value N cells left

    // General loops (recursively optimized body)
    Loop(Vec<OptimizedInstruction>, SourceRange),
}

/// Optimized program with metadata
pub struct OptimizedProgram {
    pub instructions: Vec<OptimizedInstruction>,
    pub original_count: usize,
    pub optimized_count: usize,
}
```

**API:**

```rust
/// Main entry point: optimize BF AST to IR
pub fn optimize(instructions: &[Instruction]) -> OptimizedProgram
```

### Implemented Optimizations

### 1. Instruction Fusion

Combines sequential operations of the same type:

| Pattern | Before | After | Speedup |
|---------|--------|-------|---------|
| Increment | `++++` (4 ops) | `Add(4)` (1 op) | 4× |
| Decrement | `----` (4 ops) | `Sub(4)` (1 op) | 4× |
| Move Right | `>>>>` (4 ops) | `Right(4)` (1 op) | 4× |
| Move Left | `<<<<` (4 ops) | `Left(4)` (1 op) | 4× |

**Implementation:** `optimize_block()` function uses a sliding window to count consecutive operations.

**Saturation:** Counts saturate at 255 for Add/Sub (u8 limit), unlimited for Right/Left (usize).

### 2. Loop Pattern Recognition

Detects common idioms and converts to single operations:

| Pattern | BF Code | Optimized | Description |
|---------|---------|-----------|-------------|
| Clear cell | `[-]` | `Zero` | Set current cell to 0 (not `[+]`: checked cells reject the wrap) |
| Set | `[-]+++` | `Set(3)` | Clear, then store a constant |
| Seek right | `[>]`, `[>>]`, ... | `SeekRight(stride)` | Find next zero cell (right), `stride` cells at a time |
| Seek left | `[<]`, `[<<]`, ... | `SeekLeft(stride)` | Find previous zero cell (left), `stride` cells at a time |
| Move right | `[->+<]` | `MoveRight(1)` | Move value 1 cell right, zero source |
| Move left | `[-<+>]` | `MoveLeft(1)` | Move value 1 cell left, zero source |

**Implementation:** `recognize_loop_pattern()` function pattern-matches on loop body.

**Filter:** LoopCheck instructions are filtered out before pattern matching.

### 3. Source Location Tracking

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

### 4. Recursive Loop Optimization

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



### Future Optimizations (Not Implemented Yet)

### Copy Patterns
- `[->+>+<<]` → `CopyRight([1, 2])` - Copy value to multiple offsets
- Preserves source cell value

### Multi-cell Moves
- `[->>+<<]` → `MoveRight(2)` - Move value N cells (N > 1)
- Currently only N=1 is implemented

### Multiplication Patterns
- `[->+++<]` → `MultiplyAdd(1, 3)` - Multiply current cell by 3, add to offset 1
- Common in arithmetic-heavy programs

### Dead Code Elimination
- Remove unreachable code after infinite loops
- Remove no-op sequences

### Constant Propagation
- Track known cell values through execution
- Eliminate redundant operations

### Integration Points

### Parser Integration
```rust
let instructions = parse(source)?;
let optimized = optimize(&instructions);
```

### Interpreter Integration (TODO)
```rust
// New optimized interpreter (to be implemented)
interpret_optimized(&optimized.instructions, config)?;
```

### Profiler Integration
```rust
// Map profiling data back to original source using SourceRange
for inst in &optimized.instructions {
    let range = inst.source_range();
    println!("Optimized instruction maps to original [{}, {})", range.start, range.end);
}
```

### Debugger Integration (Future)
```rust
// Set breakpoints on original source locations
// Optimized interpreter respects SourceRange for debugging
```

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

### Optimization Pass
- **Time Complexity:** O(n) where n = instruction count
- **Space Complexity:** O(n) for optimized program
- **Fast enough to run on every execution**

### Expected Runtime Speedup
- Simple arithmetic: **5-10×** (heavy fusion)
- Pointer movement: **10-20×** (pointer fusion)
- Loop-heavy: **2-5×** (pattern recognition + fusion)
- I/O-heavy: **1.5-2×** (less opportunity for fusion)

### Next Steps

1. ✅ Design OptimizedInstruction IR with SourceRange
2. ✅ Implement instruction fusion
3. ✅ Implement loop pattern recognition
4. ✅ Add unit tests (7 tests)
5. ⏳ Implement optimized interpreter
6. ⏳ Add benchmarks comparing optimized vs unoptimized
7. ⏳ Integrate with CLI (--optimize flag)
8. ⏳ Profile hanoi.bf and mandelbrot.bf with optimizations

### References

- Original AST: `src/instruction.rs`
- Parser: `src/parser.rs`
- Interpreter: `src/interpreter.rs`
- Benchmarks: `scripts/benchmark.sh`, golden outputs in `benchmarks/expected/`
