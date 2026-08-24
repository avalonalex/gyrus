//! Type-safe wrappers for interpreter primitives.
//!
//! This module provides newtype wrappers ([`MemoryAddress`], [`MemorySize`],
//! [`StepCount`], [`InstructionIndex`]) that prevent mixing up logically
//! distinct concepts at compile time.
//!
//! # Type Safety Benefits
//!
//! Using newtype wrappers instead of raw `usize` values prevents bugs like:
//! - Passing a memory size where an address is expected
//! - Using a step count as an instruction index
//! - Confusing different numeric concepts
//!
//! # Examples
//!
//! ```rust
//! use gyrus::{MemoryAddress, MemorySize};
//!
//! let address = MemoryAddress::new(100);
//! let size = MemorySize::new(30000);
//!
//! // This would be a compile error (type mismatch):
//! // let address: MemoryAddress = size;
//! ```

use std::fmt;
use std::ops::AddAssign;

/// Cursor position on the BrainFuck tape.
///
/// Signed, because the tape contract is about *access*, not position: a program
/// may move the cursor off either end of the tape, and only reading or writing
/// out there is an error. A cursor left of cell 0 is representable, and stays
/// wrong only until something tries to use it.
///
/// Use [`Self::index`] to turn a cursor into a tape index; that is the point at
/// which being outside the tape becomes a problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct MemoryAddress(pub isize);

impl MemoryAddress {
    /// Create a new cursor position
    #[inline]
    pub const fn new(value: isize) -> Self {
        Self(value)
    }

    /// Get the inner value
    #[inline]
    pub const fn get(self) -> isize {
        self.0
    }

    /// The tape index this cursor refers to, if it is on the tape.
    ///
    /// One comparison covers both ends: a negative cursor cast to `usize` is
    /// enormous, so it fails the same `< len` test that catches running off the
    /// right. This is the single definition of "is this cursor on the tape";
    /// `VmState::cell_at` and `FixedMemory::cell` both go through it rather
    /// than open-coding the cast.
    #[inline]
    pub const fn index(self, len: usize) -> Option<usize> {
        let idx = self.0 as usize;
        if idx < len { Some(idx) } else { None }
    }

    /// Move the cursor `n` cells right. Never fails: leaving the tape is legal.
    #[inline]
    pub fn advance(&mut self, n: isize) {
        self.0 = self.0.saturating_add(n);
    }

    /// Increment the cursor
    #[inline]
    pub fn increment(&mut self) {
        self.0 = self.0.saturating_add(1);
    }

    /// Decrement the cursor. Never fails; cell -1 is a position, not an error.
    #[inline]
    pub fn decrement(&mut self) {
        self.0 = self.0.saturating_sub(1);
    }
}

impl fmt::Display for MemoryAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<isize> for MemoryAddress {
    fn from(value: isize) -> Self {
        Self(value)
    }
}

impl AddAssign<isize> for MemoryAddress {
    fn add_assign(&mut self, rhs: isize) {
        self.0 = self.0.saturating_add(rhs);
    }
}

/// Memory size (number of cells in memory array)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct MemorySize(pub usize);

impl MemorySize {
    /// Create a new memory size
    #[inline]
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    /// Get the inner value
    #[inline]
    pub const fn get(self) -> usize {
        self.0
    }
}

impl fmt::Display for MemorySize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<usize> for MemorySize {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

/// Number of execution steps/instructions executed
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct StepCount(pub u64);

impl StepCount {
    /// Create a new step count
    #[inline]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Get the inner value
    #[inline]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Increment the step count
    #[inline]
    pub fn increment(&mut self) {
        self.0 += 1;
    }
}

impl fmt::Display for StepCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for StepCount {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl AddAssign<u64> for StepCount {
    fn add_assign(&mut self, rhs: u64) {
        self.0 += rhs;
    }
}

/// Index/position of an instruction in the instruction stream
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstructionIndex(pub usize);

impl InstructionIndex {
    /// Create a new instruction index
    #[inline]
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    /// Get the inner value
    #[inline]
    pub const fn get(self) -> usize {
        self.0
    }
}

impl fmt::Display for InstructionIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<usize> for InstructionIndex {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl From<StepCount> for InstructionIndex {
    fn from(count: StepCount) -> Self {
        Self(count.0 as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_address_operations() {
        let mut addr = MemoryAddress::new(0);
        assert_eq!(addr.get(), 0);

        addr.increment();
        assert_eq!(addr.get(), 1);

        addr.decrement();
        assert_eq!(addr.get(), 0);

        // A cursor may go left of cell 0. It is a position, not an error --
        // only using it is. See the tape contract on `MemoryAddress`.
        addr.decrement();
        assert_eq!(addr.get(), -1);
        assert_eq!(addr.index(30), None, "off-tape cursor has no index");

        addr.advance(4);
        assert_eq!(addr.get(), 3);
        assert_eq!(addr.index(30), Some(3));
        assert_eq!(addr.index(3), None, "past the end has no index either");
    }

    #[test]
    fn test_memory_size_creation() {
        let size = MemorySize::new(30000);
        assert_eq!(size.get(), 30000);
        assert_eq!(size.to_string(), "30000");
    }

    #[test]
    fn test_step_count_operations() {
        let mut count = StepCount::new(0);
        assert_eq!(count.get(), 0);

        count.increment();
        assert_eq!(count.get(), 1);

        count += 5;
        assert_eq!(count.get(), 6);
    }

    #[test]
    fn test_type_safety() {
        // These are different types - can't be confused
        let addr = MemoryAddress::new(100);
        let size = MemorySize::new(100);

        // They're equal in value but different types
        assert_eq!(addr.get(), size.get() as isize);
        // This won't compile: assert_eq!(addr, size);
    }

    #[test]
    fn test_instruction_index_conversion() {
        let step = StepCount::new(42);
        let index: InstructionIndex = step.into();
        assert_eq!(index.get(), 42);
    }
}
