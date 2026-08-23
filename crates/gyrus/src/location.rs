//! Source code location tracking.
//!
//! This module provides [`SourceLocation`] for tracking line/column positions
//! in source code for error reporting and debugging. Used by the parser to
//! track positions during parsing and by the interpreter for runtime error
//! messages.
//!
//! # Examples
//!
//! ```rust
//! use gyrus::SourceLocation;
//!
//! let location = SourceLocation::start();
//! assert_eq!(location.line, 1);
//! assert_eq!(location.column, 1);
//! println!("{}", location);  // "line 1, column 1"
//! ```

use std::fmt;

/// Represents a location in the source code
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLocation {
    pub line: usize,
    pub column: usize,
    pub offset: usize,
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}, column {}", self.line, self.column)
    }
}

impl SourceLocation {
    /// Create a new SourceLocation (primarily for testing or external use)
    #[inline]
    #[allow(dead_code)]
    pub fn new(line: usize, column: usize, offset: usize) -> Self {
        Self {
            line,
            column,
            offset,
        }
    }

    #[inline]
    pub fn start() -> Self {
        Self {
            line: 1,
            column: 1,
            offset: 0,
        }
    }
}
