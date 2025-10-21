use crate::error::{BfError, Result, extract_source_context};
use crate::instruction::Instruction;
use crate::location::SourceLocation;

/// Validate bracket matching before parsing
/// Returns all bracket errors found in the source
fn validate_brackets(source: &str) -> Vec<BfError> {
    let mut errors = Vec::new();
    let mut stack: Vec<SourceLocation> = Vec::new();
    let mut location = SourceLocation::start();
    let chars: Vec<char> = source.chars().collect();

    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];

        match ch {
            '*' => {
                // Skip line comments
                i += 1;
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            '\n' => {
                location.line += 1;
                location.column = 1;
                location.offset += 1;
                i += 1;
                continue;
            }
            '[' => {
                stack.push(location);
            }
            ']' => {
                if stack.is_empty() {
                    // Unmatched closing bracket
                    errors.push(BfError::UnmatchedCloseBracket {
                        location,
                        context: extract_source_context(source, location),
                    });
                } else {
                    stack.pop();
                }
            }
            _ => {
                // Ignore other characters
            }
        }

        // Advance location
        if ch != '\n' {
            location.column += 1;
        }
        location.offset += 1;
        i += 1;
    }

    // Any remaining open brackets are unmatched
    for open_location in stack {
        errors.push(BfError::UnmatchedOpenBracket {
            location: open_location,
            context: extract_source_context(source, open_location),
        });
    }

    errors
}

/// Parse BrainFuck source code into a list of instructions
///
/// Accepts any string-like type (`&str`, `String`, `Cow<str>`, etc.)
pub fn parse(source: impl AsRef<str>) -> Result<Vec<Instruction>> {
    let source = source.as_ref();

    // First validate brackets and report all errors at once
    let bracket_errors = validate_brackets(source);
    if !bracket_errors.is_empty() {
        if bracket_errors.len() == 1 {
            // Single error - return it directly
            return Err(bracket_errors.into_iter().next().unwrap());
        } else {
            // Multiple errors - report all details to stderr, then return summary error
            let count = bracket_errors.len();
            eprintln!("Found {} bracket matching error(s):\n", count);
            for (i, error) in bracket_errors.iter().enumerate() {
                eprintln!("Error {}:", i + 1);
                eprintln!("{}\n", error);
            }
            return Err(BfError::MultipleBracketErrors { count });
        }
    }

    let mut location = SourceLocation::start();
    parse_block(source, &mut location, None).map(|instructions| instructions)
}

fn parse_block(
    source: &str,
    location: &mut SourceLocation,
    loop_start: Option<SourceLocation>,
) -> Result<Vec<Instruction>> {
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

#[inline]
fn advance_location(location: &mut SourceLocation, ch: char) {
    if ch == '\n' {
        location.line += 1;
        location.column = 1;
    } else {
        location.column += 1;
    }
    location.offset += 1;
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
    fn test_bracket_matching_valid() {
        // Valid programs should parse without errors
        let source = "[>+<-]";
        assert!(parse(source).is_ok());

        let source = "[[[]]]";
        assert!(parse(source).is_ok());

        let source = "[>+[>+[>+<]<]<]";
        assert!(parse(source).is_ok());
    }

    #[test]
    fn test_bracket_matching_single_unmatched_open() {
        let source = "[>++";
        let result = parse(source);
        assert!(result.is_err());
        assert!(matches!(result, Err(BfError::UnmatchedOpenBracket { .. })));
    }

    #[test]
    fn test_bracket_matching_single_unmatched_close() {
        let source = "++]";
        let result = parse(source);
        assert!(result.is_err());
        assert!(matches!(result, Err(BfError::UnmatchedCloseBracket { .. })));
    }

    #[test]
    fn test_bracket_matching_multiple_unmatched_open() {
        // Test with multiple unclosed opening brackets
        let source = "[>++\n[<--\n[+++";
        let result = parse(source);
        assert!(result.is_err());
        // Should return MultipleBracketErrors when there are multiple errors
        assert!(matches!(
            result,
            Err(BfError::MultipleBracketErrors { count: 3 })
        ));
    }

    #[test]
    fn test_bracket_matching_multiple_unmatched_close() {
        // Test with multiple unmatched closing brackets
        let source = "+++]\n++]\n+]";
        let result = parse(source);
        assert!(result.is_err());
        // Should return MultipleBracketErrors when there are multiple errors
        assert!(matches!(
            result,
            Err(BfError::MultipleBracketErrors { count: 3 })
        ));
    }

    #[test]
    fn test_bracket_matching_mixed_errors() {
        // Test with both unclosed opens and unmatched closes
        // Three opens, then three closes plus two extra
        let source = "+++[\n>++[\n<-[\n+++]\n]\n]\n]\n]";
        let result = parse(source);
        assert!(result.is_err());
        // Should return MultipleBracketErrors when there are multiple errors
        assert!(matches!(
            result,
            Err(BfError::MultipleBracketErrors { count: 2 })
        ));
    }

    #[test]
    fn test_bracket_matching_with_line_comments() {
        // Valid brackets with line comments
        let source = "* Opening bracket\n[>++<-] * Closing bracket\n";
        assert!(parse(source).is_ok());

        // Unmatched bracket with line comments
        let source = "* Comment\n[>++ * [ this bracket is in comment\n";
        let result = parse(source);
        assert!(result.is_err());
        assert!(matches!(result, Err(BfError::UnmatchedOpenBracket { .. })));
    }

    #[test]
    fn test_bracket_matching_location_tracking() {
        // Test that error locations are accurate
        let source = "+++\n[>++\n<--";
        let result = parse(source);
        assert!(result.is_err());
        match result {
            Err(BfError::UnmatchedOpenBracket { location, .. }) => {
                assert_eq!(location.line, 2); // Bracket is on line 2
                assert_eq!(location.column, 1); // First character of line 2
            }
            _ => panic!("Expected UnmatchedOpenBracket with location"),
        }
    }

    #[test]
    fn test_bracket_matching_nested_errors() {
        // Nested structure with missing closing bracket
        let source = "[[+]";
        let result = parse(source);
        assert!(result.is_err());
        assert!(matches!(result, Err(BfError::UnmatchedOpenBracket { .. })));
    }
}
