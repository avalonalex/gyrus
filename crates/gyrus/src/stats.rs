//! Execution statistics and performance metrics.
//!
//! This module provides [`ExecutionStats`] for tracking interpreter execution metrics
//! such as step count, memory usage, I/O operations, and runtime warnings.
//!
//! # Examples
//!
//! ```rust
//! use gyrus::{parse, interpret_with_config, ExecutionConfigBuilder};
//!
//! # fn main() -> Result<(), gyrus::BfError> {
//! let instructions = parse("+++++[>+<-]")?;
//! let config = ExecutionConfigBuilder::new()
//!     .with_memory_size(1000)
//!     .build();
//!
//! let stats = interpret_with_config(&instructions, config, None)?;
//! println!("Total steps: {}", stats.total_steps);
//! println!("Loop iterations: {}", stats.loop_iterations);
//! println!("Peak memory used: {} cells", stats.peak_memory_used);
//! # Ok(())
//! # }
//! ```

use crate::error::RuntimeWarning;
use crate::types::{MemoryAddress, MemorySize, StepCount};

/// Statistics collected during program execution
#[derive(Debug, Clone, Default)]
pub struct ExecutionStats {
    /// Total number of instructions executed
    pub total_steps: StepCount,

    /// Number of loop iterations (times a loop body was entered)
    pub loop_iterations: u64,

    /// Peak memory cell index accessed (highest pointer position + 1)
    pub peak_memory_used: MemoryAddress,

    /// Number of memory cells with non-zero values at end of execution
    pub cells_modified: usize,

    /// Total bytes read from input
    pub bytes_read: u64,

    /// Total bytes written to output
    pub bytes_written: u64,

    /// Actual memory allocated (useful for unbounded model)
    pub memory_allocated: MemorySize,

    /// Runtime warnings collected during execution
    ///
    /// Warnings indicate potentially unexpected behavior such as:
    /// - Memory expansion (in Unbounded mode)
    ///
    /// Warnings are deduplicated by instruction index to avoid spam.
    pub warnings: Vec<RuntimeWarning>,
}

impl ExecutionStats {
    /// Create new stats tracker
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Count non-zero cells in memory
    #[inline]
    pub(crate) fn count_modified_cells(memory: &[u8]) -> usize {
        memory.iter().filter(|&&byte| byte != 0).count()
    }
}
