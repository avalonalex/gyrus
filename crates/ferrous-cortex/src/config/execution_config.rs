//! Execution configuration and builder

use super::cell_model::{U8CheckedCells, U8WrappingCells};
use super::memory_model::{FixedMemory, UnboundedMemory};
use super::{CellModel, EofBehavior, MemoryModel};
use crate::error::{BfError, Result};
use crate::types::MemorySize;
use std::marker::PhantomData;

/// Constant: Default memory size (30,000 cells, traditional BrainFuck size)
pub const MEMORY_SIZE: usize = 30_000;

pub struct Unbuilt;
pub struct ReadyToBuild;

/// Configuration for BrainFuck interpreter execution
///
/// Use `ExecutionConfigBuilder` to create instances with validation.
#[derive(Debug, Clone)]
pub struct ExecutionConfig {
    memory_model: MemoryModel,
    cell_model: CellModel,
    max_steps: Option<u64>,
    timeout_ms: Option<u64>,
    allow_negative_pointer: bool,
    eof_behavior: EofBehavior,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            memory_model: MemoryModel::Fixed(FixedMemory::new(MemorySize::new(MEMORY_SIZE))),
            cell_model: CellModel::default(), // U8Wrapping by default
            max_steps: None,
            timeout_ms: None,
            allow_negative_pointer: false,
            eof_behavior: EofBehavior::default(),
        }
    }
}

impl ExecutionConfig {
    /// Get the memory model
    #[inline]
    pub fn memory_model(&self) -> &MemoryModel {
        &self.memory_model
    }

    /// Get the cell model
    #[inline]
    pub fn cell_model(&self) -> &CellModel {
        &self.cell_model
    }

    /// Get the maximum number of steps
    #[inline]
    pub fn max_steps(&self) -> Option<u64> {
        self.max_steps
    }

    /// Get the execution timeout in milliseconds
    #[inline]
    pub fn timeout_ms(&self) -> Option<u64> {
        self.timeout_ms
    }

    /// Check if negative pointer is allowed
    #[inline]
    pub fn allow_negative_pointer(&self) -> bool {
        self.allow_negative_pointer
    }

    /// Get the EOF behavior
    #[inline]
    pub fn eof_behavior(&self) -> EofBehavior {
        self.eof_behavior
    }

    /// Create a new builder
    pub fn builder() -> ExecutionConfigBuilder<Unbuilt> {
        ExecutionConfigBuilder::new()
    }
}

/// Enhanced builder for ExecutionConfig with type-state pattern and validation
pub struct ExecutionConfigBuilder<State = Unbuilt> {
    memory_model: Option<MemoryModel>,
    cell_model: CellModel,
    max_steps: Option<u64>,
    timeout_ms: Option<u64>,
    eof_behavior: EofBehavior,
    allow_negative_pointer: bool,
    _state: PhantomData<State>,
}

impl ExecutionConfigBuilder<Unbuilt> {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            memory_model: None,
            cell_model: CellModel::default(), // U8Wrapping by default
            max_steps: None,
            timeout_ms: None,
            eof_behavior: EofBehavior::default(),
            allow_negative_pointer: false,
            _state: PhantomData,
        }
    }

    /// Set fixed-size memory model
    pub fn with_memory_size(mut self, size: usize) -> ExecutionConfigBuilder<ReadyToBuild> {
        self.memory_model = Some(MemoryModel::Fixed(FixedMemory::new(MemorySize::new(size))));
        ExecutionConfigBuilder {
            memory_model: self.memory_model,
            cell_model: self.cell_model,
            max_steps: self.max_steps,
            timeout_ms: self.timeout_ms,
            eof_behavior: self.eof_behavior,
            allow_negative_pointer: self.allow_negative_pointer,
            _state: PhantomData,
        }
    }

    /// Set unbounded memory model with validation
    ///
    /// Returns an error if initial_size > max_size
    pub fn with_unbounded_memory(
        mut self,
        initial_size: usize,
        max_size: usize,
    ) -> Result<ExecutionConfigBuilder<ReadyToBuild>> {
        if initial_size > max_size {
            return Err(BfError::ConfigurationError {
                message: format!(
                    "initial_size ({}) cannot exceed max_size ({})",
                    initial_size, max_size
                ),
            });
        }

        if initial_size == 0 {
            return Err(BfError::ConfigurationError {
                message: "initial_size must be greater than 0".to_string(),
            });
        }

        if max_size == 0 {
            return Err(BfError::ConfigurationError {
                message: "max_size must be greater than 0".to_string(),
            });
        }

        self.memory_model = Some(MemoryModel::Unbounded(UnboundedMemory::new(
            MemorySize::new(initial_size),
            MemorySize::new(max_size),
        )));

        Ok(ExecutionConfigBuilder {
            memory_model: self.memory_model,
            cell_model: self.cell_model,
            max_steps: self.max_steps,
            timeout_ms: self.timeout_ms,
            eof_behavior: self.eof_behavior,
            allow_negative_pointer: self.allow_negative_pointer,
            _state: PhantomData,
        })
    }

    /// Set a custom memory model directly
    pub fn with_memory_model(mut self, model: MemoryModel) -> ExecutionConfigBuilder<ReadyToBuild> {
        self.memory_model = Some(model);
        ExecutionConfigBuilder {
            memory_model: self.memory_model,
            cell_model: self.cell_model,
            max_steps: self.max_steps,
            timeout_ms: self.timeout_ms,
            eof_behavior: self.eof_behavior,
            allow_negative_pointer: self.allow_negative_pointer,
            _state: PhantomData,
        }
    }

    /// Set cell model
    pub fn with_cell_model(mut self, model: CellModel) -> Self {
        self.cell_model = model;
        self
    }

    /// Set cell model to U8 wrapping (default)
    pub fn with_wrapping_cells(mut self) -> Self {
        self.cell_model = CellModel::U8Wrapping(U8WrappingCells);
        self
    }

    /// Set cell model to U8 checked (errors on overflow/underflow)
    pub fn with_checked_cells(mut self) -> Self {
        self.cell_model = CellModel::U8Checked(U8CheckedCells);
        self
    }

    /// Set EOF behavior
    pub fn with_eof_behavior(mut self, behavior: EofBehavior) -> Self {
        self.eof_behavior = behavior;
        self
    }

    /// Allow negative pointer
    pub fn with_negative_pointer(mut self, allow: bool) -> Self {
        self.allow_negative_pointer = allow;
        self
    }

    /// Set maximum execution steps
    pub fn with_max_steps(mut self, steps: u64) -> Self {
        self.max_steps = Some(steps);
        self
    }

    /// Set execution timeout in milliseconds
    pub fn with_timeout_ms(mut self, timeout: u64) -> Self {
        self.timeout_ms = Some(timeout);
        self
    }
}

impl ExecutionConfigBuilder<ReadyToBuild> {
    /// Set cell model
    pub fn with_cell_model(mut self, model: CellModel) -> Self {
        self.cell_model = model;
        self
    }

    /// Set cell model to U8 wrapping (default)
    pub fn with_wrapping_cells(mut self) -> Self {
        self.cell_model = CellModel::U8Wrapping(U8WrappingCells);
        self
    }

    /// Set cell model to U8 checked (errors on overflow/underflow)
    pub fn with_checked_cells(mut self) -> Self {
        self.cell_model = CellModel::U8Checked(U8CheckedCells);
        self
    }

    /// Set EOF behavior
    pub fn with_eof_behavior(mut self, behavior: EofBehavior) -> Self {
        self.eof_behavior = behavior;
        self
    }

    /// Allow negative pointer
    pub fn with_negative_pointer(mut self, allow: bool) -> Self {
        self.allow_negative_pointer = allow;
        self
    }

    /// Set maximum execution steps
    pub fn with_max_steps(mut self, steps: u64) -> Self {
        self.max_steps = Some(steps);
        self
    }

    /// Set execution timeout in milliseconds
    pub fn with_timeout_ms(mut self, timeout: u64) -> Self {
        self.timeout_ms = Some(timeout);
        self
    }

    /// Build the final ExecutionConfig
    ///
    /// This method is only available after a memory model has been set.
    pub fn build(self) -> ExecutionConfig {
        ExecutionConfig {
            memory_model: self.memory_model.expect("memory_model must be set"),
            cell_model: self.cell_model,
            max_steps: self.max_steps,
            timeout_ms: self.timeout_ms,
            eof_behavior: self.eof_behavior,
            allow_negative_pointer: self.allow_negative_pointer,
        }
    }
}

impl Default for ExecutionConfigBuilder<Unbuilt> {
    fn default() -> Self {
        Self::new()
    }
}
