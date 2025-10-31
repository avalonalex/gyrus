//! BrainFuck instruction types.
//!
//! This module defines the Abstract Syntax Tree (AST) node type [`Instruction`]
//! representing parsed BrainFuck commands. Each instruction maps to a BrainFuck
//! operator, with loops represented as nested vectors.
//!
//! # AST Structure
//!
//! The parser converts BrainFuck source code into a tree of instructions:
//! - Simple instructions: `+`, `-`, `>`, `<`, `.`, `,`
//! - Loop instruction: `[...]` contains a `Vec<Instruction>` (nested structure)
//!
//! # Examples
//!
//! ```rust
//! use ferrous_cortex::{parse, Instruction};
//!
//! # fn main() -> Result<(), ferrous_cortex::BfError> {
//! let instructions = parse("++[>+<-]")?;
//! // Instructions: [IncrementValue, IncrementValue, Loop([...])]
//! # Ok(())
//! # }
//! ```

use std::fmt;

/// BrainFuck instruction (AST node)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Instruction {
    IncrementPointer,       // >
    DecrementPointer,       // <
    IncrementValue,         // +
    DecrementValue,         // -
    Output,                 // .
    Input,                  // ,
    Loop(Vec<Instruction>), // [ ... ]
    LoopCheck, // Internal: represents the `[` condition check (always first instruction in loop body)
}

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Instruction::IncrementPointer => write!(f, ">"),
            Instruction::DecrementPointer => write!(f, "<"),
            Instruction::IncrementValue => write!(f, "+"),
            Instruction::DecrementValue => write!(f, "-"),
            Instruction::Output => write!(f, "."),
            Instruction::Input => write!(f, ","),
            Instruction::Loop(body) => {
                write!(f, "[")?;
                for instruction in body {
                    // Skip LoopCheck when displaying (it's internal)
                    if !matches!(instruction, Instruction::LoopCheck) {
                        write!(f, "{}", instruction)?;
                    }
                }
                write!(f, "]")
            }
            Instruction::LoopCheck => Ok(()), // Internal instruction, no output
        }
    }
}
