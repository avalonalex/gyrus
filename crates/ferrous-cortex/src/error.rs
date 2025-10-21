use std::fmt;
use thiserror::Error;

use crate::location::SourceLocation;
use crate::types::{InstructionIndex, MemorySize};

/// Type alias for Results using BfError
pub type Result<T> = std::result::Result<T, BfError>;

/// Extract source context around a location for error messages
pub(crate) fn extract_source_context(source: &str, location: SourceLocation) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let line_idx = location.line.saturating_sub(1);

    let start_line = line_idx.saturating_sub(2);
    let end_line = (line_idx + 3).min(lines.len());

    let mut context = String::new();
    for line_num in start_line..end_line {
        if line_num < lines.len() {
            context.push_str(&format!("{:5} | {}\n", line_num + 1, lines[line_num]));

            // Add caret pointer for the error line
            if line_num == line_idx {
                let spaces = " ".repeat(location.column.saturating_sub(1));
                context.push_str(&format!("      | {}{}\n", spaces, "^"));
            }
        }
    }
    context
}

#[non_exhaustive]
#[derive(Error, Debug)]
pub enum BfError {
    #[error("Unmatched '[' at {location}\n{context}")]
    UnmatchedOpenBracket {
        location: SourceLocation,
        context: String,
    },

    #[error("Unmatched ']' at {location}\n{context}")]
    UnmatchedCloseBracket {
        location: SourceLocation,
        context: String,
    },

    #[error("Found {count} bracket matching errors (see details above)")]
    MultipleBracketErrors { count: usize },

    #[error(
        "Memory pointer out of bounds at instruction {instruction_index}\nAttempted to access cell {attempted}, valid range: 0-{max}"
    )]
    MemoryOutOfBounds {
        instruction_index: InstructionIndex,
        attempted: isize,
        max: MemorySize,
    },

    #[error("IO error: {message}")]
    IoError { message: String },

    #[error("File read error: {0}")]
    FileError(String),

    #[error("Execution timeout: program exceeded {limit_ms}ms execution limit")]
    ExecutionTimeout { limit_ms: u64 },

    #[error("Step limit exceeded: program exceeded {limit} instruction limit")]
    StepLimitExceeded { limit: u64 },

    #[error("Configuration error: {message}")]
    ConfigurationError { message: String },
}

/// Warnings for potentially problematic but valid BrainFuck code
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum BfWarning {
    EmptyLoop {
        location: SourceLocation,
    },
    ExtremeNesting {
        location: SourceLocation,
        depth: usize,
    },
    SuspiciousPattern {
        location: SourceLocation,
        pattern: String,
        reason: String,
    },
    #[allow(dead_code)]
    DeadCode {
        location: SourceLocation,
        reason: String,
    },
}

impl fmt::Display for BfWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BfWarning::EmptyLoop { location } => {
                write!(
                    f,
                    "Warning: Empty loop at {}\n  Empty loops [] do nothing and can be removed",
                    location
                )
            }
            BfWarning::ExtremeNesting { location, depth } => {
                write!(
                    f,
                    "Warning: Extreme loop nesting at {} (depth: {})\n  Deep nesting can impact performance",
                    location, depth
                )
            }
            BfWarning::SuspiciousPattern {
                location,
                pattern,
                reason,
            } => {
                write!(
                    f,
                    "Warning: Suspicious pattern '{}' at {}\n  {}",
                    pattern, location, reason
                )
            }
            BfWarning::DeadCode { location, reason } => {
                write!(
                    f,
                    "Warning: Potentially dead code at {}\n  {}",
                    location, reason
                )
            }
        }
    }
}
