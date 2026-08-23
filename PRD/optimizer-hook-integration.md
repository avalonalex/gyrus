# PRD: Keeping Hooks Meaningful Under Optimization

**Status**: Design complete, not implemented
**Last Updated**: 2026-08-23
**Priority**: High — blocks aggressive optimization work

## Summary

Optimization and observability pull in opposite directions: fusing `+++` into
`Add(3)` or a `[-]` into `Zero` erases exactly the instruction boundaries a
debugger, profiler, or tracer wants to stop at. This document works out how far
the optimizer can go while `ExecutionHook` still sees something truthful.

Extracted from the former `optimization-and-advanced-features.md`, whose other
four sections duplicated the focused PRDs they pointed at
(`optimizer_improvements.md`, `macro-preprocessor-design.md`,
`tui_debugger_and_tutorial.md`, `compilation_backend.md`). This was the part
that existed nowhere else.

## 🔧 Hook System Integration - CRITICAL DESIGN DECISION

**Last Updated**: October 2025
**Status**: Architecture design complete, implementation pending

### Why This Matters

gyrus has a **production-ready hook system** that fundamentally changes how we approach optimizations. The hook system provides:

- ✅ **Execution monitoring** via 5 hook points (before/after instruction, loop enter/exit, completion)
- ✅ **Zero-cost abstraction** when hooks disabled
- ✅ **Built-in hooks** for stats, warnings, limits, debug tracking
- ✅ **Source location tracking** via DebugTrackingHook
- ✅ **Composable architecture** - multiple hooks work together

**Key Insight**: Optimizations must **preserve hook semantics** - debuggers, profilers, and tracers must see meaningful events even when code is optimized.

---

### Optimization Strategy: Three-Tier Approach

We adopt a **layered optimization strategy** that balances performance with debuggability:

#### Tier 1: Parse-Time Optimizations (Always-On, Hook-Transparent)
**Purpose**: Safe optimizations that don't affect hook semantics

**Optimizations**:
- ✅ **Run-Length Encoding (RLE)**: `+++` → `Add(3)`
  - **Hook Impact**: None - hooks see `Add(3)` instead of 3× `IncrementValue`
  - **Debug-Friendly**: Source locations preserved via DebugInfo
  - **Performance**: 50-70% instruction reduction

**Implementation**:
```rust
// In parser.rs
fn parse_block(source: &str) -> Result<Vec<Instruction>> {
    // During parsing, collapse consecutive operations
    match ch {
        '+' => {
            let count = count_consecutive(source, pos, '+');
            instructions.push(Instruction::Add(count));
            pos += count;
        }
        // Similar for -, >, <
    }
}
```

**Why Safe for Hooks**:
- Hooks still see every instruction (just optimized form)
- Step count is accurate (1 Add(9) = 1 step, not 9)
- Source locations maintained
- No semantic changes

#### Tier 2: Runtime/Adaptive Optimizations (Hook-Aware)
**Purpose**: Optimize hot code without breaking debugger

**Optimizations**:
- ⏳ **Clear Loop**: `[-]` → `Set(0)` (detected at runtime)
- ⏳ **Scan Loop**: `[>]` → `ScanRight` (detected at runtime)
- ⏳ **Copy/Multiply**: `[->++<]` → `Multiply(0, 1, 2)` (detected at runtime)

**Implementation via Hooks**:
```rust
pub struct AdaptiveOptimizerHook {
    hot_loops: HashMap<usize, LoopProfile>,
    optimized_loops: HashMap<usize, OptimizedInstruction>,
    threshold: usize,  // Optimize after N iterations
}

impl ExecutionHook for AdaptiveOptimizerHook {
    fn on_loop_enter(&mut self, ctx: &HookContext, loop_info: &LoopInfo) -> HookDecision {
        let profile = self.hot_loops.entry(loop_info.loop_start()).or_default();
        profile.iterations += 1;

        // Optimize hot loops
        if profile.iterations == self.threshold {
            if let Some(optimized) = analyze_and_optimize(loop_info, ctx) {
                self.optimized_loops.insert(loop_info.loop_start(), optimized);
            }
        }

        // Execute optimized version if available
        if let Some(opt) = self.optimized_loops.get(&loop_info.loop_start()) {
            // Special handling: bypass loop, execute optimized instruction
            // Hooks still fire for the optimized instruction!
            return HookDecision::ReplaceLoop(opt.clone());
        }

        HookDecision::Continue
    }
}
```

**Why Hook-Aware**:
- Optimizations only applied after observation period
- Debugger sees original loop first (can inspect behavior)
- When optimized, hooks fire for optimized instruction
- Can disable by removing optimizer hook

#### Tier 3: AOT/JIT Compilation (Future, Hook-Incompatible)
**Purpose**: Maximum performance for production code

**Optimizations**:
- 🔮 **Offset Calculation**: Batch memory operations
- 🔮 **Constant Folding**: `+++--` → `Add(1)`
- 🔮 **Native Code Gen**: Compile to machine code

**Strategy**:
```rust
// Compilation mode - hooks disabled
let config = ExecutionConfigBuilder::new()
    .with_compilation_mode(true)  // Disables hooks
    .build();

// Compile to native code (no hook support)
let compiled = compile_to_native(&instructions)?;
compiled.execute()?;  // Direct execution, no hooks
```

**Trade-off**:
- 10-100x speedup
- No debugging/profiling
- For production only

---

### Hook-Aware Implementation Guidelines

When implementing optimizations, follow these rules:

#### Rule 1: Preserve Hook Events
```rust
// ❌ BAD: Hooks miss the optimized instruction
fn execute_optimized_loop(state: &mut VmState) {
    state.memory[state.pointer] = 0;  // Direct execution
}

// ✅ GOOD: Hooks see the optimized instruction
fn execute_optimized_loop(state: &mut VmState, hooks: &mut HookManager) {
    let instruction = Instruction::Set(0);
    hooks.before_instruction(&instruction, &context);
    state.memory[state.pointer] = 0;
    hooks.after_instruction(&instruction, &context);
}
```

#### Rule 2: Maintain Source Locations
```rust
// Optimized instructions must map back to original source
struct OptimizedInstruction {
    instruction: Instruction,       // Set(0)
    original_source: SourceLocation, // Points to "[-]" in source
    original_body: Vec<Instruction>, // [DecrementValue] for reference
}
```

#### Rule 3: Make Optimizations Observable
```rust
pub struct OptimizationReporter {
    transformations: Vec<Transformation>,
}

struct Transformation {
    location: SourceLocation,
    original: String,        // "[-]"
    optimized: Instruction,  // Set(0)
    reason: String,          // "Clear loop optimization"
}
```

#### Rule 4: Allow Opt-Out
```rust
// Tier 1 (RLE): Always on, but can use --no-rle flag
let config = ExecutionConfigBuilder::new()
    .with_rle_optimization(false)  // Disable RLE
    .build();

// Tier 2 (Adaptive): Optional hook, disable by not registering
// Tier 3 (AOT/JIT): Separate compilation mode
```

---

### Design Decision: Hook Compatibility with Optimizations

**Question**: Do we need separate hooks for original vs. optimized instructions?

**Decision**: ✅ **No - Use Enhanced HookContext** (single `ExecutionHook` trait for both)

#### Three Approaches Considered

**❌ Approach 1: Separate Hook Trait**
```rust
trait ExecutionHook { ... }
trait OptimizedExecutionHook { ... }
```
**Rejected because:**
- Hook authors must implement two traits
- More complexity, code duplication
- Unclear when to use which
- Violates DRY principle

**❌ Approach 2: Wrapped Instruction Enum**
```rust
pub enum Instruction {
    Add(u8),
    // ...
    Optimized {
        original: Vec<Instruction>,
        optimized: Box<Instruction>,
    }
}
```
**Rejected because:**
- Every hook must handle `Optimized` variant
- Memory overhead for ALL instructions
- Complicates pattern matching everywhere
- Breaks existing instruction matching logic

**✅ Approach 3: Enhanced HookContext (SELECTED)**
```rust
pub struct HookContext<'a> {
    // ... existing fields

    /// Optional optimization metadata (None for non-optimized instructions)
    optimization_info: Option<&'a OptimizationInfo>,
}

pub struct OptimizationInfo {
    /// What optimization was applied
    pub transformation: OptimizationType,
    /// Original source pattern (e.g., "[-]")
    pub original_pattern: String,
    /// Original instruction(s) before optimization
    pub original_instructions: Vec<Instruction>,
}

pub enum OptimizationType {
    ClearLoop,      // [-] → Set(0)
    ScanRight,      // [>] → ScanRight
    ScanLeft,       // [<] → ScanLeft
    CopyLoop,       // [->+<] → Copy(...)
    MultiplyLoop,   // [->++<] → Multiply(...)
}
```

**Why This Wins:**
- ✅ Simple hooks don't need to change (backward compatible)
- ✅ Advanced debuggers get full optimization metadata
- ✅ Hooks see what actually executes (accurate metrics)
- ✅ Source locations still map to original code
- ✅ Zero overhead when optimization_info is None
- ✅ Single trait to implement

#### How It Works

**For Simple Hooks** (don't care about optimizations):
```rust
struct StepCounter { count: u64 }

impl ExecutionHook for StepCounter {
    fn after_instruction(&mut self, _inst: &Instruction, _ctx: &HookContext) -> HookDecision {
        self.count += 1;  // Don't care if optimized
        HookDecision::Continue
    }
}
```

**For Advanced Debuggers** (need to show transformations):
```rust
struct AdvancedDebugger;

impl ExecutionHook for AdvancedDebugger {
    fn before_instruction(&mut self, inst: &Instruction, ctx: &HookContext) -> HookDecision {
        match ctx.optimization_info() {
            Some(opt) => {
                // Show transformation
                println!("Optimized: {} → {:?}", opt.original_pattern, inst);
                println!("  Original would be: {:?}", opt.original_instructions);
                println!("  Transformation: {:?}", opt.transformation);
            }
            None => {
                // Normal instruction
                println!("{:?}", inst);
            }
        }
        HookDecision::Continue
    }
}
```

**For Performance Profilers**:
```rust
struct Profiler {
    instruction_counts: HashMap<String, u64>,
}

impl ExecutionHook for Profiler {
    fn after_instruction(&mut self, inst: &Instruction, ctx: &HookContext) -> HookDecision {
        // Count what actually executed (optimized form = reality)
        *self.instruction_counts.entry(format!("{:?}", inst)).or_insert(0) += 1;

        // Optional: Also track optimization stats
        if let Some(opt) = ctx.optimization_info() {
            *self.optimization_stats.entry(opt.transformation).or_insert(0) += 1;
        }

        HookDecision::Continue
    }
}
```

#### Examples by Optimization Tier

**Tier 1: RLE (Parse-Time)**
```rust
// Source: +++
// Optimized to: Add(3)
// Hook sees: Add(3)
// optimization_info: None  (this IS the canonical form)
```

**Tier 2: Clear Loop (Runtime)**
```rust
// Source: [-]
// Detected at runtime, optimized to: Set(0)
// Hook sees: Set(0)
// optimization_info: Some(OptimizationInfo {
//     transformation: OptimizationType::ClearLoop,
//     original_pattern: "[-]",
//     original_instructions: vec![Instruction::Sub(1)],
// })
// source_location: Points to "[-]" in original source
```

**Tier 2: Scan Loop (Runtime)**
```rust
// Source: [>]
// Optimized to: ScanRight
// Hook sees: ScanRight
// optimization_info: Some(OptimizationInfo {
//     transformation: OptimizationType::ScanRight,
//     original_pattern: "[>]",
//     original_instructions: vec![Instruction::Right(1)],
// })
```

#### Implementation Checklist

Phase 1.5 (After RLE, before Tier 2 optimizations):

1. **Define OptimizationInfo type** in `src/hooks/mod.rs`:
   - `OptimizationType` enum with variants for each optimization
   - `OptimizationInfo` struct with transformation, pattern, original instructions

2. **Update HookContext**:
   - Add `optimization_info: Option<&'a OptimizationInfo>` field
   - Add `pub fn optimization_info(&self) -> Option<&OptimizationInfo>` getter
   - Update constructor to accept optional `optimization_info` parameter
   - Update all existing call sites to pass `None` (backward compatible)

3. **Document in hook examples**:
   - Add example showing optimization-aware hook
   - Add example showing optimization-ignorant hook
   - Document that most hooks can ignore this field

4. **Use in Tier 2 optimizations**:
   - When clear loop detected, create `OptimizationInfo`
   - Pass to `HookContext::new()` when dispatching
   - Verify debugger can inspect both forms

#### Benefits Summary

| Concern | How Enhanced Context Solves It |
|---------|-------------------------------|
| **Backward Compatibility** | Existing hooks don't change - `optimization_info` is optional |
| **Debugger Needs** | Full access to original pattern and transformation type |
| **Performance Metrics** | Hooks see actual execution (1 step for Set(0), not ~256) |
| **Source Location** | Existing `source_location` field still maps to original code |
| **Memory Overhead** | OptimizationInfo only created when optimization applied |
| **Complexity** | Simple hooks ignore field, advanced hooks opt-in |

---

### Design Decision: Debug Symbols with Optimized Code

**Question**: How do debug symbols (DebugInfo) work when instructions are optimized?

**Answer**: DebugInfo remains **unchanged and immutable** - it always maps to the original instruction stream. Optimizations maintain `instruction_index` correctness to enable accurate source location tracking.

#### Current DebugInfo Architecture

From `src/debug.rs`:
```rust
pub struct DebugInfo {
    /// Original source code
    source: String,
    /// Map from instruction index (flat execution order) to source location
    locations: HashMap<usize, SourceLocation>,
    /// Loop metadata for tracking loop boundaries
    loop_metadata: HashMap<usize, LoopMetadata>,
}
```

**Key Insight**: The `instruction_index` is the position in the **original flattened instruction stream** (depth-first traversal). This index is used for:
1. Looking up source locations: `debug_info.lookup(instruction_index)`
2. HookContext field: `context.instruction_index()`
3. Loop metadata lookups: `debug_info.get_loop_metadata(loop_start_index)`

#### How Each Optimization Tier Handles Debug Info

**Tier 1: RLE (Parse-Time Optimization)**

Source mapping "just works" because RLE happens during parsing:

```rust
// Source: +++
// Positions: 0, 1, 2

// During parsing:
let start_loc = current_location();  // Points to first '+'
let count = count_consecutive(source, pos, '+');
instructions.push(Instruction::Add(count));

// DebugInfo records:
debug_info.record(instruction_index, start_loc);  // Spans chars 0-2
instruction_index += 1;  // One instruction, one index

// At runtime:
// - instruction_index = 0
// - debug_info.lookup(0) → SourceLocation(line=1, col=1, offset=0)
// - Points to first '+' in "+++"
// - Error messages highlight all three '+' characters (source context)
```

**Result**: ✅ Source locations work perfectly - the single `Add(3)` instruction maps to the span of all three `+` characters.

**Tier 2: Clear Loop (Runtime Optimization)**

This is more complex because optimization happens at runtime:

```rust
// Source: +++[-]>>>
// Positions: 0,1,2,3,4,5,6,7

// After parsing (with debug info):
// instruction_index=0: Add(3)     → source location for '+++' (offset 0)
// instruction_index=1: Loop start → source location for '[' (offset 3)
// instruction_index=2: Sub(1)     → source location for '-' (offset 4)
// (loop body ends, back to index 1 or exit at offset 5)
// instruction_index=3: Right(3)   → source location for '>>>' (offset 6)

// At runtime, when executing Loop at instruction_index=1:
// Optimizer detects: "this is a clear loop"
// Replaces execution with: Set(0)

// HookContext created:
let ctx = HookContext::new(
    memory, pointer, step_count,
    debug_info.lookup(1),              // Source location: '[' at offset 3
    loop_depth,
    1,                                  // instruction_index = 1 (the loop start)
    Some(&opt_info),                   // Optimization metadata
);

// Hooks see:
// - instruction: Set(0)
// - context.instruction_index(): 1
// - context.source_location(): Points to '[' character
// - context.optimization_info(): Some(OptimizationInfo {
//     transformation: ClearLoop,
//     original_pattern: "[-]",
//     original_instructions: [Loop([Sub(1)])],
// })
```

**Result**: ✅ Source locations point to the loop start `[`, hooks can inspect both optimized and original forms.

#### Key Design Principles

**Principle 1: instruction_index Always Refers to Original Stream**

```rust
// WRONG: Create new instruction indices for optimized code
let optimized_index = generate_new_index();  // ❌

// RIGHT: Use the original loop's instruction_index
let original_index = loop_metadata.loop_start_index;  // ✅
```

**Why**: DebugInfo is immutable and based on original instruction stream. Changing indices would break source location lookups.

**Principle 2: DebugInfo Never Modified After Parsing**

```rust
// WRONG: Update DebugInfo when optimizing
debug_info.record(new_index, new_location);  // ❌

// RIGHT: Keep DebugInfo unchanged, pass original index to hooks
hooks.before_instruction(&optimized_inst, &context_with_original_index);  // ✅
```

**Why**: DebugInfo is shared read-only state. Optimizations are runtime decisions that don't change the source mapping.

**Principle 3: OptimizationInfo Provides Transformation Context**

```rust
// Hooks can reconstruct what happened:
if let Some(opt) = context.optimization_info() {
    // Original: Loop([Sub(1)]) at source "[−]"
    // Optimized to: Set(0)
    // Source location: Points to '[' character

    let source_span = opt.original_pattern;  // "[-]"
    let original_insts = opt.original_instructions;  // [Loop([Sub(1)])]
}
```

#### Debugger Stepping Behavior

**Without Optimization**:
```
Source: +++[-]>>>
Step 1: Execute Add(3)      at instruction_index=0
Step 2: Enter Loop          at instruction_index=1
Step 3: Execute Sub(1)      at instruction_index=2 (in loop)
Step 4: Check loop condition at instruction_index=1
  ... (loop iterates ~256 times)
Step N: Exit loop
Step N+1: Execute Right(3)  at instruction_index=3
```

**With Optimization**:
```
Source: +++[-]>>>
Step 1: Execute Add(3)      at instruction_index=0
Step 2: Execute Set(0)      at instruction_index=1 (optimized!)
        Optimization: ClearLoop, original was "[-]"
Step 3: Execute Right(3)    at instruction_index=3
```

**Debugger UI Should Show**:
```
> Step 2: instruction_index=1
  Source: [-]
           ^
  Executing: Set(0)
  ℹ Optimized from: Loop([Sub(1)])
  ℹ Transformation: Clear Loop Optimization
  ℹ Original would take ~256 iterations
```

#### Error Reporting Example

```rust
// Runtime error in optimized code:
// Source: +++[-]>>>>>...   (pointer moves beyond memory)
//                  ^^^^^
// After optimization executes Set(0), pointer movement causes error

Err(BfError::MemoryOutOfBounds {
    instruction_index: 3,  // Right(N) instruction
    source_location: debug_info.lookup(3).unwrap(),  // Points to ">>>>>"
    ..
})

// Error message shows:
// Error at line 1, column 8:
//   +++[-]>>>>>...
//         ^^^^^
// Memory out of bounds at address 30000
```

**Notice**: Even though `[-]` was optimized, the error correctly points to the actual problematic instruction (`>>>>>`), not the optimized one. This is because we maintain correct `instruction_index` values.

#### Testing Requirements

All optimizations must verify:

```rust
#[test]
fn test_optimization_preserves_debug_info() {
    let source = "+++[-]>>>";
    let (instructions, debug_info) = parse_with_debug(source).unwrap();

    // Execute with optimization
    let config = with_tier2_optimizations();
    let result = interpret_with_config(&instructions, config, Some(&debug_info));

    // Verify: instruction_index values are correct
    // Verify: Source locations point to original source
    // Verify: Error locations are accurate
}

#[test]
fn test_optimized_hook_has_correct_source_location() {
    let source = "[-]";
    let (instructions, debug_info) = parse_with_debug(source).unwrap();

    let mut locations_seen = Vec::new();
    let tracker = LocationTrackerHook::new(&mut locations_seen);

    let config = with_tier2_optimizations()
        .with_hook(Box::new(tracker))
        .build();

    interpret_with_config(&instructions, config, Some(&debug_info)).unwrap();

    // Verify: Hook saw instruction_index=0 (loop start)
    // Verify: Source location points to '[' character
    assert_eq!(locations_seen[0].instruction_index, 0);
    assert_eq!(locations_seen[0].source_location.offset, 0);  // Points to '['
}
```

#### Summary

| Aspect | How It Works |
|--------|--------------|
| **DebugInfo** | Immutable, always maps original instruction indices to source |
| **instruction_index** | Always refers to position in original instruction stream |
| **Source Locations** | Point to original source characters (e.g., `[` for clear loop) |
| **OptimizationInfo** | Carries transformation metadata (original pattern, instructions) |
| **Hooks** | See optimized instruction + original instruction_index + metadata |
| **Debugger** | Shows both optimized execution and original source context |
| **Errors** | Correctly point to problematic source location |
| **Step Count** | Increments once per optimized instruction (not original iterations) |

**Key Benefit**: This design allows debuggers to work with optimized code while maintaining full traceability back to source. Users see the performance benefits of optimization without losing debugging capability.

---

### Testing Strategy: Optimization Correctness

All optimizations must pass **hook-aware tests**:

```rust
#[test]
fn test_clear_loop_hooks_fire() {
    let source = "+++[-]";
    let (instructions, debug_info) = parse_with_debug(source).unwrap();

    let mut hook_events = Vec::new();
    let tracker = EventTrackerHook::new(&mut hook_events);

    let config = ExecutionConfigBuilder::new()
        .with_memory_size(100)
        .with_hook(Box::new(tracker))
        .with_adaptive_optimization(true)  // Enable optimizations
        .build();

    interpret_with_config(&instructions, config, Some(&debug_info)).unwrap();

    // Verify hooks saw both original and optimized instructions
    assert!(hook_events.contains(&HookEvent::BeforeInstruction(Instruction::Add(3))));
    assert!(hook_events.contains(&HookEvent::BeforeInstruction(Instruction::Set(0))));
}

#[test]
fn test_optimization_preserves_source_locations() {
    let source = "+++[-]";
    let (instructions, debug_info) = parse_with_debug(source).unwrap();

    // After optimization, source locations must still be correct
    let config = with_optimizations();
    let result = interpret_with_config(&instructions, config, Some(&debug_info));

    // If error occurs, source location should point to original source
    if let Err(error) = result {
        let loc = error.source_location().unwrap();
        assert_eq!(debug_info.lookup(loc.offset).unwrap().line, 1);
    }
}
```

---

### Updated Implementation Phases

#### Phase 0: Foundation (COMPLETE ✅)
- ✅ Hook system implementation
- ✅ DebugInfo tracking
- ✅ Property-based tests
- ✅ Benchmark infrastructure

#### Phase 1: Tier 1 Optimizations (4 weeks)
**Week 1-2: Run-Length Encoding**
- Update parser to emit `Add(n)`, `Sub(n)`, `Right(n)`, `Left(n)`
- Update interpreter to execute counted instructions
- Verify hooks fire correctly
- Benchmark performance gain

**Week 3-4: Constant Folding (Parser-level)**
- `+++--` → `Add(1)`
- `>><` → `Right(1)`
- Test with hooks enabled

**Success Criteria**:
- ✅ 3-5x speedup on typical programs
- ✅ All hooks fire correctly
- ✅ Source locations preserved
- ✅ Zero test failures

#### Phase 1.5: OptimizationInfo Infrastructure (1 week)
**Purpose**: Prepare HookContext for Tier 2 optimizations

**Tasks**:
- Add `OptimizationType` enum to `src/hooks/mod.rs`
- Add `OptimizationInfo` struct to `src/hooks/mod.rs`
- Update `HookContext` with `optimization_info: Option<&'a OptimizationInfo>` field
- Add `optimization_info()` getter method
- Update `HookContext::new()` to accept optional optimization_info parameter
- Update all existing call sites to pass `None` (backward compatible)
- Add hook examples demonstrating optimization-aware and optimization-ignorant hooks
- Add tests verifying backward compatibility

**Success Criteria**:
- ✅ All existing hooks still work (no breaking changes)
- ✅ New optimization_info field accessible but optional
- ✅ Documentation shows both simple and advanced hook examples
- ✅ Debug symbols work correctly (instruction_index mapping preserved)
- ✅ Zero test failures

**Note**: See "Design Decision: Hook Compatibility with Optimizations" and "Design Decision: Debug Symbols with Optimized Code" sections above for detailed design rationale.

#### Phase 2: Tier 2 Optimizations (6 weeks)
**Week 1-2: Clear Loop Optimization**
- Implement `AdaptiveOptimizerHook`
- Detect `[-]` pattern at runtime
- Replace with `Set(0)` instruction
- Create `OptimizationInfo` with `ClearLoop` transformation type
- **CRITICAL**: Maintain correct `instruction_index` (use loop start index from DebugInfo)
- Fire hooks for `Set(0)` with optimization metadata at original instruction_index
- Verify debuggers can inspect both original `[-]` and optimized `Set(0)`
- Test source locations point to `[` character correctly

**Week 3-4: Scan Loop Optimization**
- Detect `[>]` and `[<]` patterns
- Implement fast memory scan
- Create `OptimizationInfo` with `ScanRight`/`ScanLeft` transformation
- Maintain hook compatibility with optimization metadata
- Benchmark performance gain (should be near-instant)

**Week 5-6: Copy/Multiply Loops**
- Analyze loop bodies at runtime
- Detect copy/multiply patterns (`[->+<]`, `[->++<]`, etc.)
- Generate optimized `Copy`/`Multiply` instructions
- Create appropriate `OptimizationInfo` metadata
- Test with debuggers to ensure original loop body visible

**Success Criteria**:
- ✅ 8-12x total speedup
- ✅ Adaptive: only hot loops optimized
- ✅ Debugger can inspect original loops via `optimization_info()`
- ✅ Optimization reports show transformations
- ✅ Hooks fire with full optimization metadata

#### Phase 3: Developer Tools (Leverages Hooks) (4 weeks)
**Week 1: Enhanced Profiling**
- Extend StatsTrackerHook with instruction-level counts
- Track hot loops via loop_enter/exit hooks
- Memory access patterns via hook context

**Week 2: Step-Through Debugger**
- Implement DebuggerHook with step mode
- Breakpoints via HookDecision::Break
- Watchpoints via after_instruction

**Week 3: REPL Mode**
- Interactive interpreter with hook support
- Live memory visualization
- Optimization feedback

**Week 4: Optimization Reporter**
- Track optimization decisions
- Show applied transformations
- Performance estimates

**Success Criteria**:
- ✅ Profiler identifies bottlenecks accurately
- ✅ Debugger works with optimized code
- ✅ REPL provides instant feedback

---
