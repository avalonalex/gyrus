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
//! - **CellModel**: Controls cell arithmetic (U8Wrapping/U8Checked/U8Saturating/I8Wrapping/...)
//!
//! These are orthogonal concepts and can be mixed independently:
//! - Fixed memory + U8Wrapping cells (current default)
//! - Wrapping memory + U8Checked cells (possible future config)
//! - Unbounded memory + I8Wrapping cells (possible future config)
//!
//! See `interpreter.rs:195-200` for current cell arithmetic implementation.
//! See `validator.rs` module docs for how validation assumes u8 wrapping cells.

use crate::error::{BfError, MemoryDump, Result, RuntimeWarning};
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

/// Trait defining cell arithmetic behavior
///
/// **Important**: This trait controls CELL VALUE ARITHMETIC only, not pointer movement!
///
/// Each cell model implements this trait to provide its own arithmetic logic:
/// - `try_increment()`: Handles `+` instruction (increment cell value)
/// - `try_decrement()`: Handles `-` instruction (decrement cell value)
/// - `is_zero()`: Checks if cell value is zero (for loop conditions)
///
/// Pointer movement (`>` and `<` instructions) is NOT part of this trait.
/// Pointer behavior is controlled by `MemoryBehavior` trait.
///
/// # Cell Models
///
/// Different cell models define different overflow/underflow behaviors:
/// - **U8 Wrapping**: 255+1=0, 0-1=255 (most compatible, default)
/// - **U8 Checked**: Overflow/underflow returns error
/// - **U8 Saturating**: 255+1=255, 0-1=0 (clamps at boundaries)
///
/// # Orthogonality with MemoryModel
///
/// CellModel and MemoryModel are completely independent:
/// - **MemoryModel**: Controls pointer position (Fixed/Wrapping/Unbounded)
/// - **CellModel**: Controls cell values (U8Wrapping/U8Checked/U8Saturating)
///
/// Any combination is valid:
/// - Fixed memory + U8 Wrapping cells (default)
/// - Wrapping memory + U8 Checked cells
/// - Unbounded memory + U8 Saturating cells
pub trait CellBehavior {
    /// Try to increment the cell value by 1
    ///
    /// Returns an error if the operation would violate the cell model's constraints
    /// (e.g., overflow with checked arithmetic).
    ///
    /// Runtime warnings (e.g., overflow in wrapping mode) are collected in `warnings`.
    fn try_increment(
        &self,
        value: &mut u8,
        step_count: StepCount,
        warnings: &mut Vec<RuntimeWarning>,
    ) -> Result<()>;

    /// Try to decrement the cell value by 1
    ///
    /// Returns an error if the operation would violate the cell model's constraints
    /// (e.g., underflow with checked arithmetic).
    ///
    /// Runtime warnings (e.g., underflow in wrapping mode) are collected in `warnings`.
    fn try_decrement(
        &self,
        value: &mut u8,
        step_count: StepCount,
        warnings: &mut Vec<RuntimeWarning>,
    ) -> Result<()>;

    /// Check if the cell value represents zero
    ///
    /// Used for loop conditions `[` and `]`.
    /// Currently always checks `value == 0`, but included for future extensibility
    /// (e.g., signed types where -0 might need special handling).
    #[inline]
    fn is_zero(&self, value: u8) -> bool {
        value == 0
    }
}

/// U8 wrapping cell model (default, most compatible)
///
/// Overflow and underflow wrap around using modular arithmetic:
/// - Increment overflow: `255 + 1 = 0`
/// - Decrement underflow: `0 - 1 = 255`
///
/// This is the traditional BrainFuck behavior and matches most implementations.
/// Programs like `[+]` will terminate (not infinite) as the value wraps through 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct U8WrappingCells;

impl CellBehavior for U8WrappingCells {
    #[inline]
    fn try_increment(
        &self,
        value: &mut u8,
        step_count: StepCount,
        warnings: &mut Vec<RuntimeWarning>,
    ) -> Result<()> {
        // Detect overflow: 255 + 1 = 0
        if *value == 255 {
            warnings.push(RuntimeWarning::CellOverflow {
                instruction_index: step_count.into(),
                _reserved: (),
            });
        }
        *value = value.wrapping_add(1);
        Ok(())
    }

    #[inline]
    fn try_decrement(
        &self,
        value: &mut u8,
        step_count: StepCount,
        warnings: &mut Vec<RuntimeWarning>,
    ) -> Result<()> {
        // Detect underflow: 0 - 1 = 255
        if *value == 0 {
            warnings.push(RuntimeWarning::CellUnderflow {
                instruction_index: step_count.into(),
                _reserved: (),
            });
        }
        *value = value.wrapping_sub(1);
        Ok(())
    }
}

impl fmt::Display for U8WrappingCells {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "u8-wrapping")
    }
}

/// U8 checked cell model (errors on overflow/underflow)
///
/// Overflow and underflow return errors instead of wrapping:
/// - Increment overflow: `255 + 1` → `BfError::CellOverflow`
/// - Decrement underflow: `0 - 1` → `BfError::CellUnderflow`
///
/// Use this mode to catch arithmetic bugs in BrainFuck programs.
/// Programs like `[+]` will error when they reach 255+1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct U8CheckedCells;

impl CellBehavior for U8CheckedCells {
    fn try_increment(
        &self,
        value: &mut u8,
        step_count: StepCount,
        _warnings: &mut Vec<RuntimeWarning>,
    ) -> Result<()> {
        *value = value.checked_add(1).ok_or_else(|| BfError::CellOverflow {
            instruction_index: step_count.into(),
            current_value: 255,
            hint: "Cell value reached maximum (255) and cannot be incremented further with checked arithmetic. \
                 Try using --cell-model wrapping if wrapping behavior is intended, \
                 or check your program logic for infinite loops.".to_string(),
        })?;
        Ok(())
    }

    fn try_decrement(
        &self,
        value: &mut u8,
        step_count: StepCount,
        _warnings: &mut Vec<RuntimeWarning>,
    ) -> Result<()> {
        *value = value.checked_sub(1).ok_or_else(|| BfError::CellUnderflow {
            instruction_index: step_count.into(),
            current_value: 0,
            hint: "Cell value reached minimum (0) and cannot be decremented further with checked arithmetic. \
                 Try using --cell-model wrapping if wrapping behavior is intended, \
                 or check your program logic.".to_string(),
        })?;
        Ok(())
    }
}

impl fmt::Display for U8CheckedCells {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "u8-checked")
    }
}

/// Cell model for interpreter execution
///
/// **Important**: This controls CELL ARITHMETIC (+,-) only, not pointer movement (>,<)!
///
/// # Cell Arithmetic Behaviors
///
/// - **U8Wrapping**: Overflow wraps (255+1=0, 0-1=255) - traditional BF, most compatible (default)
/// - **U8Checked**: Overflow/underflow returns error - strict mode for catching bugs
///
/// # Use Cases
///
/// - **Production/Compatibility**: Use `U8Wrapping` - matches standard BrainFuck behavior
/// - **Development/Debugging**: Use `U8Checked` - catches arithmetic bugs immediately
///
/// # Pointer Movement (NOT controlled by this enum)
///
/// Pointer movement is controlled by `MemoryModel` (Fixed/Wrapping/Unbounded).
/// See `MemoryModel` documentation for pointer behavior.
///
/// # Orthogonality
///
/// CellModel and MemoryModel can be mixed independently:
/// ```rust
/// # use ferrous_cortex::*;
/// // Fixed memory + wrapping cells (default - traditional BF)
/// let config = ExecutionConfig::builder()
///     .with_memory_size(30000)
///     .with_wrapping_cells()
///     .build();
///
/// // Unbounded memory + checked cells (debugging)
/// let config = ExecutionConfig::builder()
///     .with_unbounded_memory(1000, 1000000).unwrap()
///     .with_checked_cells()
///     .build();
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellModel {
    /// U8 wrapping arithmetic (traditional BrainFuck, default)
    ///
    /// Cell values wrap at boundaries: 255+1=0, 0-1=255.
    /// This is the standard BrainFuck behavior and matches most implementations.
    /// Patterns like `[+]` terminate (not infinite) by wrapping through 0.
    U8Wrapping(U8WrappingCells),

    /// U8 checked arithmetic (strict debugging mode)
    ///
    /// Cell overflow/underflow raises `BfError::CellOverflow` or `BfError::CellUnderflow`.
    /// Use this to catch arithmetic bugs in programs during development.
    U8Checked(U8CheckedCells),
    // Future cell models:
    // I8Wrapping(I8WrappingCells),
    // U16Wrapping(U16WrappingCells),
}

impl Default for CellModel {
    fn default() -> Self {
        CellModel::U8Wrapping(U8WrappingCells)
    }
}

impl CellModel {
    /// Delegate increment to the specific cell model
    #[inline]
    pub fn try_increment(
        &self,
        value: &mut u8,
        step_count: StepCount,
        warnings: &mut Vec<RuntimeWarning>,
    ) -> Result<()> {
        match self {
            CellModel::U8Wrapping(m) => m.try_increment(value, step_count, warnings),
            CellModel::U8Checked(m) => m.try_increment(value, step_count, warnings),
        }
    }

    /// Delegate decrement to the specific cell model
    #[inline]
    pub fn try_decrement(
        &self,
        value: &mut u8,
        step_count: StepCount,
        warnings: &mut Vec<RuntimeWarning>,
    ) -> Result<()> {
        match self {
            CellModel::U8Wrapping(m) => m.try_decrement(value, step_count, warnings),
            CellModel::U8Checked(m) => m.try_decrement(value, step_count, warnings),
        }
    }

    /// Check if value is zero (for loop conditions)
    #[inline]
    pub fn is_zero(&self, value: u8) -> bool {
        match self {
            CellModel::U8Wrapping(m) => m.is_zero(value),
            CellModel::U8Checked(m) => m.is_zero(value),
        }
    }
}

impl fmt::Display for CellModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CellModel::U8Wrapping(m) => write!(f, "{}", m),
            CellModel::U8Checked(m) => write!(f, "{}", m),
        }
    }
}

/// Trait defining memory behavior operations
///
/// **Important**: This trait controls POINTER MOVEMENT only, not cell arithmetic!
///
/// Each memory model implements this trait to provide its own
/// pointer increment/decrement logic:
/// - `try_increment_pointer()`: Handles `>` instruction (move pointer right)
/// - `try_decrement_pointer()`: Handles `<` instruction (move pointer left)
///
/// Cell arithmetic (`+` and `-` instructions) is NOT part of this trait.
/// Cell values are always `u8` with wrapping arithmetic (see `interpreter.rs:195-200`).
pub trait MemoryBehavior {
    /// Try to increment the pointer by 1
    ///
    /// Returns an error if the operation would violate the memory model's constraints.
    /// May grow memory for unbounded models.
    ///
    /// Runtime warnings (e.g., memory expansion in unbounded mode) are collected in `warnings`.
    fn try_increment_pointer(
        &self,
        pointer: &mut MemoryAddress,
        memory: &mut Vec<u8>,
        step_count: StepCount,
        warnings: &mut Vec<RuntimeWarning>,
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
        warnings: &mut Vec<RuntimeWarning>,
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
        _warnings: &mut Vec<RuntimeWarning>,
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
                     Try increasing memory size with --memory-size {} or use --memory-model unbounded",
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
        _warnings: &mut Vec<RuntimeWarning>,
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
        warnings: &mut Vec<RuntimeWarning>,
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
        let old_size = memory.len();
        if pointer.get() >= old_size {
            let new_size = pointer.get() + 1;
            memory.resize(new_size, 0);

            // Warn about memory expansion
            warnings.push(RuntimeWarning::MemoryExpanded {
                instruction_index: step_count.into(),
                from_size: MemorySize::new(old_size),
                to_size: MemorySize::new(new_size),
                _reserved: (),
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
        _warnings: &mut Vec<RuntimeWarning>,
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
///
/// **Important**: This controls POINTER MOVEMENT (>,<) only, not cell arithmetic (+,-)!
///
/// # Pointer Overflow Behaviors
///
/// - **Fixed**: Out-of-bounds pointer access → Error
/// - **Wrapping**: Pointer wraps at boundaries (circular buffer)
/// - **Unbounded**: Memory grows dynamically up to max limit
///
/// # Cell Arithmetic (NOT controlled by this enum)
///
/// Cell arithmetic is hardcoded as `u8` wrapping regardless of MemoryModel:
/// - Cell values: 0-255 (unsigned 8-bit)
/// - Increment overflow: `255 + 1 = 0`
/// - Decrement underflow: `0 - 1 = 255`
///
/// See module-level documentation for detailed examples and future plans.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryModel {
    /// Fixed-size memory array
    ///
    /// Pointer movement beyond bounds raises `BfError::MemoryOutOfBounds`.
    /// Use this for strict BrainFuck compliance and catching bugs.
    /// This is the default and recommended for production/JIT targets.
    Fixed(FixedMemory),

    /// Unbounded memory: grows as needed up to a maximum limit
    ///
    /// Memory expands dynamically when accessed beyond current size.
    /// Use this for programs with dynamic memory needs or during development.
    Unbounded(UnboundedMemory),
}

impl MemoryModel {
    /// Get the initial memory size for this model
    #[inline]
    pub fn initial_size(&self) -> MemorySize {
        match self {
            MemoryModel::Fixed(m) => m.initial_size(),
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
        warnings: &mut Vec<RuntimeWarning>,
    ) -> Result<()> {
        match self {
            MemoryModel::Fixed(m) => m.try_increment_pointer(pointer, memory, step_count, warnings),
            MemoryModel::Unbounded(m) => {
                m.try_increment_pointer(pointer, memory, step_count, warnings)
            }
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
        warnings: &mut Vec<RuntimeWarning>,
    ) -> Result<()> {
        match self {
            MemoryModel::Fixed(m) => {
                m.try_decrement_pointer(pointer, memory, allow_negative, step_count, warnings)
            }
            MemoryModel::Unbounded(m) => {
                m.try_decrement_pointer(pointer, memory, allow_negative, step_count, warnings)
            }
        }
    }
}

impl fmt::Display for MemoryModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MemoryModel::Fixed(m) => write!(f, "{}", m),
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
