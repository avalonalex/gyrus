//! Memory management behavior models

use crate::debug::DebugInfo;
use crate::error::{BfError, MemoryDump, Result};
use crate::types::{MemoryAddress, MemorySize, StepCount};
use std::fmt;

pub trait MemoryBehavior {
    /// Borrow the cell a cursor refers to, for reading or writing.
    ///
    /// This is where the tape bound is enforced. Moving the cursor is not the
    /// model's business at all -- under the tape contract a cursor may sit
    /// anywhere, including left of cell 0, and only *using* it can fail. Models
    /// that can grow the tape do so here, so growth follows access rather than
    /// mere travel.
    ///
    /// Uses instruction_index for accurate source location lookup even in loops.
    fn cell<'a>(
        &self,
        cursor: MemoryAddress,
        memory: &'a mut Vec<u8>,
        step_count: StepCount,
        debug_info: Option<&DebugInfo>,
        instruction_index: usize,
    ) -> Result<&'a mut u8>;

    /// Get the initial memory size for this model
    fn initial_size(&self) -> MemorySize;
}

/// The error a cursor outside the tape produces when something tries to use it.
///
/// Cold and out of line: every access site calls it on failure only, and
/// inlining a `MemoryDump` construction into the hot path is what stopped
/// `check_limits` inlining once already.
#[cold]
#[inline(never)]
fn out_of_bounds(
    cursor: MemoryAddress,
    memory: &[u8],
    max: MemorySize,
    step_count: StepCount,
    debug_info: Option<&DebugInfo>,
    instruction_index: usize,
    hint: String,
) -> BfError {
    BfError::MemoryOutOfBounds {
        instruction_index: step_count.into(),
        attempted: cursor.get(),
        max,
        memory_dump: Some(Box::new(MemoryDump::from_memory(memory, cursor))),
        source_location: debug_info.and_then(|d| d.lookup(instruction_index)),
        loop_call_stack: None,
        hint,
    }
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
    #[inline]
    fn cell<'a>(
        &self,
        cursor: MemoryAddress,
        memory: &'a mut Vec<u8>,
        step_count: StepCount,
        debug_info: Option<&DebugInfo>,
        instruction_index: usize,
    ) -> Result<&'a mut u8> {
        // One comparison for both ends: a negative cursor cast to usize is
        // enormous and fails the same length test as running off the right.
        match cursor.index(memory.len()) {
            Some(idx) => Ok(&mut memory[idx]),
            // Left of cell 0 is a different failure from running off the right:
            // no tape size makes cell -1 exist, so suggesting a bigger one (or,
            // worse, a smaller one, which the arithmetic below used to do on a
            // default tape) is advice that cannot work.
            None if cursor.get() < 0 => Err(out_of_bounds(
                cursor,
                memory,
                MemorySize::new(self.size.get().saturating_sub(1)),
                step_count,
                debug_info,
                instruction_index,
                "Attempted to use cell {}, left of cell 0. Cells are indexed from 0 \
                 upwards, so no tape size makes this cell exist. Moving the cursor \
                 there is allowed; reading or writing it is not."
                    .replace("{}", &cursor.get().to_string()),
            )),
            None => Err(out_of_bounds(
                cursor,
                memory,
                MemorySize::new(self.size.get().saturating_sub(1)),
                step_count,
                debug_info,
                instruction_index,
                format!(
                    "Attempted to use cell {}, but the tape is fixed at {} cells (0..{}). \
                     Moving the cursor outside the tape is allowed; reading or writing \
                     out there is not. Try --memory-size {} or --memory-model unbounded",
                    cursor.get(),
                    self.size.get(),
                    self.size.get().saturating_sub(1),
                    (cursor.get().max(0) as usize + 1).max(self.size.get() * 2),
                ),
            )),
        }
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
    /// Create a new unbounded memory model.
    ///
    /// Rejects `initial_size > max_size` here rather than at the builder, so no
    /// route into the type can produce one. Code downstream relies on the tape
    /// never being longer than `max_size` -- the optimized interpreter's fused
    /// pointer move treats `memory.len()` as an in-bounds limit on that basis.
    pub fn new(initial_size: MemorySize, max_size: MemorySize) -> Result<Self> {
        if initial_size.get() > max_size.get() {
            return Err(BfError::ConfigurationError {
                message: format!(
                    "initial_size ({}) cannot exceed max_size ({})",
                    initial_size.get(),
                    max_size.get()
                ),
            });
        }
        if initial_size.get() == 0 {
            return Err(BfError::ConfigurationError {
                message: "initial_size must be greater than 0".to_string(),
            });
        }
        if max_size.get() == 0 {
            return Err(BfError::ConfigurationError {
                message: "max_size must be greater than 0".to_string(),
            });
        }
        Ok(Self {
            initial_size,
            max_size,
        })
    }

    /// Get the maximum size for this unbounded memory model
    pub fn max_size(&self) -> MemorySize {
        self.max_size
    }
}

impl MemoryBehavior for UnboundedMemory {
    #[inline]
    fn cell<'a>(
        &self,
        cursor: MemoryAddress,
        memory: &'a mut Vec<u8>,
        step_count: StepCount,
        debug_info: Option<&DebugInfo>,
        instruction_index: usize,
    ) -> Result<&'a mut u8> {
        let pos = cursor.get();
        if pos < 0 {
            return Err(out_of_bounds(
                cursor,
                memory,
                MemorySize::new(self.max_size.get().saturating_sub(1)),
                step_count,
                debug_info,
                instruction_index,
                "Attempted to use a cell left of cell 0. The tape grows rightwards only; \
                 moving the cursor there is allowed, using it is not."
                    .to_string(),
            ));
        }
        let idx = pos as usize;
        if idx >= self.max_size.get() {
            return Err(out_of_bounds(
                cursor,
                memory,
                MemorySize::new(self.max_size.get().saturating_sub(1)),
                step_count,
                debug_info,
                instruction_index,
                format!(
                    "Attempted to use cell {}, beyond the maximum tape size of {}. \
                     This may indicate an infinite loop moving the cursor",
                    idx,
                    self.max_size.get()
                ),
            ));
        }
        // Growth follows use, not travel: a cursor that visits cell 100_000 and
        // comes back without touching anything no longer allocates a tape for it.
        if idx >= memory.len() {
            memory.resize(idx + 1, 0);
        }
        Ok(&mut memory[idx])
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

    /// Borrow the cell a cursor refers to, dispatching on the model.
    ///
    /// Accepts instruction_index for accurate error location even in loops.
    #[inline]
    pub fn cell<'a>(
        &self,
        cursor: MemoryAddress,
        memory: &'a mut Vec<u8>,
        step_count: StepCount,
        debug_info: Option<&DebugInfo>,
        instruction_index: usize,
    ) -> Result<&'a mut u8> {
        match self {
            MemoryModel::Fixed(m) => {
                m.cell(cursor, memory, step_count, debug_info, instruction_index)
            }
            MemoryModel::Unbounded(m) => {
                m.cell(cursor, memory, step_count, debug_info, instruction_index)
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
