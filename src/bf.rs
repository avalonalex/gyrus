use std::io::{self, Read, Write};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BfError {
    #[error("Unmatched '[' at position {0}")]
    UnmatchedOpenBracket(usize),

    #[error("Unmatched ']' at position {0}")]
    UnmatchedCloseBracket(usize),

    #[error("IO error: {0}")]
    IoError(String),

    #[error("Memory pointer out of bounds")]
    MemoryOutOfBounds,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Instruction {
    IncrementPointer,      // >
    DecrementPointer,      // <
    IncrementValue,        // +
    DecrementValue,        // -
    Output,                // .
    Input,                 // ,
    Loop(Vec<Instruction>), // [ ... ]
}

/// Parse BrainFuck source code into a list of instructions
pub fn parse(source: &str) -> Result<Vec<Instruction>, BfError> {
    parse_block(source, 0).map(|(instructions, _)| instructions)
}

fn parse_block(source: &str, start: usize) -> Result<(Vec<Instruction>, usize), BfError> {
    let mut instructions = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let mut i = start;

    while i < chars.len() {
        match chars[i] {
            '>' => instructions.push(Instruction::IncrementPointer),
            '<' => instructions.push(Instruction::DecrementPointer),
            '+' => instructions.push(Instruction::IncrementValue),
            '-' => instructions.push(Instruction::DecrementValue),
            '.' => instructions.push(Instruction::Output),
            ',' => instructions.push(Instruction::Input),
            '[' => {
                // Recursively parse the loop body
                let (loop_body, end_pos) = parse_block(source, i + 1)?;
                instructions.push(Instruction::Loop(loop_body));
                i = end_pos;
            }
            ']' => {
                // End of current loop
                return Ok((instructions, i));
            }
            _ => {
                // Ignore non-BF characters (comments)
            }
        }
        i += 1;
    }

    // If we're not at the top level and we reach here, there's an unmatched '['
    if start != 0 {
        return Err(BfError::UnmatchedOpenBracket(start - 1));
    }

    Ok((instructions, i))
}

const MEMORY_SIZE: usize = 30000;

/// Interpret and execute BrainFuck instructions
pub fn interpret(instructions: &[Instruction]) -> Result<(), BfError> {
    let mut memory = vec![0u8; MEMORY_SIZE];
    let mut pointer = 0usize;

    execute_block(instructions, &mut memory, &mut pointer)
}

fn execute_block(
    instructions: &[Instruction],
    memory: &mut Vec<u8>,
    pointer: &mut usize,
) -> Result<(), BfError> {
    for instruction in instructions {
        match instruction {
            Instruction::IncrementPointer => {
                *pointer += 1;
                if *pointer >= MEMORY_SIZE {
                    return Err(BfError::MemoryOutOfBounds);
                }
            }
            Instruction::DecrementPointer => {
                if *pointer == 0 {
                    return Err(BfError::MemoryOutOfBounds);
                }
                *pointer -= 1;
            }
            Instruction::IncrementValue => {
                memory[*pointer] = memory[*pointer].wrapping_add(1);
            }
            Instruction::DecrementValue => {
                memory[*pointer] = memory[*pointer].wrapping_sub(1);
            }
            Instruction::Output => {
                io::stdout()
                    .write_all(&[memory[*pointer]])
                    .map_err(|e| BfError::IoError(e.to_string()))?;
                io::stdout()
                    .flush()
                    .map_err(|e| BfError::IoError(e.to_string()))?;
            }
            Instruction::Input => {
                let mut buf = [0u8; 1];
                io::stdin()
                    .read_exact(&mut buf)
                    .map_err(|e| BfError::IoError(e.to_string()))?;
                memory[*pointer] = buf[0];
            }
            Instruction::Loop(body) => {
                while memory[*pointer] != 0 {
                    execute_block(body, memory, pointer)?;
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let source = "+-><.,";
        let result = parse(source).unwrap();
        assert_eq!(result, vec![
            Instruction::IncrementValue,
            Instruction::DecrementValue,
            Instruction::IncrementPointer,
            Instruction::DecrementPointer,
            Instruction::Output,
            Instruction::Input,
        ]);
    }

    #[test]
    fn test_parse_loop() {
        let source = "[+]";
        let result = parse(source).unwrap();
        assert_eq!(result, vec![
            Instruction::Loop(vec![Instruction::IncrementValue])
        ]);
    }

    #[test]
    fn test_parse_nested_loops() {
        let source = "[[+]]";
        let result = parse(source).unwrap();
        assert_eq!(result, vec![
            Instruction::Loop(vec![
                Instruction::Loop(vec![Instruction::IncrementValue])
            ])
        ]);
    }

    #[test]
    fn test_unmatched_open_bracket() {
        let source = "[+";
        let result = parse(source);
        assert!(matches!(result, Err(BfError::UnmatchedOpenBracket(_))));
    }

    #[test]
    fn test_comments_ignored() {
        let source = "+ This is a comment -";
        let result = parse(source).unwrap();
        assert_eq!(result, vec![
            Instruction::IncrementValue,
            Instruction::DecrementValue,
        ]);
    }
}
