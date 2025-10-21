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
                    write!(f, "{}", instruction)?;
                }
                write!(f, "]")
            }
        }
    }
}
