//! Memory management behavior models

use crate::debug::DebugInfo;
use crate::error::{BfError, MemoryDump, Result};
use crate::types::{MemoryAddress, MemorySize, StepCount};
use std::fmt;

pub trait MemoryBehavior {
    /// Reach a cell the tape does not currently cover: grow to it, or say why
    /// it cannot be reached.
    ///
    /// **Precondition: `cursor.index(memory.len())` is `None`.** Whether a
    /// cursor is already on the tape is not a question models disagree about --
    /// [`MemoryAddress::index`] answers it once, and `VmState::cell_at` asks it
    /// before calling here. What models differ on is only what to do when the
    /// answer is no, which is why that is all this method decides.
    ///
    /// Under the tape contract, moving the cursor is never the model's business:
    /// a cursor may sit anywhere, including left of cell 0, and only *using* it
    /// can fail. Growth therefore follows access rather than mere travel.
    ///
    /// Uses instruction_index for accurate source location lookup even in loops.
    fn cell_off_tape<'a>(
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
    fn cell_off_tape<'a>(
        &self,
        cursor: MemoryAddress,
        memory: &'a mut Vec<u8>,
        step_count: StepCount,
        debug_info: Option<&DebugInfo>,
        instruction_index: usize,
    ) -> Result<&'a mut u8> {
        // A fixed tape never grows, so being off it is the end of the story.
        let last = MemorySize::new(self.size.get().saturating_sub(1));
        let hint = if cursor.get() < 0 {
            format!(
                "Attempted to use cell {}, left of cell 0. Cells are indexed from 0 \
                 upwards, so no tape size makes this cell exist. Moving the cursor \
                 there is allowed; reading or writing it is not.",
                cursor.get()
            )
        } else {
            format!(
                "Attempted to use cell {}, but the tape is fixed at {} cells (0..{}). \
                 Moving the cursor outside the tape is allowed; reading or writing \
                 out there is not. Try --memory-size {} or --memory-model unbounded",
                cursor.get(),
                self.size.get(),
                last.get(),
                (cursor.get().max(0) as usize + 1).max(self.size.get() * 2),
            )
        };
        Err(out_of_bounds(
            cursor,
            memory,
            last,
            step_count,
            debug_info,
            instruction_index,
            hint,
        ))
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
    fn cell_off_tape<'a>(
        &self,
        cursor: MemoryAddress,
        memory: &'a mut Vec<u8>,
        step_count: StepCount,
        debug_info: Option<&DebugInfo>,
        instruction_index: usize,
    ) -> Result<&'a mut u8> {
        // Growth follows use, not travel: a cursor that visits cell 100_000 and
        // comes back without touching anything does not allocate a tape for it.
        // `index` against max_size answers "could this tape ever hold it?" the
        // same way it answered "does this tape hold it?" for the caller.
        if let Some(idx) = cursor.index(self.max_size.get()) {
            memory.resize(idx + 1, 0);
            // tape-access-ok: just resized the tape to cover idx.
            return Ok(&mut memory[idx]);
        }
        let last = MemorySize::new(self.max_size.get().saturating_sub(1));
        let hint = if cursor.get() < 0 {
            "Attempted to use a cell left of cell 0. The tape grows rightwards only; \
             moving the cursor there is allowed, using it is not."
                .to_string()
        } else {
            format!(
                "Attempted to use cell {}, beyond the maximum tape size of {}. \
                 This may indicate an infinite loop moving the cursor",
                cursor.get(),
                self.max_size.get()
            )
        };
        Err(out_of_bounds(
            cursor,
            memory,
            last,
            step_count,
            debug_info,
            instruction_index,
            hint,
        ))
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

    /// Reach a cell off the tape, dispatching on the model.
    /// See [`MemoryBehavior::cell_off_tape`] for the precondition.
    ///
    /// Accepts instruction_index for accurate error location even in loops.
    #[inline]
    pub fn cell_off_tape<'a>(
        &self,
        cursor: MemoryAddress,
        memory: &'a mut Vec<u8>,
        step_count: StepCount,
        debug_info: Option<&DebugInfo>,
        instruction_index: usize,
    ) -> Result<&'a mut u8> {
        match self {
            MemoryModel::Fixed(m) => {
                m.cell_off_tape(cursor, memory, step_count, debug_info, instruction_index)
            }
            MemoryModel::Unbounded(m) => {
                m.cell_off_tape(cursor, memory, step_count, debug_info, instruction_index)
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
