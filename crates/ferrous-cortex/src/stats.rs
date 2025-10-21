/// Statistics collected during program execution
#[derive(Debug, Clone, Default)]
pub struct ExecutionStats {
    /// Total number of instructions executed
    pub total_steps: u64,

    /// Number of loop iterations (times a loop body was entered)
    pub loop_iterations: u64,

    /// Peak memory cell index accessed (highest pointer position + 1)
    pub peak_memory_used: usize,

    /// Number of memory cells with non-zero values at end of execution
    pub cells_modified: usize,

    /// Total bytes read from input
    pub bytes_read: u64,

    /// Total bytes written to output
    pub bytes_written: u64,

    /// Actual memory allocated (useful for unbounded model)
    pub memory_allocated: usize,
}

impl ExecutionStats {
    /// Create new stats tracker
    pub fn new() -> Self {
        Self::default()
    }

    /// Count non-zero cells in memory
    pub(crate) fn count_modified_cells(memory: &[u8]) -> usize {
        memory.iter().filter(|&&byte| byte != 0).count()
    }
}
