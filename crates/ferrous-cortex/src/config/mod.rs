//! Execution configuration and memory models
//!
//! # Important: MemoryModel vs Cell Arithmetic
//!
//! **MemoryModel controls POINTER MOVEMENT only, not cell values!**
//!
//! ## What MemoryModel Controls (Pointer Overflow)
//!
//! - **Fixed**: Pointer moving beyond bounds → ERROR
//! - **Wrapping**: Pointer wraps around at boundaries (circular buffer)
//! - **Unbounded**: Memory grows dynamically up to max limit
//!
//! ## What MemoryModel Does NOT Control (Cell Arithmetic)
//!
//! Cell arithmetic is **hardcoded** in `interpreter.rs` as:
//! - **Cell type**: `u8` (unsigned 8-bit, range 0-255)
//! - **Increment overflow**: `255 + 1 = 0` (via `wrapping_add(1)`)
//! - **Decrement underflow**: `0 - 1 = 255` (via `wrapping_sub(1)`)
//!
//! This means:
//! - MemoryModel affects `>` and `<` instructions (pointer movement)
//! - Cell arithmetic (`+` and `-`) always uses u8 wrapping, regardless of MemoryModel
//!
//! ## Examples
//!
//! **Fixed memory with pointer overflow**:
//! ```text
//! Memory: [0][1][2]...[29999]  (30,000 cells)
//! Pointer at 29999, execute `>` → ERROR (out of bounds)
//! Cell value 255, execute `+` → wraps to 0 (cell arithmetic, always wraps)
//! ```
//!
//! **Wrapping memory with pointer overflow**:
//! ```text
//! Memory: [0][1][2]...[29999]  (30,000 cells, circular)
//! Pointer at 29999, execute `>` → wraps to 0 (pointer wraps)
//! Cell value 255, execute `+` → wraps to 0 (cell arithmetic, always wraps)
//! ```
//!
//! **Unbounded memory**:
//! ```text
//! Memory: [0][1]...[N]  (grows as needed up to max)
//! Pointer at N, execute `>` → grows memory to N+1 (if under max)
//! Cell value 255, execute `+` → wraps to 0 (cell arithmetic, always wraps)
//! ```
//!
//! ## Future: Configurable Cell Arithmetic
//!
//! When cell arithmetic becomes configurable (via future `CellModel`), we will have:
//! - **MemoryModel**: Controls pointer movement (Fixed/Wrapping/Unbounded)
//! - **CellModel**: Controls cell values (U8Wrapping/U8Checked/U8Saturating)
//!
//! This will allow combinations like:
//! - Fixed memory + U8 Checked cells (strict mode for both)
//! - Unbounded memory + U8 Wrapping cells (lenient on memory, traditional on cells)

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
