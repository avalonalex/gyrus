# PRD: Interpreter Refactoring - Breaking Down God Methods

**Status**: Not Started (Design Complete ✅)
**Last Updated**: 2025-10-30
**Priority**: Medium
**Estimated Effort**: 3-5 hours
**Risk Level**: Medium (touches core execution loop)

## Summary

The interpreter module contains two "God methods" that have accumulated too many responsibilities over time. This PRD proposes breaking down these methods into smaller, more focused components to improve maintainability, testability, and extensibility.

## Motivation

### Current Pain Points

1. **`execute_block()` - 200 lines, 6 responsibilities**
   - Hard to understand the execution flow
   - Difficult to test hook dispatching separately
   - Changes to hook logic require modifying the main execution loop
   - Hook code is deeply interleaved with execution code

2. **`interpret_with_io()` - 140 lines, 8 responsibilities**
   - Setup code dominates the function (70+ lines)
   - Hard to test hook registration logic in isolation
   - Error enrichment logic is complex and duplicated
   - Difficult to extend with new auto-registered hooks

### Why This Matters

- **Maintainability**: Smaller functions are easier to understand and modify
- **Testability**: Isolated components can be unit tested independently
- **Extensibility**: Adding new hook points or auto-hooks becomes easier
- **Code Quality**: Follows Single Responsibility Principle
- **Future-proofing**: Makes optimization work (from optimization PRD) easier

## Current Architecture Analysis

### File: `interpreter.rs`
- **Total lines**: 2,840
- **Actual code**: 698 lines
- **Tests**: 2,142 lines
- **Verdict**: File size is acceptable (mostly tests)

### God Method #1: `execute_block()`

**Location**: `interpreter.rs:506-706` (200 lines)

**Current Responsibilities**:
1. Instruction iteration (main loop)
2. Hook dispatching: `before_instruction`
3. Loop execution with depth tracking
4. Hook dispatching: `on_loop_enter`, `on_loop_exit`
5. Recursive execution for nested loops
6. Instruction delegation + `after_instruction` hooks

**Complexity Metrics**:
- Lines: 200
- Cyclomatic complexity: ~11 control flow statements
- Hook dispatch code: ~80 lines (40% of function)
- Core execution logic: ~120 lines (60% of function)

**Problem Pattern**: Hook logic is deeply nested within execution logic, making both harder to reason about.

### God Method #2: `interpret_with_io()`

**Location**: `interpreter.rs:178-318` (140 lines)

**Current Responsibilities**:
1. Auto-register `StatsTrackerHook`
2. Auto-register `WarningCollectorHook`
3. Auto-register `DebugTrackingHook` (conditional)
4. Auto-register `LimitEnforcerHook` (conditional)
5. Create and initialize VmState
6. Execute program (delegate to `execute_block`)
7. Check and enrich limit hook errors
8. Enrich execution errors with debug info
9. Extract statistics from hooks
10. Extract warnings from hooks

**Complexity Metrics**:
- Lines: 140
- Hook setup: ~30 lines (21%)
- Execution: ~5 lines (4%)
- Error handling: ~70 lines (50%)
- Statistics extraction: ~35 lines (25%)

**Problem Pattern**: Function tries to do everything - setup, execution, and cleanup all in one place.

## Proposed Design

### Phase 1: Extract Hook Dispatcher (High Priority)

**Goal**: Separate hook dispatching logic from execution logic

**New Component**: `HookDispatcher`

```rust
/// Handles all hook dispatching for the interpreter.
///
/// This component centralizes hook-related logic, making it easier to:
/// - Test hook behavior in isolation
/// - Add new hook points without modifying execute_block
/// - Maintain consistent hook behavior across the interpreter
struct HookDispatcher<'a> {
    /// The execution config containing registered hooks
    config: &'a mut ExecutionConfig,

    /// Optional debug information for source location tracking
    debug_info: Option<&'a DebugInfo>,
}

impl<'a> HookDispatcher<'a> {
    /// Create a new hook dispatcher
    fn new(config: &'a mut ExecutionConfig, debug_info: Option<&'a DebugInfo>) -> Self {
        Self { config, debug_info }
    }

    /// Dispatch before_instruction hook
    ///
    /// Returns HookDecision::Continue, Break, or Skip
    fn dispatch_before(
        &mut self,
        instruction: &Instruction,
        state: &VmState,
        instruction_index: usize,
    ) -> HookDecision {
        if let Some(hook_manager) = self.config.hook_manager_mut() {
            let context = HookContext::new(
                &state.memory,
                state.pointer,
                state.step_count,
                None, // Source location tracked by DebugTrackingHook
                state.loop_depth,
                instruction_index,
            );
            hook_manager.before_instruction(instruction, &context)
        } else {
            HookDecision::Continue
        }
    }

    /// Dispatch after_instruction hook
    fn dispatch_after(
        &mut self,
        instruction: &Instruction,
        state: &VmState,
        instruction_index: usize,
    ) -> HookDecision {
        if let Some(hook_manager) = self.config.hook_manager_mut() {
            let context = HookContext::new(
                &state.memory,
                state.pointer,
                state.step_count,
                None,
                state.loop_depth,
                instruction_index,
            );
            hook_manager.after_instruction(instruction, &context)
        } else {
            HookDecision::Continue
        }
    }

    /// Dispatch on_loop_enter hook
    fn dispatch_loop_enter(
        &mut self,
        state: &VmState,
        loop_instruction_index: usize,
        body_start_index: usize,
        body_size: usize,
    ) -> HookDecision {
        if let Some(hook_manager) = self.config.hook_manager_mut() {
            let context = HookContext::new(
                &state.memory,
                state.pointer,
                state.step_count,
                None,
                state.loop_depth,
                loop_instruction_index,
            );
            let loop_info = LoopInfo {
                loop_instruction_index,
                body_start_index,
                body_size,
            };
            hook_manager.on_loop_enter(&context, Some(&loop_info))
        } else {
            HookDecision::Continue
        }
    }

    /// Dispatch on_loop_exit hook
    fn dispatch_loop_exit(&mut self, state: &VmState, instruction_index: usize) -> HookDecision {
        if let Some(hook_manager) = self.config.hook_manager_mut() {
            let context = HookContext::new(
                &state.memory,
                state.pointer,
                state.step_count,
                None,
                state.loop_depth,
                instruction_index,
            );
            hook_manager.on_loop_exit(&context)
        } else {
            HookDecision::Continue
        }
    }

    /// Dispatch on_complete hook
    fn dispatch_complete(&mut self, state: &VmState) {
        if let Some(hook_manager) = self.config.hook_manager_mut() {
            let context = HookContext::new(
                &state.memory,
                state.pointer,
                state.step_count,
                None,
                state.loop_depth,
                0, // instruction_index not meaningful for on_complete
            );
            hook_manager.on_complete(&context);
        }
    }
}
```

**Refactored `execute_block()`**:

```rust
fn execute_block<I: BfInput, O: BfOutput>(
    instructions: &[Instruction],
    state: &mut VmState,
    dispatcher: &mut HookDispatcher,
    input: &mut I,
    output: &mut O,
    start_index: usize,
) -> Result<()> {
    let mut local_index = 0;

    for instruction in instructions {
        let instruction_index = start_index + local_index;
        state.step_count.increment();

        // Dispatch before_instruction hook
        match dispatcher.dispatch_before(instruction, state, instruction_index) {
            HookDecision::Continue => {}
            HookDecision::Break => {
                return Err(BfError::ExecutionPaused {
                    instruction_index: state.step_count.into(),
                    source_location: None,
                    message: Some(format!(
                        "Execution paused by hook at instruction {}",
                        state.step_count.get()
                    )),
                });
            }
            HookDecision::Skip => {
                local_index += 1;
                continue;
            }
        }

        // Execute the instruction
        match instruction {
            Instruction::Loop(body) => {
                let body_start_index = instruction_index + 1;
                let body_size = count_instructions(body);

                while state.memory[state.pointer.get()] != 0 {
                    state.loop_depth += 1;

                    // Dispatch on_loop_enter hook
                    match dispatcher.dispatch_loop_enter(
                        state,
                        instruction_index,
                        body_start_index,
                        body_size,
                    ) {
                        HookDecision::Break => {
                            state.loop_depth -= 1;
                            return Err(BfError::ExecutionPaused {
                                instruction_index: state.step_count.into(),
                                source_location: None,
                                message: Some("Execution paused by hook in loop".to_string()),
                            });
                        }
                        _ => {}
                    }

                    // Recursive execution
                    execute_block(body, state, dispatcher, input, output, body_start_index)?;

                    // Dispatch on_loop_exit hook
                    dispatcher.dispatch_loop_exit(state, instruction_index);

                    state.loop_depth -= 1;
                }
            }
            _ => {
                // Delegate to execute_single_instruction
                execute_single_instruction(instruction, state, input, output)?;
            }
        }

        // Dispatch after_instruction hook
        if let HookDecision::Break = dispatcher.dispatch_after(instruction, state, instruction_index) {
            return Err(BfError::ExecutionPaused {
                instruction_index: state.step_count.into(),
                source_location: None,
                message: Some("Execution paused by hook after instruction".to_string()),
            });
        }

        local_index += 1;
    }

    Ok(())
}
```

**Benefits**:
- ✅ `execute_block()` reduced from 200 → ~100 lines
- ✅ Hook logic isolated and testable
- ✅ Easier to add new hook points
- ✅ Clearer separation of concerns

**Risks**:
- ⚠️ Core execution loop changes (high test coverage mitigates this)
- ⚠️ Slight performance overhead from extra function calls (likely negligible)

---

### Phase 2: Extract Interpreter Setup (Medium Priority)

**Goal**: Separate hook auto-registration and error enrichment from core execution

**New Component**: `InterpreterContext`

```rust
/// Context for interpreter execution with auto-registered hooks.
///
/// This struct handles the lifecycle of auto-registered hooks:
/// 1. Setup: Register built-in hooks (stats, warnings, debug, limits)
/// 2. Execution: Run the program
/// 3. Cleanup: Extract results and enrich errors
pub struct InterpreterContext<'a> {
    config: ExecutionConfig,
    debug_info: Option<&'a DebugInfo>,

    // Hook handles for extracting results after execution
    stats_handle: Arc<Mutex<StatsTrackerHook>>,
    warning_handle: Arc<Mutex<WarningCollectorHook>>,
    debug_handle: Option<Arc<Mutex<DebugTrackingHook>>>,
    limit_handle: Option<Arc<Mutex<LimitEnforcerHook>>>,
}

impl<'a> InterpreterContext<'a> {
    /// Create a new interpreter context with auto-registered hooks
    pub fn new(config: ExecutionConfig, debug_info: Option<&'a DebugInfo>) -> Self {
        let mut config = config;

        // Auto-register built-in hooks
        let (stats_hook, stats_handle) = SharedStatsHook::new();
        let (warning_hook, warning_handle) = SharedWarningHook::new();
        config.register_hook(Box::new(stats_hook));
        config.register_hook(Box::new(warning_hook));

        // Register debug tracking hook if debug info is provided
        let debug_handle = debug_info.map(|info| {
            let (debug_hook, handle) = SharedDebugTrackingHook::new(info.clone());
            config.register_hook(Box::new(debug_hook));
            handle
        });

        // Register limit enforcement hook if limits are configured
        let limit_handle = if config.max_steps().is_some() || config.timeout_ms().is_some() {
            let (limit_hook, handle) = SharedLimitHook::new(config.max_steps(), config.timeout_ms());
            config.register_hook(Box::new(limit_hook));
            Some(handle)
        } else {
            None
        };

        Self {
            config,
            debug_info,
            stats_handle,
            warning_handle,
            debug_handle,
            limit_handle,
        }
    }

    /// Execute the program and return statistics
    pub fn execute<I: BfInput, O: BfOutput>(
        mut self,
        instructions: &[Instruction],
        input: &mut I,
        output: &mut O,
    ) -> Result<ExecutionStats> {
        // Create VM state
        let mut state = VmState::new(*self.config.memory_model());

        // Get debug info clone if needed
        let debug_info_clone = self.debug_handle
            .as_ref()
            .map(|handle| handle.lock().unwrap().debug_info().clone());

        // Create hook dispatcher
        let mut dispatcher = HookDispatcher::new(&mut self.config, debug_info_clone.as_ref());

        // Execute the program
        let execute_result = execute_block(
            instructions,
            &mut state,
            &mut dispatcher,
            input,
            output,
            0,
        );

        // Dispatch on_complete hook
        dispatcher.dispatch_complete(&state);

        // Check for limit errors
        self.check_limit_errors()?;

        // Handle execution result
        match execute_result {
            Ok(()) => self.extract_stats(),
            Err(error) => Err(self.enrich_error(error)),
        }
    }

    /// Check if limit hook stopped execution with an error
    fn check_limit_errors(&self) -> Result<()> {
        if let Some(handle) = &self.limit_handle {
            if let Some(mut error) = handle.lock().unwrap().take_error() {
                // Enrich with debug info
                error = self.enrich_limit_error(error);
                return Err(error);
            }
        }
        Ok(())
    }

    /// Enrich step limit errors with source location
    fn enrich_limit_error(&self, mut error: BfError) -> BfError {
        if let Some(debug_handle) = &self.debug_handle {
            if let BfError::StepLimitExceeded { instruction_index, .. } = &error {
                let debug_hook = debug_handle.lock().unwrap();
                if let Some(loc) = debug_hook.debug_info().lookup(*instruction_index) {
                    error = error.with_step_limit_source_location(loc);
                }
            }
        }
        error
    }

    /// Enrich execution errors with debug information
    fn enrich_error(&self, mut error: BfError) -> BfError {
        if let Some(handle) = &self.debug_handle {
            let debug_hook = handle.lock().unwrap();

            // Add loop call stack
            let loop_stack = debug_hook.loop_stack().to_vec();
            let loop_call_stack: Vec<LoopStackFrame> = loop_stack
                .into_iter()
                .map(|ctx| LoopStackFrame {
                    source_location: ctx.source_location,
                    iteration: ctx.iteration,
                })
                .collect();
            error = error.with_loop_call_stack(loop_call_stack);

            // Add source location for step limit errors
            if let BfError::StepLimitExceeded { instruction_index, .. } = &error {
                if let Some(loc) = debug_hook.debug_info().lookup(*instruction_index) {
                    error = error.with_step_limit_source_location(loc);
                }
            }
        }
        error
    }

    /// Extract statistics from hooks
    fn extract_stats(self) -> Result<ExecutionStats> {
        let mut stats = self.stats_handle.lock().unwrap().stats().clone();

        // Add warnings to stats
        let warnings = self.warning_handle.lock().unwrap().warnings().to_vec();
        stats.warnings = warnings;

        Ok(stats)
    }
}
```

**Refactored `interpret_with_io()`**:

```rust
pub fn interpret_with_io<I: BfInput, O: BfOutput>(
    instructions: &[Instruction],
    config: ExecutionConfig,
    input: &mut I,
    output: &mut O,
    debug_info: Option<&DebugInfo>,
) -> Result<ExecutionStats> {
    let context = InterpreterContext::new(config, debug_info);
    context.execute(instructions, input, output)
}
```

**Benefits**:
- ✅ `interpret_with_io()` reduced from 140 → ~10 lines
- ✅ Hook setup logic isolated and testable
- ✅ Error enrichment logic centralized
- ✅ Easy to add new auto-registered hooks

**Risks**:
- ⚠️ API change (internal only, public API unchanged)
- ⚠️ Need to verify all error paths are covered

---

## Implementation Plan

### Phase 1: Extract HookDispatcher (Week 1)

**Tasks**:
1. ✅ Create `HookDispatcher` struct in `interpreter.rs`
2. ✅ Implement dispatch methods
3. ✅ Refactor `execute_block()` to use dispatcher
4. ✅ Update `execute_single_instruction()` if needed
5. ✅ Run full test suite
6. ✅ Verify no performance regression (benchmarks)

**Success Criteria**:
- All existing tests pass
- `execute_block()` is under 120 lines
- Hook dispatching code is isolated
- No performance regression (< 5% overhead acceptable)

**Estimated Effort**: 2-3 hours

---

### Phase 2: Extract InterpreterContext (Week 2)

**Tasks**:
1. ✅ Create `InterpreterContext` struct
2. ✅ Move hook setup logic
3. ✅ Move error enrichment logic
4. ✅ Move statistics extraction
5. ✅ Refactor `interpret_with_io()` to use context
6. ✅ Run full test suite
7. ✅ Update documentation

**Success Criteria**:
- All existing tests pass
- `interpret_with_io()` is under 20 lines
- Hook setup is isolated and testable
- Error enrichment is centralized

**Estimated Effort**: 1-2 hours

---

### Phase 3: Add Unit Tests (Week 2)

**Tasks**:
1. ✅ Test `HookDispatcher` in isolation
2. ✅ Test `InterpreterContext` hook registration
3. ✅ Test error enrichment logic
4. ✅ Test statistics extraction

**Success Criteria**:
- 90%+ code coverage for new components
- Edge cases covered (no hooks, multiple hooks, etc.)

**Estimated Effort**: 1 hour

---

## Success Metrics

### Code Quality Metrics

**Before**:
- `execute_block()`: 200 lines, complexity ~11
- `interpret_with_io()`: 140 lines, complexity ~8
- Total interpreter code: 698 lines

**After**:
- `execute_block()`: ~100 lines, complexity ~6
- `interpret_with_io()`: ~10 lines, complexity ~1
- `HookDispatcher`: ~80 lines, complexity ~4
- `InterpreterContext`: ~120 lines, complexity ~5
- Total interpreter code: ~710 lines (slight increase OK)

**Key Improvements**:
- ✅ Reduced cyclomatic complexity by ~40%
- ✅ Improved Single Responsibility Principle adherence
- ✅ Increased testability (new components can be unit tested)

### Maintainability Metrics

**Ease of Adding New Features**:
- Adding new hook points: Easier (centralized in HookDispatcher)
- Adding new auto-hooks: Easier (centralized in InterpreterContext)
- Debugging hook issues: Easier (isolated components)

**Test Coverage**:
- Before: ~85% (estimated)
- After: ~90% (with new unit tests)

---

## Risks and Mitigation

### Risk 1: Breaking Changes
**Probability**: Medium
**Impact**: High
**Mitigation**:
- Comprehensive test suite (2,142 test lines)
- Run benchmarks to detect regressions
- Phase by phase rollout (can revert if issues found)

### Risk 2: Performance Regression
**Probability**: Low
**Impact**: Medium
**Mitigation**:
- Measure baseline performance before refactoring
- Run benchmarks after each phase
- Keep hot paths inlined (use `#[inline]` where needed)
- Target: < 5% overhead acceptable

### Risk 3: Increased Complexity
**Probability**: Low
**Impact**: Medium
**Mitigation**:
- Add comprehensive documentation
- Follow Rust idioms (ownership, lifetimes)
- Keep component interfaces simple

---

## Alternatives Considered

### Alternative 1: Leave As-Is
**Pros**: No risk, no effort
**Cons**: Technical debt accumulates, harder to extend
**Verdict**: ❌ Rejected - Code quality would degrade over time

### Alternative 2: Complete Rewrite
**Pros**: Could redesign from scratch
**Cons**: High risk, high effort, breaks backward compatibility
**Verdict**: ❌ Rejected - Too risky for marginal benefit

### Alternative 3: Extract Only Critical Parts
**Pros**: Lower risk, lower effort
**Cons**: Doesn't fully solve the problem
**Verdict**: ✅ This PRD - Balanced approach

---

## Dependencies

### Prerequisites
- ✅ Hook system (completed)
- ✅ Debug symbols (completed)
- ✅ Comprehensive test suite (exists)

### Blocked By
- None

### Blocks
- None (this is a refactoring, not a new feature)

---

## Future Considerations

### After This Refactoring

With cleaner components, these become easier:

1. **Optimization Work** (from optimization-and-advanced-features.md)
   - Adding optimization passes before execution
   - Implementing JIT compilation
   - Hook-aware optimizations

2. **Debugger Features**
   - Step-by-step execution
   - Breakpoint management
   - State inspection

3. **Additional Hook Points**
   - `on_instruction_skip` for optimization reporting
   - `on_memory_grow` for unbounded memory tracking
   - `on_error` for custom error handling

---

## References

- **Codebase Analysis**: Conducted 2025-10-30
- **Related PRDs**:
  - `optimization-and-advanced-features.md` - Will benefit from cleaner interpreter
  - `debug-symbols-and-runtime-diagnostics.md` - Already leverages hooks
- **Design Patterns**:
  - Strategy Pattern (HookDispatcher)
  - Builder Pattern (InterpreterContext)
  - Single Responsibility Principle

---

## Appendix: Detailed Metrics

### Current Code Distribution (interpreter.rs)

```
Total lines: 2,840
├── Actual code: 698 (24.6%)
│   ├── Public API: ~50 lines (7%)
│   ├── execute_block: 200 lines (29%)
│   ├── interpret_with_io: 140 lines (20%)
│   ├── execute_single_instruction: 113 lines (16%)
│   └── Other helpers: ~195 lines (28%)
└── Tests: 2,142 (75.4%)
```

### Proposed Code Distribution

```
Total lines: ~2,850 (slight increase)
├── Actual code: ~710 (+12 lines, +1.7%)
│   ├── Public API: ~50 lines (7%)
│   ├── execute_block: ~100 lines (14%) ⬇️ 50% reduction
│   ├── interpret_with_io: ~10 lines (1%) ⬇️ 93% reduction
│   ├── execute_single_instruction: ~113 lines (16%)
│   ├── HookDispatcher: ~80 lines (11%) NEW
│   ├── InterpreterContext: ~120 lines (17%) NEW
│   ├── Other helpers: ~237 lines (33%)
└── Tests: ~2,140 (75.0%)
```

**Net Result**: Slightly more total code (+1.7%), but significantly better organized.

---

## Review Checklist

Before starting implementation:
- [ ] Review proposed design with team
- [ ] Confirm test coverage is adequate
- [ ] Set up performance benchmarks
- [ ] Plan rollback strategy if issues arise

During implementation:
- [ ] Run tests after each phase
- [ ] Measure performance after each phase
- [ ] Document new components
- [ ] Update examples if needed

After implementation:
- [ ] Full test suite passes
- [ ] Performance within acceptable range (< 5% overhead)
- [ ] Documentation updated
- [ ] PRD moved to archived/
