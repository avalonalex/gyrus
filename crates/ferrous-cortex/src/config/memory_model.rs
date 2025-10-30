//! Memory management behavior models

use crate::debug::{DebugInfo, LoopContext};
use crate::error::{BfError, MemoryDump, Result, RuntimeWarning};
use crate::types::{MemoryAddress, MemorySize, StepCount};
use std::fmt;

pub trait MemoryBehavior {
    /// Try to increment the pointer by 1
    ///
    /// Returns an error if the operation would violate the memory model's constraints.
    /// May grow memory for unbounded models.
    ///
    /// Runtime warnings (e.g., memory expansion in unbounded mode) are collected in `warnings`.
    ///
    /// Phase 2: Uses instruction_index for accurate source location lookup even in loops.
    fn try_increment_pointer(
        &self,
        pointer: &mut MemoryAddress,
        memory: &mut Vec<u8>,
        step_count: StepCount,
        warnings: &mut Vec<RuntimeWarning>,
        debug_info: Option<&DebugInfo>,
        instruction_index: usize,        // Phase 2: current instruction in flat index
        loop_stack: &[LoopContext],      // Phase 2: active loop contexts
    ) -> Result<()>;

    /// Try to decrement the pointer by 1
    ///
    /// Returns an error if the operation would violate the memory model's constraints.
    ///
    /// Phase 2: Uses instruction_index for accurate source location lookup even in loops.
    fn try_decrement_pointer(
        &self,
        pointer: &mut MemoryAddress,
        memory: &[u8],
        allow_negative: bool,
        step_count: StepCount,
        warnings: &mut Vec<RuntimeWarning>,
        debug_info: Option<&DebugInfo>,
        instruction_index: usize,        // Phase 2: current instruction in flat index
        loop_stack: &[LoopContext],      // Phase 2: active loop contexts
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
        debug_info: Option<&DebugInfo>,
        instruction_index: usize,        // Phase 2: use this directly
        loop_stack: &[LoopContext],      // Phase 2: active loop contexts
    ) -> Result<()> {
        pointer.increment();

        if pointer.get() >= self.size.get() {
            let dump = MemoryDump::from_memory(memory, *pointer);
            // Phase 2: Use instruction_index directly (accurate even in loops)
            let source_location = debug_info.and_then(|d| d.lookup(instruction_index));

            // Phase 2: Build loop call stack
            let loop_call_stack = if !loop_stack.is_empty() {
                Some(
                    loop_stack
                        .iter()
                        .map(|ctx| crate::error::LoopStackFrame {
                            source_location: ctx.source_location,
                            iteration: ctx.iteration,
                        })
                        .collect(),
                )
            } else {
                None
            };

            return Err(BfError::MemoryOutOfBounds {
                instruction_index: step_count.into(),
                attempted: pointer.get() as isize,
                max: MemorySize::new(self.size.get() - 1),
                memory_dump: Some(dump),
                source_location,
                loop_call_stack,
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
        debug_info: Option<&DebugInfo>,
        instruction_index: usize,        // Phase 2: use this directly
        loop_stack: &[LoopContext],      // Phase 2: active loop contexts
    ) -> Result<()> {
        if pointer.get() == 0 && !allow_negative {
            let dump = MemoryDump::from_memory(memory, *pointer);
            // Phase 2: Use instruction_index directly (accurate even in loops)
            let source_location = debug_info.and_then(|d| d.lookup(instruction_index));

            // Phase 2: Build loop call stack
            let loop_call_stack = if !loop_stack.is_empty() {
                Some(
                    loop_stack
                        .iter()
                        .map(|ctx| crate::error::LoopStackFrame {
                            source_location: ctx.source_location,
                            iteration: ctx.iteration,
                        })
                        .collect(),
                )
            } else {
                None
            };

            return Err(BfError::MemoryOutOfBounds {
                instruction_index: step_count.into(),
                attempted: -1,
                max: MemorySize::new(self.size.get() - 1),
                memory_dump: Some(dump),
                source_location,
                loop_call_stack,
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
        debug_info: Option<&DebugInfo>,
        instruction_index: usize,        // Phase 2: use this directly
        loop_stack: &[LoopContext],      // Phase 2: active loop contexts
    ) -> Result<()> {
        pointer.increment();

        if pointer.get() >= self.max_size.get() {
            let dump = MemoryDump::from_memory(memory, *pointer);
            // Phase 2: Use instruction_index directly (accurate even in loops)
            let source_location = debug_info.and_then(|d| d.lookup(instruction_index));

            // Phase 2: Build loop call stack
            let loop_call_stack = if !loop_stack.is_empty() {
                Some(
                    loop_stack
                        .iter()
                        .map(|ctx| crate::error::LoopStackFrame {
                            source_location: ctx.source_location,
                            iteration: ctx.iteration,
                        })
                        .collect(),
                )
            } else {
                None
            };

            return Err(BfError::MemoryOutOfBounds {
                instruction_index: step_count.into(),
                attempted: pointer.get() as isize,
                max: MemorySize::new(self.max_size.get() - 1),
                memory_dump: Some(dump),
                source_location,
                loop_call_stack,
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
            // Phase 2: Use instruction_index directly
            let source_location = debug_info.and_then(|d| d.lookup(instruction_index));
            warnings.push(RuntimeWarning::MemoryExpanded {
                instruction_index: step_count.into(),
                from_size: MemorySize::new(old_size),
                to_size: MemorySize::new(new_size),
                source_location,
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
        debug_info: Option<&DebugInfo>,
        instruction_index: usize,        // Phase 2: use this directly
        loop_stack: &[LoopContext],      // Phase 2: active loop contexts
    ) -> Result<()> {
        if pointer.get() == 0 && !allow_negative {
            let dump = MemoryDump::from_memory(memory, *pointer);
            // Phase 2: Use instruction_index directly (accurate even in loops)
            let source_location = debug_info.and_then(|d| d.lookup(instruction_index));

            // Phase 2: Build loop call stack
            let loop_call_stack = if !loop_stack.is_empty() {
                Some(
                    loop_stack
                        .iter()
                        .map(|ctx| crate::error::LoopStackFrame {
                            source_location: ctx.source_location,
                            iteration: ctx.iteration,
                        })
                        .collect(),
                )
            } else {
                None
            };

            return Err(BfError::MemoryOutOfBounds {
                instruction_index: step_count.into(),
                attempted: -1,
                max: MemorySize::new(self.max_size.get() - 1),
                memory_dump: Some(dump),
                source_location,
                loop_call_stack,
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
    ///
    /// Phase 2: Accepts instruction_index and loop_stack for accurate error location.
    #[inline]
    pub fn try_increment_pointer(
        &self,
        pointer: &mut MemoryAddress,
        memory: &mut Vec<u8>,
        step_count: StepCount,
        warnings: &mut Vec<RuntimeWarning>,
        debug_info: Option<&DebugInfo>,
        instruction_index: usize,        // Phase 2
        loop_stack: &[LoopContext],      // Phase 2
    ) -> Result<()> {
        match self {
            MemoryModel::Fixed(m) => {
                m.try_increment_pointer(pointer, memory, step_count, warnings, debug_info, instruction_index, loop_stack)
            }
            MemoryModel::Unbounded(m) => {
                m.try_increment_pointer(pointer, memory, step_count, warnings, debug_info, instruction_index, loop_stack)
            }
        }
    }

    /// Handle pointer decrement based on memory model
    ///
    /// Delegates to the specific memory model implementation.
    ///
    /// Phase 2: Accepts instruction_index and loop_stack for accurate error location.
    #[inline]
    pub fn try_decrement_pointer(
        &self,
        pointer: &mut MemoryAddress,
        memory: &[u8],
        allow_negative: bool,
        step_count: StepCount,
        warnings: &mut Vec<RuntimeWarning>,
        debug_info: Option<&DebugInfo>,
        instruction_index: usize,        // Phase 2
        loop_stack: &[LoopContext],      // Phase 2
    ) -> Result<()> {
        match self {
            MemoryModel::Fixed(m) => m.try_decrement_pointer(
                pointer,
                memory,
                allow_negative,
                step_count,
                warnings,
                debug_info,
                instruction_index,
                loop_stack,
            ),
            MemoryModel::Unbounded(m) => m.try_decrement_pointer(
                pointer,
                memory,
                allow_negative,
                step_count,
                warnings,
                debug_info,
                instruction_index,
                loop_stack,
            ),
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
