//! Execution configuration and builder

use super::cell_model::{U8CheckedCells, U8WrappingCells};
use super::memory_model::{FixedMemory, UnboundedMemory};
use super::{CellModel, EofBehavior, MemoryModel};
use crate::error::Result;
use crate::hooks::{BoxedHook, HookManager};
use crate::types::MemorySize;
use std::marker::PhantomData;

/// Constant: Default memory size (30,000 cells, traditional BrainFuck size)
pub const MEMORY_SIZE: usize = 30_000;

pub struct Unbuilt;
pub struct ReadyToBuild;

/// Configuration for BrainFuck interpreter execution
///
/// Use `ExecutionConfigBuilder` to create instances with validation.
#[derive(Debug)]
pub struct ExecutionConfig {
    memory_model: MemoryModel,
    cell_model: CellModel,
    max_steps: Option<u64>,
    timeout_ms: Option<u64>,
    eof_behavior: EofBehavior,
    pub(crate) hook_manager: Option<HookManager>,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            memory_model: MemoryModel::Fixed(FixedMemory::new(MemorySize::new(MEMORY_SIZE))),
            cell_model: CellModel::default(), // U8Wrapping by default
            max_steps: None,
            timeout_ms: None,
            eof_behavior: EofBehavior::default(),
            hook_manager: None,
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

    /// Get the EOF behavior
    #[inline]
    pub fn eof_behavior(&self) -> EofBehavior {
        self.eof_behavior
    }

    /// Get a mutable reference to the hook manager, if present
    ///
    /// This is used internally by the interpreter to dispatch hook events.
    #[inline]
    pub(crate) fn hook_manager_mut(&mut self) -> Option<&mut HookManager> {
        self.hook_manager.as_mut()
    }

    /// Register a hook for execution monitoring and control.
    ///
    /// This creates a HookManager if one doesn't exist and registers the hook.
    /// Multiple hooks can be registered and will be called in registration order.
    ///
    /// # Example
    ///
    /// ```rust
    /// use gyrus::{ExecutionConfigBuilder, ExecutionConfig};
    /// use gyrus::hooks::builtin::StatsTrackerHook;
    ///
    /// let mut config = ExecutionConfigBuilder::new()
    ///     .with_memory_size(1000)
    ///     .build();
    ///
    /// // Register a custom hook after building
    /// config.register_hook(Box::new(StatsTrackerHook::new()));
    /// ```
    pub fn register_hook(&mut self, hook: BoxedHook) {
        self.hook_manager
            .get_or_insert_with(HookManager::new)
            .register(hook);
    }

    /// Check if hooks are enabled
    #[inline]
    pub fn has_hooks(&self) -> bool {
        self.hook_manager.is_some()
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
    hook_manager: Option<HookManager>,
    _state: PhantomData<State>,
}

/// Macro to generate common builder methods shared between Unbuilt and ReadyToBuild states
macro_rules! common_builder_methods {
    () => {
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

        /// Enable hooks and register a hook
        ///
        /// This will create a HookManager if one doesn't exist and register the hook.
        ///
        /// # Example
        ///
        /// ```rust,ignore
        /// let config = ExecutionConfigBuilder::new()
        ///     .with_memory_size(1000)
        ///     .with_hook(Box::new(MyHook::new()))
        ///     .build();
        /// ```
        pub fn with_hook(mut self, hook: BoxedHook) -> Self {
            self.hook_manager
                .get_or_insert_with(HookManager::new)
                .register(hook);
            self
        }

        /// Enable hooks without registering any yet
        ///
        /// Creates an empty HookManager. You can add hooks later via `with_hook()`.
        pub fn with_hooks_enabled(mut self) -> Self {
            if self.hook_manager.is_none() {
                self.hook_manager = Some(HookManager::new());
            }
            self
        }
    };
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
            hook_manager: None,
            _state: PhantomData,
        }
    }

    /// Set fixed-size memory model
    ///
    /// # Panics
    ///
    /// Panics if `size` is 0. A zero-length tape has no cell 0 to read or write, so
    /// the first instruction would otherwise fail with an opaque out-of-bounds panic
    /// deep inside the interpreter. `with_unbounded_memory` rejects 0 the same way.
    #[track_caller]
    pub fn with_memory_size(mut self, size: usize) -> ExecutionConfigBuilder<ReadyToBuild> {
        assert!(size > 0, "memory size must be greater than 0");
        self.memory_model = Some(MemoryModel::Fixed(FixedMemory::new(MemorySize::new(size))));
        ExecutionConfigBuilder {
            memory_model: self.memory_model,
            cell_model: self.cell_model,
            max_steps: self.max_steps,
            timeout_ms: self.timeout_ms,
            eof_behavior: self.eof_behavior,
            hook_manager: self.hook_manager,
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
        // Validation lives in UnboundedMemory::new so that every route into the
        // type is checked, not only this one.
        self.memory_model = Some(MemoryModel::Unbounded(UnboundedMemory::new(
            MemorySize::new(initial_size),
            MemorySize::new(max_size),
        )?));

        Ok(ExecutionConfigBuilder {
            memory_model: self.memory_model,
            cell_model: self.cell_model,
            max_steps: self.max_steps,
            timeout_ms: self.timeout_ms,
            eof_behavior: self.eof_behavior,
            hook_manager: self.hook_manager,
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
            hook_manager: self.hook_manager,
            _state: PhantomData,
        }
    }

    // Common builder methods (shared with ReadyToBuild)
    common_builder_methods!();
}

impl ExecutionConfigBuilder<ReadyToBuild> {
    // Common builder methods (shared with Unbuilt)
    common_builder_methods!();

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
            hook_manager: self.hook_manager,
        }
    }
}

impl Default for ExecutionConfigBuilder<Unbuilt> {
    fn default() -> Self {
        Self::new()
    }
}
