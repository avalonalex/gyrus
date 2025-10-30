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
//! - `4` → `<` inside loop (back to index 1 if cell != 0, else continue)
//! - `5` → `-` after loop
//! - `6` → `.` at the end
//!
//! At runtime, the interpreter's StepCount increments as 0, 1, 2, 3, 4, 5, 6...
//! which matches our flat indices, enabling O(1) lookup of source locations.
//!
//! # Phase 2: Loop Tracking
//!
//! Phase 2 adds loop metadata to enable source location tracking even after many
//! loop iterations. The key insight is tracking `instruction_index` (which cycles
//! through loop bodies) separately from `step_count` (which always increments).

use crate::location::SourceLocation;
use std::collections::HashMap;

/// Metadata about a loop collected during parsing
///
/// This information enables the interpreter to track which instruction is executing
/// even after many loop iterations, by maintaining an `instruction_index` that cycles
/// through the loop body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopMetadata {
    /// Flat index of the '[' instruction that starts this loop
    pub loop_start_index: usize,

    /// Flat index of the first instruction inside the loop body
    /// This is `loop_start_index + 1`
    pub body_start_index: usize,

    /// Number of instructions in the loop body (excluding the '[' and ']')
    pub body_size: usize,

    /// Flat index of the parent loop's '[', if this loop is nested
    /// None if this is a top-level loop
    pub parent_loop: Option<usize>,

    /// Source location of the '[' character (for call stack display)
    pub source_location: SourceLocation,
}

/// Runtime context for an active loop (Phase 2)
///
/// This tracks information about a loop that is currently executing,
/// enabling the interpreter to maintain accurate instruction_index values
/// even after many iterations of nested loops.
///
/// This struct is passed to error creation sites to provide loop call stack
/// information for better error messages.
#[derive(Debug, Clone)]
pub struct LoopContext {
    /// Flat index of the '[' instruction that starts this loop
    pub loop_instruction_index: usize,

    /// Flat index of the first instruction inside the loop body
    pub body_start_index: usize,

    /// Number of instructions in the loop body
    pub body_size: usize,

    /// Current iteration number (1-based, starts at 1 for first iteration)
    pub iteration: u64,

    /// Source location of the '[' character (for call stack display)
    pub source_location: SourceLocation,
}

/// Debug information mapping instruction indices to source locations
///
/// This is collected during parsing and used during execution to provide
/// meaningful error messages and warnings with source context.
///
/// The key design is that instruction indices match the interpreter's StepCount,
/// allowing direct O(1) lookup at runtime.
///
/// # Phase 2 Enhancement
///
/// In Phase 2, we also track loop metadata to enable source location tracking
/// even after many loop iterations. The `loop_metadata` map stores information
/// about loop boundaries and nesting.
#[derive(Debug, Clone, Default)]
pub struct DebugInfo {
    /// Original source code (used for displaying context)
    source: String,
    /// Map from step index (execution order) to source location
    locations: HashMap<usize, SourceLocation>,
    /// Map from loop start index to loop metadata (Phase 2)
    loop_metadata: HashMap<usize, LoopMetadata>,
}

impl DebugInfo {
    /// Create a new empty DebugInfo
    pub fn new() -> Self {
        Self {
            source: String::new(),
            locations: HashMap::new(),
            loop_metadata: HashMap::new(),
        }
    }

    /// Create DebugInfo with source code
    pub fn with_source(source: String) -> Self {
        Self {
            source,
            locations: HashMap::new(),
            loop_metadata: HashMap::new(),
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

    /// Record loop metadata for a loop (Phase 2)
    ///
    /// This should be called during parsing when a loop is encountered,
    /// storing information about the loop's boundaries and nesting.
    pub fn record_loop_metadata(&mut self, metadata: LoopMetadata) {
        self.loop_metadata
            .insert(metadata.loop_start_index, metadata);
    }

    /// Look up loop metadata by loop start index (Phase 2)
    ///
    /// Returns metadata for the loop starting at the given instruction index,
    /// or None if no loop starts at that index.
    pub fn get_loop_metadata(&self, loop_start_index: usize) -> Option<&LoopMetadata> {
        self.loop_metadata.get(&loop_start_index)
    }

    /// Get the number of recorded loops (Phase 2)
    pub fn loop_count(&self) -> usize {
        self.loop_metadata.len()
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

    // Phase 2 tests

    #[test]
    fn test_loop_metadata_simple() {
        let mut debug_info = DebugInfo::new();

        // Simple loop: +[>+<-] at indices 0,1,2,3,4,5
        let metadata = LoopMetadata {
            loop_start_index: 1,
            body_start_index: 2,
            body_size: 4,
            parent_loop: None,
            source_location: SourceLocation::new(1, 2, 1),
        };

        debug_info.record_loop_metadata(metadata.clone());

        assert_eq!(debug_info.loop_count(), 1);
        assert_eq!(debug_info.get_loop_metadata(1), Some(&metadata));
        assert_eq!(debug_info.get_loop_metadata(999), None);
    }

    #[test]
    fn test_loop_metadata_nested() {
        let mut debug_info = DebugInfo::new();

        // Outer loop: +[>+[<.>-]<-] at indices 0,1,2,3,4,5,6,7,8,9
        let outer = LoopMetadata {
            loop_start_index: 1,
            body_start_index: 2,
            body_size: 8,
            parent_loop: None,
            source_location: SourceLocation::new(1, 2, 1),
        };

        // Inner loop: [<.>-] at indices 4,5,6,7
        let inner = LoopMetadata {
            loop_start_index: 4,
            body_start_index: 5,
            body_size: 3,
            parent_loop: Some(1),
            source_location: SourceLocation::new(1, 5, 4),
        };

        debug_info.record_loop_metadata(outer.clone());
        debug_info.record_loop_metadata(inner.clone());

        assert_eq!(debug_info.loop_count(), 2);
        assert_eq!(debug_info.get_loop_metadata(1), Some(&outer));
        assert_eq!(debug_info.get_loop_metadata(4), Some(&inner));

        // Verify parent relationship
        let inner_meta = debug_info.get_loop_metadata(4).unwrap();
        assert_eq!(inner_meta.parent_loop, Some(1));
    }

    #[test]
    fn test_loop_metadata_triple_nested() {
        let mut debug_info = DebugInfo::new();

        // Triple nested: +++[>+[>+[>+<-]<-]<-]
        let outer = LoopMetadata {
            loop_start_index: 3,
            body_start_index: 4,
            body_size: 14,
            parent_loop: None,
            source_location: SourceLocation::new(1, 4, 3),
        };

        let middle = LoopMetadata {
            loop_start_index: 6,
            body_start_index: 7,
            body_size: 9,
            parent_loop: Some(3),
            source_location: SourceLocation::new(1, 7, 6),
        };

        let inner = LoopMetadata {
            loop_start_index: 9,
            body_start_index: 10,
            body_size: 4,
            parent_loop: Some(6),
            source_location: SourceLocation::new(1, 10, 9),
        };

        debug_info.record_loop_metadata(outer);
        debug_info.record_loop_metadata(middle);
        debug_info.record_loop_metadata(inner);

        assert_eq!(debug_info.loop_count(), 3);

        // Verify parent chain
        let inner_meta = debug_info.get_loop_metadata(9).unwrap();
        assert_eq!(inner_meta.parent_loop, Some(6));

        let middle_meta = debug_info.get_loop_metadata(6).unwrap();
        assert_eq!(middle_meta.parent_loop, Some(3));

        let outer_meta = debug_info.get_loop_metadata(3).unwrap();
        assert_eq!(outer_meta.parent_loop, None);
    }
}
