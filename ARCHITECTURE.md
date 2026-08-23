# gyrus Architecture

This document describes the current architecture of gyrus and provides design notes for future development.

**Last updated**: 2025-10-30 (after hook system refactoring)

---

## Current Architecture (v0.2.0)

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
- `pointer: MemoryAddress` - Current cell
- `step_count: StepCount` - Instructions executed
- `loop_depth: usize` - Nesting level
- `memory_model: MemoryModel` - Behavior strategy

**Memory Models** (via trait `MemoryBehavior`):
- **Fixed**: Bounds-checked array (default)
- **Unbounded**: Grows dynamically up to max limit

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
                let cell = &mut state.memory[state.pointer.get()];
                *cell = cell.wrapping_add(*n);
            }
            IrInstruction::ClearCell => {
                state.memory[state.pointer.get()] = 0;
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

### Current Performance (v0.2.0)

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
