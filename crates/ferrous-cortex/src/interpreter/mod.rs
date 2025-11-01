//! BrainFuck instruction execution engine.
//!
//! This module provides the core interpreter that executes BrainFuck instructions
//! with configurable memory models, cell models, EOF behavior, and resource limits.
//! It features a tree-walking interpreter with recursive execution for nested loops.
//!
//! # Features
//!
//! - **Multiple memory models**: Fixed, wrapping, and unbounded memory
//! - **Multiple cell models**: Wrapping (standard BF) and checked (debugging)
//! - **Configurable EOF behavior**: SetZero, SetNegOne, NoChange, or Error
//! - **Execution limits**: Step counting and timeout support
//! - **Execution hooks**: Extensible hook system for custom instrumentation and debugging
//! - **Statistics tracking**: Steps, memory usage, I/O operations (via hooks)
//! - **Runtime warnings**: Cell overflow/underflow, memory expansion (via hooks)
//! - **Custom I/O**: Support for string-based, file-based, or custom I/O
//! - **Debug symbols**: Optional source location tracking for runtime diagnostics
//!
//! # Execution Model
//!
//! The interpreter uses a **tree-walking** approach with recursive execution:
//! - Each instruction is executed directly from the AST
//! - Loops are represented as `Instruction::Loop(Vec<Instruction>)`
//! - Recursive `execute_block()` handles nested loop execution
//! - Memory and pointer state are maintained in `VmState`
//! - Interpreter state is exposed to hooks via `HookContext` snapshots
//!
//! ## Internal Architecture
//!
//! The interpreter separates concerns for clarity and extensibility:
//!
//! - **`execute_block()`**: Main execution loop that handles control flow
//!   - Checks execution limits (step count, timeout)
//!   - Manages loop iteration and depth tracking
//!   - Delegates instruction execution to `execute_single_instruction()`
//!
//! - **`execute_single_instruction()`**: Executes individual non-loop instructions
//!   - Handles all BrainFuck operations except loops
//!   - Clean separation with hook integration points
//!   - Easier to test and reason about
//!
//! - **`VmState`**: Encapsulates all runtime state (private)
//!   - Memory tape, pointer position, step count
//!   - Loop depth tracking for debugging
//!   - State exposed to hooks via `HookContext` immutable snapshots
//!
//! # Examples
//!
//! ## Basic execution
//!
//! ```rust,no_run
//! use ferrous_cortex::{parse, interpret_with_config, ExecutionConfigBuilder};
//!
//! # fn main() -> Result<(), ferrous_cortex::BfError> {
//! let instructions = parse("+[>+]")?;
//! let config = ExecutionConfigBuilder::new()
//!     .with_memory_size(1000)
//!     .with_max_steps(100)
//!     .build();
//!
//! // This will hit the step limit since the program is an infinite loop
//! let stats = interpret_with_config(&instructions, config, None)?;
//! println!("Executed {} steps", stats.total_steps);
//! # Ok(())
//! # }
//! ```
//!
//! ## Custom I/O
//!
//! ```rust
//! use ferrous_cortex::{parse, interpret_with_io, io::StringIo, ExecutionConfigBuilder};
//!
//! # fn main() -> Result<(), ferrous_cortex::BfError> {
//! let instructions = parse(",[.,]")?;  // Echo program
//! let config = ExecutionConfigBuilder::new()
//!     .with_memory_size(100)
//!     .build();
//!
//! let mut input = StringIo::new("Hello");
//! let mut output = StringIo::empty();
//! interpret_with_io(&instructions, config, &mut input, &mut output, None)?;
//! assert_eq!(output.output_string(), "Hello");
//! # Ok(())
//! # }
//! ```
//!
//! ## Different memory models
//!
//! ```rust
//! use ferrous_cortex::{parse, interpret_with_config, ExecutionConfigBuilder};
//!
//! # fn main() -> Result<(), ferrous_cortex::BfError> {
//! let instructions = parse("+>+>+")?;
//!
//! // Fixed memory (traditional, bounds-checked)
//! let config = ExecutionConfigBuilder::new()
//!     .with_memory_size(30000)
//!     .build();
//! let stats = interpret_with_config(&instructions, config, None)?;
//!
//! // Unbounded memory (dynamic growth)
//! let config = ExecutionConfigBuilder::new()
//!     .with_unbounded_memory(1000, 100000)?
//!     .build();
//! let stats = interpret_with_config(&instructions, config, None)?;
//! # Ok(())
//! # }
//! ```

// Sub-modules
mod dispatch;
mod execution;
mod state;

// Tests module
#[cfg(test)]
mod tests;

// Internal imports
use crate::config::ExecutionConfig;
use crate::debug::DebugInfo;
use crate::error::{BfError, Result};
use crate::instruction::Instruction;
use crate::io::{BfInput, BfOutput, StdInput, StdOutput};
use crate::stats::ExecutionStats;

// Re-export sub-module items for internal use
use dispatch::HookDispatcher;
use execution::execute_block;
use state::VmState;

/// Interpret and execute BrainFuck instructions with default configuration
///
/// This is a convenience function that uses default settings.
/// For custom configuration, use `interpret_with_config()`.
/// Discards execution statistics.
#[allow(dead_code)]
pub fn interpret(instructions: &[Instruction]) -> Result<()> {
    interpret_with_config(instructions, ExecutionConfig::default(), None).map(|_| ())
}

/// Context for interpreter execution with built-in hooks.
///
/// This struct handles the lifecycle of built-in hooks:
/// 1. Setup: Create built-in hooks (stats, warnings, debug, limits)
/// 2. Execution: Run the program
/// 3. Cleanup: Extract results and enrich errors
///
/// Built-in hooks are stored directly (no Arc<Mutex>) since the interpreter
/// is single-threaded. User hooks from ExecutionConfig are still supported.
struct InterpreterContext {
    config: ExecutionConfig,

    // Built-in hooks stored directly (no Arc<Mutex> needed!)
    stats_hook: crate::hooks::builtin::StatsTrackerHook,
    warning_hook: crate::hooks::builtin::WarningCollectorHook,
    debug_hook: Option<crate::hooks::builtin::DebugTrackingHook>,
    limit_hook: Option<crate::hooks::builtin::LimitEnforcerHook>,
}

impl InterpreterContext {
    /// Create a new interpreter context with built-in hooks
    fn new(config: ExecutionConfig, debug_info: Option<&DebugInfo>) -> Self {
        use crate::hooks::builtin::{
            DebugTrackingHook, LimitEnforcerHook, StatsTrackerHook, WarningCollectorHook,
        };

        // Create built-in hooks directly (no Arc<Mutex> wrappers!)
        let stats_hook = StatsTrackerHook::new();
        let warning_hook = WarningCollectorHook::new();

        // Create debug tracking hook if debug info is provided
        let debug_hook = debug_info.map(|info| DebugTrackingHook::new(info.clone()));

        // Create limit enforcement hook if limits are configured
        let limit_hook = if config.max_steps().is_some() || config.timeout_ms().is_some() {
            Some(LimitEnforcerHook::new(
                config.max_steps(),
                config.timeout_ms(),
            ))
        } else {
            None
        };

        Self {
            config,
            stats_hook,
            warning_hook,
            debug_hook,
            limit_hook,
        }
    }

    /// Execute the program and return statistics
    fn execute<I: BfInput, O: BfOutput>(
        mut self,
        instructions: &[Instruction],
        input: &mut I,
        output: &mut O,
    ) -> Result<ExecutionStats> {
        // Create VM state
        let mut state = VmState::new(*self.config.memory_model());

        // Clone debug_info before borrowing hooks mutably (DebugInfo is Arc, so cheap to clone)
        let debug_info = self.debug_hook.as_ref().map(|h| h.debug_info().clone());

        // Create hook dispatcher with built-in hooks (no Arc<Mutex>!)
        let mut dispatcher = HookDispatcher::new(
            &mut self.config,
            &mut self.stats_hook,
            &mut self.warning_hook,
            self.debug_hook.as_mut(),
            self.limit_hook.as_mut(),
        );

        // Execute the program
        let execute_result = execute_block(
            instructions,
            &mut state,
            &mut dispatcher,
            input,
            output,
            0,
            debug_info.as_ref(),
        );

        // Call on_complete if execution succeeded, then drop dispatcher
        if execute_result.is_ok() {
            dispatcher.dispatch_complete(&state);
        }
        // Dispatcher is dropped automatically here, ending all mutable borrows

        // Check for limit errors (takes precedence over ExecutionPaused)
        if let Some(limit_hook) = &mut self.limit_hook
            && let Some(mut error) = limit_hook.take_error()
        {
            // Enrich StepLimitExceeded with source_location
            if let Some(debug_hook) = &self.debug_hook
                && let BfError::StepLimitExceeded {
                    instruction_index, ..
                } = &error
                && let Some(loc) = debug_hook.debug_info().lookup(*instruction_index)
            {
                error = error.with_step_limit_source_location(loc);
            }
            return Err(error);
        }

        // Handle execution result
        match execute_result {
            Ok(_) => {
                // Extract statistics (no mutex locking!)
                let mut stats = self.stats_hook.stats().clone();
                stats.warnings = self.warning_hook.warnings().to_vec();
                Ok(stats)
            }
            Err(mut error) => {
                // Enrich error with debug information (no mutex locking!)
                if let Some(debug_hook) = &self.debug_hook {
                    // Add loop call stack
                    let loop_stack = debug_hook.loop_stack();
                    let loop_call_stack: Vec<crate::error::LoopStackFrame> = loop_stack
                        .iter()
                        .map(|ctx| crate::error::LoopStackFrame {
                            source_location: ctx.source_location,
                            iteration: ctx.iteration,
                        })
                        .collect();
                    error = error.with_loop_call_stack(loop_call_stack);

                    // Add source location for step limit errors
                    if let BfError::StepLimitExceeded {
                        instruction_index, ..
                    } = &error
                        && let Some(loc) = debug_hook.debug_info().lookup(*instruction_index)
                    {
                        error = error.with_step_limit_source_location(loc);
                    }
                }
                Err(error)
            }
        }
    }
}

/// Interpret and execute BrainFuck instructions with custom I/O.
///
/// This is the primary interpreter function that allows custom input and output.
///
/// # Examples
///
/// ```rust
/// use ferrous_cortex::{parse, interpret_with_io, io::StringIo, ExecutionConfig};
///
/// // Echo program: reads input and outputs it
/// let instructions = parse(",[.,]")?;
/// let mut input = StringIo::new("Hi");
/// let mut output = StringIo::empty();
/// let stats = interpret_with_io(&instructions, ExecutionConfig::default(), &mut input, &mut output, None)?;
/// assert_eq!(output.output_string(), "Hi");
/// # Ok::<(), ferrous_cortex::BfError>(())
/// ```
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

/// Interpret and execute BrainFuck instructions with custom configuration.
///
/// This is a convenience function that uses stdin/stdout for I/O.
/// For custom I/O, use `interpret_with_io()`.
/// Returns execution statistics.
///
/// # Examples
///
/// ```rust,no_run
/// use ferrous_cortex::{parse, interpret_with_config, ExecutionConfig};
///
/// let instructions = parse("++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.")?;
/// let stats = interpret_with_config(&instructions, ExecutionConfig::default(), None)?;
/// println!("Executed {} steps", stats.total_steps);
/// # Ok::<(), ferrous_cortex::BfError>(())
/// ```
pub fn interpret_with_config(
    instructions: &[Instruction],
    config: ExecutionConfig,
    debug_info: Option<&DebugInfo>,
) -> Result<ExecutionStats> {
    let mut input = StdInput;
    let mut output = StdOutput;
    interpret_with_io(instructions, config, &mut input, &mut output, debug_info)
}
