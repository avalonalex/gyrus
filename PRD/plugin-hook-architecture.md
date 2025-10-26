# PRD: Plugin/Hook Architecture for FerrousCortex

**Status**: Design Phase - Ready for Implementation
**Last Updated**: October 2025
**Implementation**: Phase 1 (Refactoring) Complete ✅

---

## Executive Summary

This document proposes a **Plugin/Hook Architecture** for FerrousCortex that enables extensibility without sacrificing performance or safety. The design allows developers to inject custom behavior at key execution points, enabling features like debugging, profiling, tracing, and optimization analysis.

**Key Features:**
- Zero-cost abstraction when hooks are disabled (default)
- Type-safe hook interface with compile-time guarantees
- Support for multiple concurrent hooks
- Async-friendly design (future compatibility)
- Integration with existing debug symbols and statistics systems

**Use Cases Enabled:**
- Interactive debuggers with breakpoints and watchpoints
- Execution tracing and replay
- Performance profiling and hotspot detection
- Custom instrumentation for research
- Debug commands (`#`) and breakpoints (`@`)
- REPL mode with step-through execution
- Memory access pattern analysis
- Optimization pass validation

---

## Table of Contents

1. [Motivation & Prerequisites](#motivation--prerequisites)
2. [Architecture Overview](#architecture-overview)
3. [Detailed Design](#detailed-design)
4. [Built-in Hooks Library](#built-in-hooks-library)
5. [Use Case Examples](#use-case-examples)
6. [Implementation Roadmap](#implementation-roadmap)
7. [Performance Considerations](#performance-considerations)
8. [Related PRDs](#related-prds)

---

## Motivation & Prerequisites

### Current State Analysis

#### ✅ What's Already Good

The interpreter is in excellent shape for hook integration:

1. **Clean separation of concerns** ✅ (Phase 1 Complete)
   - `VmState` encapsulates all runtime state
   - `ExecutionConfig` holds configuration (immutable during execution)
   - I/O abstraction via `BfInput`/`BfOutput` traits
   - Memory models via `MemoryModel` trait

2. **Refactored execution model** ✅ (Phase 1 Complete)
   - `execute_block()` handles control flow and loops
   - `execute_single_instruction()` handles instruction execution
   - Clean match statement for instruction dispatch
   - Loop depth tracking infrastructure in place

3. **Debug info infrastructure** ✅
   - `DebugInfo` exists for source location mapping
   - Passed through execution path as `Option<&DebugInfo>`

4. **VmState with hook-ready fields** ✅ (Phase 1 Complete)
   - `loop_depth: usize` - tracks nesting level
   - Helper methods: `current_loop_depth()`, `memory_slice()`, `current_source_location()`

#### 🎯 What Hooks Will Add

Hooks will enable **observing and controlling execution** at runtime:

1. **Debugging**: Pause execution, inspect state, step through code
2. **Profiling**: Track which instructions are hot spots
3. **Tracing**: Log execution for replay or analysis
4. **Custom Extensions**: Implement domain-specific behavior

### Design Goals

1. **Zero Overhead**: When hooks are disabled (default), zero performance cost
2. **Type Safety**: Hooks are type-safe and prevent misuse at compile time
3. **Composability**: Multiple hooks work together seamlessly
4. **Performance**: Hook dispatch is fast when enabled
5. **Ergonomics**: Easy to write and use hooks
6. **Integration**: Works with existing debug symbols, stats, and I/O

---

## Architecture Overview

### Three-Layer Design

```
┌─────────────────────────────────────────────────────────┐
│                    Hook Trait Layer                      │
│  (Defines what hooks can observe and control)           │
└─────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────┐
│                  Hook Manager Layer                      │
│  (Dispatches events to registered hooks efficiently)    │
└─────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────┐
│                  Integration Layer                       │
│  (Interpreter calls hooks at strategic points)          │
└─────────────────────────────────────────────────────────┘
```

### Hook Points

Hooks can intercept execution at these points:

1. **Before Instruction**: Called before executing each instruction
2. **After Instruction**: Called after executing each instruction
3. **Loop Enter**: Called when entering a loop (`[`)
4. **Loop Exit**: Called when exiting a loop (`]`)
5. **Memory Access**: Called on memory read/write (optional, high-frequency)
6. **I/O Operation**: Called on input/output
7. **Custom Extension**: Called for extension instructions (`#`, `@`)

---

## Detailed Design

### 1. Hook Trait Definition

**File:** `crates/ferrous-cortex/src/hooks.rs` (new file)

```rust
use crate::instruction::Instruction;
use crate::location::SourceLocation;
use crate::types::{MemoryAddress, StepCount};

/// Immutable snapshot of interpreter state exposed to hooks.
#[derive(Debug)]
pub struct HookContext<'a> {
    memory: &'a [u8],
    pointer: MemoryAddress,
    step_count: StepCount,
    source_location: Option<&'a SourceLocation>,
    loop_depth: usize,
}

impl<'a> HookContext<'a> {
    pub fn new(
        memory: &'a [u8],
        pointer: MemoryAddress,
        step_count: StepCount,
        source_location: Option<&'a SourceLocation>,
        loop_depth: usize,
    ) -> Self {
        Self {
            memory,
            pointer,
            step_count,
            source_location,
            loop_depth,
        }
    }

    pub fn memory(&self) -> &[u8] { self.memory }
    pub fn pointer(&self) -> MemoryAddress { self.pointer }
    pub fn step_count(&self) -> StepCount { self.step_count }
    pub fn current_cell(&self) -> u8 { self.memory[self.pointer.get()] }
    pub fn source_location(&self) -> Option<&SourceLocation> { self.source_location }
    pub fn loop_depth(&self) -> usize { self.loop_depth }
}

/// Decision returned by hooks to control execution flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookDecision {
    Continue,  // Continue execution normally
    Break,     // Pause execution (for debugger/breakpoint)
    Skip,      // Skip the current instruction
}

/// Main hook trait that defines all possible hook points.
pub trait ExecutionHook: Send {
    fn before_instruction(
        &mut self,
        _instruction: &Instruction,
        _context: &HookContext,
    ) -> HookDecision {
        HookDecision::Continue
    }

    fn after_instruction(
        &mut self,
        _instruction: &Instruction,
        _context: &HookContext,
    ) -> HookDecision {
        HookDecision::Continue
    }

    fn on_loop_enter(
        &mut self,
        _context: &HookContext,
    ) -> HookDecision {
        HookDecision::Continue
    }

    fn on_loop_exit(
        &mut self,
        _context: &HookContext,
    ) -> HookDecision {
        HookDecision::Continue
    }

    fn on_complete(&mut self, _context: &HookContext) {}
}

pub type BoxedHook = Box<dyn ExecutionHook>;
```

### 2. Hook Manager

```rust
pub struct HookManager {
    hooks: Vec<BoxedHook>,
    has_before_instruction: bool,
    has_after_instruction: bool,
    has_loop_hooks: bool,
}

impl HookManager {
    pub fn new() -> Self {
        Self {
            hooks: Vec::new(),
            has_before_instruction: false,
            has_after_instruction: false,
            has_loop_hooks: false,
        }
    }

    pub fn register(&mut self, hook: BoxedHook) {
        self.hooks.push(hook);
        self.has_before_instruction = true;
        self.has_after_instruction = true;
        self.has_loop_hooks = true;
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.hooks.is_empty()
    }

    #[inline]
    pub fn before_instruction(
        &mut self,
        instruction: &Instruction,
        context: &HookContext,
    ) -> HookDecision {
        if !self.has_before_instruction {
            return HookDecision::Continue;
        }

        for hook in &mut self.hooks {
            match hook.before_instruction(instruction, context) {
                HookDecision::Continue => continue,
                decision => return decision,
            }
        }
        HookDecision::Continue
    }

    // Similar for after_instruction, on_loop_enter, on_loop_exit...
}
```

### 3. Integration with ExecutionConfig

```rust
// In execution_config.rs
pub struct ExecutionConfig {
    // ... existing fields ...
    pub(crate) hook_manager: Option<HookManager>,
}

impl ExecutionConfigBuilder<Unbuilt> {
    pub fn with_hooks_enabled(mut self) -> Self {
        if self.hook_manager.is_none() {
            self.hook_manager = Some(HookManager::new());
        }
        self
    }

    pub fn with_hook(mut self, hook: BoxedHook) -> Self {
        self.hook_manager
            .get_or_insert_with(HookManager::new)
            .register(hook);
        self
    }
}
```

### 4. Integration with Interpreter

```rust
// In execute_block(), before executing instruction:
let context = HookContext::new(
    state.memory_slice(),
    state.pointer,
    state.step_count,
    state.current_source_location(),
    state.current_loop_depth(),
);

if let Some(ref mut hook_manager) = config.hook_manager {
    match hook_manager.before_instruction(instruction, &context) {
        HookDecision::Continue => {}
        HookDecision::Break => return Err(BfError::ExecutionPaused),
        HookDecision::Skip => continue,
    }
}

// ... execute instruction ...

// After instruction:
if let Some(ref mut hook_manager) = config.hook_manager {
    let context = HookContext::new(/* ... */);
    match hook_manager.after_instruction(instruction, &context) {
        HookDecision::Continue => {}
        HookDecision::Break => return Err(BfError::ExecutionPaused),
        HookDecision::Skip => {}
    }
}
```

---

## Built-in Hooks Library

### Instruction Counter

```rust
#[derive(Debug, Default)]
pub struct InstructionCounter {
    pub total: u64,
    pub by_type: HashMap<&'static str, u64>,
}

impl ExecutionHook for InstructionCounter {
    fn after_instruction(
        &mut self,
        instruction: &Instruction,
        _context: &HookContext,
    ) -> HookDecision {
        self.total += 1;
        let type_name = match instruction {
            Instruction::IncrementPointer => "increment_pointer",
            Instruction::DecrementPointer => "decrement_pointer",
            // ... etc
        };
        *self.by_type.entry(type_name).or_insert(0) += 1;
        HookDecision::Continue
    }
}
```

### Step Breakpoint

```rust
pub struct StepBreakpoint {
    pub target_step: u64,
    pub triggered: bool,
}

impl ExecutionHook for StepBreakpoint {
    fn before_instruction(
        &mut self,
        _instruction: &Instruction,
        context: &HookContext,
    ) -> HookDecision {
        if context.step_count().get() >= self.target_step && !self.triggered {
            self.triggered = true;
            HookDecision::Break
        } else {
            HookDecision::Continue
        }
    }
}
```

### Loop Profiler

```rust
#[derive(Debug, Default)]
pub struct LoopProfiler {
    pub loop_iterations: HashMap<usize, u64>,
    loop_stack: Vec<usize>,
}

impl ExecutionHook for LoopProfiler {
    fn on_loop_enter(&mut self, context: &HookContext) -> HookDecision {
        let loop_index = context.step_count().get() as usize;
        *self.loop_iterations.entry(loop_index).or_insert(0) += 1;
        self.loop_stack.push(loop_index);
        HookDecision::Continue
    }

    fn on_loop_exit(&mut self, _context: &HookContext) -> HookDecision {
        self.loop_stack.pop();
        HookDecision::Continue
    }
}
```

---

## Use Case Examples

### Use Case 1: Interactive Debugger

```rust
struct InteractiveDebugger {
    breakpoints: Vec<u64>,
    step_mode: bool,
}

impl InteractiveDebugger {
    fn interactive_prompt(&mut self, context: &HookContext) -> HookDecision {
        println!("\nBreakpoint at step {}", context.step_count().get());
        println!("Pointer: {}, Cell: {}",
            context.pointer().get(),
            context.current_cell()
        );

        loop {
            print!("(dbg) ");
            io::stdout().flush().unwrap();

            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();

            match input.trim() {
                "c" | "continue" => return HookDecision::Continue,
                "s" | "step" => {
                    self.step_mode = true;
                    return HookDecision::Continue;
                }
                "q" | "quit" => return HookDecision::Break,
                "mem" => self.print_memory(context),
                _ => println!("Commands: (c)ontinue, (s)tep, (q)uit, mem"),
            }
        }
    }
}

impl ExecutionHook for InteractiveDebugger {
    fn before_instruction(
        &mut self,
        _instruction: &Instruction,
        context: &HookContext,
    ) -> HookDecision {
        let step = context.step_count().get();
        if self.step_mode || self.breakpoints.contains(&step) {
            self.step_mode = false;
            self.interactive_prompt(context)
        } else {
            HookDecision::Continue
        }
    }
}
```

### Use Case 2: Execution Tracer

```rust
struct ExecutionTracer<W: std::io::Write> {
    writer: W,
    show_memory: bool,
}

impl<W: std::io::Write + Send> ExecutionHook for ExecutionTracer<W> {
    fn before_instruction(
        &mut self,
        instruction: &Instruction,
        context: &HookContext,
    ) -> HookDecision {
        let _ = writeln!(
            self.writer,
            "[{:08}] {:?}",
            context.step_count().get(),
            instruction
        );

        if self.show_memory {
            let _ = writeln!(
                self.writer,
                "  ptr={}, cell[ptr]={}",
                context.pointer().get(),
                context.current_cell()
            );
        }

        HookDecision::Continue
    }
}
```

### Use Case 3: Performance Profiler

```rust
struct PerformanceProfiler {
    instruction_times: HashMap<String, Duration>,
    current_start: Option<Instant>,
}

impl ExecutionHook for PerformanceProfiler {
    fn before_instruction(
        &mut self,
        _instruction: &Instruction,
        _context: &HookContext,
    ) -> HookDecision {
        self.current_start = Some(Instant::now());
        HookDecision::Continue
    }

    fn after_instruction(
        &mut self,
        instruction: &Instruction,
        _context: &HookContext,
    ) -> HookDecision {
        if let Some(start) = self.current_start.take() {
            let elapsed = start.elapsed();
            let instr_name = format!("{:?}", instruction);
            *self.instruction_times.entry(instr_name).or_insert(Duration::ZERO) += elapsed;
        }
        HookDecision::Continue
    }
}
```

---

## Implementation Roadmap

### Phase 0: Preparatory Refactoring ✅ COMPLETE

**Status**: Completed in commit `3e30d30`

- ✅ Added `loop_depth` field to `VmState`
- ✅ Extracted `execute_single_instruction()` function
- ✅ Added helper methods: `current_loop_depth()`, `memory_slice()`, `current_source_location()`
- ✅ All 124 tests passing
- ✅ Zero performance overhead

### Phase 1: Core Hook Infrastructure (2-3 weeks)

**Week 1: Hook Trait and Manager**
- [ ] Create `hooks.rs` module with core traits
- [ ] Implement `HookManager` with dispatch logic
- [ ] Add tests for hook registration and dispatch
- [ ] Document all public APIs

**Week 2: Interpreter Integration**
- [ ] Modify interpreter to call hooks at key points
- [ ] Add `hook_manager` field to `ExecutionConfig`
- [ ] Implement `ExecutionPaused` error type
- [ ] Add integration tests

**Week 3: Built-in Hooks**
- [ ] Implement 5-7 built-in hooks (counter, breakpoint, profiler, tracer)
- [ ] Write examples demonstrating each hook
- [ ] Performance benchmarks (with/without hooks)
- [ ] Documentation

### Phase 2: CLI Integration (1 week)

**Week 4: CLI Flags and Features**
- [ ] Add `--trace`, `--profile`, `--debug` flags
- [ ] Implement CLI hook setup based on flags
- [ ] Add `--interactive` mode for debugger
- [ ] Update CLI help and documentation

### Phase 3: Advanced Features (2-3 weeks)

**Week 5-6: Extension Instructions**
- [ ] Add support for `#` (debug dump)
- [ ] Add support for `@` (breakpoint)
- [ ] Implement `on_extension` hook point
- [ ] Update parser to recognize extensions

**Week 7: Polish and Documentation**
- [ ] Write comprehensive guide on writing hooks
- [ ] Add more built-in hooks based on feedback
- [ ] Performance optimization pass
- [ ] Final testing and validation

---

## Performance Considerations

### Zero-Cost When Disabled

When no hooks are registered (default case):

```rust
if let Some(ref mut hook_manager) = config.hook_manager {
    // Only executed if hooks present
}
```

This compiles to a simple pointer check. When `hook_manager` is `None`, the entire hook dispatch is optimized away.

### Optimization Strategies

1. **Inline Hook Checks**: Use `#[inline(always)]` for empty checks
2. **Branch Prediction**: Most common case (no hooks) is fast path
3. **Lazy Context Creation**: Only create `HookContext` if hooks registered
4. **Selective Hook Registration**: Track which hook points are active

### Performance Targets

**Without hooks (default)**:
- Overhead: < 0.1% (within noise)
- Memory: 0 bytes additional

**With hooks enabled**:
- Overhead: < 5% for lightweight hooks
- Memory: O(number of hooks) + O(context size per call)

---

## Related PRDs

This architecture enables features from multiple other PRDs:

- **optimization-and-advanced-features.md**
  - Section 2.1: Debug Command (`#`) → `on_extension` hook
  - Section 2.2: Breakpoint Instruction (`@`) → `StepBreakpoint` hook
  - Section 3.1: Interactive REPL → `InteractiveDebugger` hook
  - Section 3.2: Step-Through Debugger → Hook-based debugger
  - Section 3.5: Profiling → `PerformanceProfiler` hook

- **debug-symbols-and-runtime-diagnostics.md**
  - Phase 2: Loop Call Stack → `on_loop_enter`/`on_loop_exit` hooks
  - Phase 3: Execution Tracing → `ExecutionTracer` hook
  - Integration with existing debug symbols

- **performance-optimizations.md**
  - Section 6.2: Profiling Mode → Performance hooks
  - Optimization pass validation → Verification hooks

---

## Success Metrics

1. **Performance**: Zero overhead when hooks disabled (<0.1%)
2. **Performance**: <5% overhead with lightweight hooks enabled
3. **Usability**: Can implement interactive debugger in <100 lines
4. **Usability**: Can implement profiler in <50 lines
5. **Safety**: All hook APIs are type-safe (no `unsafe` needed for users)
6. **Documentation**: 100% of public APIs documented
7. **Examples**: 5+ working examples in `examples/hooks/`

---

## Conclusion

The proposed Plugin/Hook Architecture provides a powerful, type-safe, and performant way to extend FerrousCortex without modifying core interpreter logic. It enables a wide range of features including:

- Interactive debugging with breakpoints and watchpoints
- Performance profiling and hotspot detection
- Execution tracing and replay
- Custom instrumentation for research
- Language extensions (`#`, `@`, etc.)

The design prioritizes:
- **Zero overhead** when hooks are disabled (default)
- **Type safety** with compile-time guarantees
- **Composability** with support for multiple concurrent hooks
- **Ease of use** with clear APIs and comprehensive examples

**Phase 0 (Preparatory Refactoring)** is complete ✅, providing the necessary infrastructure for hook integration. The interpreter is now ready for Phase 1 implementation.
