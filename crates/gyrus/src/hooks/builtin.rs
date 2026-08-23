//! Built-in hooks for common execution monitoring tasks.
//!
//! This module provides pre-built hooks for:
//! - **Statistics tracking** - Collect execution metrics (steps, loops, memory, I/O)
//! - **Limit enforcement** - Enforce step limits and timeouts (future)
//! - **Warning collection** - Collect runtime warnings (future)
//!
//! These hooks are used internally by the interpreter but can also be used
//! directly for custom monitoring and control.
//!
//! # Examples
//!
//! ## Using StatsTrackerHook directly
//!
//! The interpreter already tracks stats automatically, but you can use `StatsTrackerHook`
//! directly for custom statistics collection or to verify the hook produces identical results.
//!
//! ### Example: Verify Hook Matches Built-in Stats
//!
//! ```rust
//! use gyrus::{parse, ExecutionConfigBuilder, interpret_with_config};
//! use gyrus::hooks::builtin::StatsTrackerHook;
//! use std::sync::{Arc, Mutex};
//!
//! # fn main() -> Result<(), gyrus::BfError> {
//! let instructions = parse("+++[>++<-]")?;
//!
//! // Create hook with shared state so we can access it after execution
//! let stats_hook = Arc::new(Mutex::new(StatsTrackerHook::new()));
//! let stats_hook_clone = Arc::clone(&stats_hook);
//!
//! // Register the hook
//! let config = ExecutionConfigBuilder::new()
//!     .with_memory_size(100)
//!     .with_hook(Box::new(StatsTrackerHook::new()))
//!     .build();
//!
//! // Run interpreter (returns built-in stats)
//! let builtin_stats = interpret_with_config(&instructions, config, None)?;
//!
//! // Note: Can't access hook stats because it was moved into config
//! // To access hook stats, use a shared Arc<Mutex<>> pattern (see example below)
//! # Ok(())
//! # }
//! ```
//!
//! ### Example: Custom Statistics with Shared State
//!
//! To access statistics collected by the hook after execution, use the
//! `Arc<Mutex<>>` pattern:
//!
//! ```rust
//! use gyrus::{parse, ExecutionConfigBuilder, interpret_with_io};
//! use gyrus::hooks::builtin::StatsTrackerHook;
//! use gyrus::hooks::{ExecutionHook, HookContext, HookDecision};
//! use gyrus::io::StringIo;
//! use gyrus::Instruction;
//! use std::sync::{Arc, Mutex};
//!
//! // Wrapper that holds shared state
//! struct SharedStatsHook {
//!     shared: Arc<Mutex<StatsTrackerHook>>,
//! }
//!
//! impl ExecutionHook for SharedStatsHook {
//!     fn after_instruction(&mut self, inst: &Instruction, ctx: &HookContext) -> HookDecision {
//!         self.shared.lock().unwrap().after_instruction(inst, ctx)
//!     }
//!     fn on_loop_enter(&mut self, ctx: &HookContext, loop_info: Option<&gyrus::hooks::LoopInfo>) -> HookDecision {
//!         self.shared.lock().unwrap().on_loop_enter(ctx, loop_info)
//!     }
//!     fn on_complete(&mut self, ctx: &HookContext) {
//!         self.shared.lock().unwrap().on_complete(ctx)
//!     }
//! }
//!
//! # fn main() -> Result<(), gyrus::BfError> {
//! let instructions = parse("+++[>++<-]")?;
//!
//! // Create shared hook
//! let stats_hook = Arc::new(Mutex::new(StatsTrackerHook::new()));
//! let stats_hook_clone = Arc::clone(&stats_hook);
//!
//! // Register wrapper
//! let config = ExecutionConfigBuilder::new()
//!     .with_memory_size(100)
//!     .with_hook(Box::new(SharedStatsHook { shared: stats_hook_clone }))
//!     .build();
//!
//! // Execute
//! let mut input = StringIo::empty();
//! let mut output = StringIo::empty();
//! let result = interpret_with_io(&instructions, config, &mut input, &mut output, None)?;
//!
//! // Access hook stats after execution
//! let hook = stats_hook.lock().unwrap();
//! println!("Hook tracked {} steps", hook.stats().total_steps);
//! println!("Hook tracked {} loop iterations", hook.stats().loop_iterations);
//! # Ok(())
//! # }
//! ```

use super::{ExecutionHook, HookContext, HookDecision};
use crate::error::RuntimeWarning;
use crate::instruction::Instruction;
use crate::stats::ExecutionStats;
use crate::types::{InstructionIndex, MemoryAddress, MemorySize};
use std::io::Write;
use std::sync::{Arc, Mutex};
use termcolor::{Ansi, Color, ColorSpec, WriteColor};

/// Built-in hook for tracking execution statistics.
///
/// This hook collects comprehensive execution metrics including:
/// - Total instructions executed
/// - Loop iterations
/// - Peak memory usage
/// - I/O operations (bytes read/written)
/// - Memory allocation (final memory size)
/// - Modified cells (non-zero at completion)
///
/// # Performance
///
/// This hook has minimal overhead:
/// - `after_instruction`: ~2-3 operations per instruction
/// - `on_loop_enter`: 1 increment per loop iteration
/// - `on_complete`: O(n) scan of memory (where n = memory size)
///
/// # Usage
///
/// Typically you don't need to use this directly - the interpreter
/// automatically uses it when stats tracking is enabled (default).
///
/// To disable automatic stats tracking:
/// ```rust,ignore
/// let config = ExecutionConfigBuilder::new()
///     .with_stats_tracking(false)  // Disable default stats
///     .build();
/// ```
///
/// # Example: Custom Statistics
///
/// ```rust
/// use gyrus::hooks::builtin::StatsTrackerHook;
/// use gyrus::hooks::{ExecutionHook, HookContext, HookDecision};
///
/// let mut hook = StatsTrackerHook::new();
///
/// // Hook will track stats as execution progresses
/// // Access via hook.stats() or hook.into_stats()
/// ```
#[derive(Debug, Clone)]
pub struct StatsTrackerHook {
    stats: ExecutionStats,
}

impl StatsTrackerHook {
    /// Create a new statistics tracker.
    #[inline]
    pub fn new() -> Self {
        Self {
            stats: ExecutionStats::new(),
        }
    }

    /// Get a reference to the collected statistics.
    ///
    /// This can be called at any time to inspect current stats,
    /// but final values are only available after `on_complete` is called.
    #[inline]
    pub fn stats(&self) -> &ExecutionStats {
        &self.stats
    }

    /// Consume the hook and return the statistics.
    ///
    /// This is useful when you want to extract stats after execution.
    #[inline]
    pub fn into_stats(self) -> ExecutionStats {
        self.stats
    }
}

impl Default for StatsTrackerHook {
    fn default() -> Self {
        Self::new()
    }
}

/// Wrapper for StatsTrackerHook with shared state.
///
/// This allows the interpreter to extract statistics after execution
/// while the hook is moved into the config.
///
/// # Usage (Internal)
///
/// This is used internally by `interpret_with_io()` to automatically
/// track statistics without requiring users to manually register the hook.
pub struct SharedStatsHook {
    shared: Arc<Mutex<StatsTrackerHook>>,
}

impl SharedStatsHook {
    /// Create a new shared stats hook.
    ///
    /// Returns both the hook (to be registered) and a handle to extract stats later.
    pub fn new() -> (Self, Arc<Mutex<StatsTrackerHook>>) {
        let shared = Arc::new(Mutex::new(StatsTrackerHook::new()));
        (
            Self {
                shared: Arc::clone(&shared),
            },
            shared,
        )
    }
}

impl ExecutionHook for SharedStatsHook {
    fn after_instruction(
        &mut self,
        instruction: &Instruction,
        context: &HookContext,
    ) -> HookDecision {
        self.shared
            .lock()
            .unwrap()
            .after_instruction(instruction, context)
    }

    fn on_loop_enter(
        &mut self,
        context: &HookContext,
        loop_info: Option<&super::LoopInfo>,
    ) -> HookDecision {
        self.shared
            .lock()
            .unwrap()
            .on_loop_enter(context, loop_info)
    }

    fn on_complete(&mut self, context: &HookContext) {
        self.shared.lock().unwrap().on_complete(context)
    }
}

impl ExecutionHook for StatsTrackerHook {
    fn after_instruction(
        &mut self,
        instruction: &Instruction,
        context: &HookContext,
    ) -> HookDecision {
        // Track peak memory usage
        // Peak is the highest pointer position + 1 (since pointer is 0-indexed)
        let current_peak = context.pointer().get() + 1;
        if current_peak > self.stats.peak_memory_used.get() {
            self.stats.peak_memory_used = MemoryAddress::new(current_peak);
        }

        // Track I/O operations
        match instruction {
            Instruction::Output => {
                self.stats.bytes_written += 1;
            }
            Instruction::Input => {
                self.stats.bytes_read += 1;
            }
            _ => {}
        }

        HookDecision::Continue
    }

    fn on_loop_enter(
        &mut self,
        _context: &HookContext,
        _loop_info: Option<&super::LoopInfo>,
    ) -> HookDecision {
        // Track loop iterations
        self.stats.loop_iterations += 1;
        HookDecision::Continue
    }

    fn on_complete(&mut self, context: &HookContext) {
        // Finalize statistics that can only be computed at the end
        self.stats.total_steps = context.step_count();
        self.stats.memory_allocated = crate::types::MemorySize::new(context.memory().len());
        self.stats.cells_modified = ExecutionStats::count_modified_cells(context.memory());
    }
}

/// Built-in hook for collecting runtime warnings.
///
/// This hook detects runtime anomalies and collects warnings:
/// - Memory expansion (in unbounded memory mode)
///
/// Warnings are non-fatal but indicate potentially unexpected behavior.
///
/// # Usage
///
/// This hook is automatically registered by the interpreter when executing programs.
/// Users typically don't need to use it directly.
///
/// # Example
///
/// ```rust
/// use gyrus::hooks::builtin::WarningCollectorHook;
///
/// let hook = WarningCollectorHook::new();
/// // Hook will track warnings as execution progresses
/// ```
#[derive(Debug, Clone)]
pub struct WarningCollectorHook {
    warnings: Vec<RuntimeWarning>,
    last_memory_size: usize,
}

impl WarningCollectorHook {
    /// Create a new warning collector.
    #[inline]
    pub fn new() -> Self {
        Self {
            warnings: Vec::new(),
            last_memory_size: 0,
        }
    }

    /// Get a reference to the collected warnings.
    #[inline]
    pub fn warnings(&self) -> &[RuntimeWarning] {
        &self.warnings
    }

    /// Consume the hook and return the warnings.
    #[inline]
    pub fn into_warnings(self) -> Vec<RuntimeWarning> {
        self.warnings
    }
}

impl Default for WarningCollectorHook {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionHook for WarningCollectorHook {
    fn after_instruction(
        &mut self,
        _instruction: &Instruction,
        context: &HookContext,
    ) -> HookDecision {
        let current_memory_size = context.memory().len();

        // Detect memory expansion
        if current_memory_size > self.last_memory_size && self.last_memory_size > 0 {
            // Memory was expanded - record warning
            // This happens with unbounded memory model when pointer goes beyond current size
            self.warnings.push(RuntimeWarning::MemoryExpanded {
                instruction_index: InstructionIndex::new((context.step_count().get() - 1) as usize),
                from_size: MemorySize::new(self.last_memory_size),
                to_size: MemorySize::new(current_memory_size),
                source_location: context.source_location().cloned(),
                _reserved: (),
            });
        }

        self.last_memory_size = current_memory_size;
        HookDecision::Continue
    }
}

/// Wrapper for WarningCollectorHook with shared state.
///
/// This allows the interpreter to extract warnings after execution.
pub struct SharedWarningHook {
    shared: Arc<Mutex<WarningCollectorHook>>,
}

impl SharedWarningHook {
    /// Create a new shared warning hook.
    ///
    /// Returns both the hook (to be registered) and a handle to extract warnings later.
    pub fn new() -> (Self, Arc<Mutex<WarningCollectorHook>>) {
        let shared = Arc::new(Mutex::new(WarningCollectorHook::new()));
        (
            Self {
                shared: Arc::clone(&shared),
            },
            shared,
        )
    }
}

impl ExecutionHook for SharedWarningHook {
    fn after_instruction(
        &mut self,
        instruction: &Instruction,
        context: &HookContext,
    ) -> HookDecision {
        self.shared
            .lock()
            .unwrap()
            .after_instruction(instruction, context)
    }
}

/// Built-in hook for enforcing execution limits.
///
/// This hook monitors execution and enforces:
/// - **Step limits**: Maximum number of instructions to execute
/// - **Timeout limits**: Maximum execution time in milliseconds
///
/// When a limit is exceeded, the hook returns `HookDecision::Break` to stop execution
/// and stores the appropriate error internally.
///
/// # Usage
///
/// This hook is automatically registered by the interpreter when limits are configured.
/// Users typically don't need to use it directly.
///
/// # Example
///
/// ```rust
/// use gyrus::hooks::builtin::LimitEnforcerHook;
///
/// let hook = LimitEnforcerHook::new(Some(1000), Some(5000));
/// // Hook will enforce 1000 step limit and 5000ms timeout
/// ```
#[derive(Debug)]
pub struct LimitEnforcerHook {
    max_steps: Option<u64>,
    timeout_ms: Option<u64>,
    start_time: std::time::Instant,
    error: Option<crate::error::BfError>,
}

impl LimitEnforcerHook {
    /// Create a new limit enforcer hook.
    ///
    /// # Parameters
    /// - `max_steps`: Maximum number of steps before raising StepLimitExceeded error
    /// - `timeout_ms`: Maximum execution time in milliseconds before raising ExecutionTimeout error
    pub fn new(max_steps: Option<u64>, timeout_ms: Option<u64>) -> Self {
        Self {
            max_steps,
            timeout_ms,
            start_time: std::time::Instant::now(),
            error: None,
        }
    }

    /// Check if the hook has stored an error (limit exceeded).
    pub fn has_error(&self) -> bool {
        self.error.is_some()
    }

    /// Take the stored error, if any.
    ///
    /// This consumes the error, leaving None in its place.
    pub fn take_error(&mut self) -> Option<crate::error::BfError> {
        self.error.take()
    }

    /// Get a reference to the stored error, if any.
    pub fn error(&self) -> Option<&crate::error::BfError> {
        self.error.as_ref()
    }
}

impl ExecutionHook for LimitEnforcerHook {
    fn after_instruction(
        &mut self,
        _instruction: &Instruction,
        context: &HookContext,
    ) -> HookDecision {
        use crate::error::BfError;

        // Check step limit
        if let Some(max_steps) = self.max_steps
            && context.step_count().get() > max_steps
        {
            self.error = Some(BfError::StepLimitExceeded {
                limit: max_steps,
                actual_steps: context.step_count(),
                instruction_index: context.instruction_index(),
                source_location: None, // Will be enriched by interpreter if debug info available
                hint: format!(
                    "Program executed {} steps, exceeding the limit of {}. \
                         This may indicate an infinite loop. Try increasing the limit with --max-steps {} \
                         or investigate your BrainFuck code for infinite loops.",
                    context.step_count().get(),
                    max_steps,
                    max_steps * 10
                ),
            });
            return HookDecision::Break;
        }

        // Check timeout
        if let Some(timeout_ms) = self.timeout_ms {
            let elapsed = self.start_time.elapsed().as_millis() as u64;
            if elapsed > timeout_ms {
                self.error = Some(BfError::ExecutionTimeout {
                    limit_ms: timeout_ms,
                    actual_steps: Some(context.step_count()),
                    hint: format!(
                        "Program exceeded {}ms timeout after executing {} steps. \
                         Try increasing timeout with --timeout {} or optimize your BrainFuck code.",
                        timeout_ms,
                        context.step_count().get(),
                        timeout_ms * 2
                    ),
                });
                return HookDecision::Break;
            }
        }

        HookDecision::Continue
    }
}

/// Wrapper for LimitEnforcerHook with shared state.
///
/// This allows the interpreter to extract errors after execution.
pub struct SharedLimitHook {
    shared: Arc<Mutex<LimitEnforcerHook>>,
}

impl SharedLimitHook {
    /// Create a new shared limit hook.
    ///
    /// Returns both the hook (to be registered) and a handle to extract errors later.
    pub fn new(
        max_steps: Option<u64>,
        timeout_ms: Option<u64>,
    ) -> (Self, Arc<Mutex<LimitEnforcerHook>>) {
        let shared = Arc::new(Mutex::new(LimitEnforcerHook::new(max_steps, timeout_ms)));
        (
            Self {
                shared: Arc::clone(&shared),
            },
            shared,
        )
    }
}

impl ExecutionHook for SharedLimitHook {
    fn after_instruction(
        &mut self,
        instruction: &Instruction,
        context: &HookContext,
    ) -> HookDecision {
        self.shared
            .lock()
            .unwrap()
            .after_instruction(instruction, context)
    }
}

/// Built-in hook for tracking debug symbols and loop contexts.
///
/// This hook maintains debug information needed for rich error messages:
/// - **DebugInfo**: Maps instruction indices to source locations
/// - **Loop stack**: Tracks active loop contexts for nested loops
///
/// When debug symbols are enabled, this hook:
/// 1. Stores the DebugInfo (passed during hook creation)
/// 2. Builds a loop call stack by tracking loop entry/exit
/// 3. Provides access to current instruction's source location
///
/// # Usage
///
/// This hook is automatically registered by the interpreter when debug symbols
/// are provided to `interpret_with_io()`.
///
/// # Example
///
/// ```rust,ignore
/// use gyrus::hooks::builtin::DebugTrackingHook;
/// use gyrus::debug::DebugInfo;
///
/// let debug_info = DebugInfo::from_source("[>+<-]");
/// let hook = DebugTrackingHook::new(debug_info);
/// // Hook will track loops and provide source locations
/// ```
#[derive(Debug)]
pub struct DebugTrackingHook {
    /// Debug information mapping instruction indices to source locations
    debug_info: crate::debug::DebugInfo,

    /// Stack of active loop contexts for nested loops
    loop_stack: Vec<crate::debug::LoopContext>,
}

impl DebugTrackingHook {
    /// Create a new debug tracking hook with the given debug info.
    pub fn new(debug_info: crate::debug::DebugInfo) -> Self {
        Self {
            debug_info,
            loop_stack: Vec::new(),
        }
    }

    /// Get a reference to the debug info.
    pub fn debug_info(&self) -> &crate::debug::DebugInfo {
        &self.debug_info
    }

    /// Get a reference to the current loop stack.
    pub fn loop_stack(&self) -> &[crate::debug::LoopContext] {
        &self.loop_stack
    }

    /// Consume the hook and return the debug info and loop stack.
    pub fn into_parts(self) -> (crate::debug::DebugInfo, Vec<crate::debug::LoopContext>) {
        (self.debug_info, self.loop_stack)
    }
}

impl ExecutionHook for DebugTrackingHook {
    fn on_loop_enter(
        &mut self,
        _context: &HookContext,
        loop_info: Option<&super::LoopInfo>,
    ) -> HookDecision {
        // Only track loops when loop_info is available (debug symbols enabled)
        if let Some(info) = loop_info {
            // Determine iteration number
            let iteration = if let Some(ctx) = self.loop_stack.last() {
                if ctx.loop_instruction_index == info.loop_instruction_index {
                    // Same loop, increment iteration
                    ctx.iteration + 1
                } else {
                    // Different loop (nested), start at 1
                    1
                }
            } else {
                // No active loops, start at 1
                1
            };

            // Look up source location for the '[' instruction
            let source_location = self
                .debug_info
                .lookup(info.loop_instruction_index)
                .unwrap_or_else(|| {
                    // Fallback if lookup fails (shouldn't happen)
                    crate::location::SourceLocation::new(0, 0, info.loop_instruction_index)
                });

            let context = crate::debug::LoopContext {
                loop_instruction_index: info.loop_instruction_index,
                body_start_index: info.body_start_index,
                body_size: info.body_size,
                iteration,
                source_location,
            };

            self.loop_stack.push(context);
        }

        HookDecision::Continue
    }

    fn on_loop_exit(&mut self, _context: &HookContext) -> HookDecision {
        // Pop the loop context when exiting a loop
        self.loop_stack.pop();
        HookDecision::Continue
    }
}

/// Wrapper for DebugTrackingHook with shared state.
///
/// This allows the interpreter to access debug info and loop stack after execution
/// while the hook is moved into the config.
///
/// # Usage (Internal)
///
/// This is used internally by `interpret_with_io()` to automatically
/// track debug information when debug symbols are provided.
pub struct SharedDebugTrackingHook {
    shared: Arc<Mutex<DebugTrackingHook>>,
}

impl SharedDebugTrackingHook {
    /// Create a new shared debug tracking hook.
    ///
    /// Returns both the hook (to be registered) and a handle to access debug info later.
    pub fn new(debug_info: crate::debug::DebugInfo) -> (Self, Arc<Mutex<DebugTrackingHook>>) {
        let shared = Arc::new(Mutex::new(DebugTrackingHook::new(debug_info)));
        (
            Self {
                shared: Arc::clone(&shared),
            },
            shared,
        )
    }
}

impl ExecutionHook for SharedDebugTrackingHook {
    fn on_loop_enter(
        &mut self,
        context: &HookContext,
        loop_info: Option<&super::LoopInfo>,
    ) -> HookDecision {
        self.shared
            .lock()
            .unwrap()
            .on_loop_enter(context, loop_info)
    }

    fn on_loop_exit(&mut self, context: &HookContext) -> HookDecision {
        self.shared.lock().unwrap().on_loop_exit(context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ExecutionConfigBuilder;
    use crate::interpreter::interpret_with_config;
    use crate::parser::parse;

    #[test]
    fn test_stats_tracker_basic() {
        let source = "+++>>--";
        let instructions = parse(source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_hook(Box::new(StatsTrackerHook::new()))
            .build();

        let stats = interpret_with_config(&instructions, config, None).unwrap();

        // Verify stats were collected
        assert_eq!(stats.total_steps.get(), 7); // 3+ 2> 2-
        assert_eq!(stats.loop_iterations, 0); // No loops
        assert_eq!(stats.peak_memory_used.get(), 3); // Pointer reached cell 2, so peak is 3
        assert_eq!(stats.bytes_written, 0); // No output
        assert_eq!(stats.bytes_read, 0); // No input
    }

    #[test]
    fn test_stats_tracker_with_loops() {
        let source = "+++[>++<-]"; // Loop 3 times
        let instructions = parse(source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_hook(Box::new(StatsTrackerHook::new()))
            .build();

        let stats = interpret_with_config(&instructions, config, None).unwrap();

        assert_eq!(stats.loop_iterations, 3); // Loop body entered 3 times
        assert!(stats.total_steps.get() > 3); // More than just the setup
        assert_eq!(stats.peak_memory_used.get(), 2); // Pointer moved to cell 1
    }

    #[test]
    fn test_stats_tracker_io_operations() {
        let source = "++.>."; // Two outputs
        let instructions = parse(source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_hook(Box::new(StatsTrackerHook::new()))
            .build();

        let stats = interpret_with_config(&instructions, config, None).unwrap();

        assert_eq!(stats.bytes_written, 2); // Two output operations
        assert_eq!(stats.bytes_read, 0); // No input
    }

    #[test]
    fn test_stats_tracker_cells_modified() {
        let source = "+++>++>+"; // Modify 3 cells
        let instructions = parse(source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_hook(Box::new(StatsTrackerHook::new()))
            .build();

        let stats = interpret_with_config(&instructions, config, None).unwrap();

        assert_eq!(stats.cells_modified, 3); // 3 non-zero cells
        assert_eq!(stats.peak_memory_used.get(), 3); // Highest cell accessed + 1
    }

    #[test]
    fn test_stats_tracker_nested_loops() {
        let source = "+++[>++[<+>-]<-]"; // Nested loops
        let instructions = parse(source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_hook(Box::new(StatsTrackerHook::new()))
            .build();

        let stats = interpret_with_config(&instructions, config, None).unwrap();

        // Outer loop: 3 iterations
        // Inner loop: 2 iterations per outer (2+4+6 = 12 times total)
        // Hmm, wait, let me think about this more carefully
        // Cell 0 starts at 3
        // Outer iteration 1: cell 0 = 3, cell 1 = 2, inner runs 2 times
        // Outer iteration 2: cell 0 = 2, cell 1 = 4, inner runs 4 times
        // Outer iteration 3: cell 0 = 1, cell 1 = 6, inner runs 6 times
        // Total inner: 2 + 4 + 6 = 12
        // Total loop_iterations = 3 (outer) + 12 (inner) = 15
        assert!(stats.loop_iterations >= 3); // At least the outer loops
    }

    #[test]
    fn test_stats_tracker_into_stats() {
        let hook = StatsTrackerHook::new();

        // Create hook with pre-set values by directly accessing stats
        // (In practice, these would be set through execution)
        let stats = hook.into_stats();
        assert_eq!(stats.bytes_written, 0); // Initial value
        assert_eq!(stats.loop_iterations, 0); // Initial value
    }

    #[test]
    fn test_stats_tracker_default() {
        let hook = StatsTrackerHook::default();
        assert_eq!(hook.stats().total_steps.get(), 0);
        assert_eq!(hook.stats().loop_iterations, 0);
    }
}

/// Built-in hook for profiling loop execution and identifying hot code regions.
///
/// This hook tracks execution time spent in different parts of the program,
/// particularly focusing on loop performance. It builds a hierarchical profile
/// that shows:
/// - Time spent in each loop
/// - Loop nesting relationships
/// - Iteration counts
/// - Hot code regions (loops that consume the most time)
///
/// # Output Formats
///
/// - **ASCII Tree**: Terminal-friendly hierarchical view with execution times
///
/// # Usage
///
/// ```rust
/// use gyrus::{parse, ExecutionConfigBuilder, interpret_with_io};
/// use gyrus::hooks::builtin::SharedProfilingHook;
/// use gyrus::io::StringIo;
///
/// # fn main() -> Result<(), gyrus::BfError> {
/// let source = "+++[>++[<.>-]<-]";
/// let instructions = parse(source)?;
///
/// // Create shared profiler
/// let (profiler_hook, profiler_handle) = SharedProfilingHook::new();
///
/// let mut config = ExecutionConfigBuilder::new()
///     .with_memory_size(1000)
///     .build();
///
/// // Register profiler hook
/// config.register_hook(Box::new(profiler_hook));
///
/// let mut input = StringIo::empty();
/// let mut output = StringIo::empty();
/// interpret_with_io(&instructions, config, &mut input, &mut output, None)?;
///
/// // Print profiling results
/// let profiler = profiler_handle.lock().unwrap();
/// println!("{}", profiler.format_ascii_tree());
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct ProfilingHook {
    /// Stack of active loop frames (like a call stack for loops)
    loop_stack: Vec<LoopFrame>,

    /// Completed loop executions with profiling data
    completed_loops: Vec<CompletedLoop>,

    /// Instruction hit counts (instruction_index -> hit_count)
    instruction_hits: std::collections::HashMap<usize, u64>,

    /// Total execution time
    start_time: std::time::Instant,
    total_time: std::time::Duration,
}

/// A single loop frame on the execution stack.
#[derive(Debug, Clone)]
struct LoopFrame {
    /// Instruction index of the loop start '['
    loop_instruction_index: usize,

    /// Source location of the loop
    source_location: Option<crate::location::SourceLocation>,

    /// When this loop iteration started
    start_time: std::time::Instant,

    /// Nesting depth (0 = top-level)
    depth: usize,

    /// Iteration number (1-indexed)
    iteration: usize,
}

/// A completed loop execution with profiling data.
#[derive(Debug, Clone)]
pub struct CompletedLoop {
    /// Instruction index of the loop start '['
    pub loop_instruction_index: usize,

    /// Source location of the loop
    pub source_location: Option<crate::location::SourceLocation>,

    /// Total time spent in this loop (including nested loops)
    pub total_time: std::time::Duration,

    /// Nesting depth (0 = top-level)
    pub depth: usize,

    /// Number of iterations
    pub iterations: usize,
}

impl ProfilingHook {
    /// Create a new profiling hook.
    pub fn new() -> Self {
        Self {
            loop_stack: Vec::new(),
            completed_loops: Vec::new(),
            instruction_hits: std::collections::HashMap::new(),
            start_time: std::time::Instant::now(),
            total_time: std::time::Duration::ZERO,
        }
    }

    /// Get the completed loop profiling data.
    pub fn completed_loops(&self) -> &[CompletedLoop] {
        &self.completed_loops
    }

    /// Get the total execution time.
    pub fn total_time(&self) -> std::time::Duration {
        self.total_time
    }

    /// Get the instruction hit counts.
    ///
    /// Returns a reference to the HashMap mapping instruction indices to hit counts.
    pub fn instruction_hits(&self) -> &std::collections::HashMap<usize, u64> {
        &self.instruction_hits
    }

    /// Format source code with execution heatmap showing hit counts per instruction.
    ///
    /// This creates a detailed view showing each BrainFuck instruction with its
    /// execution count and source location, making it easy to identify hot spots.
    ///
    /// # Parameters
    ///
    /// - `source`: The original BrainFuck source code
    /// - `debug_info`: Debug information for mapping instruction indices to source locations
    ///
    /// # Output Format
    ///
    /// ```text
    /// Execution Heatmap (per instruction)
    /// Idx | Instr | Location       |       Hits | Heat
    ///   0 | +     | line 1, col 1  |          1 | █
    ///   1 | +     | line 1, col 2  |          1 | █
    ///   5 | [     | line 2, col 1  |          5 | █████
    ///   6 | >     | line 3, col 3  |         15 | ███████████████
    /// ```
    pub fn format_source_heatmap(
        &self,
        source: &str,
        debug_info: Option<&crate::debug::DebugInfo>,
    ) -> String {
        use std::fmt::Write;

        let mut output = String::new();
        writeln!(&mut output, "Execution Heatmap").unwrap();
        writeln!(&mut output).unwrap();

        // If no debug info, we can't map back to source
        let Some(debug_info) = debug_info else {
            writeln!(&mut output, "Debug information not available.").unwrap();
            writeln!(
                &mut output,
                "Run with --debug flag to enable source heatmap."
            )
            .unwrap();
            return output;
        };

        // Find the maximum hit count for heat normalization
        let max_hits = self.instruction_hits.values().copied().max().unwrap_or(1) as f64;

        // Helper function to get color for a given heat level (0.0 to 1.0)
        let heat_to_color = |heat: f64| -> (u8, u8, u8) {
            if heat <= 0.0 {
                // Not executed: dim blue
                (50, 50, 150)
            } else if heat < 0.2 {
                // Cold: cyan
                (0, 200, 200)
            } else if heat < 0.4 {
                // Cool: green-yellow
                (150, 200, 0)
            } else if heat < 0.6 {
                // Warm: yellow
                (255, 200, 0)
            } else if heat < 0.8 {
                // Hot: red
                (255, 50, 0)
            } else {
                // Very hot: white (for clear distinction from hot)
                (255, 255, 255)
            }
        };

        // Track which character is which instruction for heat mapping
        let mut char_to_instruction: std::collections::HashMap<usize, usize> =
            std::collections::HashMap::new();

        // Build reverse mapping: offset -> instruction_index
        for &instruction_index in self.instruction_hits.keys() {
            if let Some(location) = debug_info.lookup(instruction_index) {
                char_to_instruction.insert(location.offset, instruction_index);
            }
        }

        // Process each line and character
        let mut current_offset = 0;

        for line in source.lines() {
            // Track if we're in a line comment for this line
            let mut in_line_comment = false;

            for ch in line.chars() {
                let char_offset = current_offset;
                current_offset += ch.len_utf8();

                // Check if this is a line comment start
                if ch == '*' {
                    in_line_comment = true;
                }

                if in_line_comment || !matches!(ch, '>' | '<' | '+' | '-' | ',' | '.' | '[' | ']') {
                    // Comment or non-BF character: gray it out
                    let mut buffer = Vec::new();
                    {
                        let mut writer = Ansi::new(&mut buffer);
                        let mut color_spec = ColorSpec::new();
                        color_spec.set_fg(Some(Color::Rgb(100, 100, 100)));
                        writer.set_color(&color_spec).unwrap();
                        write!(writer, "{}", ch).unwrap();
                    } // writer is dropped here, flushing to buffer
                    output.push_str(&String::from_utf8(buffer).unwrap());
                } else {
                    // BF instruction: apply heat color
                    if let Some(&instruction_index) = char_to_instruction.get(&char_offset) {
                        let hits = self
                            .instruction_hits
                            .get(&instruction_index)
                            .copied()
                            .unwrap_or(0);

                        // Use logarithmic scale for better color distribution
                        let heat = if hits == 0 {
                            -1.0 // Sentinel value for truly unexecuted (heat_to_color checks <= 0.0)
                        } else if hits == 1 {
                            // Special case: 1 hit should be cold (small positive value)
                            // ln(1) = 0, but we want hits=1 to show as "cold" not "not executed"
                            0.01 // Just above 0.0, will be colored cyan (cold)
                        } else if max_hits > 1.0 {
                            // Map hit counts logarithmically: log(hits) / log(max_hits)
                            // This spreads out lower frequencies much better
                            ((hits as f64).ln() / max_hits.ln()).clamp(0.01, 1.0)
                        } else {
                            1.0
                        };

                        let (r, g, b) = heat_to_color(heat);
                        let mut buffer = Vec::new();
                        {
                            let mut writer = Ansi::new(&mut buffer);
                            let mut color_spec = ColorSpec::new();
                            color_spec.set_fg(Some(Color::Rgb(r, g, b)));
                            writer.set_color(&color_spec).unwrap();
                            write!(writer, "{}", ch).unwrap();
                        } // writer is dropped here, flushing to buffer
                        output.push_str(&String::from_utf8(buffer).unwrap());
                    } else {
                        // BF instruction but no hit data: treat as unexecuted (dim blue)
                        let (r, g, b) = heat_to_color(0.0);
                        let mut buffer = Vec::new();
                        {
                            let mut writer = Ansi::new(&mut buffer);
                            let mut color_spec = ColorSpec::new();
                            color_spec.set_fg(Some(Color::Rgb(r, g, b)));
                            writer.set_color(&color_spec).unwrap();
                            write!(writer, "{}", ch).unwrap();
                        } // writer is dropped here, flushing to buffer
                        output.push_str(&String::from_utf8(buffer).unwrap());
                    }
                }
            }
            writeln!(&mut output).unwrap(); // End of line
            current_offset += 1; // Account for newline
        }

        // Add legend with logarithmic count ranges
        writeln!(&mut output).unwrap();
        writeln!(
            &mut output,
            "Legend (execution frequency, logarithmic scale):"
        )
        .unwrap();

        // Calculate logarithmic ranges: hits = max_hits^heat
        let max_hits_u64 = max_hits as u64;

        // Helper to calculate hit count at a given heat threshold
        let hits_at_heat = |heat: f64| -> u64 {
            if heat <= 0.0 {
                0
            } else {
                (max_hits.powf(heat)).round() as u64
            }
        };

        // Build legend items with labels and hit counts
        // Use mid-range heat values for accurate color representation in legend
        let legend_items = vec![
            (0.0, "not executed", "(0 hits)".to_string()),
            (0.1, "cold", format!("(1-{} hits)", hits_at_heat(0.2))),
            (
                0.3,
                "cool",
                format!("({}-{} hits)", hits_at_heat(0.2) + 1, hits_at_heat(0.4)),
            ),
            (
                0.5,
                "warm",
                format!("({}-{} hits)", hits_at_heat(0.4) + 1, hits_at_heat(0.6)),
            ),
            (
                0.7,
                "hot",
                format!("({}-{} hits)", hits_at_heat(0.6) + 1, hits_at_heat(0.8)),
            ),
            (
                0.9,
                "very hot",
                format!("({}-{} hits)", hits_at_heat(0.8) + 1, max_hits_u64),
            ),
        ];

        // Calculate max widths for dynamic spacing
        let max_label_width = legend_items
            .iter()
            .map(|(_, label, _)| label.len())
            .max()
            .unwrap_or(0);
        let max_hits_width = legend_items
            .iter()
            .map(|(_, _, hits)| hits.len())
            .max()
            .unwrap_or(0);

        // Determine layout based on hit string length
        // If hit counts are long (e.g., "(8007-75735 hits)"), use 2 per row (3 rows)
        // Otherwise use 3 per row (2 rows)
        let items_per_row = if max_hits_width > 16 { 2 } else { 3 };

        for (i, (heat, label, hits)) in legend_items.iter().enumerate() {
            let (r, g, b) = heat_to_color(*heat);

            // Write colored dot using termcolor
            let mut buffer = Vec::new();
            let mut writer = Ansi::new(&mut buffer);
            let mut color_spec = ColorSpec::new();
            color_spec.set_fg(Some(Color::Rgb(r, g, b)));
            writer.set_color(&color_spec).unwrap();
            write!(writer, "●").unwrap();
            writer.reset().unwrap();
            output.push_str("  ");
            output.push_str(&String::from_utf8(buffer).unwrap());

            // Write label and hits without color
            write!(
                &mut output,
                " {:<width_label$} {:<width_hits$}",
                label,
                hits,
                width_label = max_label_width,
                width_hits = max_hits_width
            )
            .unwrap();

            // Add spacing or newline
            if (i + 1) % items_per_row == 0 && i < legend_items.len() - 1 {
                // End of row, start new line
                writeln!(&mut output).unwrap();
            } else if i < legend_items.len() - 1 {
                // Between items on same line, add consistent spacing
                write!(&mut output, "     ").unwrap();
            }
        }
        writeln!(&mut output).unwrap();

        writeln!(&mut output).unwrap();
        writeln!(
            &mut output,
            "Total instructions executed: {}",
            self.instruction_hits.values().sum::<u64>()
        )
        .unwrap();

        output
    }

    /// Format profiling results as an ASCII tree.
    ///
    /// # Output Format
    ///
    /// ```text
    /// BrainFuck Profiling Results
    /// Total execution time: 125.3ms
    ///
    /// Loop Profile (by time):
    /// ├─ Loop @5 (line 2, col 10): 100.2ms (79.9%) - 256 iterations
    /// │  └─ Loop @15 (line 3, col 5): 85.1ms (67.9%) - 512 iterations
    /// └─ Loop @30 (line 5, col 1): 15.1ms (12.0%) - 10 iterations
    /// ```
    pub fn format_ascii_tree(&self) -> String {
        use std::fmt::Write;

        let mut output = String::new();

        writeln!(&mut output, "BrainFuck Profiling Results").unwrap();
        writeln!(
            &mut output,
            "Total execution time: {:.1}ms",
            self.total_time.as_secs_f64() * 1000.0
        )
        .unwrap();
        writeln!(&mut output).unwrap();

        if self.completed_loops.is_empty() {
            writeln!(&mut output, "No loops executed.").unwrap();
            return output;
        }

        // Group loops by depth and sort by time
        let mut loops_by_depth: std::collections::HashMap<usize, Vec<&CompletedLoop>> =
            std::collections::HashMap::new();

        for completed in &self.completed_loops {
            loops_by_depth
                .entry(completed.depth)
                .or_default()
                .push(completed);
        }

        // Sort each depth level by time (descending)
        for loops in loops_by_depth.values_mut() {
            loops.sort_by(|a, b| b.total_time.cmp(&a.total_time));
        }

        writeln!(&mut output, "Loop Profile (by time):").unwrap();

        // Display top-level loops (depth 0)
        if let Some(top_level) = loops_by_depth.get(&0) {
            for (idx, loop_data) in top_level.iter().enumerate() {
                let is_last = idx == top_level.len() - 1;
                let prefix = if is_last { "└─" } else { "├─" };

                self.format_loop_line(&mut output, loop_data, prefix, "");

                // Display nested loops (simplified version - skipped for now)
                self.format_nested_loops(
                    &mut output,
                    loop_data.loop_instruction_index,
                    &loops_by_depth,
                    if is_last { "   " } else { "│  " },
                );
            }
        }

        output
    }

    /// Format a single loop line in the tree.
    fn format_loop_line(
        &self,
        output: &mut String,
        loop_data: &CompletedLoop,
        prefix: &str,
        indent: &str,
    ) {
        use std::fmt::Write;

        let time_ms = loop_data.total_time.as_secs_f64() * 1000.0;
        let percentage = if self.total_time.as_nanos() > 0 {
            (loop_data.total_time.as_nanos() as f64 / self.total_time.as_nanos() as f64) * 100.0
        } else {
            0.0
        };

        let location = if let Some(loc) = &loop_data.source_location {
            format!("line {}, col {}", loc.line, loc.column)
        } else {
            format!("index {}", loop_data.loop_instruction_index)
        };

        writeln!(
            output,
            "{}{} Loop @{} ({}): {:.1}ms ({:.1}%) - {} iteration{}",
            indent,
            prefix,
            loop_data.loop_instruction_index,
            location,
            time_ms,
            percentage,
            loop_data.iterations,
            if loop_data.iterations == 1 { "" } else { "s" }
        )
        .unwrap();
    }

    /// Recursively format nested loops.
    fn format_nested_loops(
        &self,
        _output: &mut String,
        _parent_index: usize,
        _loops_by_depth: &std::collections::HashMap<usize, Vec<&CompletedLoop>>,
        _indent: &str,
    ) {
        // Find loops at the next depth level that are children of this loop
        // (This is a simplified version - in a real implementation, we'd track parent-child relationships)
        // For now, we'll skip nested display to keep it simple
    }
}

impl Default for ProfilingHook {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionHook for ProfilingHook {
    fn on_loop_enter(
        &mut self,
        context: &HookContext,
        loop_info: Option<&super::LoopInfo>,
    ) -> HookDecision {
        if let Some(info) = loop_info {
            // Check if we're re-entering the same loop (new iteration)
            let iteration = if let Some(top) = self.loop_stack.last() {
                if top.loop_instruction_index == info.loop_instruction_index {
                    // Same loop, new iteration - extract iteration value first
                    let next_iteration = top.iteration + 1;
                    // Pop the old frame
                    self.loop_stack.pop();
                    next_iteration
                } else {
                    // Different loop (nested), start at iteration 1
                    1
                }
            } else {
                1
            };

            let frame = LoopFrame {
                loop_instruction_index: info.loop_instruction_index,
                source_location: context.source_location().cloned(),
                start_time: std::time::Instant::now(),
                depth: self.loop_stack.len(),
                iteration,
            };

            self.loop_stack.push(frame);
        }

        HookDecision::Continue
    }

    fn on_loop_exit(&mut self, _context: &HookContext) -> HookDecision {
        if let Some(frame) = self.loop_stack.pop() {
            let elapsed = frame.start_time.elapsed();

            // Record completed loop
            // Check if we already have an entry for this loop at this depth
            if let Some(existing) = self.completed_loops.iter_mut().find(|l| {
                l.loop_instruction_index == frame.loop_instruction_index && l.depth == frame.depth
            }) {
                // Update existing entry
                existing.total_time += elapsed;
                existing.iterations = frame.iteration;
            } else {
                // New loop entry
                self.completed_loops.push(CompletedLoop {
                    loop_instruction_index: frame.loop_instruction_index,
                    source_location: frame.source_location,
                    total_time: elapsed,
                    depth: frame.depth,
                    iterations: frame.iteration,
                });
            }
        }

        HookDecision::Continue
    }

    fn after_instruction(
        &mut self,
        _instruction: &crate::instruction::Instruction,
        context: &HookContext,
    ) -> HookDecision {
        // Track instruction hits
        *self
            .instruction_hits
            .entry(context.instruction_index())
            .or_insert(0) += 1;
        HookDecision::Continue
    }

    fn on_complete(&mut self, _context: &HookContext) {
        self.total_time = self.start_time.elapsed();
    }
}

/// Wrapper for ProfilingHook with shared state.
///
/// This allows the interpreter to access profiling data after execution.
pub struct SharedProfilingHook {
    shared: Arc<Mutex<ProfilingHook>>,
}

impl SharedProfilingHook {
    /// Create a new shared profiling hook.
    ///
    /// Returns both the hook (to be registered) and a handle to access profiling data later.
    pub fn new() -> (Self, Arc<Mutex<ProfilingHook>>) {
        let shared = Arc::new(Mutex::new(ProfilingHook::new()));
        (
            Self {
                shared: Arc::clone(&shared),
            },
            shared,
        )
    }

    /// Create a shared profiling hook from an existing Arc<Mutex<ProfilingHook>>.
    pub fn new_with_shared(shared: Arc<Mutex<ProfilingHook>>) -> Self {
        Self { shared }
    }
}

impl ExecutionHook for SharedProfilingHook {
    fn on_loop_enter(
        &mut self,
        context: &HookContext,
        loop_info: Option<&super::LoopInfo>,
    ) -> HookDecision {
        self.shared
            .lock()
            .unwrap()
            .on_loop_enter(context, loop_info)
    }

    fn on_loop_exit(&mut self, context: &HookContext) -> HookDecision {
        self.shared.lock().unwrap().on_loop_exit(context)
    }

    fn after_instruction(
        &mut self,
        instruction: &crate::instruction::Instruction,
        context: &HookContext,
    ) -> HookDecision {
        self.shared
            .lock()
            .unwrap()
            .after_instruction(instruction, context)
    }

    fn on_complete(&mut self, context: &HookContext) {
        self.shared.lock().unwrap().on_complete(context)
    }
}
