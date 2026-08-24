//! Execution configuration and memory models
//!
//! # Important: MemoryModel vs Cell Arithmetic
//!
//! **MemoryModel controls POINTER MOVEMENT only, not cell values!**
//!
//! ## What MemoryModel Controls (Pointer Overflow)
//!
//! - **Fixed**: Pointer moving beyond bounds → ERROR
//! - **Unbounded**: Memory grows dynamically up to max limit
//!
//! ## What MemoryModel Does NOT Control (Cell Arithmetic)
//!
//! Cell arithmetic is [`CellModel`]'s job, and the two are fully independent:
//! any memory model combines with any cell model.
//!
//! - [`CellModel::U8Wrapping`] (default): `255 + 1 = 0`, `0 - 1 = 255`
//! - [`CellModel::U8Checked`]: those raise `CellOverflow` / `CellUnderflow`
//!
//! So MemoryModel governs `>` and `<`; CellModel governs `+` and `-`.
//!
//! ## Examples
//!
//! **Fixed memory with pointer overflow**:
//! ```text
//! Memory: [0][1][2]...[29999]  (30,000 cells)
//! Pointer at 29999, execute `>` → ERROR (out of bounds)
//! ```
//!
//! **Unbounded memory**:
//! ```text
//! Memory: [0][1]...[N]  (grows as needed up to max)
//! Pointer at N, execute `>` → grows memory to N+1 (if under max)
//! ```
//!
//! ## Combining the two
//!
//! Any pairing is valid:
//! - Fixed memory + U8Checked cells (strict on both)
//! - Unbounded memory + U8Wrapping cells (lenient on memory, traditional on cells)

// Submodules
mod cell_model;
mod eof_behavior;
mod execution_config;
mod memory_model;

// Re-exports from submodules
pub use cell_model::{CellBehavior, CellModel, U8CheckedCells, U8WrappingCells};
pub use eof_behavior::EofBehavior;
pub use execution_config::{
    ExecutionConfig, ExecutionConfigBuilder, MEMORY_SIZE, ReadyToBuild, Unbuilt,
};
pub use memory_model::{MemoryBehavior, MemoryModel};
