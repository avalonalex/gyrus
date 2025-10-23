use crate::error::{BfError, MemoryDump, Result};
use crate::types::{MemoryAddress, MemorySize, StepCount};
use std::fmt;
use std::marker::PhantomData;

/// Default memory size (30,000 bytes - traditional BrainFuck)
pub const MEMORY_SIZE: usize = 30000;

/// Behavior when input (,) encounters EOF
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EofBehavior {
    /// Set cell to 0 on EOF (most common, used by many interpreters)
    #[default]
    SetZero,

    /// Set cell to 255 (-1 as unsigned byte) on EOF
    SetNegOne,

    /// Leave cell unchanged on EOF
    NoChange,

    /// Return error on EOF (strictest, prevents silent bugs)
    Error,
}

/// Trait defining memory behavior operations
///
/// Each memory model implements this trait to provide its own
/// pointer increment/decrement logic.
pub trait MemoryBehavior {
    /// Try to increment the pointer by 1
    ///
    /// Returns an error if the operation would violate the memory model's constraints.
    /// May grow memory for unbounded models.
    fn try_increment_pointer(
        &self,
        pointer: &mut MemoryAddress,
        memory: &mut Vec<u8>,
        step_count: StepCount,
    ) -> Result<()>;

    /// Try to decrement the pointer by 1
    ///
    /// Returns an error if the operation would violate the memory model's constraints.
    fn try_decrement_pointer(
        &self,
        pointer: &mut MemoryAddress,
        memory: &[u8],
        allow_negative: bool,
        step_count: StepCount,
    ) -> Result<()>;

    /// Get the initial memory size for this model
    fn initial_size(&self) -> MemorySize;
}

/// Fixed-size memory model
///
/// Out-of-bounds access returns an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedMemory {
    size: MemorySize,
}

impl FixedMemory {
    /// Create a new fixed memory model with the given size
    pub fn new(size: MemorySize) -> Self {
        Self { size }
    }
}

impl MemoryBehavior for FixedMemory {
    fn try_increment_pointer(
        &self,
        pointer: &mut MemoryAddress,
        memory: &mut Vec<u8>,
        step_count: StepCount,
    ) -> Result<()> {
        pointer.increment();

        if pointer.get() >= self.size.get() {
            let dump = MemoryDump::from_memory(memory, *pointer);
            return Err(BfError::MemoryOutOfBounds {
                instruction_index: step_count.into(),
                attempted: pointer.get() as isize,
                max: MemorySize::new(self.size.get() - 1),
                memory_dump: Some(dump),
                hint: format!(
                    "Attempted to access cell {}, but memory size is fixed at {} cells. \
                     Try increasing memory size with --memory-size {} or use --memory-model wrapping",
                    pointer.get(),
                    self.size.get(),
                    pointer.get() + 1000
                ),
            });
        }

        Ok(())
    }

    fn try_decrement_pointer(
        &self,
        pointer: &mut MemoryAddress,
        memory: &[u8],
        allow_negative: bool,
        step_count: StepCount,
    ) -> Result<()> {
        if pointer.get() == 0 && !allow_negative {
            let dump = MemoryDump::from_memory(memory, *pointer);
            return Err(BfError::MemoryOutOfBounds {
                instruction_index: step_count.into(),
                attempted: -1,
                max: MemorySize::new(self.size.get() - 1),
                memory_dump: Some(dump),
                hint: "Attempted to move pointer below cell 0. Memory cells are indexed from 0 onwards.".to_string(),
            });
        }
        if pointer.get() > 0 {
            pointer.decrement();
        }
        Ok(())
    }

    fn initial_size(&self) -> MemorySize {
        self.size
    }
}

impl fmt::Display for FixedMemory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fixed({} bytes)", self.size)
    }
}

/// Wrapping memory model
///
/// Pointer wraps around at boundaries.
/// e.g., at size 30000: pointer 30000 -> 0, pointer -1 -> 29999
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrappingMemory {
    size: MemorySize,
}

impl WrappingMemory {
    /// Create a new wrapping memory model with the given size
    pub fn new(size: MemorySize) -> Self {
        Self { size }
    }
}

impl MemoryBehavior for WrappingMemory {
    fn try_increment_pointer(
        &self,
        pointer: &mut MemoryAddress,
        _memory: &mut Vec<u8>,
        _step_count: StepCount,
    ) -> Result<()> {
        pointer.increment();

        if pointer.get() >= self.size.get() {
            *pointer = MemoryAddress::new(0); // Wrap around to beginning
        }

        Ok(())
    }

    fn try_decrement_pointer(
        &self,
        pointer: &mut MemoryAddress,
        _memory: &[u8],
        _allow_negative: bool,
        _step_count: StepCount,
    ) -> Result<()> {
        if pointer.get() == 0 {
            *pointer = MemoryAddress::new(self.size.get() - 1); // Wrap around to end
        } else {
            pointer.decrement();
        }
        Ok(())
    }

    fn initial_size(&self) -> MemorySize {
        self.size
    }
}

impl fmt::Display for WrappingMemory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Wrapping({} bytes)", self.size)
    }
}

/// Unbounded memory model
///
/// Grows as needed up to a maximum limit.
/// Starts small and expands when accessed beyond current size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnboundedMemory {
    initial_size: MemorySize,
    max_size: MemorySize,
}

impl UnboundedMemory {
    /// Create a new unbounded memory model
    pub fn new(initial_size: MemorySize, max_size: MemorySize) -> Self {
        Self {
            initial_size,
            max_size,
        }
    }

    /// Get the maximum size for this unbounded memory model
    pub fn max_size(&self) -> MemorySize {
        self.max_size
    }
}

impl MemoryBehavior for UnboundedMemory {
    fn try_increment_pointer(
        &self,
        pointer: &mut MemoryAddress,
        memory: &mut Vec<u8>,
        step_count: StepCount,
    ) -> Result<()> {
        pointer.increment();

        if pointer.get() >= self.max_size.get() {
            let dump = MemoryDump::from_memory(memory, *pointer);
            return Err(BfError::MemoryOutOfBounds {
                instruction_index: step_count.into(),
                attempted: pointer.get() as isize,
                max: MemorySize::new(self.max_size.get() - 1),
                memory_dump: Some(dump),
                hint: format!(
                    "Attempted to access cell {}, exceeding maximum size of {}. \
                     This may indicate an infinite loop moving the pointer",
                    pointer.get(),
                    self.max_size.get()
                ),
            });
        }
        // Grow memory if needed
        if pointer.get() >= memory.len() {
            memory.resize(pointer.get() + 1, 0);
        }

        Ok(())
    }

    fn try_decrement_pointer(
        &self,
        pointer: &mut MemoryAddress,
        memory: &[u8],
        allow_negative: bool,
        step_count: StepCount,
    ) -> Result<()> {
        if pointer.get() == 0 && !allow_negative {
            let dump = MemoryDump::from_memory(memory, *pointer);
            return Err(BfError::MemoryOutOfBounds {
                instruction_index: step_count.into(),
                attempted: -1,
                max: MemorySize::new(self.max_size.get() - 1),
                memory_dump: Some(dump),
                hint: "Attempted to move pointer below cell 0. Memory cells are indexed from 0 onwards.".to_string(),
            });
        }
        if pointer.get() > 0 {
            pointer.decrement();
        }
        Ok(())
    }

    fn initial_size(&self) -> MemorySize {
        self.initial_size
    }
}

impl fmt::Display for UnboundedMemory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Unbounded(initial: {}, max: {})",
            self.initial_size, self.max_size
        )
    }
}

/// Memory model for interpreter execution
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryModel {
    /// Fixed-size memory array
    Fixed(FixedMemory),

    /// Wrapping memory: pointer wraps around at boundaries
    Wrapping(WrappingMemory),

    /// Unbounded memory: grows as needed up to a maximum limit
    Unbounded(UnboundedMemory),
}

impl MemoryModel {
    /// Get the initial memory size for this model
    #[inline]
    pub fn initial_size(&self) -> MemorySize {
        match self {
            MemoryModel::Fixed(m) => m.initial_size(),
            MemoryModel::Wrapping(m) => m.initial_size(),
            MemoryModel::Unbounded(m) => m.initial_size(),
        }
    }

    /// Handle pointer increment based on memory model
    ///
    /// Delegates to the specific memory model implementation.
    #[inline]
    pub fn try_increment_pointer(
        &self,
        pointer: &mut MemoryAddress,
        memory: &mut Vec<u8>,
        step_count: StepCount,
    ) -> Result<()> {
        match self {
            MemoryModel::Fixed(m) => m.try_increment_pointer(pointer, memory, step_count),
            MemoryModel::Wrapping(m) => m.try_increment_pointer(pointer, memory, step_count),
            MemoryModel::Unbounded(m) => m.try_increment_pointer(pointer, memory, step_count),
        }
    }

    /// Handle pointer decrement based on memory model
    ///
    /// Delegates to the specific memory model implementation.
    #[inline]
    pub fn try_decrement_pointer(
        &self,
        pointer: &mut MemoryAddress,
        memory: &[u8],
        allow_negative: bool,
        step_count: StepCount,
    ) -> Result<()> {
        match self {
            MemoryModel::Fixed(m) => {
                m.try_decrement_pointer(pointer, memory, allow_negative, step_count)
            }
            MemoryModel::Wrapping(m) => {
                m.try_decrement_pointer(pointer, memory, allow_negative, step_count)
            }
            MemoryModel::Unbounded(m) => {
                m.try_decrement_pointer(pointer, memory, allow_negative, step_count)
            }
        }
    }
}

impl fmt::Display for MemoryModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MemoryModel::Fixed(m) => write!(f, "{}", m),
            MemoryModel::Wrapping(m) => write!(f, "{}", m),
            MemoryModel::Unbounded(m) => write!(f, "{}", m),
        }
    }
}

/// Type-state markers for builder pattern
pub struct Unbuilt;
pub struct ReadyToBuild;

/// Configuration for BrainFuck interpreter execution
///
/// Use `ExecutionConfigBuilder` to create instances with validation.
#[derive(Debug, Clone)]
pub struct ExecutionConfig {
    memory_model: MemoryModel,
    max_steps: Option<u64>,
    timeout_ms: Option<u64>,
    allow_negative_pointer: bool,
    eof_behavior: EofBehavior,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            memory_model: MemoryModel::Fixed(FixedMemory::new(MemorySize::new(MEMORY_SIZE))),
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
            max_steps: self.max_steps,
            timeout_ms: self.timeout_ms,
            eof_behavior: self.eof_behavior,
            allow_negative_pointer: self.allow_negative_pointer,
            _state: PhantomData,
        }
    }

    /// Set wrapping memory model
    pub fn with_wrapping_memory(mut self, size: usize) -> ExecutionConfigBuilder<ReadyToBuild> {
        self.memory_model = Some(MemoryModel::Wrapping(WrappingMemory::new(MemorySize::new(
            size,
        ))));
        ExecutionConfigBuilder {
            memory_model: self.memory_model,
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
            max_steps: self.max_steps,
            timeout_ms: self.timeout_ms,
            eof_behavior: self.eof_behavior,
            allow_negative_pointer: self.allow_negative_pointer,
            _state: PhantomData,
        }
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
