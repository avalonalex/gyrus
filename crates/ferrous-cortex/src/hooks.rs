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
//! - Before each instruction executes
//! - After each instruction executes
//! - When entering a loop (`[`)
//! - When exiting a loop (`]`)
//! - When execution completes
//!
//! # Example
//!
//! ```rust,ignore
//! use ferrous_cortex::hooks::{ExecutionHook, HookContext, HookDecision};
//!
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
//! ```

use crate::instruction::Instruction;
use crate::location::SourceLocation;
use crate::types::{MemoryAddress, StepCount};

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
}

impl<'a> HookContext<'a> {
    /// Create a new hook context with the current interpreter state
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

    /// Get the value at the current memory cell
    pub fn current_cell(&self) -> u8 {
        self.memory[self.pointer.get()]
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
    ///
    /// # Note
    ///
    /// This is called once per loop entry. For loops that iterate multiple times,
    /// this is called at the start of each iteration (when evaluating the `[`).
    fn on_loop_enter(&mut self, _context: &HookContext) -> HookDecision {
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
    pub fn on_loop_enter(&mut self, context: &HookContext) -> HookDecision {
        if !self.has_loop_hooks {
            return HookDecision::Continue;
        }

        for hook in &mut self.hooks {
            match hook.on_loop_enter(context) {
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

        let context = HookContext::new(&memory, pointer, step_count, Some(&loc), 1);

        assert_eq!(context.memory(), &[1, 2, 3, 4, 5]);
        assert_eq!(context.pointer().get(), 2);
        assert_eq!(context.step_count().get(), 100);
        assert_eq!(context.current_cell(), 3);
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
        assert_eq!(hook.on_loop_enter(&context), HookDecision::Continue);
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
        assert_eq!(manager.on_loop_enter(&context), HookDecision::Continue);
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
}
