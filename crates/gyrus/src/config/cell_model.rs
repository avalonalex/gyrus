//! Cell arithmetic behavior models

use crate::debug::DebugInfo;
use crate::error::Result;
use crate::types::StepCount;
use std::fmt;

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
    fn try_increment(
        &self,
        value: &mut u8,
        step_count: StepCount,
        debug_info: Option<&DebugInfo>,
    ) -> Result<()>;

    /// Try to decrement the cell value by 1
    ///
    /// Returns an error if the operation would violate the cell model's constraints
    /// (e.g., underflow with checked arithmetic).
    fn try_decrement(
        &self,
        value: &mut u8,
        step_count: StepCount,
        debug_info: Option<&DebugInfo>,
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
    fn try_increment(
        &self,
        value: &mut u8,
        _step_count: StepCount,
        _debug_info: Option<&DebugInfo>,
    ) -> Result<()> {
        *value = value.wrapping_add(1);
        Ok(())
    }

    fn try_decrement(
        &self,
        value: &mut u8,
        _step_count: StepCount,
        _debug_info: Option<&DebugInfo>,
    ) -> Result<()> {
        *value = value.wrapping_sub(1);
        Ok(())
    }
}

impl fmt::Display for U8WrappingCells {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "U8 Wrapping Cells (255+1=0, 0-1=255)")
    }
}

/// U8 checked cell model (debugging mode, strict)
///
/// Returns an error on overflow or underflow instead of wrapping:
/// - Increment overflow: `255 + 1` → ERROR
/// - Decrement underflow: `0 - 1` → ERROR
///
/// This mode is useful for debugging programs to catch unexpected wraps.
/// Programs like `[+]` will error on overflow instead of terminating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct U8CheckedCells;

impl CellBehavior for U8CheckedCells {
    fn try_increment(
        &self,
        value: &mut u8,
        step_count: StepCount,
        debug_info: Option<&DebugInfo>,
    ) -> Result<()> {
        let old_value = *value;
        *value = value.checked_add(1).ok_or_else(|| {
            use crate::error::BfError;
            // step_count is incremented BEFORE instruction execution, so subtract 1 to get actual instruction index
            let instruction_idx = step_count.get().saturating_sub(1) as usize;
            let source_location = debug_info.and_then(|d| d.lookup(instruction_idx));
            BfError::CellOverflow {
                instruction_index: step_count.into(),
                current_value: old_value,
                source_location,
                hint: "Cell value reached maximum (255) and cannot be incremented further with checked arithmetic. \
                       Use --cell-model wrapping to allow overflow, or modify your program.".to_string(),
            }
        })?;
        Ok(())
    }

    fn try_decrement(
        &self,
        value: &mut u8,
        step_count: StepCount,
        debug_info: Option<&DebugInfo>,
    ) -> Result<()> {
        let old_value = *value;
        *value = value.checked_sub(1).ok_or_else(|| {
            use crate::error::BfError;
            // step_count is incremented BEFORE instruction execution, so subtract 1 to get actual instruction index
            let instruction_idx = step_count.get().saturating_sub(1) as usize;
            let source_location = debug_info.and_then(|d| d.lookup(instruction_idx));
            BfError::CellUnderflow {
                instruction_index: step_count.into(),
                current_value: old_value,
                source_location,
                hint: "Cell value is at minimum (0) and cannot be decremented further with checked arithmetic. \
                       Use --cell-model wrapping to allow underflow, or modify your program.".to_string(),
            }
        })?;
        Ok(())
    }
}

impl fmt::Display for U8CheckedCells {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "U8 Checked Cells (255+1=error, 0-1=error, strict overflow detection)"
        )
    }
}

/// Cell model enumeration
///
/// This enum wraps different cell behavior implementations, allowing
/// dynamic dispatch at runtime based on CLI flags.
///
/// **Default**: `U8Wrapping` (most compatible)
///
/// # Variants
///
/// - **U8Wrapping**: Traditional BF behavior (255+1=0, 0-1=255)
/// - **U8Checked**: Strict mode (overflow/underflow are errors)
///
/// # Usage
///
/// ```rust
/// use gyrus::{CellModel, U8WrappingCells, U8CheckedCells};
///
/// let wrapping = CellModel::U8Wrapping(U8WrappingCells);
/// let checked = CellModel::U8Checked(U8CheckedCells);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellModel {
    /// U8 cells with wrapping overflow/underflow (255+1=0, 0-1=255)
    U8Wrapping(U8WrappingCells),

    /// U8 cells with checked overflow/underflow (errors on overflow/underflow)
    U8Checked(U8CheckedCells),
}

impl Default for CellModel {
    fn default() -> Self {
        Self::U8Wrapping(U8WrappingCells)
    }
}

impl CellModel {
    /// Get the cell behavior trait object for this model
    ///
    /// Returns a reference to the underlying `CellBehavior` implementation,
    /// allowing dynamic dispatch to the appropriate arithmetic logic.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use gyrus::{CellModel, U8WrappingCells};
    ///
    /// let model = CellModel::U8Wrapping(U8WrappingCells);
    /// let behavior = model.behavior();
    /// // Now you can call try_increment(), try_decrement(), etc.
    /// ```
    pub fn behavior(&self) -> &dyn CellBehavior {
        match self {
            CellModel::U8Wrapping(cells) => cells,
            CellModel::U8Checked(cells) => cells,
        }
    }
}

impl fmt::Display for CellModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CellModel::U8Wrapping(cells) => write!(f, "{}", cells),
            CellModel::U8Checked(cells) => write!(f, "{}", cells),
        }
    }
}
