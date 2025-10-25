use std::fmt;
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

    #[error("Memory pointer out of bounds at instruction {instruction_index}")]
    MemoryOutOfBounds {
        instruction_index: InstructionIndex,
        attempted: isize,
        max: MemorySize,
        memory_dump: Option<MemoryDump>,
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
        hint: String,
    },

    #[error(
        "Cell underflow at instruction {instruction_index}: attempted to decrement cell with value {current_value}"
    )]
    CellUnderflow {
        instruction_index: InstructionIndex,
        current_value: u8,
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
