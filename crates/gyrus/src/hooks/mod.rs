//! Hook system for extensible execution observation and control
//!
//! This module provides a plugin architecture that allows external code to observe
//! and control BrainFuck program execution without modifying the core interpreter.
//!
//! # Design Goals
//!
//! - **Zero-cost when disabled**: No overhead if hooks are not used
//! - **Type-safe**: All hook APIs prevent misuse at compile time
//! - **Composable**: Multiple hooks can work together
//! - **Performance**: Fast dispatch when hooks are enabled
//!
//! # Hook Points
//!
//! Hooks can intercept execution at these points:
//! - **Before each instruction executes** - Inspect state, set breakpoints, skip instructions
//! - **After each instruction executes** - Track changes, collect statistics
//! - **When entering a loop (`[`)** - Monitor loop nesting, detect infinite loops
//! - **When exiting a loop (`]`)** - Track iteration counts, measure loop performance
//! - **When execution completes** - Generate reports, finalize statistics
//!
//! # Quick Start
//!
//! ## Basic Usage
//!
//! ```rust,ignore
//! use gyrus::{parse, ExecutionConfigBuilder, interpret_with_config};
//! use gyrus::hooks::{ExecutionHook, HookContext, HookDecision};
//!
//! // 1. Define your hook
//! struct InstructionCounter {
//!     count: u64,
//! }
//!
//! impl ExecutionHook for InstructionCounter {
//!     fn after_instruction(
//!         &mut self,
//!         _instruction: &Instruction,
//!         _context: &HookContext,
//!     ) -> HookDecision {
//!         self.count += 1;
//!         HookDecision::Continue
//!     }
//! }
//!
//! // 2. Register the hook with your config
//! let instructions = parse("+++[>++<-]")?;
//! let config = ExecutionConfigBuilder::new()
//!     .with_memory_size(100)
//!     .with_hook(Box::new(InstructionCounter { count: 0 }))
//!     .build();
//!
//! // 3. Run the interpreter
//! let stats = interpret_with_config(&instructions, config, None)?;
//! ```
//!
//! # Common Use Cases
//!
//! ## Debugger: Step Breakpoint
//!
//! Break execution at a specific step for interactive debugging:
//!
//! ```rust,ignore
//! struct StepBreakpoint {
//!     target_step: u64,
//! }
//!
//! impl ExecutionHook for StepBreakpoint {
//!     fn before_instruction(
//!         &mut self,
//!         _instruction: &Instruction,
//!         context: &HookContext,
//!     ) -> HookDecision {
//!         if context.step_count().get() >= self.target_step {
//!             HookDecision::Break  // Pause execution
//!         } else {
//!             HookDecision::Continue
//!         }
//!     }
//! }
//! ```
//!
//! ## Watchpoint: Memory Change Detector
//!
//! Pause execution when a specific memory cell changes:
//!
//! ```rust,ignore
//! struct MemoryWatchpoint {
//!     watch_address: usize,
//!     last_value: u8,
//! }
//!
//! impl ExecutionHook for MemoryWatchpoint {
//!     fn after_instruction(
//!         &mut self,
//!         _instruction: &Instruction,
//!         context: &HookContext,
//!     ) -> HookDecision {
//!         let current_value = context.memory()[self.watch_address];
//!         if current_value != self.last_value {
//!             self.last_value = current_value;
//!             HookDecision::Break  // Pause when value changes
//!         } else {
//!             HookDecision::Continue
//!         }
//!     }
//! }
//! ```
//!
//! ## Tracer: Execution Log
//!
//! Record every instruction executed with full state:
//!
//! ```rust,ignore
//! struct ExecutionTracer {
//!     log: Vec<String>,
//! }
//!
//! impl ExecutionHook for ExecutionTracer {
//!     fn after_instruction(
//!         &mut self,
//!         instruction: &Instruction,
//!         context: &HookContext,
//!     ) -> HookDecision {
//!         self.log.push(format!(
//!             "Step {}: {:?} @ cell {} = {}",
//!             context.step_count().get(),
//!             instruction,
//!             context.pointer().get(),
//!             context.current_cell()
//!         ));
//!         HookDecision::Continue
//!     }
//! }
//! ```
//!
//! ## Profiler: Loop Performance
//!
//! Measure how many iterations each loop executes:
//!
//! ```rust,ignore
//! struct LoopProfiler {
//!     loop_iterations: HashMap<usize, u64>,  // instruction_index -> count
//! }
//!
//! impl ExecutionHook for LoopProfiler {
//!     fn on_loop_enter(&mut self, context: &HookContext) -> HookDecision {
//!         if let Some(loc) = context.source_location() {
//!             *self.loop_iterations.entry(loc.offset).or_insert(0) += 1;
//!         }
//!         HookDecision::Continue
//!     }
//! }
//! ```
//!
//! # Hook Decisions
//!
//! Hooks return a [`HookDecision`] to control execution flow:
//!
//! - **`Continue`** - Normal execution continues
//! - **`Break`** - Pause execution (returns `BfError::ExecutionPaused`)
//! - **`Skip`** - Skip current instruction (only valid in `before_instruction`)
//!
//! ## Example: Instruction Filter
//!
//! Skip all decrement instructions:
//!
//! ```rust,ignore
//! impl ExecutionHook for DecrementSkipper {
//!     fn before_instruction(
//!         &mut self,
//!         instruction: &Instruction,
//!         _context: &HookContext,
//!     ) -> HookDecision {
//!         if matches!(instruction, Instruction::DecrementValue) {
//!             HookDecision::Skip  // Skip this instruction
//!         } else {
//!             HookDecision::Continue
//!         }
//!     }
//! }
//! ```
//!
//! # Hook Context
//!
//! The [`HookContext`] provides immutable access to interpreter state:
//!
//! - `memory()` - View entire memory array
//! - `pointer()` - Current pointer position
//! - `current_cell()` - Value at current pointer
//! - `step_count()` - Total instructions executed
//! - `loop_depth()` - Current loop nesting level
//! - `source_location()` - Source code location (if debug info available)
//!
//! # Multiple Hooks
//!
//! Register multiple hooks to combine functionality:
//!
//! ```rust,ignore
//! let config = ExecutionConfigBuilder::new()
//!     .with_memory_size(100)
//!     .with_hook(Box::new(StepBreakpoint { target_step: 100 }))
//!     .with_hook(Box::new(InstructionCounter { count: 0 }))
//!     .with_hook(Box::new(ExecutionTracer { log: Vec::new() }))
//!     .build();
//! ```
//!
//! Hooks are called in registration order. If any hook returns `Break`, remaining
//! hooks are not called and execution pauses immediately.
//!
//! # Performance
//!
//! When no hooks are registered (`Option<HookManager>` is `None`):
//! - **Zero overhead** - No performance impact
//! - Compiler can optimize away all hook checks
//!
//! When hooks are registered:
//! - Fast dispatch via vtable
//! - Early-exit optimization (stops on first `Break`)
//! - Minimal overhead per instruction (~10-20ns)
//!
//! # Thread Safety
//!
//! Hooks must implement `Send` to support potential future multi-threaded execution.
//! Use `Arc<Mutex<T>>` for shared state between hooks and main thread.
//!
//! # See Also
//!
//! - [`ExecutionHook`] - Main trait to implement
//! - [`HookManager`] - Manages multiple hooks
//! - [`HookContext`] - Immutable state snapshot
//! - [`HookDecision`] - Execution control enum

pub mod builtin;

use crate::instruction::Instruction;
use crate::location::SourceLocation;
use crate::types::{MemoryAddress, StepCount};

/// Loop structural information passed to hooks when entering a loop.
///
/// This provides the structural details about a loop that hooks need
/// for accurate tracking and debugging, especially for building loop call stacks.
#[derive(Debug, Clone, Copy)]
pub struct LoopInfo {
    /// Flat index of the '[' instruction that starts this loop
    pub loop_instruction_index: usize,
    /// Flat index of the first instruction inside the loop body
    pub body_start_index: usize,
    /// Number of instructions in the loop body
    pub body_size: usize,
}

impl LoopInfo {
    /// Create new loop information
    pub fn new(loop_instruction_index: usize, body_start_index: usize, body_size: usize) -> Self {
        Self {
            loop_instruction_index,
            body_start_index,
            body_size,
        }
    }
}

/// Immutable snapshot of interpreter state exposed to hooks.
///
/// This provides a read-only view of the interpreter's current state,
/// allowing hooks to inspect execution without modifying it directly.
///
/// # Design Note
///
/// HookContext is deliberately immutable. Hooks observe and control execution
/// through `HookDecision` return values, not by mutating state directly.
#[derive(Debug)]
pub struct HookContext<'a> {
    /// View of the entire memory array
    memory: &'a [u8],
    /// Current memory pointer position
    pointer: MemoryAddress,
    /// Current step count (total instructions executed)
    step_count: StepCount,
    /// Source location of current instruction (if debug info available)
    source_location: Option<&'a SourceLocation>,
    /// Current loop nesting depth (0 = not in loop)
    loop_depth: usize,
    /// Current position in the flat instruction list (0-indexed)
    ///
    /// This tracks the position in the flattened instruction stream,
    /// which is useful for debug symbol lookups and error reporting.
    instruction_index: usize,
}

impl<'a> HookContext<'a> {
    /// Create a new hook context with the current interpreter state
    pub fn new(
        memory: &'a [u8],
        pointer: MemoryAddress,
        step_count: StepCount,
        source_location: Option<&'a SourceLocation>,
        loop_depth: usize,
        instruction_index: usize,
    ) -> Self {
        Self {
            memory,
            pointer,
            step_count,
            source_location,
            loop_depth,
            instruction_index,
        }
    }

    /// Get a view of the entire memory array
    pub fn memory(&self) -> &[u8] {
        self.memory
    }

    /// Get the current memory pointer position
    pub fn pointer(&self) -> MemoryAddress {
        self.pointer
    }

    /// Get the current step count (total instructions executed so far)
    pub fn step_count(&self) -> StepCount {
        self.step_count
    }

    /// Get the value at the current memory cell.
    ///
    /// Returns `None` when the cursor is off the tape. Under the tape contract
    /// that is a legal position -- it is only using it that would be an error --
    /// so a hook observing one gets no value rather than a panic.
    pub fn current_cell(&self) -> Option<u8> {
        self.pointer
            .index(self.memory.len())
            .map(|idx| self.memory[idx])
    }

    /// Get the source location of the current instruction, if available
    ///
    /// Returns `None` if the program was executed without debug symbols.
    pub fn source_location(&self) -> Option<&SourceLocation> {
        self.source_location
    }

    /// Get the current loop nesting depth
    ///
    /// Returns 0 if not currently inside any loop, 1 for the outermost loop, etc.
    pub fn loop_depth(&self) -> usize {
        self.loop_depth
    }

    /// Get the current instruction index in the flat instruction list
    ///
    /// This is the position in the flattened instruction stream (0-indexed),
    /// useful for debug symbol lookups and accurate error reporting.
    pub fn instruction_index(&self) -> usize {
        self.instruction_index
    }
}

/// Decision returned by hooks to control execution flow.
///
/// Hooks return this to indicate what the interpreter should do after
/// the hook completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookDecision {
    /// Continue execution normally
    Continue,

    /// Pause execution (for debugger/breakpoint)
    ///
    /// The interpreter will stop and return `BfError::ExecutionPaused`.
    /// The caller can then inspect state, modify configuration, and resume.
    Break,

    /// Skip the current instruction
    ///
    /// Only valid for `before_instruction` hook. The instruction will not
    /// be executed, but execution continues with the next instruction.
    Skip,
}

/// Main hook trait that defines all possible hook points.
///
/// Implement this trait to create custom execution hooks. All methods have
/// default implementations that return `HookDecision::Continue`, so you only
/// need to implement the hook points you care about.
///
/// # Thread Safety
///
/// Hooks must be `Send` to allow potential future multi-threaded execution.
/// Most hooks will be naturally `Send` unless they contain `Rc` or other
/// non-thread-safe types.
///
/// # Example: Instruction Counter
///
/// ```rust,ignore
/// struct InstructionCounter {
///     total: u64,
/// }
///
/// impl ExecutionHook for InstructionCounter {
///     fn after_instruction(
///         &mut self,
///         _instruction: &Instruction,
///         _context: &HookContext,
///     ) -> HookDecision {
///         self.total += 1;
///         HookDecision::Continue
///     }
/// }
/// ```
///
/// # Example: Step Breakpoint
///
/// ```rust,ignore
/// struct StepBreakpoint {
///     target_step: u64,
/// }
///
/// impl ExecutionHook for StepBreakpoint {
///     fn before_instruction(
///         &mut self,
///         _instruction: &Instruction,
///         context: &HookContext,
///     ) -> HookDecision {
///         if context.step_count().get() >= self.target_step {
///             HookDecision::Break
///         } else {
///             HookDecision::Continue
///         }
///     }
/// }
/// ```
pub trait ExecutionHook: Send {
    /// Called before executing each instruction
    ///
    /// # Parameters
    ///
    /// - `instruction`: The instruction about to be executed
    /// - `context`: Current interpreter state
    ///
    /// # Returns
    ///
    /// - `Continue`: Execute the instruction normally
    /// - `Break`: Pause execution (return `ExecutionPaused` error)
    /// - `Skip`: Don't execute this instruction, continue to next
    fn before_instruction(
        &mut self,
        _instruction: &Instruction,
        _context: &HookContext,
    ) -> HookDecision {
        HookDecision::Continue
    }

    /// Called after executing each instruction
    ///
    /// # Parameters
    ///
    /// - `instruction`: The instruction that just executed
    /// - `context`: Current interpreter state (after execution)
    ///
    /// # Returns
    ///
    /// - `Continue`: Continue execution normally
    /// - `Break`: Pause execution (return `ExecutionPaused` error)
    /// - `Skip`: No effect (instruction already executed)
    fn after_instruction(
        &mut self,
        _instruction: &Instruction,
        _context: &HookContext,
    ) -> HookDecision {
        HookDecision::Continue
    }

    /// Called when entering a loop (executing `[`)
    ///
    /// # Parameters
    ///
    /// - `context`: Current interpreter state at loop entry
    /// - `loop_info`: Structural information about this loop (indices and size), if available
    ///   Only present when debug symbols are enabled.
    ///
    /// # Note
    ///
    /// This is called once per loop entry. For loops that iterate multiple times,
    /// this is called at the start of each iteration (when evaluating the `[`).
    fn on_loop_enter(
        &mut self,
        _context: &HookContext,
        _loop_info: Option<&LoopInfo>,
    ) -> HookDecision {
        HookDecision::Continue
    }

    /// Called when exiting a loop (executing `]`)
    ///
    /// # Parameters
    ///
    /// - `context`: Current interpreter state at loop exit
    ///
    /// # Note
    ///
    /// This is called when the loop condition is false (cell is 0) and execution
    /// continues past the loop.
    fn on_loop_exit(&mut self, _context: &HookContext) -> HookDecision {
        HookDecision::Continue
    }

    /// Called when program execution completes successfully
    ///
    /// # Parameters
    ///
    /// - `context`: Final interpreter state
    ///
    /// # Note
    ///
    /// This is not called if execution is paused (via `Break`) or errors occur.
    fn on_complete(&mut self, _context: &HookContext) {}
}

/// Type alias for boxed hooks
///
/// This is the standard way to store hooks in collections, since they're
/// trait objects with different concrete types.
pub type BoxedHook = Box<dyn ExecutionHook>;

/// Manager for dispatching execution events to registered hooks.
///
/// The `HookManager` orchestrates multiple hooks, calling them at appropriate
/// points during execution and aggregating their decisions.
///
/// # Performance
///
/// - When empty: O(1) check via `is_empty()`
/// - When populated: O(n) where n = number of registered hooks
/// - Early exit: Stops calling hooks if any returns `Break`
///
/// # Example
///
/// ```rust,ignore
/// let mut manager = HookManager::new();
/// manager.register(Box::new(InstructionCounter::new()));
/// manager.register(Box::new(StepBreakpoint::new(1000)));
///
/// // During execution:
/// let decision = manager.before_instruction(&instruction, &context);
/// if decision == HookDecision::Break {
///     // Pause execution
/// }
/// ```
pub struct HookManager {
    hooks: Vec<BoxedHook>,
    // Optimization flags - track which hook points are actually used
    has_before_instruction: bool,
    has_after_instruction: bool,
    has_loop_hooks: bool,
}

impl std::fmt::Debug for HookManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookManager")
            .field("hook_count", &self.hooks.len())
            .field("has_before_instruction", &self.has_before_instruction)
            .field("has_after_instruction", &self.has_after_instruction)
            .field("has_loop_hooks", &self.has_loop_hooks)
            .finish()
    }
}

impl Default for HookManager {
    fn default() -> Self {
        Self::new()
    }
}

impl HookManager {
    /// Create a new empty hook manager
    pub fn new() -> Self {
        Self {
            hooks: Vec::new(),
            has_before_instruction: false,
            has_after_instruction: false,
            has_loop_hooks: false,
        }
    }

    /// Register a new hook
    ///
    /// The hook will be called for all future execution events.
    /// Hooks are called in registration order.
    pub fn register(&mut self, hook: BoxedHook) {
        self.hooks.push(hook);
        // Conservatively mark all hook points as active
        // TODO: In the future, we could introspect the hook to see which
        // methods it overrides and only set the relevant flags
        self.has_before_instruction = true;
        self.has_after_instruction = true;
        self.has_loop_hooks = true;
    }

    /// Check if the manager has any registered hooks
    ///
    /// This is used for fast-path optimization when no hooks are registered.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.hooks.is_empty()
    }

    /// Call all hooks before executing an instruction
    ///
    /// Returns the aggregated decision from all hooks. If any hook returns
    /// `Break`, that decision is returned immediately without calling remaining hooks.
    ///
    /// # Performance
    ///
    /// Uses `has_before_instruction` flag for early exit when no hooks
    /// implement this method (though currently we conservatively assume all do).
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
                decision => return decision, // Break or Skip
            }
        }
        HookDecision::Continue
    }

    /// Call all hooks after executing an instruction
    ///
    /// Returns the aggregated decision from all hooks. If any hook returns
    /// `Break`, that decision is returned immediately without calling remaining hooks.
    #[inline]
    pub fn after_instruction(
        &mut self,
        instruction: &Instruction,
        context: &HookContext,
    ) -> HookDecision {
        if !self.has_after_instruction {
            return HookDecision::Continue;
        }

        for hook in &mut self.hooks {
            match hook.after_instruction(instruction, context) {
                HookDecision::Continue => continue,
                decision => return decision,
            }
        }
        HookDecision::Continue
    }

    /// Call all hooks when entering a loop
    ///
    /// Returns the aggregated decision from all hooks. If any hook returns
    /// `Break`, that decision is returned immediately without calling remaining hooks.
    #[inline]
    pub fn on_loop_enter(
        &mut self,
        context: &HookContext,
        loop_info: Option<&LoopInfo>,
    ) -> HookDecision {
        if !self.has_loop_hooks {
            return HookDecision::Continue;
        }

        for hook in &mut self.hooks {
            match hook.on_loop_enter(context, loop_info) {
                HookDecision::Continue => continue,
                decision => return decision,
            }
        }
        HookDecision::Continue
    }

    /// Call all hooks when exiting a loop
    ///
    /// Returns the aggregated decision from all hooks. If any hook returns
    /// `Break`, that decision is returned immediately without calling remaining hooks.
    #[inline]
    pub fn on_loop_exit(&mut self, context: &HookContext) -> HookDecision {
        if !self.has_loop_hooks {
            return HookDecision::Continue;
        }

        for hook in &mut self.hooks {
            match hook.on_loop_exit(context) {
                HookDecision::Continue => continue,
                decision => return decision,
            }
        }
        HookDecision::Continue
    }

    /// Call all hooks when execution completes
    ///
    /// This is called only on successful completion (not on errors or pauses).
    pub fn on_complete(&mut self, context: &HookContext) {
        for hook in &mut self.hooks {
            hook.on_complete(context);
        }
    }

    /// Get the number of registered hooks
    pub fn len(&self) -> usize {
        self.hooks.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_context_creation() {
        let memory = vec![1, 2, 3, 4, 5];
        let pointer = MemoryAddress::new(2);
        let step_count = StepCount::new(100);
        let loc = SourceLocation::new(1, 5, 4);

        let context = HookContext::new(&memory, pointer, step_count, Some(&loc), 1, 0);

        assert_eq!(context.memory(), &[1, 2, 3, 4, 5]);
        assert_eq!(context.pointer().get(), 2);
        assert_eq!(context.step_count().get(), 100);
        assert_eq!(context.current_cell(), Some(3));
        assert_eq!(context.loop_depth(), 1);
        assert!(context.source_location().is_some());
    }

    #[test]
    fn test_hook_decision_variants() {
        assert_eq!(HookDecision::Continue, HookDecision::Continue);
        assert_ne!(HookDecision::Continue, HookDecision::Break);
        assert_ne!(HookDecision::Continue, HookDecision::Skip);
    }

    // Test that a minimal hook compiles
    struct NoOpHook;

    impl ExecutionHook for NoOpHook {
        // Use all default implementations
    }

    #[test]
    fn test_noop_hook_compiles() {
        let mut hook = NoOpHook;
        let memory = vec![0; 10];
        let context = HookContext::new(
            &memory,
            MemoryAddress::new(0),
            StepCount::new(0),
            None,
            0,
            0,
        );

        // Should all return Continue by default
        assert_eq!(
            hook.before_instruction(&Instruction::IncrementValue, &context),
            HookDecision::Continue
        );
        assert_eq!(
            hook.after_instruction(&Instruction::IncrementValue, &context),
            HookDecision::Continue
        );
        assert_eq!(hook.on_loop_enter(&context, None), HookDecision::Continue);
        assert_eq!(hook.on_loop_exit(&context), HookDecision::Continue);
        hook.on_complete(&context); // Doesn't return anything
    }

    // Tests for HookManager

    #[test]
    fn test_hook_manager_empty() {
        let mut manager = HookManager::new();
        assert!(manager.is_empty());
        assert_eq!(manager.len(), 0);

        let memory = vec![0; 10];
        let context = HookContext::new(
            &memory,
            MemoryAddress::new(0),
            StepCount::new(0),
            None,
            0,
            0,
        );

        // Empty manager should always return Continue
        assert_eq!(
            manager.before_instruction(&Instruction::IncrementValue, &context),
            HookDecision::Continue
        );
        assert_eq!(
            manager.after_instruction(&Instruction::IncrementValue, &context),
            HookDecision::Continue
        );
        assert_eq!(
            manager.on_loop_enter(&context, None),
            HookDecision::Continue
        );
        assert_eq!(manager.on_loop_exit(&context), HookDecision::Continue);
        manager.on_complete(&context); // Shouldn't panic
    }

    #[test]
    fn test_hook_manager_single_hook() {
        let mut manager = HookManager::new();
        manager.register(Box::new(NoOpHook));

        assert!(!manager.is_empty());
        assert_eq!(manager.len(), 1);
    }

    // Test hook that counts how many times it's called
    struct CountingHook {
        before_count: usize,
        after_count: usize,
    }

    impl ExecutionHook for CountingHook {
        fn before_instruction(
            &mut self,
            _instruction: &Instruction,
            _context: &HookContext,
        ) -> HookDecision {
            self.before_count += 1;
            HookDecision::Continue
        }

        fn after_instruction(
            &mut self,
            _instruction: &Instruction,
            _context: &HookContext,
        ) -> HookDecision {
            self.after_count += 1;
            HookDecision::Continue
        }
    }

    #[test]
    fn test_hook_manager_calls_hooks() {
        let mut manager = HookManager::new();
        let hook = Box::new(CountingHook {
            before_count: 0,
            after_count: 0,
        });
        manager.register(hook);

        let memory = vec![0; 10];
        let context = HookContext::new(
            &memory,
            MemoryAddress::new(0),
            StepCount::new(0),
            None,
            0,
            0,
        );

        manager.before_instruction(&Instruction::IncrementValue, &context);
        manager.after_instruction(&Instruction::IncrementValue, &context);

        // Note: We can't easily check the counts since hooks are moved into
        // the manager, but at least we verify it doesn't panic
    }

    // Test hook that breaks on specific step
    struct BreakpointHook {
        target_step: u64,
    }

    impl ExecutionHook for BreakpointHook {
        fn before_instruction(
            &mut self,
            _instruction: &Instruction,
            context: &HookContext,
        ) -> HookDecision {
            if context.step_count().get() >= self.target_step {
                HookDecision::Break
            } else {
                HookDecision::Continue
            }
        }
    }

    #[test]
    fn test_hook_manager_early_exit_on_break() {
        let mut manager = HookManager::new();
        manager.register(Box::new(BreakpointHook { target_step: 5 }));

        let memory = vec![0; 10];

        // Before target step - should continue
        let context = HookContext::new(
            &memory,
            MemoryAddress::new(0),
            StepCount::new(3),
            None,
            0,
            0,
        );
        assert_eq!(
            manager.before_instruction(&Instruction::IncrementValue, &context),
            HookDecision::Continue
        );

        // At target step - should break
        let context = HookContext::new(
            &memory,
            MemoryAddress::new(0),
            StepCount::new(5),
            None,
            0,
            0,
        );
        assert_eq!(
            manager.before_instruction(&Instruction::IncrementValue, &context),
            HookDecision::Break
        );
    }

    #[test]
    fn test_hook_manager_multiple_hooks() {
        let mut manager = HookManager::new();
        manager.register(Box::new(NoOpHook));
        manager.register(Box::new(NoOpHook));
        manager.register(Box::new(NoOpHook));

        assert_eq!(manager.len(), 3);
    }

    // Integration tests with interpreter
    #[cfg(test)]
    mod integration_tests {
        use super::*;
        use crate::config::ExecutionConfigBuilder;
        use crate::error::BfError;
        use crate::interpreter::interpret_with_config;
        use crate::parser::parse;
        use std::sync::{Arc, Mutex};

        // Hook that counts total instructions executed
        struct InstructionCounter {
            count: Arc<Mutex<usize>>,
        }

        impl ExecutionHook for InstructionCounter {
            fn after_instruction(
                &mut self,
                _instruction: &Instruction,
                _context: &HookContext,
            ) -> HookDecision {
                *self.count.lock().unwrap() += 1;
                HookDecision::Continue
            }
        }

        #[test]
        fn test_hook_counts_instructions() {
            let source = "+++>>--"; // 7 instructions (3+, 2>, 2-)
            let instructions = parse(source).unwrap();

            let count = Arc::new(Mutex::new(0));
            let config = ExecutionConfigBuilder::new()
                .with_memory_size(100)
                .with_hook(Box::new(InstructionCounter {
                    count: count.clone(),
                }))
                .build();

            let result = interpret_with_config(&instructions, config, None);
            assert!(result.is_ok());
            assert_eq!(*count.lock().unwrap(), 7);
        }

        // Hook that breaks execution at a specific step
        struct StepBreakpoint {
            target_step: u64,
        }

        impl ExecutionHook for StepBreakpoint {
            fn before_instruction(
                &mut self,
                _instruction: &Instruction,
                context: &HookContext,
            ) -> HookDecision {
                if context.step_count().get() >= self.target_step {
                    HookDecision::Break
                } else {
                    HookDecision::Continue
                }
            }
        }

        #[test]
        fn test_hook_pauses_execution() {
            let source = "++++++++++"; // 10 instructions
            let instructions = parse(source).unwrap();

            let config = ExecutionConfigBuilder::new()
                .with_memory_size(100)
                .with_hook(Box::new(StepBreakpoint { target_step: 5 }))
                .build();

            let result = interpret_with_config(&instructions, config, None);
            assert!(matches!(result, Err(BfError::ExecutionPaused { .. })));

            if let Err(BfError::ExecutionPaused {
                instruction_index, ..
            }) = result
            {
                // Should have paused at step 5
                assert_eq!(instruction_index.get(), 5);
            }
        }

        // Hook that tracks memory changes
        struct MemoryWatcher {
            changes: Arc<Mutex<Vec<(isize, u8, u8)>>>, // (cursor, old_value, new_value)
        }

        impl ExecutionHook for MemoryWatcher {
            fn after_instruction(
                &mut self,
                instruction: &Instruction,
                context: &HookContext,
            ) -> HookDecision {
                // Only track increment/decrement
                if matches!(
                    instruction,
                    Instruction::IncrementValue | Instruction::DecrementValue
                ) {
                    let addr = context.pointer().get();
                    // None when the cursor is off the tape, which a watcher has
                    // nothing to say about.
                    if let Some(new_value) = context.current_cell() {
                        // Calculate old value based on instruction
                        let old_value = match instruction {
                            Instruction::IncrementValue => new_value.wrapping_sub(1),
                            Instruction::DecrementValue => new_value.wrapping_add(1),
                            _ => new_value,
                        };
                        self.changes
                            .lock()
                            .unwrap()
                            .push((addr, old_value, new_value));
                    }
                }
                HookDecision::Continue
            }
        }

        #[test]
        fn test_hook_watches_memory() {
            let source = "+++"; // Increment cell 0 three times
            let instructions = parse(source).unwrap();

            let changes = Arc::new(Mutex::new(Vec::new()));
            let config = ExecutionConfigBuilder::new()
                .with_memory_size(100)
                .with_hook(Box::new(MemoryWatcher {
                    changes: changes.clone(),
                }))
                .build();

            let result = interpret_with_config(&instructions, config, None);
            assert!(result.is_ok());

            let changes = changes.lock().unwrap();
            assert_eq!(changes.len(), 3);
            assert_eq!(changes[0], (0, 0, 1));
            assert_eq!(changes[1], (0, 1, 2));
            assert_eq!(changes[2], (0, 2, 3));
        }

        // Hook that tracks loop execution
        struct LoopTracker {
            loop_entries: Arc<Mutex<usize>>,
            loop_exits: Arc<Mutex<usize>>,
        }

        impl ExecutionHook for LoopTracker {
            fn on_loop_enter(
                &mut self,
                _context: &HookContext,
                _loop_info: Option<&LoopInfo>,
            ) -> HookDecision {
                *self.loop_entries.lock().unwrap() += 1;
                HookDecision::Continue
            }

            fn on_loop_exit(&mut self, _context: &HookContext) -> HookDecision {
                *self.loop_exits.lock().unwrap() += 1;
                HookDecision::Continue
            }
        }

        #[test]
        fn test_hook_tracks_loops() {
            let source = "+++[>++<-]"; // Loop that runs 3 times
            let instructions = parse(source).unwrap();

            let entries = Arc::new(Mutex::new(0));
            let exits = Arc::new(Mutex::new(0));
            let config = ExecutionConfigBuilder::new()
                .with_memory_size(100)
                .with_hook(Box::new(LoopTracker {
                    loop_entries: entries.clone(),
                    loop_exits: exits.clone(),
                }))
                .build();

            let result = interpret_with_config(&instructions, config, None);
            assert!(result.is_ok());

            assert_eq!(*entries.lock().unwrap(), 3); // Loop body entered 3 times
            assert_eq!(*exits.lock().unwrap(), 3); // Loop body exited 3 times
        }

        // Hook that skips specific instructions
        struct InstructionSkipper {
            skip_decrements: bool,
        }

        impl ExecutionHook for InstructionSkipper {
            fn before_instruction(
                &mut self,
                instruction: &Instruction,
                _context: &HookContext,
            ) -> HookDecision {
                if self.skip_decrements && matches!(instruction, Instruction::DecrementValue) {
                    HookDecision::Skip
                } else {
                    HookDecision::Continue
                }
            }
        }

        #[test]
        fn test_hook_skips_instructions() {
            let source = "+++--"; // 3 increments, 2 decrements
            let instructions = parse(source).unwrap();

            let config = ExecutionConfigBuilder::new()
                .with_memory_size(100)
                .with_hook(Box::new(InstructionSkipper {
                    skip_decrements: true,
                }))
                .build();

            let result = interpret_with_config(&instructions, config, None);
            assert!(result.is_ok());

            // Memory should have 3 (decrements were skipped)
            let stats = result.unwrap();
            assert_eq!(stats.total_steps.get(), 5); // All steps counted
        }

        // Hook that tracks completion
        struct CompletionTracker {
            completed: Arc<Mutex<bool>>,
        }

        impl ExecutionHook for CompletionTracker {
            fn on_complete(&mut self, _context: &HookContext) {
                *self.completed.lock().unwrap() = true;
            }
        }

        #[test]
        fn test_hook_on_complete() {
            let source = "+++";
            let instructions = parse(source).unwrap();

            let completed = Arc::new(Mutex::new(false));
            let config = ExecutionConfigBuilder::new()
                .with_memory_size(100)
                .with_hook(Box::new(CompletionTracker {
                    completed: completed.clone(),
                }))
                .build();

            let result = interpret_with_config(&instructions, config, None);
            assert!(result.is_ok());
            assert!(*completed.lock().unwrap()); // Should be called on success
        }

        #[test]
        fn test_hook_not_called_on_pause() {
            let source = "+++";
            let instructions = parse(source).unwrap();

            let completed = Arc::new(Mutex::new(false));
            let config = ExecutionConfigBuilder::new()
                .with_memory_size(100)
                .with_hook(Box::new(CompletionTracker {
                    completed: completed.clone(),
                }))
                .with_hook(Box::new(StepBreakpoint { target_step: 2 }))
                .build();

            let result = interpret_with_config(&instructions, config, None);
            assert!(matches!(result, Err(BfError::ExecutionPaused { .. })));
            assert!(!*completed.lock().unwrap()); // Should NOT be called when paused
        }

        // Test multiple hooks working together
        #[test]
        fn test_multiple_hooks_together() {
            let source = "+++>>--";
            let instructions = parse(source).unwrap();

            let count = Arc::new(Mutex::new(0));
            let changes = Arc::new(Mutex::new(Vec::new()));

            let config = ExecutionConfigBuilder::new()
                .with_memory_size(100)
                .with_hook(Box::new(InstructionCounter {
                    count: count.clone(),
                }))
                .with_hook(Box::new(MemoryWatcher {
                    changes: changes.clone(),
                }))
                .build();

            let result = interpret_with_config(&instructions, config, None);
            assert!(result.is_ok());

            assert_eq!(*count.lock().unwrap(), 7); // 7 instructions
            assert_eq!(changes.lock().unwrap().len(), 5); // 3 increments + 2 decrements tracked
        }
    }
}
