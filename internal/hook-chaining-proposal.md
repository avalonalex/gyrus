# Hook Chaining: Upstream → Downstream Data Flow

**Date**: 2025-10-30
**Proposal**: Allow hooks to provide data to downstream hooks
**Motivation**: Enable true zero-cost tracking by making it hook-driven

---

## The Core Idea

Instead of interpreter tracking step_count/loop_depth, let hooks form a pipeline:

```
┌─────────────────────────────────────────────────────────────┐
│                    Hook Chain                               │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  StepTrackerHook      →   LimitEnforcerHook                │
│  (tracks steps)           (consumes step_count)            │
│                                                             │
│  LoopDepthTracker     →   DebugTrackingHook                │
│  (tracks depth)           (consumes loop_depth)            │
│                                                             │
│  [upstream provides] → [downstream consumes]               │
└─────────────────────────────────────────────────────────────┘
```

**Key insight**: Tracking becomes a hook responsibility, not interpreter responsibility.

---

## Design Option 1: Shared Context Bag

### Architecture

```rust
/// Shared data that hooks can read/write
pub struct HookDataBag {
    data: HashMap<TypeId, Box<dyn Any>>,
}

impl HookDataBag {
    pub fn insert<T: 'static>(&mut self, value: T) {
        self.data.insert(TypeId::of::<T>(), Box::new(value));
    }

    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.data.get(&TypeId::of::<T>())
            .and_then(|b| b.downcast_ref::<T>())
    }
}

/// Hook receives both immutable context and mutable data bag
trait ExecutionHook {
    fn before_instruction(
        &mut self,
        instruction: &Instruction,
        ctx: &HookContext,
        data: &mut HookDataBag,  // Can read/write!
    ) -> HookDecision;
}

/// Example: Step tracking hook
impl ExecutionHook for StepTrackerHook {
    fn before_instruction(&mut self, ..., data: &mut HookDataBag) {
        let steps = data.get::<StepCount>().unwrap_or(&StepCount::new(0));
        let new_steps = steps.increment();
        data.insert(new_steps);  // Provide to downstream
        HookDecision::Continue
    }
}

/// Example: Limit checking hook
impl ExecutionHook for LimitEnforcerHook {
    fn before_instruction(&mut self, ..., data: &mut HookDataBag) {
        // Consume from upstream
        if let Some(steps) = data.get::<StepCount>() {
            if steps.get() > self.max_steps {
                return HookDecision::Break;
            }
        }
        HookDecision::Continue
    }
}
```

### Pros
- ✅ Flexible - any hook can provide any data
- ✅ Type-safe - uses TypeId for storage
- ✅ No order dependency - hooks read what they need
- ✅ Truly opt-in - only track what hooks need

### Cons
- ❌ HashMap lookup overhead (TypeId hash + lookup)
- ❌ Dynamic dispatch via `dyn Any`
- ❌ Runtime overhead for every hook call
- ❌ Can't enforce dependencies at compile time
- ❌ Debugging difficult (what data is available?)

### Performance Impact
```
Current: HookContext passed by reference (zero cost)
Proposed: HashMap<TypeId, Box<dyn Any>> lookup per hook
- TypeId hash: ~5-10ns
- HashMap lookup: ~10-20ns
- Downcast check: ~5ns
Total: ~20-35ns overhead per hook per instruction

For 1M instructions with 3 hooks:
- Overhead: 60-105ms (6-10% of total runtime)
- Current: 0.3ms

This is 200-350x MORE expensive!
```

**Verdict**: Too slow ❌

---

## Design Option 2: Typed Pipeline

### Architecture

```rust
/// Each hook declares what it provides
trait HookProvider {
    type Provides;

    fn before_instruction(
        &mut self,
        ctx: &HookContext,
    ) -> (HookDecision, Self::Provides);
}

/// Chain hooks with explicit data flow
struct HookChain<H1, H2>
where
    H1: HookProvider,
    H2: HookConsumer<H1::Provides>,
{
    hook1: H1,
    hook2: H2,
}

/// Example
struct StepTrackerHook;
impl HookProvider for StepTrackerHook {
    type Provides = StepCount;

    fn before_instruction(&mut self, ctx: &HookContext) -> (HookDecision, StepCount) {
        self.count.increment();
        (HookDecision::Continue, self.count)
    }
}

struct LimitEnforcerHook;
impl HookConsumer<StepCount> for LimitEnforcerHook {
    fn before_instruction(&mut self, ctx: &HookContext, steps: StepCount) -> HookDecision {
        if steps.get() > self.max {
            HookDecision::Break
        } else {
            HookDecision::Continue
        }
    }
}
```

### Pros
- ✅ Type-safe at compile time
- ✅ Zero runtime overhead (monomorphized)
- ✅ Explicit dependencies
- ✅ Clear data flow

### Cons
- ❌ Extremely complex type signatures
- ❌ Can't have dynamic number of hooks
- ❌ Must know all hooks at compile time
- ❌ Can't mix different hook types easily
- ❌ Generic explosion (one type per combination)

**Verdict**: Too complex ❌

---

## Design Option 3: Explicit Dependency Registration

### Architecture

```rust
/// Hooks declare what they need
trait ExecutionHook {
    fn dependencies(&self) -> HookDependencies {
        HookDependencies::none()
    }

    fn before_instruction(&mut self, ctx: &HookContext) -> HookDecision;
}

struct HookDependencies {
    needs_step_count: bool,
    needs_loop_depth: bool,
    needs_memory_stats: bool,
}

/// HookManager checks dependencies and builds minimal HookContext
impl HookManager {
    fn register(&mut self, hook: BoxedHook) {
        let deps = hook.dependencies();

        // Enable tracking based on what hooks need
        self.track_steps |= deps.needs_step_count;
        self.track_depth |= deps.needs_loop_depth;

        self.hooks.push(hook);
    }
}

/// Interpreter only tracks what's needed
impl VmState {
    fn increment_step_count(&mut self, track_steps: bool) {
        if track_steps {
            self.step_count.increment();
        }
    }
}
```

### Pros
- ✅ Explicit what each hook needs
- ✅ Can optimize tracking based on registered hooks
- ✅ Simple to understand
- ✅ No runtime overhead if dependencies not needed

### Cons
- ❌ Still need VmState to track (just conditionally)
- ❌ Checking `if track_steps` adds branching
- ❌ Doesn't eliminate the tracking, just makes it conditional
- ❌ Minimal benefit over current design

### Performance Impact
```
Current: Always track (unconditional increment)
Proposed: if track_steps { increment() }

Branch cost: ~1-2ns (branch prediction usually correct)
Savings when disabled: ~0.3ns (one add instruction)

Net result: SLOWER when enabled, minimal savings when disabled
```

**Verdict**: Not worth it ❌

---

## Design Option 4: Hybrid Approach (Most Promising)

### Architecture

Keep core tracking in VmState, but allow hooks to **contribute additional data**:

```rust
/// Core state that's always tracked (minimal)
struct VmState {
    memory: Vec<u8>,
    pointer: MemoryAddress,
    memory_model: MemoryModel,
    step_count: StepCount,  // Always tracked (needed for errors/limits)
}

/// Optional tracking via hooks
pub struct HookContext<'a> {
    // Core state (always available)
    memory: &'a [u8],
    pointer: MemoryAddress,
    step_count: StepCount,

    // Extended data from upstream hooks
    extensions: Option<&'a HookExtensions>,
}

/// Hooks can contribute custom data
pub struct HookExtensions {
    loop_depth: Option<usize>,      // Tracked by LoopDepthHook
    memory_stats: Option<MemoryStats>,  // Tracked by MemoryStatsHook
    custom: HashMap<TypeId, Box<dyn Any>>,  // Custom hook data
}

/// Hook manager manages extensions
impl HookManager {
    fn dispatch_before(
        &mut self,
        instruction: &Instruction,
        core_ctx: &CoreContext,  // From VmState
    ) -> HookDecision {
        // First pass: hooks can write to extensions
        for hook in &mut self.hooks {
            hook.prepare_extensions(&mut self.extensions);
        }

        // Second pass: hooks can read extensions
        let full_ctx = HookContext::new(core_ctx, &self.extensions);
        for hook in &mut self.hooks {
            match hook.before_instruction(instruction, &full_ctx) {
                HookDecision::Break => return HookDecision::Break,
                _ => continue,
            }
        }
        HookDecision::Continue
    }
}
```

### Example Usage

```rust
/// Loop depth tracking hook (optional)
struct LoopDepthHook {
    depth: usize,
}

impl ExecutionHook for LoopDepthHook {
    fn prepare_extensions(&mut self, ext: &mut HookExtensions) {
        ext.loop_depth = Some(self.depth);
    }

    fn on_loop_enter(&mut self, ctx: &HookContext) {
        self.depth += 1;
    }
}

/// Debug hook consumes loop depth from upstream
struct DebugTrackingHook;

impl ExecutionHook for DebugTrackingHook {
    fn before_instruction(&mut self, ctx: &HookContext) {
        // Read from extensions if available
        if let Some(depth) = ctx.extensions()
            .and_then(|e| e.loop_depth)
        {
            println!("At depth: {}", depth);
        }
    }
}
```

### Pros
- ✅ **Core tracking stays fast** (step_count always tracked)
- ✅ **Optional data is truly opt-in** (loop_depth, custom stats)
- ✅ **Hooks can cooperate** (upstream provides, downstream consumes)
- ✅ **No breaking changes** to existing API
- ✅ **Flexible** for future extensions

### Cons
- ⚠️ Two-pass dispatch (prepare + execute) adds complexity
- ⚠️ Still need HashMap for custom extensions (but only if used)
- ⚠️ More complex than current design

### Performance Impact
```
When no extensions needed:
- Cost: 0 (extensions = None, skip HashMap entirely)

When extensions used:
- Prepare pass: ~5-10ns per hook
- HashMap for custom data: ~20ns per lookup
- Total: ~30-50ns overhead per hook call

For hooks that need extensions (like debug tracking):
This overhead is acceptable since they're opt-in anyway.
```

**Verdict**: Worth considering ✅

---

## Comparison Table

| Design | Type Safety | Performance | Complexity | Flexibility | Verdict |
|--------|-------------|-------------|------------|-------------|---------|
| **Shared Context Bag** | Runtime | 200-350x slower | Medium | High | ❌ Too slow |
| **Typed Pipeline** | Compile-time | Zero overhead | Very High | Low | ❌ Too complex |
| **Explicit Dependencies** | Static | ~Same as current | Low | Medium | ❌ No real benefit |
| **Hybrid Approach** | Runtime | ~10-30ns per hook | Medium | High | ✅ Best tradeoff |
| **Current Design** | N/A | Baseline | Low | N/A | ✅ Simple, works well |

---

## Real-World Examples

### Middleware Pattern (Web Frameworks)

Your idea is similar to middleware in Express.js, Actix, or Axum:

```rust
// Express.js style
app.use(logger);       // Upstream: logs requests
app.use(auth);         // Consumes: uses logged data
app.use(rateLimit);    // Consumes: uses auth data

// Hook chain equivalent
hook_manager
    .register(StepTrackerHook::new())      // Provides: step_count
    .register(LimitEnforcerHook::new())    // Consumes: step_count
    .register(LoopDepthHook::new())        // Provides: loop_depth
    .register(DebugTrackingHook::new());   // Consumes: loop_depth
```

This pattern is proven and works well!

---

## Recommendation

### For Now: Keep Current Design ✅

**Reasons**:
1. **step_count must be tracked anyway** (needed for errors, limits)
2. **Current overhead is negligible** (16 bytes, 0.03% CPU)
3. **Hook chaining adds complexity** without clear wins
4. **No user complaints** about current design

### For Future: Consider Hybrid Approach 🤔

**If we add hook chaining**, use the **Hybrid Approach**:

```rust
// Core tracking stays in VmState
struct VmState {
    step_count: StepCount,  // Always tracked (essential)
}

// Optional tracking via HookExtensions
pub struct HookExtensions {
    loop_depth: Option<usize>,     // Opt-in via LoopDepthHook
    custom: HashMap<TypeId, Box<dyn Any>>,
}

// Two-tier HookContext
pub struct HookContext<'a> {
    step_count: StepCount,         // Always available
    extensions: Option<&'a HookExtensions>,  // Opt-in
}
```

**When to implement**:
- ⏳ When we have concrete use case for hook-to-hook data flow
- ⏳ When users request more flexible hook composition
- ⏳ When we see performance issues with current design

---

## Alternative: Keep It Simple

**Plot twist**: Maybe we don't need this at all!

**Consider**:
```
Current cost: 16 bytes + 0.03% CPU
Hybrid cost: Same + complexity + HashMap overhead when used

Benefit: Loop depth becomes opt-in (saves 8 bytes when unused)
Cost: Significant complexity, potential bugs, harder to understand

Is it worth it? 🤔
```

**My honest take**: The current design is **already excellent**. Hook chaining is a cool idea, but the juice isn't worth the squeeze here.

---

## Action Items

### Immediate (Do Nothing)
- ✅ Current design is good
- ✅ No performance problems
- ✅ No user complaints

### If You Want to Experiment
1. Create prototype of Hybrid Approach in a branch
2. Benchmark against current implementation
3. Write examples showing benefits
4. Get user feedback
5. Decide based on data

### If You Want Hook Chaining for Other Reasons
Consider it for:
- **Custom profiling data** (hooks sharing analysis)
- **Multi-stage transformations** (one hook processes another's output)
- **Plugin ecosystems** (third-party hooks building on core hooks)

But for `step_count` and `loop_depth`? Not worth the complexity.

---

## Conclusion

Your idea is **architecturally sound** and **similar to proven patterns** (middleware).

However, for this specific case (step_count/loop_depth), the **current design is already excellent**:
- ✅ Minimal overhead
- ✅ Simple to understand
- ✅ No hidden costs
- ✅ Works perfectly

**Recommendation**: Document the current design as intentional, not a limitation. Hook chaining is a great idea for future features, but not needed here.

---

## DECISION (2025-10-30)

**Status**: DEFERRED until debugger and JIT compiler are implemented

**Rationale**:
- Current interpreter design is solid and performant
- No concrete pain points that hook chaining would solve today
- Debugger and JIT compiler will reveal real use cases for hook composition
- Better to build features first, then optimize architecture based on learnings

**Revisit after**:
1. ✅ Visual TUI debugger is complete
2. ✅ Cranelift-based JIT/AOT compiler is working
3. ✅ We have concrete examples where hook chaining would help

**Potential future use cases** that debugger/compiler might reveal:
- **Debugger**: Breakpoint hooks need to coordinate with step tracking
- **Compiler**: IR generation hooks might need data flow analysis from profiler
- **Time-travel debugging**: State capture hooks need to snapshot all tracking data
- **Multi-tier compilation**: Hot path detection hooks feed into JIT trigger hooks

When these features exist, we'll have **real data** to guide the architecture decision.

**Current status**: Keep it simple, ship features, iterate based on evidence. ✅
