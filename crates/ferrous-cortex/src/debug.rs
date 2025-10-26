//! Debug symbol support for tracking source locations during execution
//!
//! This module provides infrastructure for mapping runtime instruction execution
//! back to source code locations. This enables meaningful runtime warnings that
//! point to specific lines and columns in the original BrainFuck source.
//!
//! # Design
//!
//! Debug symbols are tracked using "instruction paths" - a sequence of indices
//! that uniquely identify each instruction in the nested instruction tree.
//!
//! For example, given this code:
//! ```brainfuck
//! +[>+<]-.
//! ```
//!
//! The instruction paths would be:
//! - `[0]` → first `+`
//! - `[1]` → the loop
//! - `[1, 0]` → `>` inside the loop
//! - `[1, 1]` → `+` inside the loop
//! - `[1, 2]` → `<` inside the loop
//! - `[2]` → `-` after the loop
//! - `[3]` → `.` at the end
//!
//! This path-based approach handles nested loops correctly and doesn't require
//! modifying the `Instruction` enum.

use crate::location::SourceLocation;
use std::collections::HashMap;

/// Instruction path - uniquely identifies an instruction in the nested tree
///
/// Example: `[0, 2, 1]` means:
/// - 0th instruction in the root
/// - 2nd instruction in that loop
/// - 1st instruction in that nested loop
pub type InstructionPath = Vec<usize>;

/// Debug information mapping instruction paths to source locations
///
/// This is collected during parsing and used during execution to provide
/// meaningful error messages and warnings with source context.
#[derive(Debug, Clone, Default)]
pub struct DebugInfo {
    locations: HashMap<InstructionPath, SourceLocation>,
}

impl DebugInfo {
    /// Create a new empty DebugInfo
    pub fn new() -> Self {
        Self {
            locations: HashMap::new(),
        }
    }

    /// Record the source location for an instruction path
    pub fn record(&mut self, path: InstructionPath, location: SourceLocation) {
        self.locations.insert(path, location);
    }

    /// Look up the source location for an instruction path
    pub fn lookup(&self, path: &[usize]) -> Option<SourceLocation> {
        self.locations.get(path).copied()
    }

    /// Get the number of recorded locations
    pub fn len(&self) -> usize {
        self.locations.len()
    }

    /// Check if the debug info is empty
    pub fn is_empty(&self) -> bool {
        self.locations.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_info_basic() {
        let mut debug_info = DebugInfo::new();
        let loc = SourceLocation::new(1, 5, 4);

        debug_info.record(vec![0], loc);
        debug_info.record(vec![1, 2], SourceLocation::new(2, 3, 10));

        assert_eq!(debug_info.lookup(&[0]), Some(loc));
        assert_eq!(
            debug_info.lookup(&[1, 2]),
            Some(SourceLocation::new(2, 3, 10))
        );
        assert_eq!(debug_info.lookup(&[999]), None);
    }

    #[test]
    fn test_debug_info_len() {
        let mut debug_info = DebugInfo::new();
        assert_eq!(debug_info.len(), 0);
        assert!(debug_info.is_empty());

        debug_info.record(vec![0], SourceLocation::start());
        assert_eq!(debug_info.len(), 1);
        assert!(!debug_info.is_empty());
    }
}
