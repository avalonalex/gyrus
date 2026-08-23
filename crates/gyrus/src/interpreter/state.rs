//! VM state and execution flow types for the BrainFuck interpreter.

use crate::config::MemoryModel;
use crate::error::BfError;
use crate::types::{MemoryAddress, StepCount};

/// Control flow decision after instruction execution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExecutionFlow {
    /// Continue normal execution
    Continue,
    /// Exit the current loop (LoopCheck instruction returns this when cell is zero)
    LoopExit,
}

/// Result type for instruction execution
///
/// `Ok(ExecutionFlow)` indicates successful execution with control flow decision
/// `Err(BfError)` indicates an actual error occurred
pub(super) type ExecutionResult = std::result::Result<ExecutionFlow, BfError>;

/// VM state containing memory, pointer, and execution counters
pub(super) struct VmState {
    /// Memory tape (array of cells)
    pub memory: Vec<u8>,
    /// Current memory pointer position
    pub pointer: MemoryAddress,
    /// Number of steps executed so far
    pub step_count: StepCount,
    /// Current loop nesting depth (0 = top-level, 1 = inside one loop, etc.)
    /// This is incremented when entering a loop body and decremented when exiting.
    /// Useful for debugging, profiling, and hook context.
    pub loop_depth: usize,
    /// Memory model that dictates how memory operations behave
    pub memory_model: MemoryModel,
}

impl VmState {
    /// Create a new VM state with the given memory model
    pub fn new(memory_model: MemoryModel) -> Self {
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
