//! BrainFuck source code parser.
//!
//! Parses BrainFuck source code into an Abstract Syntax Tree (AST) of [`Instruction`]
//! nodes. The parser validates bracket matching, tracks source locations for error
//! reporting, and supports line comments using the `*` character.
//!
//! # Features
//!
//! - **Recursive descent parsing**: Converts source text into a nested tree structure
//! - **Location tracking**: Maintains line, column, and offset for every position
//! - **Bracket validation**: Pre-parse phase validates ALL bracket matching errors at once
//! - **Line comments**: `*` starts a line comment (everything after `*` is ignored)
//! - **Implicit comments**: Non-BF characters are ignored
//! - **Rich error messages**: Shows source context with 2 lines before/after
//! - **Debug symbols**: Optional source location mapping for runtime diagnostics
//!
//! # Parsing Process
//!
//! 1. **Bracket validation**: Pre-scan to find all bracket errors
//! 2. **Recursive descent**: Parse instructions into AST
//! 3. **Location tracking**: Record source position for each instruction
//! 4. **Debug info** (optional): Build index for runtime location lookup
//!
//! # Examples
//!
//! ## Basic parsing
//!
//! ```rust
//! use ferrous_cortex::parse;
//!
//! # fn main() -> Result<(), ferrous_cortex::BfError> {
//! let instructions = parse("++[>++<-]")?;
//! println!("Parsed {} instructions", instructions.len());
//! # Ok(())
//! # }
//! ```
//!
//! ## Parsing with debug info
//!
//! ```rust
//! use ferrous_cortex::parse_with_debug;
//!
//! # fn main() -> Result<(), ferrous_cortex::BfError> {
//! let (instructions, debug_info) = parse_with_debug("++[>++<-]")?;
//! // debug_info can be used to map runtime step count to source location
//! # Ok(())
//! # }
//! ```
//!
//! ## Line comments
//!
//! ```rust
//! use ferrous_cortex::parse;
//!
//! # fn main() -> Result<(), ferrous_cortex::BfError> {
//! let source = r#"
//!     * This is a line comment
//!     +++  * Increment three times
//! "#;
//! let instructions = parse(source)?;
//! # Ok(())
//! # }
//! ```

use crate::debug::DebugInfo;
use crate::error::{BfError, Result, extract_source_context};
use crate::instruction::Instruction;
use crate::location::SourceLocation;

/// Validate bracket matching before parsing
/// Returns all bracket errors found in the source
fn validate_brackets(source: &str, chars: &[char]) -> Vec<BfError> {
    let mut errors = Vec::new();
    let mut stack: Vec<SourceLocation> = Vec::new();
    let mut location = SourceLocation::start();

    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];

        match ch {
            '*' => {
                // Skip line comments
                advance_location(&mut location, ch);
                i += 1;
                while i < chars.len() && chars[i] != '\n' {
                    advance_location(&mut location, chars[i]);
                    i += 1;
                }
                continue;
            }
            '\n' => {
                advance_location(&mut location, ch);
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
        advance_location(&mut location, ch);
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
    let (instructions, _debug_info) = parse_with_debug(source)?;
    Ok(instructions)
}

/// Parse BrainFuck source code with debug symbol collection
///
/// Returns both the parsed instructions and debug information mapping
/// step indices to source locations. This enables runtime warnings
/// to show source context.
///
/// The step indices match the interpreter's execution order (StepCount),
/// allowing O(1) lookup of source locations at runtime.
pub fn parse_with_debug(source: impl AsRef<str>) -> Result<(Vec<Instruction>, DebugInfo)> {
    let source = source.as_ref();

    // Collect chars once for both validation and parsing (performance optimization)
    let chars: Vec<char> = source.chars().collect();

    // First validate brackets and report all errors at once
    let bracket_errors = validate_brackets(source, &chars);
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
    let mut debug_info = DebugInfo::with_source(source.to_string());
    let mut step_index = 0;
    let instructions = parse_block_with_debug(
        source,
        &chars,
        &mut location,
        None,
        &mut debug_info,
        &mut step_index,
        None, // Phase 2: no parent loop at top level
    )?;
    Ok((instructions, debug_info))
}

fn parse_block_with_debug(
    source: &str,
    chars: &[char],
    location: &mut SourceLocation,
    loop_start: Option<SourceLocation>,
    debug_info: &mut DebugInfo,
    step_index: &mut usize,
    parent_loop_index: Option<usize>, // Phase 2: track parent loop's instruction index
) -> Result<Vec<Instruction>> {
    let mut instructions = Vec::new();

    while location.offset < chars.len() {
        let ch = chars[location.offset];
        let instruction_location = *location;

        match ch {
            '>' => {
                debug_info.record(*step_index, instruction_location);
                *step_index += 1;
                instructions.push(Instruction::IncrementPointer);
            }
            '<' => {
                debug_info.record(*step_index, instruction_location);
                *step_index += 1;
                instructions.push(Instruction::DecrementPointer);
            }
            '+' => {
                debug_info.record(*step_index, instruction_location);
                *step_index += 1;
                instructions.push(Instruction::IncrementValue);
            }
            '-' => {
                debug_info.record(*step_index, instruction_location);
                *step_index += 1;
                instructions.push(Instruction::DecrementValue);
            }
            '.' => {
                debug_info.record(*step_index, instruction_location);
                *step_index += 1;
                instructions.push(Instruction::Output);
            }
            ',' => {
                debug_info.record(*step_index, instruction_location);
                *step_index += 1;
                instructions.push(Instruction::Input);
            }
            '[' => {
                // Semantics:
                // '[' = Check condition (if cell is zero, skip loop)
                // ']' = Jump back to '[' (unconditional)
                //
                // Implementation:
                // - Loop instruction: AST container (no step count)
                // - LoopCheck: Actual condition check (counts as 1 step)
                // - Loop body: Each instruction counts as steps

                let loop_location = *location;
                advance_location(location, ch);

                // The '[' bracket IS the condition check, implemented by LoopCheck.
                // We record only ONE step for the LoopCheck instruction.
                let loop_start_index = *step_index;
                let body_start_index = *step_index; // LoopCheck is first in body

                debug_info.record(*step_index, loop_location);
                *step_index += 1;

                // Recursively parse the loop body
                let loop_body = parse_block_with_debug(
                    source,
                    chars,
                    location,
                    Some(loop_location),
                    debug_info,
                    step_index,
                    Some(loop_start_index),
                )?;

                // Prepend LoopCheck to implement the '[' condition check
                // This ensures even empty loops like [] consume steps
                let mut body_with_check = vec![Instruction::LoopCheck];
                body_with_check.extend(loop_body);

                // Record loop metadata (body_size includes LoopCheck)
                let body_size = *step_index - body_start_index;
                debug_info.record_loop_metadata(crate::debug::LoopMetadata {
                    loop_start_index,
                    body_start_index,
                    body_size,
                    parent_loop: parent_loop_index,
                    source_location: loop_location,
                });

                instructions.push(Instruction::Loop(body_with_check));
                continue; // Location already advanced
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
                advance_location(location, ch);
                while location.offset < chars.len() && chars[location.offset] != '\n' {
                    advance_location(location, chars[location.offset]);
                }
                // Don't skip the newline itself - let it be processed normally
                continue;
            }
            '\n' => {
                advance_location(location, ch);
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
            vec![Instruction::Loop(vec![
                Instruction::LoopCheck,
                Instruction::IncrementValue
            ])]
        );
    }

    #[test]
    fn test_parse_nested_loops() {
        let source = "[[+]]";
        let result = parse(source).unwrap();
        assert_eq!(
            result,
            vec![Instruction::Loop(vec![
                Instruction::LoopCheck,
                Instruction::Loop(vec![Instruction::LoopCheck, Instruction::IncrementValue])
            ])]
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
        // Test that error context shows correct source lines with caret
        let source = "+++\n[->+<]\n[\n+++";
        let result = parse(source);

        if let Err(BfError::UnmatchedOpenBracket {
            context, location, ..
        }) = result
        {
            // Verify location is correct
            assert_eq!(location.line, 3);
            assert_eq!(location.column, 1);

            // Verify exact context format:
            // - Shows line 1-4 (all available lines since error is on line 3)
            // - Caret points to column 1 on line 3
            let expected_context = concat!(
                "    1 | +++\n",
                "    2 | [->+<]\n",
                "    3 | [\n",
                "      | ^\n",
                "    4 | +++"
            );

            // Compare without trailing newline (context may have one)
            assert_eq!(
                context.trim_end(),
                expected_context,
                "\n\nExpected context:\n{}\n\nActual context:\n{}\n",
                expected_context,
                context.trim_end()
            );
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
        assert!(matches!(instructions[1], Instruction::IncrementValue));
        assert!(matches!(instructions[2], Instruction::IncrementValue));
        assert!(matches!(instructions[3], Instruction::IncrementPointer));
        assert!(matches!(instructions[4], Instruction::Output));
    }

    #[test]
    fn test_line_comments_multiline() {
        let source = "* Comment line 1\n* Comment line 2\n+++\n* Another comment\n>.";
        let instructions = parse(source).unwrap();
        assert_eq!(instructions.len(), 5); // +++, >, .
        assert!(matches!(instructions[0], Instruction::IncrementValue));
        assert!(matches!(instructions[1], Instruction::IncrementValue));
        assert!(matches!(instructions[2], Instruction::IncrementValue));
        assert!(matches!(instructions[3], Instruction::IncrementPointer));
        assert!(matches!(instructions[4], Instruction::Output));
    }

    #[test]
    fn test_line_comments_in_loops() {
        let source = "[  * Loop start\n++  * Increment\n]  * Loop end";
        let instructions = parse(source).unwrap();
        assert_eq!(instructions.len(), 1);
        if let Instruction::Loop(body) = &instructions[0] {
            assert_eq!(body.len(), 3); // LoopCheck + ++
            assert!(matches!(body[0], Instruction::LoopCheck));
            assert!(matches!(body[1], Instruction::IncrementValue));
            assert!(matches!(body[2], Instruction::IncrementValue));
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

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    // Property: Parsing never panics (on any input)
    proptest! {
        #[test]
        fn parse_never_panics(source in ".*") {
            let _ = parse(&source);  // Should never panic, only return Ok or Err
        }
    }

    // Property: Valid BF programs always parse successfully
    proptest! {
        #[test]
        fn valid_bf_always_parses(source in valid_bf_source()) {
            let result = parse(&source);
            prop_assert!(result.is_ok(), "Valid BF program failed to parse: {:?}", result.err());
        }
    }

    // Property: Parsing is deterministic (same input = same output)
    proptest! {
        #[test]
        fn parse_is_deterministic(source in ".*") {
            let result1 = parse(&source);
            let result2 = parse(&source);

            match (result1, result2) {
                (Ok(ast1), Ok(ast2)) => prop_assert_eq!(ast1, ast2),
                (Err(_), Err(_)) => {}, // Both failed, that's fine
                _ => prop_assert!(false, "Parse gave different results"),
            }
        }
    }

    // Property: Balanced brackets always parse (ignoring other validity)
    proptest! {
        #[test]
        fn balanced_brackets_parse(source in balanced_brackets_source()) {
            let result = parse(&source);
            prop_assert!(result.is_ok(), "Balanced brackets should parse: {}", source);
        }
    }

    // Property: Comments don't affect validity
    proptest! {
        #[test]
        fn comments_dont_affect_validity(bf_code in valid_bf_source(), comment in "[a-zA-Z ]+") {
            let with_comment = format!("* {}\n{}", comment, bf_code);
            let without_comment = bf_code;

            let result1 = parse(&with_comment);
            let result2 = parse(&without_comment);

            prop_assert_eq!(result1.is_ok(), result2.is_ok());
        }
    }

    // Use shared proptest strategies from test_utils
    // This avoids code duplication and ensures consistency across tests
    use crate::test_utils::proptest_strategies::arb_bf_program;

    // Alias for compatibility with existing tests
    fn valid_bf_source() -> impl Strategy<Value = String> {
        arb_bf_program()
    }

    // Alias for compatibility with existing tests
    fn balanced_brackets_source() -> impl Strategy<Value = String> {
        arb_bf_program()
    }

    // Helper function to verify LoopCheck invariant recursively
    fn verify_loop_check_invariant(instructions: &[Instruction]) -> bool {
        for instruction in instructions {
            if let Instruction::Loop(body) = instruction {
                // Verify LoopCheck is the first instruction
                if body.is_empty() {
                    return false; // Loops should never be empty (at minimum LoopCheck)
                }
                if !matches!(body[0], Instruction::LoopCheck) {
                    return false; // First instruction must be LoopCheck
                }
                // Recursively verify nested loops
                if !verify_loop_check_invariant(body) {
                    return false;
                }
            }
        }
        true
    }

    // Property: LoopCheck is always the first instruction in Loop bodies
    proptest! {
        #[test]
        fn loop_check_always_first(source in valid_bf_source()) {
            let result = parse(&source);
            if let Ok(instructions) = result {
                prop_assert!(
                    verify_loop_check_invariant(&instructions),
                    "LoopCheck invariant violated: LoopCheck must be first instruction in all Loop bodies"
                );
            }
        }
    }
}

// Loop metadata collection tests
#[cfg(test)]
mod loop_metadata_tests {
    use super::*;

    #[test]
    fn test_simple_loop_metadata() {
        // Simple: +[>+<-]
        // New index mapping (Loop instruction doesn't count as step):
        // 0: +
        // 1: LoopCheck (this IS the '[' condition check)
        // 2: >
        // 3: +
        // 4: <
        // 5: -
        let source = "+[>+<-]";
        let (_instructions, debug_info) = parse_with_debug(source).unwrap();

        // Verify loop metadata was collected
        assert_eq!(debug_info.loop_count(), 1);

        let metadata = debug_info.get_loop_metadata(1).unwrap();
        assert_eq!(metadata.loop_start_index, 1); // LoopCheck is at index 1
        assert_eq!(metadata.body_start_index, 1); // Body starts with LoopCheck
        assert_eq!(metadata.body_size, 5); // LoopCheck + >+<- = 5 instructions
        assert_eq!(metadata.parent_loop, None); // Top-level loop
        assert_eq!(metadata.source_location.line, 1);
        assert_eq!(metadata.source_location.column, 2);
    }

    #[test]
    fn test_nested_loop_metadata() {
        // Nested: +[>+[<.>-]<-]
        // New index mapping (Loop instructions don't count as steps):
        // 0: +
        // 1: LoopCheck (outer - this IS the '[')
        // 2: >
        // 3: +
        // 4: LoopCheck (inner - this IS the second '[')
        // 5: <
        // 6: .
        // 7: >
        // 8: -
        // 9: <
        // 10: -
        let source = "+[>+[<.>-]<-]";
        let (_instructions, debug_info) = parse_with_debug(source).unwrap();

        // Verify both loops collected
        assert_eq!(debug_info.loop_count(), 2);

        // Outer loop
        let outer = debug_info.get_loop_metadata(1).unwrap();
        assert_eq!(outer.loop_start_index, 1);
        assert_eq!(outer.body_start_index, 1);
        assert_eq!(outer.body_size, 10); // LoopCheck + >+[<.>-]<- = 10 instructions
        assert_eq!(outer.parent_loop, None);

        // Inner loop
        let inner = debug_info.get_loop_metadata(4).unwrap();
        assert_eq!(inner.loop_start_index, 4);
        assert_eq!(inner.body_start_index, 4);
        assert_eq!(inner.body_size, 5); // LoopCheck + <.>- = 5 instructions
        assert_eq!(inner.parent_loop, Some(1)); // Parent is outer loop
    }

    #[test]
    fn test_triple_nested_loop_metadata() {
        // Triple nested: +++[>+[>+[>+<-]<-]<-]
        // New index mapping:
        // 0-2: +++
        // 3: LoopCheck (outer)
        // 4: >
        // 5: +
        // 6: LoopCheck (middle)
        // 7: >
        // 8: +
        // 9: LoopCheck (inner)
        // 10-13: >+<-
        // 14-15: <-
        // 16-17: <-
        let source = "+++[>+[>+[>+<-]<-]<-]";
        let (_instructions, debug_info) = parse_with_debug(source).unwrap();

        // Verify all three loops collected
        assert_eq!(debug_info.loop_count(), 3);

        // Outer loop (index 3)
        let outer = debug_info.get_loop_metadata(3).unwrap();
        assert_eq!(outer.loop_start_index, 3);
        assert_eq!(outer.body_start_index, 3);
        assert_eq!(outer.body_size, 15); // LoopCheck + >+[>+[>+<-]<-]<- = 15 instructions
        assert_eq!(outer.parent_loop, None);

        // Middle loop (index 6)
        let middle = debug_info.get_loop_metadata(6).unwrap();
        assert_eq!(middle.loop_start_index, 6);
        assert_eq!(middle.body_start_index, 6);
        assert_eq!(middle.body_size, 10); // LoopCheck(1) + >+(2) + inner_loop(5) + <-(2) = 10
        assert_eq!(middle.parent_loop, Some(3)); // Parent is outer

        // Inner loop (index 9)
        let inner = debug_info.get_loop_metadata(9).unwrap();
        assert_eq!(inner.loop_start_index, 9);
        assert_eq!(inner.body_start_index, 9);
        assert_eq!(inner.body_size, 5); // LoopCheck + >+<- = 5 instructions
        assert_eq!(inner.parent_loop, Some(6)); // Parent is middle
    }

    #[test]
    fn test_sibling_loops_metadata() {
        // Two sibling loops: +[>+<-]+[>-<+]
        // New index mapping:
        // 0: +
        // 1: LoopCheck (first loop)
        // 2-5: >+<-
        // 6: +
        // 7: LoopCheck (second loop)
        // 8-11: >-<+
        let source = "+[>+<-]+[>-<+]";
        let (_instructions, debug_info) = parse_with_debug(source).unwrap();

        // Verify both loops collected
        assert_eq!(debug_info.loop_count(), 2);

        // First loop
        let first = debug_info.get_loop_metadata(1).unwrap();
        assert_eq!(first.loop_start_index, 1);
        assert_eq!(first.body_size, 5); // LoopCheck + >+<- = 5 instructions
        assert_eq!(first.parent_loop, None);

        // Second loop
        let second = debug_info.get_loop_metadata(7).unwrap();
        assert_eq!(second.loop_start_index, 7);
        assert_eq!(second.body_size, 5); // LoopCheck + >-<+ = 5 instructions
        assert_eq!(second.parent_loop, None); // Also top-level
    }

    #[test]
    fn test_empty_loop_metadata() {
        // Empty loop: []
        // New index mapping:
        // 0: LoopCheck (only instruction)
        let source = "[]";
        let (_instructions, debug_info) = parse_with_debug(source).unwrap();

        assert_eq!(debug_info.loop_count(), 1);

        let metadata = debug_info.get_loop_metadata(0).unwrap();
        assert_eq!(metadata.loop_start_index, 0);
        assert_eq!(metadata.body_start_index, 0);
        assert_eq!(metadata.body_size, 1); // Only LoopCheck (empty body)
        assert_eq!(metadata.parent_loop, None);
    }
}
