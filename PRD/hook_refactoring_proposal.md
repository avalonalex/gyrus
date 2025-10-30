# Hook Refactoring Proposal: Simplifying interpreter.rs

**Status**: Proposal
**Date**: 2025-10-30
**Context**: After implementing the hook system, we can refactor existing interpreter features to use hooks instead of being built-in

## Motivation

The core interpreter (`interpreter.rs`) currently has several features directly embedded in the execution loop:

1. **Statistics tracking** - Counting steps, loops, memory usage, I/O operations
2. **Limit checking** - Enforcing step limits and timeouts
3. **Runtime warnings** - Tracking memory expansion (currently minimal)

**Problems with current approach**:
- Non-essential logic mixed with core execution
- Performance overhead even when features not needed
- Hard to extend or customize
- Testing is more complex

**Benefits of using hooks**:
- ✅ **Zero-cost abstraction**: No overhead when hooks disabled
- ✅ **Cleaner core**: Hot path contains only essential execution logic
- ✅ **User extensibility**: Users can implement custom statistics/limits
- ✅ **Better testing**: Test features in isolation
- ✅ **Composable**: Mix and match features as needed

## Current State Analysis

### Features Built Into Interpreter

#### 1. Statistics Tracking (ExecutionStats)

**Location**: Lines 256-258, 364-365, 414, 421, 557

```rust
// Scattered throughout interpreter.rs:
state.stats.total_steps = state.step_count;
state.stats.peak_memory_used = MemoryAddress::new(state.pointer.get() + 1);
state.stats.bytes_written += 1;
state.stats.bytes_read += 1;
state.stats.loop_iterations += 1;
```

**What's tracked**:
- `total_steps` - Total instructions executed
- `loop_iterations` - Times a loop body was entered
- `peak_memory_used` - Highest memory cell accessed
- `cells_modified` - Non-zero cells at end
- `bytes_read` - Input operations
- `bytes_written` - Output operations
- `memory_allocated` - Actual memory size (for unbounded)

#### 2. Limit Checking

**Location**: Lines 477-512

```rust
// In execute_block() loop:
if let Some(max_steps) = config.max_steps()
    && state.step_count.get() > max_steps
{
    return Err(BfError::StepLimitExceeded { ... });
}

if let Some(start) = &state.start_time
    && let Some(timeout_ms) = config.timeout_ms()
{
    let elapsed = start.elapsed().as_millis() as u64;
    if elapsed > timeout_ms {
        return Err(BfError::ExecutionTimeout { ... });
    }
}
```

**What's checked**:
- Step limit (max_steps)
- Timeout (timeout_ms)

#### 3. Runtime Warnings

**Location**: Lines 311, 326, 387, 396

```rust
// Currently minimal - only memory expansion in unbounded mode
state.stats.warnings.push(RuntimeWarning::MemoryExpanded { ... });
```

### Current Execution Loop Structure

```rust
fn execute_block(
    config: &mut ExecutionConfig,
    instructions: &[Instruction],
    state: &mut VmState,
    input: &mut dyn BfInput,
    output: &mut dyn BfOutput,
    start_index: usize,
) -> Result<(), BfError> {
    for instruction in instructions {
        state.step_count.increment();

        // ❌ BUILT-IN: Limit checking
        if let Some(max_steps) = config.max_steps() { ... }
        if let Some(timeout_ms) = config.timeout_ms() { ... }

        // ✅ HOOK: before_instruction
        if let Some(hook_manager) = config.hook_manager_mut() {
            hook_manager.before_instruction(...);
        }

        // Core execution
        match instruction {
            Instruction::Loop(body) => { ... }
            _ => execute_single_instruction(...)?
        }

        // ✅ HOOK: after_instruction
        if let Some(hook_manager) = config.hook_manager_mut() {
            hook_manager.after_instruction(...);
        }

        // ❌ BUILT-IN: Stats tracking happens inside execution
    }
    Ok(())
}
```

## Proposed Refactoring

### Phase 1: Extract Statistics to Hook

**Goal**: Move all statistics tracking from interpreter to a `StatsTrackerHook`

#### Create Built-in Hook: `StatsTrackerHook`

```rust
// In crates/ferrous-cortex/src/hooks/builtin.rs (NEW FILE)

/// Built-in hook for tracking execution statistics
pub struct StatsTrackerHook {
    stats: ExecutionStats,
}

impl ExecutionHook for StatsTrackerHook {
    fn after_instruction(&mut self, instruction: &Instruction, context: &HookContext) -> HookDecision {
        // Track step count (already in context)
        // Track peak memory
        if context.pointer().get() + 1 > self.stats.peak_memory_used.get() {
            self.stats.peak_memory_used = MemoryAddress::new(context.pointer().get() + 1);
        }

        // Track I/O
        match instruction {
            Instruction::Output => self.stats.bytes_written += 1,
            Instruction::Input => self.stats.bytes_read += 1,
            _ => {}
        }

        HookDecision::Continue
    }

    fn on_loop_enter(&mut self, _context: &HookContext) -> HookDecision {
        self.stats.loop_iterations += 1;
        HookDecision::Continue
    }

    fn on_complete(&mut self, context: &HookContext) {
        self.stats.total_steps = context.step_count();
        self.stats.memory_allocated = MemorySize::new(context.memory().len());
        self.stats.cells_modified = ExecutionStats::count_modified_cells(context.memory());
    }
}
```

#### Changes to Interpreter

**Remove**:
- All `state.stats.*` assignments from execution loop
- Stats tracking from `execute_single_instruction()`

**Keep**:
- `state.step_count` (needed for hook context)
- `state.loop_depth` (needed for hook context)

#### API Changes

**Before** (current):
```rust
let stats = interpret_with_config(&instructions, config, None)?;
println!("Steps: {}", stats.total_steps);
```

**After** (with hook):
```rust
// Option 1: Enable by default (backward compatible)
let stats = interpret_with_config(&instructions, config, None)?;
println!("Steps: {}", stats.total_steps);  // Still works!

// Option 2: Explicit control
let config = ExecutionConfigBuilder::new()
    .with_stats_tracking(true)   // Enable stats (default)
    .build();

// Option 3: Maximum performance (no stats)
let config = ExecutionConfigBuilder::new()
    .with_stats_tracking(false)  // Disable all stats tracking
    .build();
```

**Implementation Detail**: `interpret_with_config()` automatically adds `StatsTrackerHook` unless explicitly disabled.

### Phase 2: Extract Limit Checking to Hook

**Goal**: Move step/timeout checking to a `LimitCheckerHook`

#### Create Built-in Hook: `LimitCheckerHook`

```rust
/// Built-in hook for enforcing execution limits
pub struct LimitCheckerHook {
    max_steps: Option<u64>,
    timeout_ms: Option<u64>,
    start_time: Option<Instant>,
}

impl ExecutionHook for LimitCheckerHook {
    fn before_instruction(&mut self, _: &Instruction, context: &HookContext) -> HookDecision {
        // Check step limit
        if let Some(max_steps) = self.max_steps {
            if context.step_count().get() > max_steps {
                // Note: Can't return BfError directly from hook
                // Solution: Return Break and let interpreter check why
                return HookDecision::Break;
            }
        }

        // Check timeout
        if let Some(start) = &self.start_time {
            if let Some(timeout_ms) = self.timeout_ms {
                let elapsed = start.elapsed().as_millis() as u64;
                if elapsed > timeout_ms {
                    return HookDecision::Break;
                }
            }
        }

        HookDecision::Continue
    }
}
```

**Challenge**: Hooks return `HookDecision`, not `BfError`.

**Solution**:
1. Hook returns `Break` when limit exceeded
2. Interpreter checks which limit was hit and returns appropriate error
3. OR: Extend `HookDecision` enum with error details

```rust
pub enum HookDecision {
    Continue,
    Break,
    Skip,
    Error(BfError),  // NEW: Allow hooks to return errors directly
}
```

#### Changes to Interpreter

**Remove**:
- Lines 477-512 (step limit and timeout checks)
- `state.start_time` field
- Timeout tracking from VmState

**Keep**:
- Error handling (but moved to hook response)

### Phase 3: Extract Runtime Warnings to Hook

**Goal**: Centralize warning collection in a `WarningCollectorHook`

Currently warnings are minimal (only memory expansion). This hook would:
- Collect memory expansion warnings
- Could be extended for other warnings in the future
- Deduplicate warnings by instruction index

```rust
/// Built-in hook for collecting runtime warnings
pub struct WarningCollectorHook {
    warnings: Vec<RuntimeWarning>,
    seen_warnings: HashSet<usize>,  // instruction_index -> deduplicate
}

impl ExecutionHook for WarningCollectorHook {
    fn after_instruction(&mut self, instruction: &Instruction, context: &HookContext) -> HookDecision {
        // Example: Detect memory expansion
        // (This would need memory model info)

        // Deduplicate by instruction index
        if let Some(loc) = context.source_location() {
            if !self.seen_warnings.contains(&loc.offset) {
                // Add warning logic here
            }
        }

        HookDecision::Continue
    }
}
```

## Implementation Plan

### Step 1: Create Built-in Hooks Module ✅ Ready

```
crates/ferrous-cortex/src/
├── hooks.rs                    (existing - core hooks system)
└── hooks/
    ├── mod.rs                  (NEW - public API)
    └── builtin.rs              (NEW - built-in hooks)
```

**Content**:
```rust
// hooks/builtin.rs
pub struct StatsTrackerHook { ... }
pub struct LimitCheckerHook { ... }
pub struct WarningCollectorHook { ... }

// hooks/mod.rs
pub mod builtin;
pub use builtin::*;
```

### Step 2: Implement StatsTrackerHook 📝 Design Complete

1. Create `hooks/builtin.rs`
2. Implement `StatsTrackerHook`
3. Add tests in `hooks/builtin.rs`
4. Update `ExecutionConfigBuilder` with `.with_stats_tracking()`

### Step 3: Refactor Interpreter (Stats) 🚧 Pending

1. Remove stats tracking from `execute_block()`
2. Remove stats tracking from `execute_single_instruction()`
3. Auto-register `StatsTrackerHook` in `interpret_with_config()`
4. Ensure backward compatibility

### Step 4: Implement LimitCheckerHook 🚧 Pending

1. Extend `HookDecision` with `Error(BfError)` variant (?)
2. Implement `LimitCheckerHook`
3. Add tests
4. Update config builder

### Step 5: Refactor Interpreter (Limits) 🚧 Pending

1. Remove limit checking from `execute_block()`
2. Remove `start_time` from VmState
3. Auto-register `LimitCheckerHook` in `interpret_with_config()`
4. Handle error propagation from hook

### Step 6: Implement WarningCollectorHook 🚧 Pending

1. Design warning detection logic
2. Implement hook
3. Add tests
4. Auto-register if warnings enabled

### Step 7: Integration Testing 🚧 Pending

1. Test backward compatibility
2. Test performance with/without hooks
3. Benchmark hot path
4. Update documentation

## Design Decisions

### Decision 1: Default Behavior (Backward Compatibility)

**Question**: Should stats/limits be enabled by default?

**Recommendation**: YES - Auto-register built-in hooks for backward compatibility

```rust
pub fn interpret_with_config(
    instructions: &[Instruction],
    mut config: ExecutionConfig,
    debug_info: Option<&DebugInfo>,
) -> Result<ExecutionStats, BfError> {
    // Auto-register built-in hooks unless explicitly disabled
    if config.stats_tracking_enabled() {
        config.register_hook(Box::new(StatsTrackerHook::new()));
    }
    if config.limits_enabled() {
        config.register_hook(Box::new(LimitCheckerHook::from_config(&config)));
    }

    // ... execute
}
```

**Pro**: Existing code works unchanged
**Con**: Small overhead even if user doesn't want stats

**Alternative**: Require explicit opt-in (breaking change)

### Decision 2: Hook Error Handling

**Question**: How should hooks report errors (like limit exceeded)?

**Option A**: Extend `HookDecision` enum
```rust
pub enum HookDecision {
    Continue,
    Break,
    Skip,
    Error(BfError),  // Hook can return error directly
}
```

**Option B**: Use Break + check afterward
```rust
// Hook returns Break
HookDecision::Break

// Interpreter figures out why
if step_count > max_steps {
    return Err(StepLimitExceeded);
}
```

**Recommendation**: Option A - Cleaner and more flexible

### Decision 3: VmState Simplification

**Question**: What can we remove from VmState?

**Can Remove**:
- `stats: ExecutionStats` (moved to hook)
- `start_time: Option<Instant>` (moved to hook)

**Must Keep**:
- `step_count: StepCount` (needed for hook context)
- `loop_depth: usize` (needed for hook context)
- `instruction_index: usize` (needed for source location lookup)

**After refactoring**, VmState becomes:
```rust
struct VmState<'a> {
    // Core execution state
    memory: Vec<u8>,
    pointer: MemoryAddress,
    step_count: StepCount,
    loop_depth: usize,
    instruction_index: usize,

    // Configuration
    memory_model: MemoryModel,
    debug_info: Option<&'a DebugInfo>,
    loop_stack: Vec<LoopContext>,

    // stats and start_time REMOVED
}
```

## Performance Analysis

### Before Refactoring

**Hot path** (per instruction):
1. Increment step count
2. ❌ Check max_steps (conditional)
3. ❌ Check timeout (conditional + time check)
4. Hook: before_instruction (if enabled)
5. Execute instruction
6. ❌ Update stats (multiple assignments)
7. Hook: after_instruction (if enabled)

**Overhead**: ~5-8 operations per instruction regardless of need

### After Refactoring

**Hot path** (per instruction):
1. Increment step count
2. Hook: before_instruction (if enabled) - includes limits
3. Execute instruction
4. Hook: after_instruction (if enabled) - includes stats
5. Hook: on_loop_enter/exit (if loop)

**With all hooks enabled**: Same overhead as before
**With no hooks**: **Only 3 operations** (increment, execute, done)

**Performance gain**: When hooks disabled, ~40-60% reduction in per-instruction overhead!

### Benchmark Expectations

| Configuration | Current | After Refactoring | Speedup |
|---------------|---------|-------------------|---------|
| All features enabled | 100% | 100% | 0% |
| No stats, with limits | 100% | ~80% | +25% |
| No stats, no limits | 100% | ~40% | +150% |
| Maximum performance | 100% | ~40% | +150% |

## Migration Guide

### For Library Users

**No changes required** - Existing code continues to work:

```rust
// This code works exactly the same before and after refactoring
let instructions = parse(source)?;
let config = ExecutionConfigBuilder::new()
    .with_memory_size(30000)
    .with_max_steps(1_000_000)
    .build();
let stats = interpret_with_config(&instructions, config, None)?;
println!("Steps: {}", stats.total_steps);
```

**New capabilities** after refactoring:

```rust
// Disable stats for maximum performance
let config = ExecutionConfigBuilder::new()
    .with_stats_tracking(false)
    .build();

// Custom stats hook
struct MyCustomStats { ... }
impl ExecutionHook for MyCustomStats { ... }

let config = ExecutionConfigBuilder::new()
    .with_stats_tracking(false)  // Disable default
    .with_hook(Box::new(MyCustomStats::new()))  // Add custom
    .build();
```

### For Contributors

**Testing strategy**:
1. All existing tests should pass unchanged
2. Add new tests for hook-based implementation
3. Add performance benchmarks

## Open Questions

1. **Should we expose built-in hooks publicly?**
   - Pro: Users can customize (e.g., different limit errors)
   - Con: More API surface area

2. **Should `HookDecision::Error` be added?**
   - Pro: Cleaner error propagation
   - Con: Breaking change to hook API

3. **Should stats be opt-out or opt-in?**
   - Current proposal: Opt-out (enabled by default)
   - Alternative: Opt-in (user must explicitly enable)

4. **Performance target?**
   - What's acceptable overhead when all hooks enabled?
   - Should we optimize for "all enabled" or "all disabled"?

## Success Criteria

✅ **Backward Compatibility**: All existing code works unchanged
✅ **Performance**: No regression when hooks enabled, significant gain when disabled
✅ **Code Clarity**: interpreter.rs is simpler and easier to understand
✅ **Test Coverage**: All refactored features have equivalent or better test coverage
✅ **Documentation**: Clear migration guide and performance characteristics

## Timeline Estimate

- **Phase 1** (Built-in hooks module): 1-2 hours
- **Phase 2** (StatsTrackerHook): 2-3 hours
- **Phase 3** (Interpreter refactor - stats): 3-4 hours
- **Phase 4** (LimitCheckerHook): 2-3 hours
- **Phase 5** (Interpreter refactor - limits): 2-3 hours
- **Phase 6** (WarningCollectorHook): 1-2 hours
- **Phase 7** (Integration & testing): 3-4 hours

**Total**: 14-21 hours of development time

## Conclusion

This refactoring will:
1. **Simplify** the core interpreter
2. **Improve** performance when features not needed
3. **Enable** user customization
4. **Maintain** backward compatibility

**Recommendation**: Proceed with refactoring in phases, starting with StatsTrackerHook as a proof of concept.
