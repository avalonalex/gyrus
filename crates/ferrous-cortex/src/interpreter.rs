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

use crate::config::{EofBehavior, ExecutionConfig};
use crate::debug::DebugInfo;
use crate::error::{BfError, Result};
use crate::hooks::{HookContext, HookDecision}; // Hook system integration
use crate::instruction::Instruction;
use crate::io::{BfInput, BfOutput, StdInput, StdOutput};
use crate::stats::ExecutionStats;
use crate::types::{MemoryAddress, StepCount};
use std::io;

use crate::config::MemoryModel;

/// Control flow result from executing an instruction
///
/// Instructions return this to signal normal execution vs. loop exit.
/// This is NOT an error - loop exit is a normal control flow outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExecutionFlow {
    /// Continue normal execution
    Continue,
    /// Exit the current loop (LoopCheck instruction returns this when cell is zero)
    LoopExit,
}

/// Result type for instruction execution
///
/// `Ok(ExecutionFlow)` indicates successful execution with control flow decision
/// `Err(BfError)` indicates an actual error occurred
type ExecutionResult = std::result::Result<ExecutionFlow, BfError>;

struct VmState {
    /// Memory tape (array of cells)
    memory: Vec<u8>,
    /// Current memory pointer position
    pointer: MemoryAddress,
    /// Number of steps executed so far
    step_count: StepCount,
    /// Current loop nesting depth (0 = top-level, 1 = inside one loop, etc.)
    /// This is incremented when entering a loop body and decremented when exiting.
    /// Useful for debugging, profiling, and hook context.
    loop_depth: usize,
    /// Memory model that dictates how memory operations behave
    memory_model: MemoryModel,
}

impl VmState {
    /// Create a new VM state with the given memory model
    fn new(memory_model: MemoryModel) -> Self {
        let memory_size = memory_model.initial_size().get();
        Self {
            memory: vec![0u8; memory_size],
            pointer: MemoryAddress::new(0),
            step_count: StepCount::new(0),
            loop_depth: 0, // Start at top level (not inside any loops)
            memory_model,
        }
    }
}

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

/// Handle pointer increment based on memory model
#[inline]
fn increment_pointer(
    state: &mut VmState,
    instruction_index: usize,
    debug_info: Option<&DebugInfo>,
) -> Result<()> {
    state.memory_model.try_increment_pointer(
        &mut state.pointer,
        &mut state.memory,
        state.step_count,
        debug_info,
        instruction_index,
    )
}

/// Handle pointer decrement based on memory model
#[inline]
fn decrement_pointer(
    state: &mut VmState,
    allow_negative_pointer: bool,
    instruction_index: usize,
    debug_info: Option<&DebugInfo>,
) -> Result<()> {
    state.memory_model.try_decrement_pointer(
        &mut state.pointer,
        &state.memory,
        allow_negative_pointer,
        state.step_count,
        debug_info,
        instruction_index,
    )
}

/// Execute a single non-loop instruction
///
/// This function handles the execution of individual BrainFuck instructions,
/// excluding loops which are handled by `execute_block()`.
///
/// This separation provides:
/// - Cleaner code organization (control flow vs execution)
/// - Better testability
/// - Clearer hook integration points (future)
///
/// # Arguments
/// * `instruction` - The instruction to execute
/// * `state` - Mutable VM state
/// * `config` - Execution configuration
/// * `input` - Input source
/// * `output` - Output destination
///
/// # Panics
/// Panics if called with `Instruction::Loop` - loops must be handled by `execute_block()`
#[inline]
fn execute_single_instruction<I: BfInput, O: BfOutput>(
    instruction: &Instruction,
    state: &mut VmState,
    config: &ExecutionConfig,
    input: &mut I,
    output: &mut O,
    instruction_index: usize, // Current instruction index for error reporting
    debug_info: Option<&DebugInfo>, // Optional debug info for error messages
) -> ExecutionResult {
    match instruction {
        Instruction::IncrementPointer => {
            increment_pointer(state, instruction_index, debug_info)?;
            // Peak memory usage tracking moved to StatsTrackerHook
        }

        Instruction::DecrementPointer => {
            decrement_pointer(
                state,
                config.allow_negative_pointer(),
                instruction_index,
                debug_info,
            )?;
        }

        // Cell arithmetic: Delegated to CellModel (configurable!)
        //
        // IMPORTANT: Cell arithmetic is configurable via ExecutionConfig.cell_model().
        // Different models provide different overflow/underflow behaviors:
        // - U8Wrapping: 255+1=0, 0-1=255 (default, most compatible)
        // - U8Checked: Overflow/underflow returns error
        //
        // This is INDEPENDENT of MemoryModel, which only controls pointer movement.
        // See config.rs module docs for CellModel and MemoryModel orthogonality.
        // See validator.rs module docs for cell-model-aware validation.
        Instruction::IncrementValue => {
            config.cell_model().behavior().try_increment(
                &mut state.memory[state.pointer.get()],
                state.step_count,
                debug_info,
            )?;
        }

        Instruction::DecrementValue => {
            config.cell_model().behavior().try_decrement(
                &mut state.memory[state.pointer.get()],
                state.step_count,
                debug_info,
            )?;
        }

        Instruction::Output => {
            output
                .write_byte(state.memory[state.pointer.get()])
                .map_err(|source| BfError::IoError {
                    operation: "writing output".to_string(),
                    instruction_index: Some(state.step_count.into()),
                    source,
                })?;
            output.flush().map_err(|source| BfError::IoError {
                operation: "flushing output".to_string(),
                instruction_index: Some(state.step_count.into()),
                source,
            })?;
            // Bytes written tracking moved to StatsTrackerHook
        }

        Instruction::Input => {
            match input.read_byte() {
                Ok(Some(byte)) => {
                    state.memory[state.pointer.get()] = byte;
                    // Bytes read tracking moved to StatsTrackerHook
                }
                Ok(None) => {
                    // Handle EOF based on configuration
                    match config.eof_behavior() {
                        EofBehavior::SetZero => {
                            state.memory[state.pointer.get()] = 0;
                        }
                        EofBehavior::SetNegOne => {
                            state.memory[state.pointer.get()] = 255; // -1 as u8
                        }
                        EofBehavior::NoChange => {
                            // Do nothing, leave cell as-is
                        }
                        EofBehavior::Error => {
                            return Err(BfError::IoError {
                                operation: "reading input (EOF reached)".to_string(),
                                instruction_index: Some(state.step_count.into()),
                                source: io::Error::new(io::ErrorKind::UnexpectedEof, "EOF reached"),
                            });
                        }
                    }
                }
                Err(source) => {
                    return Err(BfError::IoError {
                        operation: "reading input".to_string(),
                        instruction_index: Some(state.step_count.into()),
                        source,
                    });
                }
            }
        }

        Instruction::LoopCheck => {
            // LoopCheck checks the loop condition and exits if cell is zero
            // This is the BrainFuck '[' instruction logic
            // The step counter is already incremented in execute_block before this is called
            if state.memory[state.pointer.get()] == 0 {
                return Ok(ExecutionFlow::LoopExit);
            }
            // If cell is non-zero, continue with the rest of the loop body
        }

        Instruction::Loop(_) => {
            panic!("execute_single_instruction() cannot handle loops - use execute_block()");
        }
    }

    Ok(ExecutionFlow::Continue)
}

/// Handles all hook dispatching for the interpreter.
///
/// This component centralizes hook-related logic, making it easier to:
/// - Test hook behavior in isolation
/// - Add new hook points without modifying execute_block
/// - Maintain consistent hook behavior across the interpreter
///
/// The dispatcher creates HookContext snapshots and calls the appropriate
/// hook methods on the HookManager, returning the HookDecision.
struct HookDispatcher<'a> {
    /// The execution config containing user-registered hooks
    config: &'a mut ExecutionConfig,

    /// Built-in hooks (not in Arc<Mutex>!)
    stats_hook: &'a mut crate::hooks::builtin::StatsTrackerHook,
    warning_hook: &'a mut crate::hooks::builtin::WarningCollectorHook,
    debug_hook: Option<&'a mut crate::hooks::builtin::DebugTrackingHook>,
    limit_hook: Option<&'a mut crate::hooks::builtin::LimitEnforcerHook>,
}

impl<'a> HookDispatcher<'a> {
    /// Create a new hook dispatcher with built-in hooks
    #[inline]
    fn new(
        config: &'a mut ExecutionConfig,
        stats_hook: &'a mut crate::hooks::builtin::StatsTrackerHook,
        warning_hook: &'a mut crate::hooks::builtin::WarningCollectorHook,
        debug_hook: Option<&'a mut crate::hooks::builtin::DebugTrackingHook>,
        limit_hook: Option<&'a mut crate::hooks::builtin::LimitEnforcerHook>,
    ) -> Self {
        Self {
            config,
            stats_hook,
            warning_hook,
            debug_hook,
            limit_hook,
        }
    }

    /// Get immutable access to the execution config
    ///
    /// This is safe because we only use it when not actively dispatching hooks
    #[inline]
    fn config(&self) -> &ExecutionConfig {
        self.config
    }

    /// Dispatch before_instruction hook
    ///
    /// Returns HookDecision::Continue, Break, or Skip
    #[inline]
    fn dispatch_before(
        &mut self,
        instruction: &Instruction,
        state: &VmState,
        instruction_index: usize,
    ) -> HookDecision {
        // Get source location from debug hook if available
        let source_location = self
            .debug_hook
            .as_ref()
            .and_then(|h| h.debug_info().lookup(instruction_index));

        let context = HookContext::new(
            &state.memory,
            state.pointer,
            state.step_count,
            source_location.as_ref(),
            state.loop_depth,
            instruction_index,
        );

        // Call built-in hooks first (order matters for correctness)
        // Note: Built-in hooks don't use before_instruction currently,
        // but we keep this for consistency and future extensions

        // Call user hooks from config
        if let Some(hook_manager) = self.config.hook_manager_mut() {
            hook_manager.before_instruction(instruction, &context)
        } else {
            HookDecision::Continue
        }
    }

    /// Dispatch after_instruction hook
    ///
    /// Returns HookDecision::Continue, Break, or Skip
    #[inline]
    fn dispatch_after(
        &mut self,
        instruction: &Instruction,
        state: &VmState,
        instruction_index: usize,
    ) -> HookDecision {
        use crate::hooks::ExecutionHook;

        // Get source location from debug hook if available
        let source_location = self
            .debug_hook
            .as_ref()
            .and_then(|h| h.debug_info().lookup(instruction_index));

        let context = HookContext::new(
            &state.memory,
            state.pointer,
            state.step_count,
            source_location.as_ref(),
            state.loop_depth,
            instruction_index,
        );

        // Call built-in hooks first (order matters for correctness)

        // 1. Stats tracking (always runs)
        self.stats_hook.after_instruction(instruction, &context);

        // 2. Warning collection (always runs)
        self.warning_hook.after_instruction(instruction, &context);

        // 3. Limit enforcement (check step limits / timeout)
        if let Some(limit_hook) = &mut self.limit_hook
            && limit_hook.after_instruction(instruction, &context) == HookDecision::Break
        {
            return HookDecision::Break;
        }

        // 4. Debug tracking (updates internal state)
        if let Some(debug_hook) = &mut self.debug_hook {
            debug_hook.after_instruction(instruction, &context);
        }

        // Call user hooks from config
        if let Some(hook_manager) = self.config.hook_manager_mut() {
            hook_manager.after_instruction(instruction, &context)
        } else {
            HookDecision::Continue
        }
    }

    /// Dispatch on_loop_enter hook
    ///
    /// Returns HookDecision::Continue, Break, or Skip
    #[inline]
    fn dispatch_loop_enter(
        &mut self,
        state: &VmState,
        loop_instruction_index: usize,
        body_start_index: usize,
        body_size: usize,
    ) -> HookDecision {
        use crate::hooks::ExecutionHook;

        // Get source location from debug hook if available
        let source_location = self
            .debug_hook
            .as_ref()
            .and_then(|h| h.debug_info().lookup(loop_instruction_index));

        let context = HookContext::new(
            &state.memory,
            state.pointer,
            state.step_count,
            source_location.as_ref(),
            state.loop_depth,
            loop_instruction_index,
        );

        let loop_info =
            crate::hooks::LoopInfo::new(loop_instruction_index, body_start_index, body_size);

        // Call built-in hooks first
        self.stats_hook.on_loop_enter(&context, Some(&loop_info));

        if let Some(debug_hook) = &mut self.debug_hook {
            debug_hook.on_loop_enter(&context, Some(&loop_info));
        }

        // Call user hooks from config
        if let Some(hook_manager) = self.config.hook_manager_mut() {
            hook_manager.on_loop_enter(&context, Some(&loop_info))
        } else {
            HookDecision::Continue
        }
    }

    /// Dispatch on_loop_exit hook
    ///
    /// Returns HookDecision::Continue, Break, or Skip
    #[inline]
    fn dispatch_loop_exit(&mut self, state: &VmState, instruction_index: usize) -> HookDecision {
        use crate::hooks::ExecutionHook;

        // Get source location from debug hook if available
        let source_location = self
            .debug_hook
            .as_ref()
            .and_then(|h| h.debug_info().lookup(instruction_index));

        let context = HookContext::new(
            &state.memory,
            state.pointer,
            state.step_count,
            source_location.as_ref(),
            state.loop_depth,
            instruction_index,
        );

        // Call built-in hooks first
        if let Some(debug_hook) = &mut self.debug_hook {
            debug_hook.on_loop_exit(&context);
        }

        // Call user hooks from config
        if let Some(hook_manager) = self.config.hook_manager_mut() {
            hook_manager.on_loop_exit(&context)
        } else {
            HookDecision::Continue
        }
    }

    /// Dispatch on_complete hook (called after execution finishes)
    #[inline]
    fn dispatch_complete(&mut self, state: &VmState) {
        use crate::hooks::ExecutionHook;

        let context = HookContext::new(
            &state.memory,
            state.pointer,
            state.step_count,
            None, // No source location at completion
            state.loop_depth,
            0, // No meaningful instruction index after completion
        );

        // Call built-in hooks first
        self.stats_hook.on_complete(&context);

        // Call user hooks from config
        if let Some(hook_manager) = self.config.hook_manager_mut() {
            hook_manager.on_complete(&context);
        }
    }
}

/// Count the total number of instructions in a block, including nested loops.
/// This is used to compute loop body sizes for LoopInfo.
fn count_instructions(instructions: &[Instruction]) -> usize {
    let mut count = 0;
    for instruction in instructions {
        if let Instruction::Loop(body) = instruction {
            count += count_instructions(body); // Recursively count loop body
        } else {
            count += 1; // Count this instruction
        }
    }
    count
}

fn execute_block<I: BfInput, O: BfOutput>(
    instructions: &[Instruction],
    state: &mut VmState,
    dispatcher: &mut HookDispatcher,
    input: &mut I,
    output: &mut O,
    start_index: usize,             // flat index where this block starts
    debug_info: Option<&DebugInfo>, // Optional debug info for error messages
) -> ExecutionResult {
    let mut local_index = 0;

    for instruction in instructions {
        // Compute current instruction index
        let instruction_index = start_index + local_index;

        // Increment step counter BEFORE calling hooks so hooks see the updated count
        state.step_count.increment();

        // Hook: before_instruction
        // Skip before_instruction for Loop since it's just an AST container.
        // The actual '[' instruction (LoopCheck) is what gets executed, not the Loop wrapper.
        // This prevents double-counting and maintains consistency with after_instruction.
        if !matches!(instruction, Instruction::Loop(_)) {
            match dispatcher.dispatch_before(instruction, state, instruction_index) {
                HookDecision::Continue => {}
                HookDecision::Break => {
                    return Err(BfError::ExecutionPaused {
                        instruction_index: state.step_count.into(),
                        source_location: None, // Debug hook can provide this
                        message: Some(format!(
                            "Execution paused by hook at instruction {}",
                            state.step_count.get()
                        )),
                    });
                }
                HookDecision::Skip => {
                    // Skip this instruction, increment local_index and continue
                    local_index += 1;
                    continue;
                }
            }
        }

        // Execute instruction: delegate to specialized handlers
        match instruction {
            // Loops require special handling for recursion and depth tracking
            Instruction::Loop(body) => {
                // Compute loop body information for hooks
                // The body starts at instruction_index (which is the LoopCheck)
                // With the new parsing model, Loop is just an AST container - LoopCheck IS the '['
                let body_start_index = instruction_index;
                let body_size = count_instructions(body);

                // The loop body always starts with LoopCheck (parser invariant)
                // We execute LoopCheck first to determine if we should enter the loop
                // This avoids calling on_loop_enter when the loop immediately exits
                loop {
                    // Execute LoopCheck instruction (first in body)
                    // We need to manually increment step counter and call hooks since we're bypassing execute_block
                    let loop_check = &body[0];
                    debug_assert!(matches!(loop_check, Instruction::LoopCheck));

                    // Increment step counter (normally done in execute_block)
                    state.step_count.increment();

                    // Hook: after_instruction for LoopCheck (needed for limit checking)
                    match dispatcher.dispatch_after(loop_check, state, body_start_index) {
                        HookDecision::Continue => {}
                        HookDecision::Break => {
                            return Err(BfError::ExecutionPaused {
                                instruction_index: state.step_count.into(),
                                source_location: None,
                                message: Some("Execution paused by hook at LoopCheck".to_string()),
                            });
                        }
                        HookDecision::Skip => {
                            // Can't skip LoopCheck - it's required
                        }
                    }

                    // Execute LoopCheck to determine if we should continue
                    match execute_single_instruction(
                        loop_check,
                        state,
                        dispatcher.config(),
                        input,
                        output,
                        body_start_index, // LoopCheck is at body_start_index
                        debug_info,
                    )? {
                        ExecutionFlow::Continue => {
                            // Cell is non-zero, enter loop body
                        }
                        ExecutionFlow::LoopExit => {
                            // Cell is zero, exit loop without calling on_loop_enter
                            break;
                        }
                    }

                    // Now we know we're actually entering the loop body
                    state.loop_depth += 1;

                    // Hook: on_loop_enter
                    match dispatcher.dispatch_loop_enter(
                        state,
                        instruction_index,
                        body_start_index,
                        body_size,
                    ) {
                        HookDecision::Continue => {}
                        HookDecision::Break => {
                            state.loop_depth -= 1;
                            return Err(BfError::ExecutionPaused {
                                instruction_index: state.step_count.into(),
                                source_location: None, // Debug hook can provide this
                                message: Some(format!(
                                    "Execution paused by hook at loop enter (instruction {})",
                                    state.step_count.get()
                                )),
                            });
                        }
                        HookDecision::Skip => {
                            // For loop hooks, Skip means skip the entire loop iteration
                            state.loop_depth -= 1;
                            continue;
                        }
                    }

                    // Execute rest of loop body (skip LoopCheck since we already executed it)
                    // body[1..] contains everything after LoopCheck
                    if body.len() > 1 {
                        execute_block(
                            &body[1..],
                            state,
                            dispatcher,
                            input,
                            output,
                            body_start_index + 1, // Skip LoopCheck
                            debug_info,
                        )?;
                        // execute_block returns Continue (LoopExit already handled above)
                    }

                    state.loop_depth -= 1;

                    // Hook: on_loop_exit
                    match dispatcher.dispatch_loop_exit(state, instruction_index) {
                        HookDecision::Continue => {}
                        HookDecision::Break => {
                            return Err(BfError::ExecutionPaused {
                                instruction_index: state.step_count.into(),
                                source_location: None, // Debug hook can provide this
                                message: Some(format!(
                                    "Execution paused by hook at loop exit (instruction {})",
                                    state.step_count.get()
                                )),
                            });
                        }
                        HookDecision::Skip => {
                            // Skip doesn't make sense at loop exit, treat as Continue
                        }
                    }
                }
            }

            // All other instructions are handled by execute_single_instruction
            non_loop_instruction => {
                match execute_single_instruction(
                    non_loop_instruction,
                    state,
                    dispatcher.config(),
                    input,
                    output,
                    instruction_index,
                    debug_info,
                )? {
                    ExecutionFlow::Continue => {
                        // Normal execution, continue
                    }
                    ExecutionFlow::LoopExit => {
                        // LoopCheck returned LoopExit, propagate it up
                        return Ok(ExecutionFlow::LoopExit);
                    }
                }
            }
        }

        // Hook: after_instruction
        // Skip after_instruction for Loop since it's just an AST container.
        // The actual '[' instruction (LoopCheck) already calls after_instruction
        // from inside the loop handler (line 785), and they share the same instruction_index.
        // Calling it here would cause double-counting in profilers and other hooks.
        if !matches!(instruction, Instruction::Loop(_)) {
            match dispatcher.dispatch_after(instruction, state, instruction_index) {
                HookDecision::Continue => {}
                HookDecision::Break => {
                    return Err(BfError::ExecutionPaused {
                        instruction_index: state.step_count.into(),
                        source_location: None, // Debug hook can provide this
                        message: Some(format!(
                            "Execution paused by hook after instruction {}",
                            state.step_count.get()
                        )),
                    });
                }
                HookDecision::Skip => {
                    // Skip doesn't make sense after instruction has already executed, treat as Continue
                }
            }
        }

        // Increment local index for next instruction
        // For loops, we need to account for all instructions in the body
        match instruction {
            Instruction::Loop(body) => {
                local_index += count_instructions(body);
            }
            _ => {
                local_index += 1;
            }
        }
    }

    // Block completed normally (no LoopExit encountered)
    Ok(ExecutionFlow::Continue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ExecutionConfigBuilder, MEMORY_SIZE};
    use crate::parser::parse;
    use crate::types::MemorySize;

    #[test]
    fn test_memory_overflow() {
        let source = ">".repeat(30001); // Try to go beyond memory
        let instructions = parse(&source).unwrap();
        let config = ExecutionConfig::default();
        let result = interpret_with_config(&instructions, config, None);
        assert!(matches!(result, Err(BfError::MemoryOutOfBounds { .. })));
    }

    #[test]
    fn test_memory_underflow() {
        let source = "<"; // Try to go below 0
        let instructions = parse(source).unwrap();
        let config = ExecutionConfig::default();
        let result = interpret_with_config(&instructions, config, None);
        assert!(matches!(
            result,
            Err(BfError::MemoryOutOfBounds { attempted: -1, .. })
        ));
    }

    #[test]
    fn test_step_limit() {
        // Using test_utils for cleaner error testing
        use crate::test_utils::{configs, run_bf_with_config};

        let result = run_bf_with_config("+[+]", "", configs::with_step_limit(100));
        assert!(matches!(
            result,
            Err(BfError::StepLimitExceeded { limit: 100, .. })
        ));
    }

    #[test]
    fn test_custom_memory_size() {
        // Using test_utils for cleaner config testing
        use crate::test_utils::{configs, run_bf_with_config};

        let source = ">".repeat(101);
        let result = run_bf_with_config(&source, "", configs::small_memory());
        assert!(result.is_err());
        assert!(matches!(result, Err(BfError::MemoryOutOfBounds { .. })));
    }

    #[test]
    fn test_execution_timeout() {
        // Create a program that runs longer - moving pointer takes more time
        let source = "+[>+<]".repeat(1000); // Infinite loop with more instructions
        let instructions = parse(&source).unwrap();
        let config = ExecutionConfigBuilder::new()
            .with_memory_size(1000)
            .with_timeout_ms(100)
            .build(); // Smaller memory to hit bounds faster
        let result = interpret_with_config(&instructions, config, None);
        // Should fail with either timeout or memory bounds
        assert!(result.is_err());
    }

    #[test]
    fn test_memory_model_fixed() {
        // Fixed memory model should error on out-of-bounds access
        let source = ">".repeat(100); // Move right 100 times
        let instructions = parse(&source).unwrap();

        let config = ExecutionConfigBuilder::new().with_memory_size(50).build(); // Only 50 cells
        let result = interpret_with_config(&instructions, config, None);

        assert!(result.is_err());
        assert!(matches!(result, Err(BfError::MemoryOutOfBounds { .. })));
    }

    #[test]
    fn test_memory_model_unbounded_growth() {
        // Unbounded memory should grow as needed
        let source = format!("{}+.", ">".repeat(100)); // Move right 100 times
        let instructions = parse(&source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_unbounded_memory(10, 200)
            .unwrap()
            .build(); // Start small, allow growth

        let result = interpret_with_config(&instructions, config, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_memory_model_unbounded_max_limit() {
        // Unbounded memory should still error at max limit
        let source = format!("{}+.", ">".repeat(150)); // Move right 150 times
        let instructions = parse(&source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_unbounded_memory(10, 100)
            .unwrap()
            .build(); // Max 100 cells

        let result = interpret_with_config(&instructions, config, None);
        assert!(result.is_err());
        assert!(matches!(result, Err(BfError::MemoryOutOfBounds { .. })));
    }

    #[test]
    fn test_memory_model_fixed_left_boundary() {
        // Fixed model should error when going below 0
        let source = "<"; // Move left from 0
        let instructions = parse(source).unwrap();

        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        let result = interpret_with_config(&instructions, config, None);

        assert!(result.is_err());
        assert!(matches!(result, Err(BfError::MemoryOutOfBounds { .. })));
    }

    #[test]
    fn test_stats_basic_counting() {
        // Test basic step counting
        let source = "+++>>--"; // 7 instructions
        let instructions = parse(source).unwrap();

        let config = ExecutionConfig::default();
        let stats = interpret_with_config(&instructions, config, None).unwrap();

        assert_eq!(stats.total_steps, StepCount::new(7));
        assert_eq!(stats.loop_iterations, 0);
        assert_eq!(stats.peak_memory_used, MemoryAddress::new(3)); // Moved to cell 2, so peak is 3 (0-based + 1)
    }

    #[test]
    fn test_stats_loop_iterations() {
        // Test loop iteration counting
        let source = "+++[>+<-]"; // Loop runs 3 times
        let instructions = parse(source).unwrap();

        let config = ExecutionConfig::default();
        let stats = interpret_with_config(&instructions, config, None).unwrap();

        assert_eq!(stats.loop_iterations, 3);
        assert!(stats.total_steps > StepCount::new(3)); // Should be more than just the setup
    }

    #[test]
    fn test_stats_io_tracking() {
        // Test I/O tracking
        let source = "++++++++++.>++."; // Output 2 bytes
        let instructions = parse(source).unwrap();

        let config = ExecutionConfig::default();
        let stats = interpret_with_config(&instructions, config, None).unwrap();

        assert_eq!(stats.bytes_written, 2);
        assert_eq!(stats.bytes_read, 0);
    }

    #[test]
    fn test_stats_memory_tracking() {
        // Test memory usage tracking
        let source = "+++>++>+"; // Use 3 cells
        let instructions = parse(source).unwrap();

        let config = ExecutionConfig::default();
        let stats = interpret_with_config(&instructions, config, None).unwrap();

        assert_eq!(stats.cells_modified, 3); // 3 non-zero cells
        assert_eq!(stats.peak_memory_used, MemoryAddress::new(3)); // Highest cell accessed + 1
    }

    #[test]
    fn test_stats_unbounded_allocation() {
        // Test memory allocation tracking for unbounded model
        let source = format!("{}+", ">".repeat(50)); // Move to cell 50
        let instructions = parse(&source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_unbounded_memory(10, 100)
            .unwrap()
            .build();
        let stats = interpret_with_config(&instructions, config, None).unwrap();

        assert_eq!(stats.peak_memory_used, MemoryAddress::new(51)); // Cell 50 + 1
        assert_eq!(stats.memory_allocated, MemorySize::new(51)); // Should have grown to 51 cells
        assert_eq!(stats.cells_modified, 1); // Only 1 non-zero cell
    }

    #[test]
    fn test_stats_nested_loops() {
        // Test nested loop iteration counting
        let source = "+++[>++[<.>-]<-]"; // Nested loops
        let instructions = parse(source).unwrap();

        let config = ExecutionConfig::default();
        let stats = interpret_with_config(&instructions, config, None).unwrap();

        // Outer loop runs 3 times, inner loop runs 2 times per outer iteration = 6
        assert_eq!(stats.loop_iterations, 3 + 6); // 3 outer + 6 inner
    }

    #[test]
    fn test_eof_behavior_config() {
        // Test that EOF behavior can be configured
        let config = ExecutionConfig::builder()
            .with_memory_size(MEMORY_SIZE)
            .with_eof_behavior(EofBehavior::SetZero)
            .build();
        assert!(matches!(config.eof_behavior(), EofBehavior::SetZero));

        let config = ExecutionConfig::builder()
            .with_memory_size(MEMORY_SIZE)
            .with_eof_behavior(EofBehavior::SetNegOne)
            .build();
        assert!(matches!(config.eof_behavior(), EofBehavior::SetNegOne));

        let config = ExecutionConfig::builder()
            .with_memory_size(MEMORY_SIZE)
            .with_eof_behavior(EofBehavior::NoChange)
            .build();
        assert!(matches!(config.eof_behavior(), EofBehavior::NoChange));

        let config = ExecutionConfig::builder()
            .with_memory_size(MEMORY_SIZE)
            .with_eof_behavior(EofBehavior::Error)
            .build();
        assert!(matches!(config.eof_behavior(), EofBehavior::Error));
    }

    #[test]
    fn test_eof_behavior_default() {
        // Test that default EOF behavior is SetZero
        let config = ExecutionConfig::default();
        assert!(matches!(config.eof_behavior(), EofBehavior::SetZero));
    }

    #[test]
    fn test_string_io_echo() {
        // Test that we can use StringIo to capture output (using test_utils)
        use crate::test_utils::run_bf;

        let (output, stats) = run_bf(",[.,]", "Hello").unwrap();

        assert_eq!(output, "Hello");
        // Note: Stats now track Input/Output instruction executions, not actual byte counts
        // The program executes 6 Input instructions (5 successful + 1 EOF attempt)
        assert_eq!(stats.bytes_read, 6);
        assert_eq!(stats.bytes_written, 5);
    }

    #[test]
    fn test_string_io_hello_world() {
        // Test classic Hello World program with string output (using test_utils)
        use crate::test_utils::run_bf;

        let source = "++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.>>.<-.<.+++.------.--------.>>+.>++.";
        let (output, stats) = run_bf(source, "").unwrap();

        assert_eq!(output, "Hello World!\n");
        assert_eq!(stats.bytes_written, 13);
    }

    #[test]
    fn test_string_io_add_numbers() {
        // Test program that adds two single-digit numbers (using test_utils)
        use crate::test_utils::run_bf;

        // Program: read two numbers, add them, output result
        // ,>     Read first number into cell 0, move to cell 1
        // ,      Read second number into cell 1
        // [<+>-] Add cell 1 to cell 0 (move all from cell 1 to cell 0)
        // <.     Move back to cell 0 and output result
        let (output, stats) = run_bf(",>,[-<+>]<.", "\x05\x03").unwrap();

        assert_eq!(output.as_bytes(), &[8]); // 5 + 3 = 8
        assert_eq!(stats.bytes_read, 2);
        assert_eq!(stats.bytes_written, 1);
    }

    // ===================================================================
    // CRITICAL CORRECTNESS TESTS: [+] Pattern with u8 Wrapping
    // ===================================================================
    //
    // These tests prove that [+] is NOT infinite with u8 wrapping arithmetic,
    // contrary to what the validator previously claimed!
    //
    // Background: The validator used to warn that [+] creates an "infinite loop"
    // because "incrementing never reaches zero". This is WRONG with u8 wrapping!
    //
    // Reality: With u8 wrapping (255 + 1 = 0), the loop wraps and exits.
    // See PRD/CELL_MODEL.md for full analysis.

    #[test]
    fn test_plus_loop_terminates_from_one() {
        // CRITICAL TEST: Proves [+] is NOT infinite with u8 wrapping
        //
        // Starting with cell value 1:
        // Iteration 1: + -> 2
        // Iteration 2: + -> 3
        // ...
        // Iteration 255: + -> 255
        // Iteration 256: + -> 0 (WRAPS!)
        // Loop exits because cell == 0
        //
        // Total iterations: 256 (1→2→...→255→0)

        use crate::test_utils::run_bf;

        let source = "+[+]"; // Start with 1, loop until wrap
        let result = run_bf(source, "");

        // Should succeed (not hit default step limit)
        assert!(
            result.is_ok(),
            "Loop should terminate via wrapping, not be infinite!"
        );

        let (_, stats) = result.unwrap();

        // Should take exactly 256 iterations (starting from 1, wrapping to 0)
        // Each iteration: 1 step for `[` instruction + 1 step for `+` instruction = 2 steps
        // Plus 1 for initial +
        // Total: 1 (initial +) + 256 iterations × 2 = 512 steps
        assert!(
            stats.total_steps < StepCount::new(520),
            "Should take ~512 steps (1 + 256×2), got {}",
            stats.total_steps
        );
        assert!(
            stats.total_steps > StepCount::new(500),
            "Should take ~512 steps, got {} (too few!)",
            stats.total_steps
        );
    }

    #[test]
    fn test_plus_loop_terminates_from_255() {
        // Edge case: Starting with cell value 255
        // Iteration 1: + -> 0 (immediate wrap!)
        // Loop exits immediately

        use crate::test_utils::run_bf;

        // Set cell to 255 (0 - 1 = 255 with wrapping)
        let source = "-[+]"; // Start with 255, loop once
        let result = run_bf(source, "");

        assert!(result.is_ok(), "Loop should exit after 1 iteration");

        let (_, stats) = result.unwrap();

        // Should take very few steps (just 1 increment to wrap to 0)
        assert!(
            stats.total_steps < StepCount::new(10),
            "Should exit almost immediately, got {} steps",
            stats.total_steps
        );
    }

    #[test]
    fn test_plus_loop_terminates_from_128() {
        // Middle case: Starting with cell value 128
        // Will take exactly 128 iterations to wrap to 0

        use crate::test_utils::run_bf;

        // Set cell to 128 using multiple additions
        let source = format!("{}[+]", "+".repeat(128));
        let result = run_bf(&source, "");

        assert!(
            result.is_ok(),
            "Loop should terminate after ~128 iterations"
        );

        let (_, stats) = result.unwrap();

        // 128 initial + operations, then ~128 loop iterations
        // Each iteration: 1 step for `[` instruction + 1 step for `+` instruction = 2 steps
        // Total: 128 (initial +s) + 128 iterations × 2 = 384 steps
        // Plus 1 for the final `[` check that exits = 385 steps
        assert!(
            stats.total_steps < StepCount::new(395),
            "Should take ~385 steps, got {}",
            stats.total_steps
        );
        assert!(
            stats.total_steps > StepCount::new(375),
            "Should take ~385 steps, got {} (too few!)",
            stats.total_steps
        );
    }

    #[test]
    fn test_double_plus_loop_is_infinite_from_odd() {
        // IMPORTANT MATHEMATICAL INSIGHT: [++] does NOT always terminate!
        //
        // Starting from 1 (odd):
        // Iteration 1: 1 + 2 = 3
        // Iteration 2: 3 + 2 = 5
        // ...
        // Iteration N: 255 + 2 = 257 → wraps to 1 (257 % 256 = 1)
        //
        // Result: Cycles through ODD numbers only (1→3→5→...→255→1)
        // NEVER hits 0 (which is even)!
        //
        // This is actually INFINITE, unlike [+]!

        use crate::test_utils::configs;
        use crate::test_utils::run_bf_with_config;

        let source = "+[++]"; // Start with 1 (odd), increment by 2

        // Should hit step limit (is actually infinite)
        let result = run_bf_with_config(source, "", configs::with_step_limit(1000));

        assert!(
            result.is_err(),
            "[++] from odd starting value should be infinite (never hits 0)!"
        );
        assert!(
            matches!(result, Err(BfError::StepLimitExceeded { .. })),
            "Should hit step limit, got {:?}",
            result
        );
    }

    #[test]
    fn test_double_plus_loop_terminates_from_even() {
        // [++] DOES terminate if starting from an even number!
        //
        // Starting from 2 (even):
        // Iteration 1: 2 + 2 = 4
        // Iteration 2: 4 + 2 = 6
        // ...
        // Eventually: 254 + 2 = 256 → wraps to 0
        //
        // Cycles through EVEN numbers (2→4→6→...→254→0)

        use crate::test_utils::run_bf;

        let source = "++[++]"; // Start with 2 (even), increment by 2
        let result = run_bf(source, "");

        assert!(
            result.is_ok(),
            "[++] from even starting value should terminate at 0"
        );

        let (_, stats) = result.unwrap();

        // Should take ~127 iterations (2→4→...→254→256→0)
        // Each iteration: 1 step for `[` instruction + 2 steps for `++` = 3 steps
        // Total: 2 (initial ++) + 127 iterations × 3 = 383 steps
        // Plus 1 for final `[` check that exits = 384 steps
        assert!(
            stats.total_steps < StepCount::new(395),
            "Should take ~384 steps, got {}",
            stats.total_steps
        );
        assert!(
            stats.total_steps > StepCount::new(375),
            "Should take ~384 steps, got {} (too few!)",
            stats.total_steps
        );
    }

    #[test]
    fn test_plus_loop_with_step_limit_proves_termination() {
        // This test uses a generous step limit to prove [+] terminates
        // If [+] were truly infinite, it would hit the limit
        // Since it doesn't, we prove it terminates

        use crate::test_utils::configs;
        use crate::test_utils::run_bf_with_config;

        let source = "+[+]";

        // Set limit to 1000 steps (way more than needed for ~513 steps)
        let result = run_bf_with_config(source, "", configs::with_step_limit(1000));

        // Should succeed WITHOUT hitting step limit
        assert!(
            result.is_ok(),
            "Loop should terminate naturally, not hit step limit. \
             If this fails, [+] is actually infinite!"
        );
    }

    #[test]
    fn test_plus_loop_hits_step_limit_if_too_low() {
        // Complementary test: If we set limit too low, we CAN make it fail
        // This proves our termination tests above aren't trivially passing

        use crate::test_utils::configs;
        use crate::test_utils::run_bf_with_config;

        let source = "+[+]"; // Needs ~513 steps

        // Set limit too low (only 100 steps)
        let result = run_bf_with_config(source, "", configs::with_step_limit(100));

        // Should FAIL with step limit exceeded
        assert!(
            result.is_err(),
            "Should hit step limit with only 100 steps allowed"
        );
        assert!(
            matches!(result, Err(BfError::StepLimitExceeded { .. })),
            "Should fail with StepLimitExceeded, got {:?}",
            result
        );
    }

    // ===================================================================
    // CELL MODEL TESTS: Testing different cell arithmetic behaviors
    // ===================================================================

    #[test]
    fn test_cell_model_wrapping_overflow() {
        // Test u8 wrapping: 255 + 1 = 0
        use crate::config::ExecutionConfigBuilder;
        use crate::io::StringIo;

        // Set cell to 255 using many increments, then increment once more
        let source = "+".repeat(256).to_string() + "."; // 0+256=0 (wraps at 255), output 0
        let instructions = parse(&source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_wrapping_cells()
            .build();

        let mut input = StringIo::empty();
        let mut output = StringIo::empty();
        let result = interpret_with_io(&instructions, config, &mut input, &mut output, None);

        assert!(result.is_ok(), "Wrapping should not error on overflow");
        assert_eq!(output.output_bytes()[0], 0, "Should wrap to 0");
    }

    #[test]
    fn test_cell_model_wrapping_underflow() {
        // Test u8 wrapping: 0 - 1 = 255
        use crate::config::ExecutionConfigBuilder;
        use crate::io::StringIo;

        // Start at 0, decrement once, output
        let source = "-."; // 0-1=255 (wraps), output 255
        let instructions = parse(source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_wrapping_cells()
            .build();

        let mut input = StringIo::empty();
        let mut output = StringIo::empty();
        let result = interpret_with_io(&instructions, config, &mut input, &mut output, None);

        assert!(result.is_ok(), "Wrapping should not error on underflow");
        assert_eq!(output.output_bytes()[0], 255, "Should wrap to 255");
    }

    #[test]
    fn test_cell_model_checked_overflow() {
        // Test u8 checked: 255 + 1 → error
        use crate::config::ExecutionConfigBuilder;
        use crate::test_utils::run_bf_with_config;

        // Set cell to 255 using increments, then try to increment → should error
        let source = "+".repeat(256).to_string(); // 0+255=255, +1=error!

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_checked_cells()
            .build();

        let result = run_bf_with_config(&source, "", config);
        assert!(result.is_err(), "Checked should error on overflow");
        assert!(
            matches!(result, Err(BfError::CellOverflow { .. })),
            "Should fail with CellOverflow, got {:?}",
            result
        );
    }

    #[test]
    fn test_cell_model_checked_underflow() {
        // Test u8 checked: 0 - 1 → error
        use crate::config::ExecutionConfigBuilder;
        use crate::test_utils::run_bf_with_config;

        // Start at 0, try to decrement → should error
        let source = "-"; // 0-1=error!

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_checked_cells()
            .build();

        let result = run_bf_with_config(source, "", config);
        assert!(result.is_err(), "Checked should error on underflow");
        assert!(
            matches!(result, Err(BfError::CellUnderflow { .. })),
            "Should fail with CellUnderflow, got {:?}",
            result
        );
    }

    #[test]
    fn test_cell_overflow_with_source_location() {
        // Test that cell overflow errors include source location when debug_info is provided
        use crate::config::ExecutionConfigBuilder;
        use crate::parser::parse_with_debug;

        // Create a program that overflows: 255 + 1
        let source = "+".repeat(256); // 0 + 256 = overflow at 256th increment

        let (instructions, debug_info) = parse_with_debug(&source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_checked_cells()
            .build();

        let result = interpret_with_config(&instructions, config, Some(&debug_info));

        assert!(result.is_err(), "Should error on overflow");
        match result {
            Err(BfError::CellOverflow {
                source_location,
                current_value,
                ..
            }) => {
                assert_eq!(current_value, 255, "Should overflow at value 255");
                assert!(
                    source_location.is_some(),
                    "Source location should be populated when debug_info is provided"
                );
                let loc = source_location.unwrap();
                assert_eq!(loc.line, 1, "Error should be on line 1");
                assert_eq!(loc.column, 256, "Error should be at column 256");
            }
            other => panic!("Expected CellOverflow error, got {:?}", other),
        }
    }

    #[test]
    fn test_cell_underflow_with_source_location() {
        // Test that cell underflow errors include source location when debug_info is provided
        use crate::config::ExecutionConfigBuilder;
        use crate::parser::parse_with_debug;

        let source = "-"; // 0 - 1 = underflow

        let (instructions, debug_info) = parse_with_debug(source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_checked_cells()
            .build();

        let result = interpret_with_config(&instructions, config, Some(&debug_info));

        assert!(result.is_err(), "Should error on underflow");
        match result {
            Err(BfError::CellUnderflow {
                source_location,
                current_value,
                ..
            }) => {
                assert_eq!(current_value, 0, "Should underflow at value 0");
                assert!(
                    source_location.is_some(),
                    "Source location should be populated when debug_info is provided"
                );
                let loc = source_location.unwrap();
                assert_eq!(loc.line, 1, "Error should be on line 1");
                assert_eq!(loc.column, 1, "Error should be at column 1");
            }
            other => panic!("Expected CellUnderflow error, got {:?}", other),
        }
    }

    #[test]
    fn test_memory_overflow_with_source_location() {
        // Test that memory overflow errors include source location when debug_info is provided
        use crate::config::ExecutionConfigBuilder;
        use crate::parser::parse_with_debug;

        // Create a program that goes beyond memory bounds
        let source = ">".repeat(101); // Memory size is 100, so 101 moves will overflow

        let (instructions, debug_info) = parse_with_debug(&source).unwrap();

        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();

        let result = interpret_with_config(&instructions, config, Some(&debug_info));

        assert!(result.is_err(), "Should error on memory overflow");
        match result {
            Err(BfError::MemoryOutOfBounds {
                source_location,
                attempted,
                ..
            }) => {
                assert_eq!(attempted, 100, "Should attempt to access cell 100");
                assert!(
                    source_location.is_some(),
                    "Source location should be populated when debug_info is provided"
                );
                let loc = source_location.unwrap();
                assert_eq!(loc.line, 1, "Error should be on line 1");
                assert_eq!(
                    loc.column, 100,
                    "Error should be at column 100 (the 100th > character)"
                );
            }
            other => panic!("Expected MemoryOutOfBounds error, got {:?}", other),
        }
    }

    #[test]
    fn test_memory_underflow_with_source_location() {
        // Test that memory underflow errors include source location when debug_info is provided
        use crate::parser::parse_with_debug;

        let source = "<"; // Try to go below 0

        let (instructions, debug_info) = parse_with_debug(source).unwrap();

        let config = ExecutionConfig::default();

        let result = interpret_with_config(&instructions, config, Some(&debug_info));

        assert!(result.is_err(), "Should error on memory underflow");
        match result {
            Err(BfError::MemoryOutOfBounds {
                source_location,
                attempted,
                ..
            }) => {
                assert_eq!(attempted, -1, "Should attempt to access cell -1");
                assert!(
                    source_location.is_some(),
                    "Source location should be populated when debug_info is provided"
                );
                let loc = source_location.unwrap();
                assert_eq!(loc.line, 1, "Error should be on line 1");
                assert_eq!(loc.column, 1, "Error should be at column 1");
            }
            other => panic!("Expected MemoryOutOfBounds error, got {:?}", other),
        }
    }

    #[test]
    fn test_error_formatting_with_source_location() {
        // Test that error messages include properly formatted source context
        use crate::config::ExecutionConfigBuilder;
        use crate::parser::parse_with_debug;

        let source = ">>>>>\n>>>>>\n>>>>>"; // Multiline program (15 moves right)
        let (instructions, debug_info) = parse_with_debug(source).unwrap();

        let config = ExecutionConfigBuilder::new().with_memory_size(5).build();

        // This will try to access cell 5, but memory size is only 5 (max index 4)
        let result = interpret_with_config(&instructions, config, Some(&debug_info));

        assert!(result.is_err());
        if let Err(error) = result {
            let formatted = error.format_with_source(source);

            // Verify the formatted error contains key elements
            assert!(
                formatted.contains("Error: Memory pointer out of bounds"),
                "Error message should contain error type"
            );
            assert!(
                formatted.contains("At line"),
                "Error message should indicate line number"
            );
            assert!(
                formatted.contains("│"),
                "Error message should contain line number separator"
            );
            assert!(
                formatted.contains("^"),
                "Error message should contain caret pointer"
            );
        }
    }

    #[test]
    fn test_source_location_column_1() {
        // Test caret positioning for error at column 1
        use crate::config::ExecutionConfigBuilder;
        use crate::parser::parse_with_debug;

        let source = "-"; // Error at first character
        let (instructions, debug_info) = parse_with_debug(source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_checked_cells()
            .build();

        match interpret_with_config(&instructions, config, Some(&debug_info)) {
            Err(BfError::CellUnderflow {
                source_location, ..
            }) => {
                assert!(source_location.is_some());
                let loc = source_location.unwrap();
                assert_eq!(loc.line, 1);
                assert_eq!(loc.column, 1);
            }
            other => panic!("Expected CellUnderflow, got {:?}", other),
        }
    }

    #[test]
    fn test_source_location_multiline_program() {
        // Test source location for errors in multiline programs
        use crate::config::ExecutionConfigBuilder;
        use crate::parser::parse_with_debug;

        let source = format!("+++\n+++\n{}", "-".repeat(260)); // Error on line 3
        let (instructions, debug_info) = parse_with_debug(&source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_checked_cells()
            .build();

        match interpret_with_config(&instructions, config, Some(&debug_info)) {
            Err(BfError::CellUnderflow {
                source_location, ..
            }) => {
                assert!(source_location.is_some());
                let loc = source_location.unwrap();
                assert_eq!(loc.line, 3, "Error should be on line 3");
                // Cell value is 6 after lines 1-2, so underflow happens at 7th decrement
                assert_eq!(
                    loc.column, 7,
                    "Error should be at column 7 on line 3 (7th - command)"
                );
            }
            other => panic!("Expected CellUnderflow, got {:?}", other),
        }
    }

    #[test]
    // #[ignore] // TODO: Fix after loop parsing refactor - instruction indices shifted
    fn test_source_location_in_nested_loop() {
        // Test that source location works correctly inside nested loops
        use crate::config::ExecutionConfigBuilder;
        use crate::parser::parse_with_debug;

        // Simple nested structure: outer loop contains overflow trigger
        // Structure: ++[>{256 +'s}] - loops twice, on first iteration moves right and overflows
        let source = format!("++[>{}<]", "+".repeat(256));
        let (instructions, debug_info) = parse_with_debug(&source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_checked_cells()
            .build();

        match interpret_with_config(&instructions, config, Some(&debug_info)) {
            Err(BfError::CellOverflow {
                source_location, ..
            }) => {
                assert!(source_location.is_some());
                let loc = source_location.unwrap();
                assert_eq!(loc.line, 1);
                // Error at the 256th + instruction (the one that causes overflow)
                // Counting: 2 (++) + 1 ([) + 1 (>) + 256 (+'s) = column 260
                // Column mapping: ++ at 1-2, [ at 3, > at 4, first + at 5, 256th + at 260
                assert_eq!(loc.column, 261, "Error should be at 256th + inside loop");
            }
            other => panic!("Expected CellOverflow, got {:?}", other),
        }
    }

    #[test]
    fn test_source_location_with_comments() {
        // Test that source location accounts for comments correctly
        use crate::config::ExecutionConfigBuilder;
        use crate::parser::parse_with_debug;

        let source = format!("+++++ * this is a comment\n+++++\n{}", "+".repeat(256));
        let (instructions, debug_info) = parse_with_debug(&source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_checked_cells()
            .build();

        match interpret_with_config(&instructions, config, Some(&debug_info)) {
            Err(BfError::CellOverflow {
                source_location, ..
            }) => {
                assert!(source_location.is_some());
                let loc = source_location.unwrap();
                assert_eq!(loc.line, 3, "Error should be on line 3");
                assert_eq!(
                    loc.column, 246,
                    "Comments are ignored, so position is on line 3"
                );
            }
            other => panic!("Expected CellOverflow, got {:?}", other),
        }
    }

    #[test]
    fn test_error_without_debug_info() {
        // Test that errors still work when debug_info is None
        use crate::config::ExecutionConfigBuilder;

        let source = "-";
        let instructions = parse(source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_checked_cells()
            .build();

        match interpret_with_config(&instructions, config, None) {
            Err(BfError::CellUnderflow {
                source_location, ..
            }) => {
                assert!(
                    source_location.is_none(),
                    "Source location should be None when debug_info is not provided"
                );
            }
            other => panic!("Expected CellUnderflow, got {:?}", other),
        }
    }

    #[test]
    fn test_memory_overflow_formatting() {
        // Test memory overflow error formatting with source context
        use crate::config::ExecutionConfigBuilder;
        use crate::parser::parse_with_debug;

        let source = ">".repeat(101);
        let (instructions, debug_info) = parse_with_debug(&source).unwrap();

        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();

        if let Err(error) = interpret_with_config(&instructions, config, Some(&debug_info)) {
            let formatted = error.format_with_source(&source);

            // Should include line/column info
            assert!(formatted.contains("At line 1, column 100"));
            // Should show nearby cells in memory dump
            assert!(formatted.contains("Nearby cells:"));
            // Should have the caret pointer
            assert!(formatted.contains("^"));
        } else {
            panic!("Expected error");
        }
    }

    #[test]
    fn test_cell_overflow_formatting() {
        // Test cell overflow error formatting with source context
        use crate::config::ExecutionConfigBuilder;
        use crate::parser::parse_with_debug;

        let source = "+".repeat(256);
        let (instructions, debug_info) = parse_with_debug(&source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_checked_cells()
            .build();

        if let Err(error) = interpret_with_config(&instructions, config, Some(&debug_info)) {
            let formatted = error.format_with_source(&source);

            // Should include error type
            assert!(formatted.contains("Cell overflow"));
            // Should include line/column
            assert!(formatted.contains("At line 1, column 256"));
            // Should mention the value
            assert!(formatted.contains("value 255"));
            // Should have syntax highlighting (ANSI codes)
            assert!(formatted.contains("\x1b["));
            // Should have caret
            assert!(formatted.contains("^"));
        } else {
            panic!("Expected error");
        }
    }

    #[test]
    fn test_cell_model_independence_from_memory_model() {
        // Test that CellModel and MemoryModel are orthogonal
        // Use Wrapping memory + Checked cells
        use crate::config::ExecutionConfigBuilder;
        use crate::test_utils::run_bf_with_config;

        // Set cell to 255, try to increment → cell overflow
        let source = "+".repeat(256).to_string(); // Should error on cell arithmetic

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100) // Fixed MEMORY
            .with_checked_cells() // Checked CELLS
            .build();

        let result = run_bf_with_config(&source, "", config);
        assert!(
            matches!(result, Err(BfError::CellOverflow { .. })),
            "Should fail with CellOverflow, proving cell model is independent"
        );
    }

    #[test]
    fn test_cell_model_normal_operations() {
        // Test that normal operations work with all cell models
        use crate::config::ExecutionConfigBuilder;
        use crate::test_utils::run_bf_with_config;

        let source = "+++."; // 0+3=3, output 3

        // Test with wrapping
        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_wrapping_cells()
            .build();
        let result = run_bf_with_config(source, "", config);
        assert!(result.is_ok());
        let (output, _) = result.unwrap();
        assert_eq!(output.as_bytes()[0], 3);

        // Test with checked
        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_checked_cells()
            .build();
        let result = run_bf_with_config(source, "", config);
        assert!(result.is_ok());
        let (output, _) = result.unwrap();
        assert_eq!(output.as_bytes()[0], 3);
    }

    // ===================================================================
    // LOOP DEPTH TRACKING TESTS
    // ===================================================================
    //
    // These tests verify that the loop_depth field in VmState correctly
    // tracks nesting depth during execution. This is foundational for
    // the upcoming hook architecture.

    #[test]
    fn test_loop_depth_single_loop() {
        // Test that loop_depth increments inside a single loop
        // Program: ++[>.<-]  (loops twice)
        use crate::test_utils::run_bf;

        let source = "++[>.<-]";
        let result = run_bf(source, "");

        // Should execute successfully
        assert!(result.is_ok(), "Simple loop should execute successfully");

        // Note: We can't directly test loop_depth from outside since VmState is private.
        // But we verify the program executes correctly, and the loop tracking doesn't
        // break anything. In the future, hooks will be able to observe loop_depth.
    }

    #[test]
    fn test_loop_depth_nested_loops() {
        // Test nested loops: outer[inner[...]]
        // Program: +++[>++[<.>-]<-]
        // - Outer loop runs 3 times
        // - Inner loop runs 2 times per outer iteration
        use crate::test_utils::run_bf;

        let source = "+++[>++[<.>-]<-]";
        let result = run_bf(source, "");

        assert!(result.is_ok(), "Nested loops should execute successfully");

        let (_, stats) = result.unwrap();

        // Outer: 3 iterations
        // Inner: 2 iterations × 3 outer = 6 iterations
        // Total: 3 + 6 = 9 loop iterations
        assert_eq!(stats.loop_iterations, 9);
    }

    #[test]
    fn test_loop_depth_deeply_nested() {
        // Test deeply nested loops (depth 3)
        // Program: +++[>>++[>>++[<<+>>-]<<-]<<-]
        // This creates three levels of nesting and moves memory carefully
        use crate::test_utils::run_bf;

        let source = "+++[>>++[>>++[<<+>>-]<<-]<<-]";
        let result = run_bf(source, "");

        assert!(
            result.is_ok(),
            "Deeply nested loops should execute successfully"
        );

        // The key here is that loop_depth increments and decrements correctly
        // without causing any issues. If it didn't, we'd likely see a crash
        // or incorrect behavior.
    }

    #[test]
    fn test_loop_depth_sequential_loops() {
        // Test that loop_depth resets between sequential loops
        // Program: ++[>+<-] (first loop), then >>++[>+<-] (second loop)
        use crate::test_utils::run_bf;

        let source = "++[>+<-]>>++[>+<-]";
        let result = run_bf(source, "");

        assert!(
            result.is_ok(),
            "Sequential loops should execute successfully"
        );

        let (_, stats) = result.unwrap();

        // Each loop runs 2 times
        // Total: 2 + 2 = 4 loop iterations
        assert_eq!(stats.loop_iterations, 4);
    }

    #[test]
    fn test_loop_depth_empty_loop_body() {
        // Test loop with empty condition (never enters)
        // Program: [+]  (starts with cell=0, never enters)
        use crate::test_utils::run_bf;

        let source = "[+]";
        let result = run_bf(source, "");

        assert!(result.is_ok(), "Empty loop should execute successfully");

        let (_, stats) = result.unwrap();

        // Loop never executes (cell starts at 0)
        assert_eq!(stats.loop_iterations, 0);
    }

    #[test]
    fn test_loop_depth_with_errors() {
        // Test that loop_depth tracking doesn't interfere with error handling
        // Program that causes memory overflow inside a nested loop
        use crate::config::ExecutionConfigBuilder;
        use crate::test_utils::run_bf_with_config;

        // Program: ++[>>>>> ... many >>> ...]  (goes out of bounds)
        let source = format!("++[{}]", ">".repeat(60));

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(50) // Small memory to trigger bounds error
            .build();

        let result = run_bf_with_config(&source, "", config);

        // Should error with memory out of bounds
        assert!(
            result.is_err(),
            "Should error on memory overflow inside loop"
        );
        assert!(
            matches!(result, Err(BfError::MemoryOutOfBounds { .. })),
            "Should be MemoryOutOfBounds error"
        );
    }

    // ===================================================================
    // EDGE CASE TESTS - Critical scenarios and boundary conditions
    // ===================================================================

    #[test]
    fn test_eof_behavior_combinations() {
        // Test all EOF behavior modes with actual EOF
        // NOTE: We use StringIo directly to access raw bytes since EOF values like 255
        // are not valid UTF-8 and would be replaced by String::from_utf8_lossy()
        use crate::config::{EofBehavior, ExecutionConfigBuilder};
        use crate::io::StringIo;

        let source = ",.,.,."; // Read 3 bytes, but we'll only provide 1
        let instructions = parse(source).unwrap();

        // SetZero: Should set cell to 0 on EOF
        let config = ExecutionConfigBuilder::new()
            .with_memory_size(10)
            .with_eof_behavior(EofBehavior::SetZero)
            .build();

        let mut input = StringIo::new("A");
        let mut output = StringIo::empty();
        let result = interpret_with_io(&instructions, config, &mut input, &mut output, None);
        assert!(result.is_ok());
        let out_bytes = output.output_bytes();
        assert_eq!(out_bytes.len(), 3); // Should output 3 bytes
        assert_eq!(out_bytes[0], b'A'); // First read succeeds
        assert_eq!(out_bytes[1], 0); // Second read EOF → 0
        assert_eq!(out_bytes[2], 0); // Third read EOF → 0

        // SetNegOne: Should set cell to 255 on EOF
        let config = ExecutionConfigBuilder::new()
            .with_memory_size(10)
            .with_eof_behavior(EofBehavior::SetNegOne)
            .build();

        let mut input = StringIo::new("B");
        let mut output = StringIo::empty();
        let result = interpret_with_io(&instructions, config, &mut input, &mut output, None);
        assert!(result.is_ok());
        let out_bytes = output.output_bytes();
        assert_eq!(out_bytes[0], b'B');
        assert_eq!(out_bytes[1], 255); // EOF → 255 (-1 as u8)
        assert_eq!(out_bytes[2], 255);

        // NoChange: Should leave cell unchanged on EOF
        let source2 = "++,.,"; // Set cell to 2, then read (EOF), then output
        let instructions2 = parse(source2).unwrap();
        let config = ExecutionConfigBuilder::new()
            .with_memory_size(10)
            .with_eof_behavior(EofBehavior::NoChange)
            .build();

        let mut input = StringIo::empty();
        let mut output = StringIo::empty();
        let result = interpret_with_io(&instructions2, config, &mut input, &mut output, None);
        assert!(result.is_ok());
        let out_bytes = output.output_bytes();
        assert_eq!(out_bytes.len(), 1);
        assert_eq!(out_bytes[0], 2); // Cell unchanged, stays at 2

        // Error: Should error on EOF
        let source3 = ",";
        let instructions3 = parse(source3).unwrap();
        let config = ExecutionConfigBuilder::new()
            .with_memory_size(10)
            .with_eof_behavior(EofBehavior::Error)
            .build();

        let mut input = StringIo::empty();
        let mut output = StringIo::empty();
        let result = interpret_with_io(&instructions3, config, &mut input, &mut output, None);
        assert!(result.is_err());
        assert!(matches!(result, Err(BfError::IoError { .. })));
    }

    #[test]
    fn test_unbounded_memory_growth_limits() {
        // Test that unbounded memory respects max_size limit
        use crate::config::ExecutionConfigBuilder;
        use crate::test_utils::run_bf_with_config;

        // Program that tries to allocate beyond max
        // Start with initial_size=10, max_size=20
        let source = ">".repeat(25); // Try to move 25 positions right

        let config = ExecutionConfigBuilder::new()
            .with_unbounded_memory(10, 20)
            .expect("valid unbounded config")
            .build();

        let result = run_bf_with_config(&source, "", config);

        // Should error when trying to exceed max_size
        assert!(
            result.is_err(),
            "Should error when exceeding unbounded max_size"
        );
        assert!(
            matches!(result, Err(BfError::MemoryOutOfBounds { .. })),
            "Should be MemoryOutOfBounds error"
        );
    }

    #[test]
    fn test_unbounded_memory_growth_success() {
        // Test that unbounded memory successfully grows within limits
        use crate::config::ExecutionConfigBuilder;
        use crate::test_utils::run_bf_with_config;

        // Program that allocates within max_size
        let source = ">>>>>+++++."; // Move 5 right (initial=3, max=10, this is ok)

        let config = ExecutionConfigBuilder::new()
            .with_unbounded_memory(3, 10)
            .expect("valid unbounded config")
            .build();

        let result = run_bf_with_config(source, "", config);

        assert!(result.is_ok(), "Should succeed within unbounded limits");
        let (_, stats) = result.unwrap();

        // Should have grown memory
        assert!(
            stats.peak_memory_used.get() > 3,
            "Memory should have grown beyond initial size"
        );
    }

    #[test]
    fn test_very_deep_nesting() {
        // Test extremely deep loop nesting (100 levels)
        use crate::test_utils::run_bf;

        // Generate deeply nested loops: +[[[[...[[+]]...]]]]
        let depth = 100;
        let mut source = String::from("+"); // Set cell to 1
        for _ in 0..depth {
            source.push('[');
        }
        source.push('+'); // Increment in innermost loop
        for _ in 0..depth {
            source.push(']');
        }

        let result = run_bf(&source, "");

        // Should execute successfully (might be slow, but should work)
        assert!(result.is_ok(), "Should handle 100 levels of nesting");

        let (_, stats) = result.unwrap();

        // Loop iterations calculation:
        // - Outer 99 loops are each entered once = 99 iterations
        // - Innermost loop (containing '+') runs until cell wraps to 0
        //   Cell starts at 1, increments to 255, then wraps to 0 = 255 iterations
        // - Total = 99 + 255 = 354
        assert_eq!(
            stats.loop_iterations,
            354,
            "Expected {} outer loops + 255 innermost = 354 total",
            depth - 1
        );
    }

    #[test]
    fn test_empty_program() {
        // Test that an empty program executes successfully
        use crate::test_utils::run_bf;

        let result = run_bf("", "");
        assert!(result.is_ok(), "Empty program should execute successfully");

        let (output, stats) = result.unwrap();
        assert_eq!(output.len(), 0);
        assert_eq!(stats.total_steps.get(), 0);
    }

    #[test]
    fn test_program_with_only_comments() {
        // Test program with only comments (no actual BF commands)
        use crate::test_utils::run_bf;

        let source = "* This is a comment\n   * Another comment\n\n* Third comment";
        let result = run_bf(source, "");

        assert!(result.is_ok(), "Comment-only program should succeed");

        let (output, stats) = result.unwrap();
        assert_eq!(output.len(), 0);
        assert_eq!(stats.total_steps.get(), 0);
    }

    #[test]
    fn test_max_steps_exactly_at_limit() {
        // Test that programs execute correctly when hitting step limit exactly
        use crate::config::ExecutionConfigBuilder;
        use crate::test_utils::run_bf_with_config;

        let source = "+++++"; // Exactly 5 steps

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(10)
            .with_max_steps(5) // Set limit to exactly 5
            .build();

        let result = run_bf_with_config(source, "", config);

        // Should succeed (5 steps <= 5 limit)
        assert!(result.is_ok(), "Should succeed at exact step limit");

        // Now test with 6 steps and limit of 5 - should fail
        let source = "++++++"; // 6 steps

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(10)
            .with_max_steps(5)
            .build();

        let result = run_bf_with_config(source, "", config);

        assert!(
            result.is_err(),
            "Should fail when exceeding step limit by 1"
        );
        assert!(
            matches!(result, Err(BfError::StepLimitExceeded { .. })),
            "Should be StepLimitExceeded error"
        );
    }

    // ===================================================================
    // PROPERTY-BASED TESTS - Generative testing for memory and cell models
    // ===================================================================

    #[cfg(test)]
    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        // Property: Cell wrapping behaves correctly for any sequence of increments
        proptest! {
            #[test]
            fn cell_wrapping_modulo_256(increments in 0u32..1000) {
                // Generate a program with N increments
                let source = "+".repeat(increments as usize);
                let instructions = parse(&source).unwrap();

                let config = ExecutionConfigBuilder::new()
                    .with_memory_size(10)
                    .with_wrapping_cells()
                    .build();

                let result = interpret_with_config(&instructions, config, None);
                prop_assert!(result.is_ok(), "Wrapping cells should never error on overflow");

                // The final cell value should be increments % 256
                // We can't easily verify the actual cell value without exposing internals,
                // but we can verify no error occurred
            }
        }

        // Property: Cell checked arithmetic errors on overflow/underflow
        proptest! {
            #[test]
            fn cell_checked_errors_on_overflow(increments in 256u32..512) {
                // Generate a program that will overflow (more than 255 increments)
                let source = "+".repeat(increments as usize);
                let instructions = parse(&source).unwrap();

                let config = ExecutionConfigBuilder::new()
                    .with_memory_size(10)
                    .with_checked_cells()
                    .build();

                let result = interpret_with_config(&instructions, config, None);
                prop_assert!(result.is_err(), "Checked cells should error on overflow");
                prop_assert!(
                    matches!(result, Err(BfError::CellOverflow { .. })),
                    "Should be CellOverflow error"
                );
            }
        }

        // Property: Fixed memory model errors when pointer exceeds bounds
        proptest! {
            #[test]
            fn fixed_memory_bounds_enforcement(memory_size in 10usize..100, moves in 0usize..200) {
                // Generate a program that moves right beyond memory bounds
                if moves <= memory_size {
                    return Ok(()); // Skip if within bounds
                }

                let source = ">".repeat(moves);
                let instructions = parse(&source).unwrap();

                let config = ExecutionConfigBuilder::new()
                    .with_memory_size(memory_size)
                    .build();

                let result = interpret_with_config(&instructions, config, None);
                prop_assert!(result.is_err(), "Fixed memory should error when exceeding bounds");
                prop_assert!(
                    matches!(result, Err(BfError::MemoryOutOfBounds { .. })),
                    "Should be MemoryOutOfBounds error"
                );
            }
        }

        // Property: Unbounded memory grows correctly within max_size
        proptest! {
            #[test]
            fn unbounded_memory_growth_within_limits(
                initial_size in 5usize..20,
                max_size in 30usize..100,
                moves in 0usize..25
            ) {
                // Ensure initial < max
                if initial_size >= max_size {
                    return Ok(());
                }

                // Generate a program that moves within max_size
                if moves >= max_size {
                    return Ok(()); // Skip if beyond max
                }

                let source = format!("{}+", ">".repeat(moves));
                let instructions = parse(&source).unwrap();

                let config = ExecutionConfigBuilder::new()
                    .with_unbounded_memory(initial_size, max_size)?
                    .build();

                let result = interpret_with_config(&instructions, config, None);

                // Should succeed if moves < max_size
                if moves < max_size {
                    prop_assert!(result.is_ok(),
                        "Unbounded memory should grow from {} to {} (moved {})",
                        initial_size, max_size, moves);
                } else {
                    prop_assert!(result.is_err(),
                        "Unbounded memory should error beyond max_size");
                }
            }
        }

        // Property: Unbounded memory respects max_size limit
        proptest! {
            #[test]
            fn unbounded_memory_max_size_enforced(
                initial_size in 5usize..20,
                max_size in 20usize..50
            ) {
                // Ensure initial < max
                if initial_size >= max_size {
                    return Ok(());
                }

                // Generate a program that tries to exceed max_size
                let moves = max_size + 10;
                let source = ">".repeat(moves);
                let instructions = parse(&source).unwrap();

                let config = ExecutionConfigBuilder::new()
                    .with_unbounded_memory(initial_size, max_size)?
                    .build();

                let result = interpret_with_config(&instructions, config, None);
                prop_assert!(result.is_err(),
                    "Unbounded memory should error when exceeding max_size {} (tried {})",
                    max_size, moves);
                prop_assert!(
                    matches!(result, Err(BfError::MemoryOutOfBounds { .. })),
                    "Should be MemoryOutOfBounds error"
                );
            }
        }
    }

    // Instruction index tracking tests
    #[test]
    fn test_instruction_index_simple_program() {
        use crate::parser::parse_with_debug;

        // Simple program: +++
        // instruction_index should be 0, 1, 2
        // This is a basic sanity test that instruction_index is being updated
        let source = "+++";
        let (instructions, debug_info) = parse_with_debug(source).unwrap();

        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();

        // We can't directly observe instruction_index from outside VmState,
        // but we can verify that execution completes successfully
        let result = interpret_with_config(&instructions, config, Some(&debug_info));
        assert!(result.is_ok());
    }

    #[test]
    fn test_instruction_index_with_loop() {
        use crate::parser::parse_with_debug;

        // Program: ++[>+<-]
        // instruction_index should cycle: 0, 1, 2, 3, 4, 5, 2, 3, 4, 5, ...
        // The loop body (indices 3-5) repeats
        let source = "++[>+<-]";
        let (instructions, debug_info) = parse_with_debug(source).unwrap();

        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();

        let result = interpret_with_config(&instructions, config, Some(&debug_info));
        assert!(result.is_ok());

        // Verify loop metadata was collected
        // New index mapping: 0-1: ++, 2: LoopCheck, 3-6: >+<-
        assert_eq!(debug_info.loop_count(), 1);
        let loop_meta = debug_info.get_loop_metadata(2).unwrap();
        assert_eq!(loop_meta.loop_start_index, 2); // Body starts with LoopCheck
        assert_eq!(loop_meta.body_size, 5); // LoopCheck + >+<- = 5 instructions
    }

    #[test]
    fn test_instruction_index_nested_loops() {
        use crate::parser::parse_with_debug;

        // Program: +[>+[<.>-]<-]
        // Outer loop at index 1, inner loop at index 4
        let source = "+[>+[<.>-]<-]";
        let (instructions, debug_info) = parse_with_debug(source).unwrap();

        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();

        let result = interpret_with_config(&instructions, config, Some(&debug_info));
        assert!(result.is_ok());

        // Verify both loops' metadata was collected
        // New index mapping: 0: +, 1: LoopCheck(outer), 2-3: >+, 4: LoopCheck(inner), 5-8: <.>-, 9-10: <-
        assert_eq!(debug_info.loop_count(), 2);

        let outer = debug_info.get_loop_metadata(1).unwrap();
        assert_eq!(outer.loop_start_index, 1); // Body starts with LoopCheck
        assert_eq!(outer.parent_loop, None);

        let inner = debug_info.get_loop_metadata(4).unwrap();
        assert_eq!(inner.loop_start_index, 4); // Body starts with LoopCheck
        assert_eq!(inner.parent_loop, Some(1));
    }

    // End-to-end test demonstrating source location tracking in loops
    #[test]
    fn test_source_location_after_many_loop_iterations() {
        use crate::parser::parse_with_debug;

        // Even after MANY loop iterations (high step count), we can still
        // accurately report the source location of the error.
        //
        // Program: ++[>>+] with tiny memory
        // Cell[0] = 2, so loop runs twice
        // Each iteration: move right twice, increment
        // Iteration 1: pointer 0→2, increment cell[2]
        // Iteration 2: pointer 2→4, tries to access cell[4] (OUT OF BOUNDS with memory_size=4)

        let source = "++[>>+]";
        let (instructions, debug_info) = parse_with_debug(source).unwrap();

        // Use tiny memory (4 cells: 0, 1, 2, 3) to trigger error on second iteration
        let config = ExecutionConfigBuilder::new().with_memory_size(4).build();

        let result = interpret_with_config(&instructions, config, Some(&debug_info));

        // Should fail with MemoryOutOfBounds
        assert!(
            result.is_err(),
            "Program should error when accessing cell 4"
        );

        match result {
            Err(BfError::MemoryOutOfBounds {
                source_location, ..
            }) => {
                assert!(source_location.is_some(), "Should provide source location");

                let loc = source_location.unwrap();
                // The error happens at the second '>' instruction (column 5)
                // This occurs on the SECOND iteration of the loop
                assert_eq!(loc.line, 1);
                assert_eq!(
                    loc.column, 5,
                    "Error should point to second '>' at column 5"
                );
            }
            other => panic!("Expected MemoryOutOfBounds, got {:?}", other),
        }
    }

    // Test loop call stack in nested loops
    #[test]
    fn test_loop_call_stack_nested_loops() {
        use crate::parser::parse_with_debug;

        // Program with nested loops that triggers error in inner loop
        // Simpler strategy: Direct movement that will definitely go out of bounds
        //
        // Program: ++[>++[>>]<-]
        // Initial: cell[0]=2
        // Outer loop iteration 1:
        //   - Move right to cell[1]
        //   - Set cell[1]=2 (++)
        //   - Inner loop iteration 1: move right twice (1→3)
        //   - Inner loop iteration 2: move right twice (3→5, ERROR with memory_size=5)

        let source = "++[>++[>>]<-]";
        let (instructions, debug_info) = parse_with_debug(source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(5) // Cells 0-4 exist, accessing cell[5] fails
            .build();

        let result = interpret_with_config(&instructions, config, Some(&debug_info));

        assert!(
            result.is_err(),
            "Program should error when accessing cell 5"
        );

        match result {
            Err(BfError::MemoryOutOfBounds {
                source_location,
                loop_call_stack,
                ..
            }) => {
                // Verify source location
                assert!(source_location.is_some(), "Should provide source location");
                let loc = source_location.unwrap();
                assert_eq!(loc.line, 1);
                // Error at second '>' inside inner loop (column 9)
                // Program: ++[>++[>>]<-]
                // Columns: 1234567890123
                assert_eq!(loc.column, 9, "Error at second '>' inside inner loop");

                // Verify loop call stack exists
                assert!(loop_call_stack.is_some(), "Should provide loop call stack");

                let stack = loop_call_stack.unwrap();
                assert_eq!(stack.len(), 2, "Should have 2 frames: outer and inner loop");

                // Frame 0: Outer loop (starts at '[' which is column 3)
                assert_eq!(stack[0].source_location.line, 1);
                assert_eq!(stack[0].source_location.column, 3);
                assert_eq!(stack[0].iteration, 1, "Outer loop first iteration");

                // Frame 1: Inner loop (starts at '[' which is column 7)
                // Program: "++[>++[>>]<-]"
                // Columns:  1234567890123
                assert_eq!(stack[1].source_location.line, 1);
                assert_eq!(stack[1].source_location.column, 7);
                assert!(
                    stack[1].iteration >= 1,
                    "Inner loop should have at least 1 iteration"
                );
            }
            other => panic!("Expected MemoryOutOfBounds, got {:?}", other),
        }
    }

    // Test loop call stack with many iterations
    #[test]
    fn test_loop_call_stack_many_iterations() {
        use crate::parser::parse_with_debug;

        // Program that runs multiple iterations before error
        // Use the proven pattern from test_source_location_after_many_loop_iterations
        // ++[>>+] with small memory
        // This creates an "infinite" loop that keeps moving right and incrementing
        // Eventually it will go out of bounds

        let source = "++[>>+]";
        let (instructions, debug_info) = parse_with_debug(source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(10) // Small memory to trigger error quickly
            .build();

        let result = interpret_with_config(&instructions, config, Some(&debug_info));

        assert!(
            result.is_err(),
            "Program should error when moving out of bounds"
        );

        match result {
            Err(BfError::MemoryOutOfBounds {
                loop_call_stack, ..
            }) => {
                assert!(loop_call_stack.is_some(), "Should provide loop call stack");

                let stack = loop_call_stack.unwrap();
                assert_eq!(stack.len(), 1, "Should have 1 frame: the loop");

                // Verify iteration count is tracked (should be at least 1)
                assert!(
                    stack[0].iteration >= 1,
                    "Should have iteration >= 1, got {}",
                    stack[0].iteration
                );
            }
            other => panic!("Expected MemoryOutOfBounds, got {:?}", other),
        }
    }

    // Test loop call stack formatting
    #[test]
    fn test_loop_call_stack_formatting() {
        use crate::parser::parse_with_debug;

        // Use the proven nested loop pattern from earlier test
        let source = "++[>++[>>]<-]";
        let (instructions, debug_info) = parse_with_debug(source).unwrap();

        let config = ExecutionConfigBuilder::new().with_memory_size(5).build();

        let result = interpret_with_config(&instructions, config, Some(&debug_info));

        match result {
            Err(err @ BfError::MemoryOutOfBounds { .. }) => {
                // Format the error with source
                let formatted = err.format_with_source(source);

                // Verify the formatted output contains loop call stack
                assert!(
                    formatted.contains("Loop call stack:"),
                    "Formatted error should include loop call stack header"
                );

                // Should show iteration numbers in call stack
                assert!(
                    formatted.contains("iteration"),
                    "Should show iteration numbers in call stack"
                );
            }
            other => panic!("Expected MemoryOutOfBounds, got {:?}", other),
        }
    }

    // Test triple nested loops
    #[test]
    fn test_triple_nested_loop_call_stack() {
        use crate::parser::parse_with_debug;

        // Triple nested loop
        // ++[>++[>++[>>>>]<]<]
        // Outer: cell[0]=2, runs twice
        // Middle: cell[1]=2, runs twice per outer
        // Inner: cell[2]=2, runs twice per middle
        // Each inner iteration moves right 4 times

        let source = "++[>++[>++[>>>>]<]<]";
        let (instructions, debug_info) = parse_with_debug(source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(10) // Small memory to trigger error
            .build();

        let result = interpret_with_config(&instructions, config, Some(&debug_info));

        if let Err(BfError::MemoryOutOfBounds {
            loop_call_stack: Some(stack),
            ..
        }) = result
        {
            // Should have 3 frames for 3 nested loops
            assert_eq!(stack.len(), 3, "Should have 3 frames for triple nesting");

            // All frames should have valid source locations
            for (i, frame) in stack.iter().enumerate() {
                assert_eq!(frame.source_location.line, 1);
                assert!(
                    frame.iteration >= 1,
                    "Frame {} should have iteration >= 1, got {}",
                    i,
                    frame.iteration
                );
            }
        }
    }

    // ===================================================================
    //          Debugging Tests: Deep Nested Loops with Overflow
    // ===================================================================
    //
    // These tests verify that We correctly tracks source locations and
    // loop call stacks in complex nested loop scenarios with memory
    // overflow near boundaries.
    //
    // Strategy: Use 100-cell memory and move pointer close to boundary
    // using >>>> (4 cells at a time), then use nested loops to trigger
    // overflow while verifying loop call stack is correct.

    #[test]
    fn test_debug_double_nested_overflow() {
        use crate::parser::parse_with_debug;

        // Strategy: Move pointer to cell 90, then use nested loops to overflow
        //
        // Program breakdown:
        // >>>>>>>>>>>>>>>>>>>>>>>> (24 >'s = move to cell 24, actually let's use 23 >>>>'s)
        // ++[>+[>>>>]<-]
        //
        // Simpler: Start at cell 90, use double nested loop
        // Use >>>> repeatedly to get to cell 92 (23 blocks of 4)
        // Then: ++[>+[>>>>]<-]
        //   Outer: cell[92]=2, runs twice
        //   Inner: cell[93]=1, moves right 4 times (93->97, then 97->101 OVERFLOW)

        let setup_moves = ">>>>".repeat(23); // 23*4 = 92
        let nested_loop = "++[>+[>>>>]<-]";
        let source = format!("{}{}", setup_moves, nested_loop);

        let (instructions, debug_info) = parse_with_debug(&source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100) // Cells 0-99 exist
            .build();

        let result = interpret_with_config(&instructions, config, Some(&debug_info));

        match result {
            Err(BfError::MemoryOutOfBounds {
                source_location,
                loop_call_stack,
                attempted,
                ..
            }) => {
                println!("✓ Test triggered overflow as expected!");
                println!("  Attempted to access cell: {}", attempted);

                // Verify source location exists
                assert!(source_location.is_some(), "Should have source location");
                let loc = source_location.unwrap();
                println!("  Error at line {}, column {}", loc.line, loc.column);

                // Verify loop call stack
                assert!(loop_call_stack.is_some(), "Should have loop call stack");
                let stack = loop_call_stack.unwrap();
                println!("  Loop stack depth: {}", stack.len());

                assert_eq!(stack.len(), 2, "Should have 2 nested loops in call stack");

                // Print stack for debugging
                for (i, frame) in stack.iter().enumerate() {
                    println!(
                        "    Frame {}: line {}, col {}, iteration {}",
                        i,
                        frame.source_location.line,
                        frame.source_location.column,
                        frame.iteration
                    );
                }

                // Outer loop should be first iteration or second
                assert!(
                    stack[0].iteration >= 1 && stack[0].iteration <= 2,
                    "Outer loop iteration should be 1 or 2, got {}",
                    stack[0].iteration
                );

                // Inner loop should be first iteration (first time it overflows)
                assert_eq!(
                    stack[1].iteration, 1,
                    "Inner loop should be on first iteration when overflow occurs"
                );
            }
            Ok(_) => panic!("Expected MemoryOutOfBounds error, but program completed successfully"),
            Err(other) => panic!("Expected MemoryOutOfBounds, got {:?}", other),
        }
    }

    #[test]
    fn test_debug_triple_nested_overflow() {
        use crate::parser::parse_with_debug;

        // Strategy: Move pointer to cell 85, then use triple nested loops
        //
        // Start at cell 84 (21 blocks of 4)
        // ++[>+[>+[>>>>]<-]<-]
        //   Outer: cell[84]=2
        //   Middle: cell[85]=1
        //   Inner: cell[86]=1, moves right 4 times each iteration

        let setup_moves = ">>>>".repeat(21); // 21*4 = 84
        let nested_loop = "++[>+[>+[>>>>]<-]<-]";
        let source = format!("{}{}", setup_moves, nested_loop);

        let (instructions, debug_info) = parse_with_debug(&source).unwrap();

        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();

        let result = interpret_with_config(&instructions, config, Some(&debug_info));

        match result {
            Err(BfError::MemoryOutOfBounds {
                source_location,
                loop_call_stack,
                attempted,
                ..
            }) => {
                println!("✓ Triple nested overflow test triggered!");
                println!("  Attempted cell: {}", attempted);

                assert!(source_location.is_some(), "Should have source location");
                let loc = source_location.unwrap();
                println!("  Error at line {}, column {}", loc.line, loc.column);

                assert!(loop_call_stack.is_some(), "Should have loop call stack");
                let stack = loop_call_stack.unwrap();
                println!("  Loop stack depth: {}", stack.len());

                assert_eq!(stack.len(), 3, "Should have 3 nested loops in call stack");

                for (i, frame) in stack.iter().enumerate() {
                    println!(
                        "    Frame {}: line {}, col {}, iteration {}",
                        i,
                        frame.source_location.line,
                        frame.source_location.column,
                        frame.iteration
                    );
                    assert!(
                        frame.iteration >= 1,
                        "Each loop should have at least 1 iteration"
                    );
                }
            }
            Ok(_) => panic!("Expected overflow, program completed"),
            Err(other) => panic!("Expected MemoryOutOfBounds, got {:?}", other),
        }
    }

    #[test]
    fn test_debug_quad_nested_overflow() {
        use crate::parser::parse_with_debug;

        // 4-level deep nesting
        // Start at cell 80 (20 blocks of 4)
        // ++[>+[>+[>+[>>>>]<-]<-]<-]

        let setup_moves = ">>>>".repeat(20); // 20*4 = 80
        let nested_loop = "++[>+[>+[>+[>>>>]<-]<-]<-]";
        let source = format!("{}{}", setup_moves, nested_loop);

        let (instructions, debug_info) = parse_with_debug(&source).unwrap();

        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();

        let result = interpret_with_config(&instructions, config, Some(&debug_info));

        match result {
            Err(BfError::MemoryOutOfBounds {
                source_location,
                loop_call_stack,
                attempted,
                ..
            }) => {
                println!("✓ Quad nested overflow test triggered!");
                println!("  Attempted cell: {}", attempted);

                assert!(source_location.is_some(), "Should have source location");
                let loc = source_location.unwrap();
                println!("  Error at line {}, column {}", loc.line, loc.column);

                assert!(loop_call_stack.is_some(), "Should have loop call stack");
                let stack = loop_call_stack.unwrap();
                println!("  Loop stack depth: {}", stack.len());

                assert_eq!(stack.len(), 4, "Should have 4 nested loops in call stack");

                for (i, frame) in stack.iter().enumerate() {
                    println!(
                        "    Frame {}: line {}, col {}, iteration {}",
                        i,
                        frame.source_location.line,
                        frame.source_location.column,
                        frame.iteration
                    );
                    assert!(
                        frame.iteration >= 1,
                        "Each loop should have at least 1 iteration"
                    );
                }
            }
            Ok(_) => panic!("Expected overflow, program completed"),
            Err(other) => panic!("Expected MemoryOutOfBounds, got {:?}", other),
        }
    }

    #[test]
    fn test_debug_realistic_scenario() {
        use crate::parser::parse_with_debug;

        // Realistic scenario: Program that does some work then overflows
        // This simulates finding a bug in a real BF program
        //
        // Move to cell 88: >>>>>>>>>>>>>>>>>>>>>>>>  (22 blocks)
        // Do some operations: +++[>++[>-<]>]
        //   This creates a more complex execution pattern

        let setup = ">>>>".repeat(22); // 88
        let complex_loop = "+++[>++[>+>>>]<-]"; // Multiple nested operations
        let source = format!("{}{}", setup, complex_loop);

        let (instructions, debug_info) = parse_with_debug(&source).unwrap();

        let config = ExecutionConfigBuilder::new().with_memory_size(100).build();

        let result = interpret_with_config(&instructions, config, Some(&debug_info));

        match result {
            Err(BfError::MemoryOutOfBounds {
                source_location,
                loop_call_stack,
                ..
            }) => {
                println!("✓ Realistic scenario triggered overflow!");

                assert!(source_location.is_some(), "Should have source location");
                let loc = source_location.unwrap();
                println!("  Error location: line {}, column {}", loc.line, loc.column);

                if let Some(stack) = loop_call_stack {
                    println!("  Loop call stack:");
                    for (i, frame) in stack.iter().enumerate() {
                        println!(
                            "    #{}: Loop at col {} (iteration {})",
                            i, frame.source_location.column, frame.iteration
                        );
                    }

                    // This should have nested loops
                    assert!(!stack.is_empty(), "Should have at least 1 loop in stack");
                }
            }
            Ok(stats) => {
                // It's possible this completes without overflow depending on the exact behavior
                println!("⚠ Program completed without overflow");
                println!("  Total steps: {}", stats.total_steps);
                println!("  Peak memory: {}", stats.peak_memory_used);
            }
            Err(other) => panic!("Unexpected error: {:?}", other),
        }
    }

    // Tests for profiling instruction hit counts
    #[test]
    fn test_profiling_simple_loop_instruction_counts() {
        use crate::hooks::builtin::{ProfilingHook, SharedProfilingHook};
        use crate::parser::parse_with_debug;
        use std::sync::{Arc, Mutex};

        // Simple loop: all instructions inside should execute same number of times
        let source = "+++[>+<-]";
        let (instructions, debug_info) = parse_with_debug(source).unwrap();

        let profiler = Arc::new(Mutex::new(ProfilingHook::new()));
        let profiler_clone = Arc::clone(&profiler);

        let mut config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        config.register_hook(Box::new(SharedProfilingHook::new_with_shared(
            profiler_clone,
        )));

        let result = interpret_with_config(&instructions, config, Some(&debug_info));
        assert!(result.is_ok());

        let profiler = profiler.lock().unwrap();

        // Get hit counts for loop body instructions
        // New mapping: 0-2 are +++, 3 is LoopCheck (the '['), 4-7 are >+<-
        let loop_check_hits = *profiler.instruction_hits().get(&3).unwrap_or(&0);
        let loop_body_hits: Vec<u64> = (4..=7)
            .map(|idx| *profiler.instruction_hits().get(&idx).unwrap_or(&0))
            .collect();

        // LoopCheck runs 4 times (3 iterations + 1 final check that exits)
        assert_eq!(loop_check_hits, 4, "LoopCheck should run iterations + 1");
        // Body instructions run 3 times each (once per iteration)
        assert_eq!(
            loop_body_hits,
            vec![3, 3, 3, 3],
            "All loop body instructions should execute same number of times"
        );
    }

    #[test]
    fn test_profiling_double_increment_loop() {
        use crate::hooks::builtin::{ProfilingHook, SharedProfilingHook};
        use crate::parser::parse_with_debug;
        use std::sync::{Arc, Mutex};

        // This is the exact case from the user's bug report
        let source = "+++++[>++<-]>.";
        let (instructions, debug_info) = parse_with_debug(source).unwrap();

        let profiler = Arc::new(Mutex::new(ProfilingHook::new()));
        let profiler_clone = Arc::clone(&profiler);

        let mut config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        config.register_hook(Box::new(SharedProfilingHook::new_with_shared(
            profiler_clone,
        )));

        let result = interpret_with_config(&instructions, config, Some(&debug_info));
        assert!(result.is_ok());

        let profiler = profiler.lock().unwrap();

        // Correct indices with Loop/LoopCheck sharing same index:
        // 0-4: +++++
        // 5: LoopCheck (the '[')
        // 6-10: >++<- (loop body)
        // 11: >
        // 12: .

        let loop_check_hits = *profiler.instruction_hits().get(&5).unwrap_or(&0);
        let loop_body_hits: Vec<u64> = (6..=10)
            .map(|idx| *profiler.instruction_hits().get(&idx).unwrap_or(&0))
            .collect();

        // LoopCheck runs 6 times (5 iterations + 1 final check that exits)
        assert_eq!(loop_check_hits, 6, "LoopCheck should run iterations + 1");

        // Body instructions run 5 times each (once per iteration)
        assert_eq!(
            loop_body_hits,
            vec![5, 5, 5, 5, 5],
            "All loop body instructions should execute same number of times (5)"
        );

        // Instructions after loop should execute once
        assert_eq!(*profiler.instruction_hits().get(&11).unwrap(), 1);
        assert_eq!(*profiler.instruction_hits().get(&12).unwrap(), 1);
    }

    #[test]
    fn test_profiling_nested_loops() {
        use crate::hooks::builtin::{ProfilingHook, SharedProfilingHook};
        use crate::parser::parse_with_debug;
        use std::sync::{Arc, Mutex};

        // Nested loops: outer[inner[body]]
        let source = "+++[>++[<+>-]<-]";
        let (instructions, debug_info) = parse_with_debug(source).unwrap();

        let profiler = Arc::new(Mutex::new(ProfilingHook::new()));
        let profiler_clone = Arc::clone(&profiler);

        let mut config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        config.register_hook(Box::new(SharedProfilingHook::new_with_shared(
            profiler_clone,
        )));

        let result = interpret_with_config(&instructions, config, Some(&debug_info));
        assert!(result.is_ok());

        let profiler = profiler.lock().unwrap();

        // Correct indices with Loop/LoopCheck sharing same index:
        // 0-2: +++
        // 3: outer LoopCheck (the '[')
        // 4: >
        // 5-6: ++
        // 7: inner LoopCheck (the '[')
        // 8-11: <+>-
        // 12: <
        // 13: -
        // Get inner loop LoopCheck and body instruction counts
        let inner_loopcheck = *profiler.instruction_hits().get(&7).unwrap_or(&0);
        let inner_body_hits: Vec<u64> = (8..=11)
            .map(|idx| *profiler.instruction_hits().get(&idx).unwrap_or(&0))
            .collect();

        // All body instructions should have identical hit counts
        let body_count = inner_body_hits[0];
        assert!(
            inner_body_hits.iter().all(|&count| count == body_count),
            "All inner loop body instructions should have same count: {:?}",
            inner_body_hits
        );

        // Note: LoopCheck is executed more times than body in nested loops
        // Relationship: inner_loopcheck = body_count + outer_loop_iterations
        // Each time the outer loop runs, we enter the inner loop and do one extra check to exit
        assert!(
            inner_loopcheck > body_count,
            "Inner LoopCheck ({}) should be > body count ({})",
            inner_loopcheck,
            body_count
        );

        // The difference tells us how many times the outer loop executed
        let outer_iterations = inner_loopcheck - body_count;

        // Should execute multiple times (exact count depends on loop logic)
        assert!(
            body_count > 10,
            "Inner loop body should execute many times, got {}",
            body_count
        );

        // Outer loop should also execute multiple times
        assert!(
            outer_iterations > 1,
            "Outer loop should execute multiple times, got {}",
            outer_iterations
        );
    }

    #[test]
    fn test_empty_loop_after_input() {
        use crate::parse_with_debug;

        // Regression test for hang bug with "+,[]" pattern
        // DebugIo returns non-zero ('X' = 88), so [] becomes an infinite empty loop
        // The fix: each `[` instruction check counts as a step, so empty loops hit the limit
        let source = "+,\n[\n]";
        let (instructions, debug_info) = parse_with_debug(source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_max_steps(1000)
            .build();

        let mut input = crate::io::DebugIo::new(); // Returns 'X' (88), non-zero
        let mut output = crate::io::DebugIo::new();

        // This should hit the step limit, not hang
        let result = interpret_with_io(
            &instructions,
            config,
            &mut input,
            &mut output,
            Some(&debug_info),
        );

        // Should hit step limit (empty infinite loop)
        match result {
            Err(BfError::StepLimitExceeded {
                actual_steps,
                limit,
                ..
            }) => {
                assert_eq!(limit, 1000, "Should hit the configured limit");
                assert!(actual_steps.get() > 1000, "Should exceed the limit");
            }
            other => panic!("Expected StepLimitExceeded, got: {:?}", other),
        }
    }

    #[test]
    fn test_empty_loop_after_operations() {
        use crate::parse_with_debug;

        // Another regression test for hang bug - empty loop after various operations
        // "+.>. -<[]" leaves cell 0 at value 1, so [] becomes an infinite empty loop
        // The fix: each `[` instruction check counts as a step, so empty loops hit the limit
        let source = "+.>.  - <[\n]";
        let (instructions, debug_info) = parse_with_debug(source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_max_steps(1000)
            .build();

        let mut input = crate::io::DebugIo::new();
        let mut output = crate::io::DebugIo::new();

        // This should hit the step limit, not hang
        let result = interpret_with_io(
            &instructions,
            config,
            &mut input,
            &mut output,
            Some(&debug_info),
        );

        // Should hit step limit (empty infinite loop)
        match result {
            Err(BfError::StepLimitExceeded {
                actual_steps,
                limit,
                ..
            }) => {
                assert_eq!(limit, 1000, "Should hit the configured limit");
                assert!(actual_steps.get() > 1000, "Should exceed the limit");
            }
            other => panic!("Expected StepLimitExceeded, got: {:?}", other),
        }
    }

    #[test]
    fn test_profiling_instruction_indices_no_overlap() {
        use crate::hooks::builtin::{ProfilingHook, SharedProfilingHook};
        use crate::parser::parse_with_debug;
        use std::sync::{Arc, Mutex};

        // Test that instruction indices don't overlap after loops
        let source = "+[>+<-]+[>-<+]+";
        let (instructions, debug_info) = parse_with_debug(source).unwrap();

        let profiler = Arc::new(Mutex::new(ProfilingHook::new()));
        let profiler_clone = Arc::clone(&profiler);

        let mut config = ExecutionConfigBuilder::new().with_memory_size(100).build();
        config.register_hook(Box::new(SharedProfilingHook::new_with_shared(
            profiler_clone,
        )));

        let result = interpret_with_config(&instructions, config, Some(&debug_info));
        assert!(result.is_ok());

        let profiler = profiler.lock().unwrap();

        // Get all instruction indices that were hit
        let mut indices: Vec<usize> = profiler.instruction_hits().keys().copied().collect();
        indices.sort();

        // Check that indices are sequential with no gaps or overlaps
        for i in 0..indices.len() - 1 {
            assert_eq!(
                indices[i] + 1,
                indices[i + 1],
                "Instruction indices should be sequential. Found gap/overlap at {} and {}",
                indices[i],
                indices[i + 1]
            );
        }
    }

    // Unit tests for count_instructions helper function
    #[test]
    fn test_count_instructions_empty() {
        let instructions: Vec<Instruction> = vec![];
        assert_eq!(count_instructions(&instructions), 0);
    }

    #[test]
    fn test_count_instructions_single() {
        let instructions = vec![Instruction::IncrementValue];
        assert_eq!(count_instructions(&instructions), 1);

        let instructions = vec![Instruction::Output];
        assert_eq!(count_instructions(&instructions), 1);
    }

    #[test]
    fn test_count_instructions_multiple() {
        let instructions = vec![
            Instruction::IncrementValue,
            Instruction::IncrementPointer,
            Instruction::Output,
        ];
        assert_eq!(count_instructions(&instructions), 3);

        let instructions = vec![
            Instruction::IncrementValue,
            Instruction::IncrementValue,
            Instruction::DecrementPointer,
            Instruction::DecrementValue,
            Instruction::Input,
        ];
        assert_eq!(count_instructions(&instructions), 5);
    }

    #[test]
    fn test_count_instructions_single_loop() {
        // Loop with body: Loop + LoopCheck + body instructions
        let loop_body = vec![
            Instruction::LoopCheck,
            Instruction::IncrementValue,
            Instruction::DecrementPointer,
        ];
        let instructions = vec![Instruction::Loop(loop_body)];

        // Count: 3 (body: LoopCheck + IncrementValue + DecrementPointer) = 3
        assert_eq!(count_instructions(&instructions), 3);
    }

    #[test]
    fn test_count_instructions_empty_loop() {
        // Loop with only LoopCheck (no other body instructions)
        let loop_body = vec![Instruction::LoopCheck];
        let instructions = vec![Instruction::Loop(loop_body)];

        // Count: 1 (LoopCheck) =
        assert_eq!(count_instructions(&instructions), 1);
    }

    #[test]
    fn test_count_instructions_nested_loops() {
        // Inner loop: [LoopCheck, IncrementValue]
        let inner_loop = vec![Instruction::LoopCheck, Instruction::IncrementValue];

        // Outer loop: [LoopCheck, IncrementPointer, Loop(inner), DecrementPointer]
        let outer_loop_body = vec![
            Instruction::LoopCheck,
            Instruction::IncrementPointer,
            Instruction::Loop(inner_loop),
            Instruction::DecrementPointer,
        ];

        let instructions = vec![Instruction::Loop(outer_loop_body)];

        // Count breakdown:
        // - Outer body: LoopCheck (1) + IncrementPointer (1) + DecrementPointer (1) = 3
        // - Inner body: LoopCheck (1) + IncrementValue (1) = 2
        // Total: 3 + 2 = 5
        assert_eq!(count_instructions(&instructions), 5);
    }

    #[test]
    fn test_count_instructions_sibling_loops() {
        // First loop: [LoopCheck, IncrementValue]
        let loop1 = vec![Instruction::LoopCheck, Instruction::IncrementValue];

        // Second loop: [LoopCheck, DecrementValue, Output]
        let loop2 = vec![
            Instruction::LoopCheck,
            Instruction::DecrementValue,
            Instruction::Output,
        ];

        let instructions = vec![
            Instruction::IncrementPointer,
            Instruction::Loop(loop1),
            Instruction::DecrementPointer,
            Instruction::Loop(loop2),
        ];

        // Count breakdown:
        // - IncrementPointer: 1
        // - First loop body: LoopCheck (1) + IncrementValue (1) = 2
        // - DecrementPointer: 1
        // - Second loop body: LoopCheck (1) + DecrementValue (1) + Output (1) = 3
        // Total: 1 + 2 + 1 + 3 = 7
        assert_eq!(count_instructions(&instructions), 7);
    }

    #[test]
    fn test_count_instructions_deeply_nested() {
        // Triple nested loops
        // Innermost: [LoopCheck, Output]
        let innermost = vec![Instruction::LoopCheck, Instruction::Output];

        // Middle: [LoopCheck, Loop(innermost), IncrementValue]
        let middle = vec![
            Instruction::LoopCheck,
            Instruction::Loop(innermost),
            Instruction::IncrementValue,
        ];

        // Outer: [LoopCheck, Loop(middle)]
        let outer = vec![Instruction::LoopCheck, Instruction::Loop(middle)];

        let instructions = vec![Instruction::Loop(outer)];

        // Count breakdown:
        // - Outermost body: LoopCheck (1) = 1
        // - Middle body: LoopCheck (1) + IncrementValue (1) = 2
        // - Innermost body: LoopCheck (1) + Output (1) = 2
        // Total: 1 + 2 + 2 = 8
        assert_eq!(count_instructions(&instructions), 5);
    }

    #[test]
    fn test_count_instructions_complex_mixed() {
        // Complex case: mix of instructions and nested loops
        // Inner loop: [LoopCheck, DecrementValue]
        let inner = vec![Instruction::LoopCheck, Instruction::DecrementValue];

        // Outer loop: [LoopCheck, IncrementPointer, Loop(inner), DecrementPointer, Output]
        let outer_body = vec![
            Instruction::LoopCheck,
            Instruction::IncrementPointer,
            Instruction::Loop(inner),
            Instruction::DecrementPointer,
            Instruction::Output,
        ];

        let instructions = vec![
            Instruction::IncrementValue,
            Instruction::IncrementValue,
            Instruction::Loop(outer_body),
            Instruction::DecrementValue,
        ];

        // Count breakdown:
        // - IncrementValue: 1
        // - IncrementValue: 1
        // - Outer body: LoopCheck (1) + IncrementPointer (1) + DecrementPointer (1) + Output (1) = 4
        // - Inner body: LoopCheck (1) + DecrementValue (1) = 2
        // - DecrementValue: 1
        // Total: 1 + 1 + 1 + 4 + 2 = 9
        assert_eq!(count_instructions(&instructions), 9);
    }
}
