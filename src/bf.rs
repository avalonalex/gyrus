use std::fmt;
use std::io::{self, Read, Write};
use thiserror::Error;

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
    #[allow(dead_code)]
    pub fn new(line: usize, column: usize, offset: usize) -> Self {
        Self {
            line,
            column,
            offset,
        }
    }

    pub fn start() -> Self {
        Self {
            line: 1,
            column: 1,
            offset: 0,
        }
    }
}

/// Extract source context around a location for error messages
fn extract_source_context(source: &str, location: SourceLocation) -> String {
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

    #[error(
        "Memory pointer out of bounds at instruction {instruction_index}\nAttempted to access cell {attempted}, valid range: 0-{max}"
    )]
    MemoryOutOfBounds {
        instruction_index: usize,
        attempted: isize,
        max: usize,
    },

    #[error("IO error: {message}")]
    IoError { message: String },

    #[error("File read error: {0}")]
    FileError(String),

    #[error("Execution timeout: program exceeded {limit_ms}ms execution limit")]
    ExecutionTimeout { limit_ms: u64 },

    #[error("Step limit exceeded: program exceeded {limit} instruction limit")]
    StepLimitExceeded { limit: u64 },
}

/// Warnings for potentially problematic but valid BrainFuck code
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

#[derive(Debug, Clone, PartialEq)]
pub enum Instruction {
    IncrementPointer,       // >
    DecrementPointer,       // <
    IncrementValue,         // +
    DecrementValue,         // -
    Output,                 // .
    Input,                  // ,
    Loop(Vec<Instruction>), // [ ... ]
}

/// Parse BrainFuck source code into a list of instructions
pub fn parse(source: &str) -> Result<Vec<Instruction>, BfError> {
    let mut location = SourceLocation::start();
    parse_block(source, &mut location, None).map(|instructions| instructions)
}

/// Validate parsed instructions for common issues and potential problems
pub fn validate(instructions: &[Instruction]) -> Vec<BfWarning> {
    let mut warnings = Vec::new();
    let location = SourceLocation::start(); // Placeholder - would need actual tracking

    validate_instructions(instructions, &mut warnings, 0, location);
    warnings
}

/// Convert instructions back to BrainFuck source code (minified - no comments)
pub fn minify(instructions: &[Instruction]) -> String {
    let mut output = String::new();
    minify_instructions(instructions, &mut output);
    output
}

fn minify_instructions(instructions: &[Instruction], output: &mut String) {
    for instruction in instructions {
        match instruction {
            Instruction::IncrementPointer => output.push('>'),
            Instruction::DecrementPointer => output.push('<'),
            Instruction::IncrementValue => output.push('+'),
            Instruction::DecrementValue => output.push('-'),
            Instruction::Output => output.push('.'),
            Instruction::Input => output.push(','),
            Instruction::Loop(body) => {
                output.push('[');
                minify_instructions(body, output);
                output.push(']');
            }
        }
    }
}

fn validate_instructions(
    instructions: &[Instruction],
    warnings: &mut Vec<BfWarning>,
    depth: usize,
    location: SourceLocation,
) {
    // Check for extreme nesting
    if depth > 10 {
        warnings.push(BfWarning::ExtremeNesting { location, depth });
    }

    for instruction in instructions {
        match instruction {
            Instruction::Loop(body) => {
                // Check for empty loops
                if body.is_empty() {
                    warnings.push(BfWarning::EmptyLoop { location });
                } else {
                    // Check for suspicious patterns
                    check_suspicious_loop_patterns(body, warnings, location);

                    // Recursively validate nested loops
                    validate_instructions(body, warnings, depth + 1, location);
                }
            }
            _ => {}
        }
    }
}

fn check_suspicious_loop_patterns(
    body: &[Instruction],
    warnings: &mut Vec<BfWarning>,
    location: SourceLocation,
) {
    // Note: [>] and [<] are common patterns for seeking in BF, so we don't warn about them
    // Note: [-] is a common pattern for clearing a cell, so we don't warn about it

    // Check for [+] which creates an infinite loop (cell can never reach zero by incrementing)
    if body.len() == 1 && matches!(body[0], Instruction::IncrementValue) {
        warnings.push(BfWarning::SuspiciousPattern {
            location,
            pattern: "[+]".to_string(),
            reason:
                "This pattern creates an infinite loop (cell will never reach zero by incrementing)"
                    .to_string(),
        });
    }

    // Check for [-] followed only by other decrements (e.g., [--])
    // This is suspicious because it's not as efficient as [-]
    let all_decrements = body
        .iter()
        .all(|i| matches!(i, Instruction::DecrementValue));
    if all_decrements && body.len() > 1 {
        warnings.push(BfWarning::SuspiciousPattern {
            location,
            pattern: format!("[{}]", "-".repeat(body.len())),
            reason: format!("Multiple decrements in a loop is inefficient. Consider using [-] to clear the cell."),
        });
    }

    // Check for [+] followed by other increments (e.g., [++])
    let all_increments = body
        .iter()
        .all(|i| matches!(i, Instruction::IncrementValue));
    if all_increments && body.len() >= 1 {
        warnings.push(BfWarning::SuspiciousPattern {
            location,
            pattern: format!("[{}]", "+".repeat(body.len())),
            reason:
                "This pattern creates an infinite loop (cell will never reach zero by incrementing)"
                    .to_string(),
        });
    }
}

fn parse_block(
    source: &str,
    location: &mut SourceLocation,
    loop_start: Option<SourceLocation>,
) -> Result<Vec<Instruction>, BfError> {
    let mut instructions = Vec::new();
    let chars: Vec<char> = source.chars().collect();

    while location.offset < chars.len() {
        let ch = chars[location.offset];

        match ch {
            '>' => instructions.push(Instruction::IncrementPointer),
            '<' => instructions.push(Instruction::DecrementPointer),
            '+' => instructions.push(Instruction::IncrementValue),
            '-' => instructions.push(Instruction::DecrementValue),
            '.' => instructions.push(Instruction::Output),
            ',' => instructions.push(Instruction::Input),
            '[' => {
                let loop_location = *location;
                advance_location(location, ch);

                // Recursively parse the loop body
                let loop_body = parse_block(source, location, Some(loop_location))?;
                instructions.push(Instruction::Loop(loop_body));
                continue; // Don't advance again, parse_block already did
            }
            ']' => {
                // End of current loop
                if loop_start.is_some() {
                    advance_location(location, ch);
                    return Ok(instructions);
                } else {
                    // Unmatched closing bracket
                    return Err(BfError::UnmatchedCloseBracket {
                        location: *location,
                        context: extract_source_context(source, *location),
                    });
                }
            }
            '*' => {
                // Line comment: skip everything until newline
                location.offset += 1;
                while location.offset < chars.len() && chars[location.offset] != '\n' {
                    location.column += 1;
                    location.offset += 1;
                }
                // Don't skip the newline itself - let it be processed normally
                continue;
            }
            '\n' => {
                location.line += 1;
                location.column = 1;
                location.offset += 1;
                continue;
            }
            _ => {
                // Ignore non-BF characters (comments)
            }
        }

        advance_location(location, ch);
    }

    // If we're in a loop and reach EOF, there's an unmatched '['
    if let Some(start_loc) = loop_start {
        return Err(BfError::UnmatchedOpenBracket {
            location: start_loc,
            context: extract_source_context(source, start_loc),
        });
    }

    Ok(instructions)
}

fn advance_location(location: &mut SourceLocation, ch: char) {
    if ch == '\n' {
        location.line += 1;
        location.column = 1;
    } else {
        location.column += 1;
    }
    location.offset += 1;
}

const MEMORY_SIZE: usize = 30000;

/// Configuration for BrainFuck interpreter execution
#[derive(Debug, Clone)]
pub struct ExecutionConfig {
    pub memory_size: usize,
    pub max_steps: Option<u64>,
    pub timeout_ms: Option<u64>,
    pub allow_negative_pointer: bool,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            memory_size: MEMORY_SIZE,
            max_steps: None,
            timeout_ms: None,
            allow_negative_pointer: false,
        }
    }
}

impl ExecutionConfig {
    /// Create a new ExecutionConfig with default values (same as Default::default())
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_memory_size(mut self, size: usize) -> Self {
        self.memory_size = size;
        self
    }

    pub fn with_max_steps(mut self, steps: u64) -> Self {
        self.max_steps = Some(steps);
        self
    }

    pub fn with_timeout_ms(mut self, timeout: u64) -> Self {
        self.timeout_ms = Some(timeout);
        self
    }

    /// Allow pointer to go negative (not yet fully implemented)
    #[allow(dead_code)]
    pub fn with_negative_pointer(mut self, allow: bool) -> Self {
        self.allow_negative_pointer = allow;
        self
    }
}

/// Interpret and execute BrainFuck instructions with default configuration
///
/// This is a convenience function that uses default settings.
/// For custom configuration, use `interpret_with_config()`.
#[allow(dead_code)]
pub fn interpret(instructions: &[Instruction]) -> Result<(), BfError> {
    interpret_with_config(instructions, ExecutionConfig::default())
}

/// Interpret and execute BrainFuck instructions with custom configuration
pub fn interpret_with_config(
    instructions: &[Instruction],
    config: ExecutionConfig,
) -> Result<(), BfError> {
    use std::time::Instant;

    let mut memory = vec![0u8; config.memory_size];
    let mut pointer = 0usize;
    let mut step_count = 0u64;
    let start_time = config.timeout_ms.map(|_| Instant::now());

    execute_block(
        instructions,
        &mut memory,
        &mut pointer,
        &mut step_count,
        &config,
        &start_time,
    )
}

fn execute_block(
    instructions: &[Instruction],
    memory: &mut Vec<u8>,
    pointer: &mut usize,
    step_count: &mut u64,
    config: &ExecutionConfig,
    start_time: &Option<std::time::Instant>,
) -> Result<(), BfError> {
    for instruction in instructions {
        // Check step limit
        *step_count += 1;
        if let Some(max_steps) = config.max_steps {
            if *step_count > max_steps {
                return Err(BfError::StepLimitExceeded { limit: max_steps });
            }
        }

        // Check timeout
        if let Some(start) = start_time {
            if let Some(timeout_ms) = config.timeout_ms {
                let elapsed = start.elapsed().as_millis() as u64;
                if elapsed > timeout_ms {
                    return Err(BfError::ExecutionTimeout {
                        limit_ms: timeout_ms,
                    });
                }
            }
        }

        match instruction {
            Instruction::IncrementPointer => {
                *pointer += 1;
                if *pointer >= config.memory_size {
                    return Err(BfError::MemoryOutOfBounds {
                        instruction_index: *step_count as usize,
                        attempted: *pointer as isize,
                        max: config.memory_size - 1,
                    });
                }
            }
            Instruction::DecrementPointer => {
                if *pointer == 0 && !config.allow_negative_pointer {
                    return Err(BfError::MemoryOutOfBounds {
                        instruction_index: *step_count as usize,
                        attempted: -1,
                        max: config.memory_size - 1,
                    });
                }
                if *pointer > 0 {
                    *pointer -= 1;
                }
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
                    .map_err(|e| BfError::IoError {
                        message: e.to_string(),
                    })?;
                io::stdout().flush().map_err(|e| BfError::IoError {
                    message: e.to_string(),
                })?;
            }
            Instruction::Input => {
                let mut buf = [0u8; 1];
                io::stdin()
                    .read_exact(&mut buf)
                    .map_err(|e| BfError::IoError {
                        message: e.to_string(),
                    })?;
                memory[*pointer] = buf[0];
            }
            Instruction::Loop(body) => {
                while memory[*pointer] != 0 {
                    execute_block(body, memory, pointer, step_count, config, start_time)?;
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
        assert_eq!(
            result,
            vec![
                Instruction::IncrementValue,
                Instruction::DecrementValue,
                Instruction::IncrementPointer,
                Instruction::DecrementPointer,
                Instruction::Output,
                Instruction::Input,
            ]
        );
    }

    #[test]
    fn test_parse_loop() {
        let source = "[+]";
        let result = parse(source).unwrap();
        assert_eq!(
            result,
            vec![Instruction::Loop(vec![Instruction::IncrementValue])]
        );
    }

    #[test]
    fn test_parse_nested_loops() {
        let source = "[[+]]";
        let result = parse(source).unwrap();
        assert_eq!(
            result,
            vec![Instruction::Loop(vec![Instruction::Loop(vec![
                Instruction::IncrementValue
            ])])]
        );
    }

    #[test]
    fn test_unmatched_open_bracket() {
        let source = "[+";
        let result = parse(source);
        assert!(matches!(result, Err(BfError::UnmatchedOpenBracket { .. })));
    }

    #[test]
    fn test_comments_ignored() {
        let source = "+ This is a comment -";
        let result = parse(source).unwrap();
        assert_eq!(
            result,
            vec![Instruction::IncrementValue, Instruction::DecrementValue,]
        );
    }

    #[test]
    fn test_unmatched_close_bracket() {
        let source = "+]";
        let result = parse(source);
        assert!(matches!(result, Err(BfError::UnmatchedCloseBracket { .. })));
    }

    #[test]
    fn test_multiline_error_location() {
        let source = "+++\n[->+<]\n[\n+++";
        let result = parse(source);
        assert!(
            matches!(result, Err(BfError::UnmatchedOpenBracket { location, .. }) if location.line == 3)
        );
    }

    #[test]
    fn test_memory_overflow() {
        let source = ">".repeat(30001); // Try to go beyond memory
        let instructions = parse(&source).unwrap();
        let config = ExecutionConfig::default();
        let result = interpret_with_config(&instructions, config);
        assert!(matches!(result, Err(BfError::MemoryOutOfBounds { .. })));
    }

    #[test]
    fn test_memory_underflow() {
        let source = "<"; // Try to go below 0
        let instructions = parse(&source).unwrap();
        let config = ExecutionConfig::default();
        let result = interpret_with_config(&instructions, config);
        assert!(matches!(
            result,
            Err(BfError::MemoryOutOfBounds { attempted: -1, .. })
        ));
    }

    #[test]
    fn test_step_limit() {
        let source = "+[+]"; // Infinite loop
        let instructions = parse(&source).unwrap();
        let config = ExecutionConfig::default().with_max_steps(100);
        let result = interpret_with_config(&instructions, config);
        assert!(matches!(
            result,
            Err(BfError::StepLimitExceeded { limit: 100 })
        ));
    }

    #[test]
    fn test_custom_memory_size() {
        let source = ">".repeat(101);
        let instructions = parse(&source).unwrap();
        let config = ExecutionConfig::default().with_memory_size(100);
        let result = interpret_with_config(&instructions, config);
        assert!(matches!(
            result,
            Err(BfError::MemoryOutOfBounds { max: 99, .. })
        ));
    }

    #[test]
    fn test_execution_timeout() {
        // Create a program that runs longer - moving pointer takes more time
        let source = "+[>+<]".repeat(1000); // Infinite loop with more instructions
        let instructions = parse(&source).unwrap();
        let config = ExecutionConfig::default()
            .with_timeout_ms(100)
            .with_memory_size(1000); // Smaller memory to hit bounds faster
        let result = interpret_with_config(&instructions, config);
        // Should fail with either timeout or memory bounds
        assert!(result.is_err());
    }

    #[test]
    fn test_source_location_tracking() {
        let source = "+++\n+++\n[";
        let result = parse(source);
        if let Err(BfError::UnmatchedOpenBracket { location, .. }) = result {
            assert_eq!(location.line, 3);
            assert_eq!(location.column, 1);
        } else {
            panic!("Expected UnmatchedOpenBracket error");
        }
    }

    #[test]
    fn test_error_context_generation() {
        let source = "+++\n[->+<]\n[\n+++";
        let result = parse(source);
        if let Err(BfError::UnmatchedOpenBracket { context, .. }) = result {
            assert!(context.contains("["));
            assert!(context.contains("^"));
        } else {
            panic!("Expected UnmatchedOpenBracket error");
        }
    }

    #[test]
    fn test_validate_empty_loop() {
        let source = "[]";
        let instructions = parse(source).unwrap();
        let warnings = validate(&instructions);
        assert_eq!(warnings.len(), 1);
        assert!(matches!(warnings[0], BfWarning::EmptyLoop { .. }));
    }

    #[test]
    fn test_validate_infinite_loop() {
        let source = "+[+]";
        let instructions = parse(source).unwrap();
        let warnings = validate(&instructions);
        assert!(!warnings.is_empty());
        assert!(warnings.iter().any(
            |w| matches!(w, BfWarning::SuspiciousPattern { pattern, .. } if pattern.contains("+"))
        ));
    }

    #[test]
    fn test_validate_extreme_nesting() {
        let source = "+[+[+[+[+[+[+[+[+[+[+[+[-]]]]]]]]]]]]"; // 12 levels
        let instructions = parse(source).unwrap();
        let warnings = validate(&instructions);
        assert!(
            warnings
                .iter()
                .any(|w| matches!(w, BfWarning::ExtremeNesting { depth, .. } if *depth > 10))
        );
    }

    #[test]
    fn test_validate_clean_program() {
        let source = "+++[->+++<]>."; // Simple clean program
        let instructions = parse(source).unwrap();
        let warnings = validate(&instructions);
        // Should have no warnings
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_validate_multiple_warnings() {
        let source = "[]++[+]"; // Empty loop + infinite loop
        let instructions = parse(source).unwrap();
        let warnings = validate(&instructions);
        assert!(warnings.len() >= 2);
    }

    #[test]
    fn test_line_comments() {
        let source = "+++  * This is a comment\n>++  * Another comment";
        let instructions = parse(source).unwrap();
        // Commands: + + + > + + (6 total)
        assert_eq!(instructions.len(), 6);
        assert!(matches!(instructions[0], Instruction::IncrementValue));
        assert!(matches!(instructions[1], Instruction::IncrementValue));
        assert!(matches!(instructions[2], Instruction::IncrementValue));
        assert!(matches!(instructions[3], Instruction::IncrementPointer));
        assert!(matches!(instructions[4], Instruction::IncrementValue));
        assert!(matches!(instructions[5], Instruction::IncrementValue));
    }

    #[test]
    fn test_line_comments_with_bf_commands() {
        // BF commands after * should be ignored
        let source = "+++ * >++<-- These commands are ignored\n>.";
        let instructions = parse(source).unwrap();
        // Commands: + + + > . (5 total - commands after * are ignored)
        assert_eq!(instructions.len(), 5);
        assert!(matches!(instructions[0], Instruction::IncrementValue));
        assert!(matches!(instructions[3], Instruction::IncrementPointer));
        assert!(matches!(instructions[4], Instruction::Output));
    }

    #[test]
    fn test_line_comments_multiline() {
        let source = "* Comment line 1\n* Comment line 2\n+++\n* Another comment\n>.";
        let instructions = parse(source).unwrap();
        assert_eq!(instructions.len(), 5); // +++, >, .
    }

    #[test]
    fn test_line_comments_in_loops() {
        let source = "[  * Loop start\n++  * Increment\n]  * Loop end";
        let instructions = parse(source).unwrap();
        assert_eq!(instructions.len(), 1);
        if let Instruction::Loop(body) = &instructions[0] {
            assert_eq!(body.len(), 2); // Only ++
        } else {
            panic!("Expected loop");
        }
    }

    #[test]
    fn test_minify_simple() {
        let source = "+++  Comments here\n>++ More comments\n.";
        let instructions = parse(source).unwrap();
        let minified = minify(&instructions);
        assert_eq!(minified, "+++>++.");
    }

    #[test]
    fn test_minify_with_line_comments() {
        let source = "* Line comment\n+++  * Inline comment\n[>++<-]  * Loop comment";
        let instructions = parse(source).unwrap();
        let minified = minify(&instructions);
        assert_eq!(minified, "+++[>++<-]");
    }

    #[test]
    fn test_minify_nested_loops() {
        let source = "[[+]]";
        let instructions = parse(source).unwrap();
        let minified = minify(&instructions);
        assert_eq!(minified, "[[+]]");
    }

    #[test]
    fn test_minify_all_commands() {
        let source = "* Test all commands\n><+-.,[]";
        let instructions = parse(source).unwrap();
        let minified = minify(&instructions);
        assert_eq!(minified, "><+-.,[]");
    }

    #[test]
    fn test_minify_round_trip() {
        // Parse, minify, parse again should give same result
        let source = "+++[>++<-]>.";
        let instructions1 = parse(source).unwrap();
        let minified = minify(&instructions1);
        let instructions2 = parse(&minified).unwrap();
        assert_eq!(instructions1, instructions2);
    }
}
