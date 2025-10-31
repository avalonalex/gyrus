# Can step_count and loop_depth be moved to hooks entirely?

**Date**: 2025-10-30
**Question**: Can we make VmState truly minimal by moving step_count and loop_depth to hooks?

---

## Current State

### VmState Fields
```rust
struct VmState {
    memory: Vec<u8>,           // CORE - can't move to hooks
    pointer: MemoryAddress,    // CORE - can't move to hooks
    memory_model: MemoryModel, // CORE - can't move to hooks

    step_count: StepCount,     // TRACKING - could this be in hooks?
    loop_depth: usize,         // TRACKING - could this be in hooks?
}
```

**Cost of tracking fields**:
- Memory: 16 bytes (8 bytes each)
- CPU: 1 add per instruction (step_count) + 1 inc/dec per loop (loop_depth)

---

## Analysis: step_count

### Where it's used

#### 1. Creating HookContext (every hook dispatch)
```rust
// hooks/mod.rs:295-311
HookContext::new(
    memory: &state.memory,
    pointer: state.pointer,
    step_count: state.step_count,  // <-- Hooks need this!
    source_location: None,
    loop_depth: state.loop_depth,
    instruction_index,
)
```

#### 2. Limit Checking (LimitEnforcerHook)
```rust
// hooks/builtin.rs:478
if context.step_count().get() > max_steps {
    // Break execution
}
```

#### 3. Error Messages
```rust
// interpreter.rs:746
BfError::ExecutionPaused {
    instruction_index: state.step_count.into(),
    // ...
}
```

#### 4. Statistics (StatsTrackerHook)
```rust
// hooks/builtin.rs:283
self.stats.total_steps = context.step_count();
```

### The Circular Dependency Problem

```
┌─────────────────────────────────────────────┐
│ Hooks need HookContext to make decisions   │
│           ↓                                 │
│ HookContext needs step_count               │
│           ↓                                 │
│ To avoid tracking, move to hooks           │
│           ↓                                 │
│ But hooks need it in HookContext!          │
└─────────────────────────────────────────────┘
                   ↑
                   └─── CIRCULAR! ─────────────┘
```

**Fundamental issue**: Hooks need step_count to make decisions, so someone must track it.

---

## Potential Solutions

### Solution 1: Remove from HookContext ❌

**Idea**: Don't expose step_count in HookContext, let hooks track it themselves

```rust
pub struct HookContext<'a> {
    memory: &'a [u8],
    pointer: MemoryAddress,
    // step_count removed!
    // loop_depth removed!
}

// Each hook tracks its own step count
impl LimitEnforcerHook {
    fn before_instruction(&mut self, ...) {
        self.step_count += 1; // Each hook duplicates this
        if self.step_count > max { ... }
    }
}
```

**Problems**:
- ❌ Duplicates logic across every hook that needs step_count
- ❌ Hooks can get out of sync (error-prone)
- ❌ No shared source of truth
- ❌ Still need to increment somewhere (interpreter must tell hooks)

**Verdict**: Not viable

---

### Solution 2: Lazy HookContext Creation ❌

**Idea**: Only create HookContext fields that hooks actually need

```rust
pub struct HookContext<'a> {
    memory: &'a [u8],
    pointer: MemoryAddress,
    step_count: Option<StepCount>,  // Only Some if hooks need it
    loop_depth: Option<usize>,
}
```

**Problems**:
- ❌ How do we know what hooks need without calling them?
- ❌ Still need to track step_count to provide it when needed
- ❌ Adds complexity without eliminating work

**Verdict**: Doesn't actually save work

---

### Solution 3: Type-Level Dispatch ❌

**Idea**: Different VmState types based on what's enabled

```rust
struct VmState<const TRACK_STEPS: bool, const TRACK_DEPTH: bool> {
    memory: Vec<u8>,
    pointer: MemoryAddress,
    // step_count only present if TRACK_STEPS == true
    // loop_depth only present if TRACK_DEPTH == true
}
```

**Problems**:
- ❌ Massive code duplication
- ❌ Every function becomes generic over tracking modes
- ❌ Can't mix modes in same binary
- ❌ Extreme complexity for minimal benefit

**Verdict**: Not worth the complexity

---

### Solution 4: Hook-Driven Tracking ❌

**Idea**: Hooks tell interpreter what to track via callbacks

```rust
trait ExecutionHook {
    fn track_steps(&self) -> bool { false }
    fn track_depth(&self) -> bool { false }
}

// Interpreter checks on every instruction:
if hooks.any(|h| h.track_steps()) {
    step_count += 1;
}
```

**Problems**:
- ❌ Still doing the work (checking and incrementing)
- ❌ Adds overhead of checking "should I track this?"
- ❌ Doesn't eliminate tracking, just makes it conditional
- ❌ More complex, same cost

**Verdict**: No actual savings

---

### Solution 5: Accept Minimal Cost ✅

**Idea**: Keep step_count and loop_depth in VmState as core tracking

**Analysis**:
- ✅ step_count is needed for limit checking (common use case)
- ✅ step_count is needed for error messages (always useful)
- ✅ loop_depth is only incremented at loop boundaries (very cheap)
- ✅ Total cost: 16 bytes + ~1 CPU cycle per instruction
- ✅ Modern CPUs: This is essentially free

**Cost breakdown**:
```
Memory: 16 bytes (negligible in 30KB+ memory tape context)
CPU per instruction: 1 add (step_count) = ~0.3ns on modern CPU
CPU per loop: 1 inc + 1 dec (loop_depth) = ~0.6ns total

For a 1 million instruction program:
- Time cost: ~0.3 milliseconds
- Memory cost: 16 bytes

This is 0.03% overhead for a typical program.
```

**Verdict**: This is the right tradeoff ✅

---

## Special Case: loop_depth

**Could we make loop_depth optional?**

### Current usage
```rust
state.loop_depth += 1;  // On loop entry
state.loop_depth -= 1;  // On loop exit
```

Only used for:
1. HookContext (debugging)
2. Not used for correctness

### Potential optimization
```rust
struct VmState {
    memory: Vec<u8>,
    pointer: MemoryAddress,
    memory_model: MemoryModel,
    step_count: StepCount,        // Always tracked
    loop_depth: Option<usize>,    // Only Some when hooks need it
}
```

**Analysis**:
- ✅ Saves 8 bytes when no hooks need depth
- ❌ Still need to check `if loop_depth.is_some()` on every loop entry/exit
- ❌ Adds branching (worse than unconditional increment!)
- ❌ Minimal savings for added complexity

**Verdict**: Not worth it

---

## Conclusion

### Can we eliminate step_count? **NO**

**Reasons**:
1. Hooks need it in HookContext to make decisions (circular dependency)
2. Limit checking requires it (common use case)
3. Error messages need it (always useful)
4. No feasible way to move it to hooks without duplicating work

### Can we eliminate loop_depth? **Technically yes, but not worth it**

**Reasons**:
1. Could make it optional, but saves minimal memory
2. Checking `if should_track` is more expensive than unconditional increment
3. Only incremented at loop boundaries (already very cheap)
4. Complexity not justified by savings

---

## The Real Cost: Is it a problem?

### Performance Impact: **NEGLIGIBLE**

```
┌─────────────────────────────────────────────────────┐
│ Typical BrainFuck Program                          │
├─────────────────────────────────────────────────────┤
│ Memory: 30,000 bytes                               │
│ Instructions: 1,000,000                            │
│                                                     │
│ Tracking Overhead:                                 │
│ - Memory: 16 bytes (0.05% of tape)                 │
│ - CPU: ~300 microseconds (0.03% of runtime)        │
│                                                     │
│ For comparison:                                    │
│ - L1 cache miss: ~4ns (13x cost of step_count++)   │
│ - Syscall overhead: ~100ns (333x cost)             │
│ - Hook dispatch: ~5-10ns (16-33x cost)             │
└─────────────────────────────────────────────────────┘
```

The cost is **literally unmeasurable** in real programs.

---

## Recommendation

### ✅ **Keep current design**

**Rationale**:
1. **step_count is essential**: Needed for limits, errors, and hooks
2. **loop_depth is trivial**: Only incremented at loop boundaries
3. **Cost is negligible**: 16 bytes + <0.1% CPU overhead
4. **Complexity not justified**: Any alternative adds significant complexity
5. **Principle of least surprise**: Users expect step counting to work

### 🎯 **Where to optimize instead**

If we want true zero-cost when disabled, focus on:

1. ✅ **Hook dispatch** (already optimized - only calls when registered)
2. ✅ **Debug info tracking** (already opt-in via DebugTrackingHook)
3. ⏳ **Memory allocation** (could pool/reuse Vec allocations)
4. ⏳ **I/O buffering** (currently unbuffered, could batch)

These have **much higher** potential impact than removing step_count.

---

## Alternative: Feature Flag (Not Recommended)

If you *really* want zero tracking overhead, use compile-time feature flags:

```rust
// Cargo.toml
[features]
default = ["tracking"]
tracking = []

// interpreter.rs
#[cfg(feature = "tracking")]
struct VmState {
    memory: Vec<u8>,
    pointer: MemoryAddress,
    memory_model: MemoryModel,
    step_count: StepCount,
    loop_depth: usize,
}

#[cfg(not(feature = "tracking"))]
struct VmState {
    memory: Vec<u8>,
    pointer: MemoryAddress,
    memory_model: MemoryModel,
}
```

**Problems**:
- ❌ Can't mix tracking and non-tracking in same binary
- ❌ Massive code duplication with `#[cfg]` everywhere
- ❌ Breaks limit checking without tracking
- ❌ Users have to recompile to enable/disable

**Verdict**: Not worth the pain for 0.03% savings

---

## Final Answer

**Can we move step_count and loop_depth to hooks entirely?**

**NO** - Not without duplicating work or massive complexity.

**Should we?**

**NO** - The cost is negligible (16 bytes + <0.1% CPU), and they're actually useful core features.

**What's the real win?**

The current design is **already well-optimized**:
- ✅ Debug info is fully opt-in (via DebugTrackingHook)
- ✅ Hook dispatch is zero-cost when no hooks registered
- ✅ step_count and loop_depth are minimal overhead even when present
- ✅ Architecture is clean and maintainable

**Focus optimization efforts on**:
- I/O buffering (much higher impact)
- Memory allocation patterns
- Hot path instruction dispatch

The 16-byte tracking overhead is not worth optimizing away.
