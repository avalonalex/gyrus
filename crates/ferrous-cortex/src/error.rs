//! Error types and error handling utilities.
//!
//! This module defines [`BfError`] for runtime errors, [`BfWarning`] for validation
//! warnings, and [`RuntimeWarning`] for warnings collected during execution.
//! Errors include rich context like memory dumps, actionable hints, source location
//! tracking, and syntax-highlighted error messages with ANSI color support.
//!
//! # Error Reporting Features
//!
//! - **Syntax highlighting**: Error messages include syntax-highlighted source context
//! - **Source location tracking**: Precise line/column information for all errors
//! - **Memory dumps**: Show memory state when errors occur
//! - **Actionable hints**: Suggestions for fixing common problems
//! - **Error chaining**: Track causal relationships between errors
//!
//! # Examples
//!
//! ```rust
//! use ferrous_cortex::{parse, BfError};
//!
//! // Parse errors include source location and context
//! match parse("++[>++") {
//!     Ok(instructions) => println!("Parsed successfully"),
//!     Err(e) => {
//!         eprintln!("Error: {}", e);
//!         // Errors automatically include syntax-highlighted source context
//!     }
//! }
//! ```

use std::fmt;
use std::io::Write;
use std::path::PathBuf;
use thiserror::Error;

use crate::location::SourceLocation;
use crate::types::{InstructionIndex, MemoryAddress, MemorySize, StepCount};

/// Type alias for Results using BfError
pub type Result<T> = std::result::Result<T, BfError>;

/// Memory state snapshot for error reporting
#[derive(Debug, Clone)]
pub struct MemoryDump {
    /// Current pointer position
    pub pointer: MemoryAddress,

    /// Nearby cells (address, value) around the pointer
    pub nearby_cells: Vec<(usize, u8)>,

    /// Total number of non-zero cells
    pub non_zero_count: usize,
}

impl MemoryDump {
    /// Create a memory dump showing cells around the pointer
    pub fn from_memory(memory: &[u8], pointer: MemoryAddress) -> Self {
        let ptr = pointer.get();
        let range_start = ptr.saturating_sub(3);
        let range_end = (ptr + 4).min(memory.len());

        let nearby_cells: Vec<(usize, u8)> = (range_start..range_end)
            .map(|addr| (addr, memory[addr]))
            .collect();

        let non_zero_count = memory.iter().filter(|&&b| b != 0).count();

        Self {
            pointer,
            nearby_cells,
            non_zero_count,
        }
    }
}

impl fmt::Display for MemoryDump {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Memory state:")?;
        writeln!(f, "  Pointer: {}", self.pointer)?;
        writeln!(f, "  Non-zero cells: {}", self.non_zero_count)?;
        writeln!(f, "  Nearby cells:")?;
        for (addr, value) in &self.nearby_cells {
            let marker = if *addr == self.pointer.get() {
                "→"
            } else {
                " "
            };
            writeln!(f, "    {} [{:5}] = {}", marker, addr, value)?;
        }
        Ok(())
    }
}

/// Calculate the line range for context extraction (2 lines before, 2 lines after)
#[inline]
fn context_line_range(line_idx: usize, total_lines: usize) -> (usize, usize) {
    let start_line = line_idx.saturating_sub(2);
    let end_line = (line_idx + 3).min(total_lines);
    (start_line, end_line)
}

/// Extract source context around a location for error messages
pub(crate) fn extract_source_context(source: &str, location: SourceLocation) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let line_idx = location.line.saturating_sub(1);
    let (start_line, end_line) = context_line_range(line_idx, lines.len());

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

/// Extract source context with syntax highlighting
pub(crate) fn extract_source_context_highlighted(source: &str, location: SourceLocation) -> String {
    use crate::syntax::SyntaxHighlighter;

    // Highlight the entire source
    let highlighter = SyntaxHighlighter::new().show_line_numbers(true);
    let highlighted = highlighter.highlight(source);

    let line_idx = location.line.saturating_sub(1);
    let (start_line, end_line) = context_line_range(line_idx, highlighted.lines().len());

    // Build the context with the caret inserted right after the error line
    let mut buffer = Vec::new();

    // Write lines up to and including the error line
    highlighted
        .write_ansi_range(&mut buffer, start_line, line_idx + 1)
        .expect("write to Vec should not fail");

    // Add caret pointer right after the error line
    // Line numbers are formatted as "   N │ " (4 digits + " │ " = 8 chars)
    // Point at the instruction before the error (the last successful one)
    // Column is 1-indexed: (column-2) for prev char + 8 for prefix = column + 6
    let spaces = " ".repeat(location.column + 6);
    writeln!(buffer, "{}\x1b[1;31m^\x1b[0m", spaces).expect("write to Vec should not fail");

    // Write remaining lines after the error line
    if line_idx + 1 < end_line {
        highlighted
            .write_ansi_range(&mut buffer, line_idx + 1, end_line)
            .expect("write to Vec should not fail");
    }

    String::from_utf8(buffer).expect("UTF-8 conversion should not fail")
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

    #[error("Memory pointer out of bounds at instruction {instruction_index}")]
    MemoryOutOfBounds {
        instruction_index: InstructionIndex,
        attempted: isize,
        max: MemorySize,
        memory_dump: Option<MemoryDump>,
        source_location: Option<SourceLocation>,
        hint: String,
    },

    #[error("IO error during {operation}")]
    IoError {
        operation: String,
        instruction_index: Option<InstructionIndex>,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to read source file '{}'", path.display())]
    FileError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
        hint: String,
    },

    #[error("Execution timeout: program exceeded {limit_ms}ms execution limit")]
    ExecutionTimeout {
        limit_ms: u64,
        actual_steps: Option<StepCount>,
        hint: String,
    },

    #[error("Step limit exceeded: program exceeded {limit} instruction limit")]
    StepLimitExceeded {
        limit: u64,
        actual_steps: StepCount,
        hint: String,
    },

    #[error("Configuration error: {message}")]
    ConfigurationError { message: String },

    #[error(
        "Cell overflow at instruction {instruction_index}: attempted to increment cell with value {current_value}"
    )]
    CellOverflow {
        instruction_index: InstructionIndex,
        current_value: u8,
        source_location: Option<SourceLocation>,
        hint: String,
    },

    #[error(
        "Cell underflow at instruction {instruction_index}: attempted to decrement cell with value {current_value}"
    )]
    CellUnderflow {
        instruction_index: InstructionIndex,
        current_value: u8,
        source_location: Option<SourceLocation>,
        hint: String,
    },
}

impl BfError {
    /// Get a hint message for this error, if available
    pub fn hint(&self) -> Option<&str> {
        match self {
            BfError::MemoryOutOfBounds { hint, .. } => Some(hint),
            BfError::FileError { hint, .. } => Some(hint),
            BfError::ExecutionTimeout { hint, .. } => Some(hint),
            BfError::StepLimitExceeded { hint, .. } => Some(hint),
            BfError::CellOverflow { hint, .. } => Some(hint),
            BfError::CellUnderflow { hint, .. } => Some(hint),
            _ => None,
        }
    }

    /// Get memory dump if available
    pub fn memory_dump(&self) -> Option<&MemoryDump> {
        match self {
            BfError::MemoryOutOfBounds { memory_dump, .. } => memory_dump.as_ref(),
            _ => None,
        }
    }

    /// Format error with full context
    pub fn format_detailed(&self) -> String {
        let mut output = format!("Error: {}", self);

        // Add hint if available
        if let Some(hint) = self.hint() {
            output.push_str(&format!("\n\nHint: {}", hint));
        }

        // Add memory dump if available
        if let Some(dump) = self.memory_dump() {
            output.push_str(&format!("\n\n{}", dump));
        }

        // Add source chain if available
        if let Some(source) = std::error::Error::source(self) {
            output.push_str(&format!("\n\nCaused by:\n    {}", source));
        }

        output
    }

    /// Format error with source context if available
    pub fn format_with_source(&self, source: &str) -> String {
        match self {
            BfError::MemoryOutOfBounds {
                source_location,
                attempted,
                max,
                memory_dump,
                hint,
                ..
            } => {
                let mut output = String::new();
                if let Some(loc) = source_location {
                    output.push_str(&format!(
                        "Error: Memory pointer out of bounds\n\nAt line {}, column {}:\n",
                        loc.line, loc.column
                    ));
                    output.push_str(&extract_source_context_highlighted(source, *loc));
                    output.push_str(&format!(
                        "Attempted to access cell {}, but memory size is {} cells.\n",
                        attempted,
                        max.get() + 1
                    ));
                } else {
                    output.push_str("Error: Memory pointer out of bounds\n");
                    output.push_str(&format!(
                        "Attempted to access cell {}, but memory size is {} cells.\n",
                        attempted,
                        max.get() + 1
                    ));
                }
                output.push_str(&format!("\nHint: {}", hint));
                if let Some(dump) = memory_dump {
                    output.push_str(&format!("\n\n{}", dump));
                }
                output
            }
            BfError::CellOverflow {
                source_location,
                current_value,
                hint,
                ..
            } => {
                let mut output = String::new();
                if let Some(loc) = source_location {
                    output.push_str(&format!(
                        "Error: Cell overflow\n\nAt line {}, column {}:\n",
                        loc.line, loc.column
                    ));
                    output.push_str(&extract_source_context_highlighted(source, *loc));
                    output.push_str(&format!(
                        "Attempted to increment cell with value {}.\n",
                        current_value
                    ));
                } else {
                    output.push_str("Error: Cell overflow\n");
                    output.push_str(&format!(
                        "Attempted to increment cell with value {}.\n",
                        current_value
                    ));
                }
                output.push_str(&format!("\nHint: {}", hint));
                output
            }
            BfError::CellUnderflow {
                source_location,
                current_value,
                hint,
                ..
            } => {
                let mut output = String::new();
                if let Some(loc) = source_location {
                    output.push_str(&format!(
                        "Error: Cell underflow\n\nAt line {}, column {}:\n",
                        loc.line, loc.column
                    ));
                    output.push_str(&extract_source_context_highlighted(source, *loc));
                    output.push_str(&format!(
                        "Attempted to decrement cell with value {}.\n",
                        current_value
                    ));
                } else {
                    output.push_str("Error: Cell underflow\n");
                    output.push_str(&format!(
                        "Attempted to decrement cell with value {}.\n",
                        current_value
                    ));
                }
                output.push_str(&format!("\nHint: {}", hint));
                output
            }
            // For other errors, use detailed format
            _ => self.format_detailed(),
        }
    }
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

/// Runtime warnings collected during program execution
///
/// These warnings indicate potentially unexpected behavior that occurred
/// during runtime, such as memory expansion in unbounded mode.
/// Unlike validation warnings (which are static analysis), these capture
/// actual runtime events.
///
/// When debug symbols are available, warnings include source locations
/// showing exactly where in the code the event occurred.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeWarning {
    /// Memory was expanded in unbounded mode
    MemoryExpanded {
        instruction_index: InstructionIndex,
        from_size: MemorySize,
        to_size: MemorySize,
        source_location: Option<SourceLocation>,
        #[doc(hidden)]
        _reserved: (),
    },
}

impl RuntimeWarning {
    /// Format warning with source context if available
    pub fn format_with_source(&self, source: &str) -> String {
        match self {
            RuntimeWarning::MemoryExpanded {
                instruction_index,
                from_size,
                to_size,
                source_location,
                ..
            } => {
                if let Some(loc) = source_location {
                    format!(
                        "Runtime warning: Memory expanded from {} to {} bytes\n\nAt line {}, column {}:\n{}",
                        from_size,
                        to_size,
                        loc.line,
                        loc.column,
                        extract_source_context_highlighted(source, *loc)
                    )
                } else {
                    format!(
                        "Runtime warning at instruction {}: Memory expanded from {} to {} bytes",
                        instruction_index, from_size, to_size
                    )
                }
            }
        }
    }
}

impl fmt::Display for RuntimeWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeWarning::MemoryExpanded {
                instruction_index,
                from_size,
                to_size,
                source_location,
                ..
            } => {
                if let Some(loc) = source_location {
                    write!(
                        f,
                        "Runtime warning: Memory expanded from {} to {} bytes at {}",
                        from_size, to_size, loc
                    )
                } else {
                    write!(
                        f,
                        "Runtime warning at instruction {}: Memory expanded from {} to {} bytes",
                        instruction_index, from_size, to_size
                    )
                }
            }
        }
    }
}
