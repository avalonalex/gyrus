//! Debug symbol support for tracking source locations during execution
//!
//! This module provides infrastructure for mapping runtime instruction execution
//! back to source code locations. This enables meaningful runtime warnings that
//! point to specific lines and columns in the original BrainFuck source.
//!
//! # Design
//!
//! Debug symbols use a **flat index approach**: during parsing, we assign sequential
//! indices to instructions in execution order (depth-first traversal). This matches
//! the interpreter's StepCount, allowing direct lookup at runtime.
//!
//! For example, given this code:
//! ```brainfuck
//! +[>+<]-.
//! ```
//!
//! The flat indices in execution order would be:
//! - `0` → first `+`
//! - `1` → `[` loop start
//! - `2` → `>` inside loop (first instruction in loop body)
//! - `3` → `+` inside loop
//! - `4` → `<` inside loop
//! (back to index 1 if cell != 0, else continue)
//! - `5` → `-` after loop
//! - `6` → `.` at the end
//!
//! At runtime, the interpreter's StepCount increments as 0, 1, 2, 3, 4, 5, 6...
//! which matches our flat indices, enabling O(1) lookup of source locations.

use crate::location::SourceLocation;
use std::collections::HashMap;

/// Debug information mapping instruction indices to source locations
///
/// This is collected during parsing and used during execution to provide
/// meaningful error messages and warnings with source context.
///
/// The key design is that instruction indices match the interpreter's StepCount,
/// allowing direct O(1) lookup at runtime.
#[derive(Debug, Clone, Default)]
pub struct DebugInfo {
    /// Original source code (used for displaying context)
    source: String,
    /// Map from step index (execution order) to source location
    locations: HashMap<usize, SourceLocation>,
}

impl DebugInfo {
    /// Create a new empty DebugInfo
    pub fn new() -> Self {
        Self {
            source: String::new(),
            locations: HashMap::new(),
        }
    }

    /// Create DebugInfo with source code
    pub fn with_source(source: String) -> Self {
        Self {
            source,
            locations: HashMap::new(),
        }
    }

    /// Get the original source code
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Record the source location for a step index
    pub fn record(&mut self, step_index: usize, location: SourceLocation) {
        self.locations.insert(step_index, location);
    }

    /// Look up the source location for a step index
    pub fn lookup(&self, step_index: usize) -> Option<SourceLocation> {
        self.locations.get(&step_index).copied()
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

        debug_info.record(0, loc);
        debug_info.record(2, SourceLocation::new(2, 3, 10));

        assert_eq!(debug_info.lookup(0), Some(loc));
        assert_eq!(debug_info.lookup(2), Some(SourceLocation::new(2, 3, 10)));
        assert_eq!(debug_info.lookup(999), None);
    }

    #[test]
    fn test_debug_info_len() {
        let mut debug_info = DebugInfo::new();
        assert_eq!(debug_info.len(), 0);
        assert!(debug_info.is_empty());

        debug_info.record(0, SourceLocation::start());
        assert_eq!(debug_info.len(), 1);
        assert!(!debug_info.is_empty());
    }

    #[test]
    fn test_debug_info_with_source() {
        let source = "+++[>+<]-.";
        let debug_info = DebugInfo::with_source(source.to_string());
        assert_eq!(debug_info.source(), source);
    }
}
