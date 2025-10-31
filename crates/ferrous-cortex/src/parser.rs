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
                let loop_location = *location;
                advance_location(location, ch);

                // Phase 2: Record loop start index before incrementing
                let loop_start_index = *step_index;

                // Record loop start location
                debug_info.record(*step_index, loop_location);
                *step_index += 1;

                // Phase 2: Body starts at the next index
                let body_start_index = *step_index;

                // Reserve space for LoopCheck instruction (prepended later)
                // LoopCheck will be the first instruction in the loop body and will be executed
                // Record it with the loop location since it represents the '[' condition check
                debug_info.record(*step_index, loop_location);
                *step_index += 1;

                // Recursively parse the loop body (this will increment step_index for each instruction in the body)
                let loop_body = parse_block_with_debug(
                    source,
                    chars,
                    location,
                    Some(loop_location),
                    debug_info,
                    step_index,
                    Some(loop_start_index), // Phase 2: pass this loop's index as parent
                )?;

                // Prepend LoopCheck as the first instruction in every loop body
                // This ensures step counting is centralized and prevents empty loop hangs
                let mut body_with_check = vec![Instruction::LoopCheck];
                body_with_check.extend(loop_body);

                // Phase 2: Calculate body size and record loop metadata
                // body_size includes the LoopCheck instruction
                let body_size = *step_index - body_start_index;
                debug_info.record_loop_metadata(crate::debug::LoopMetadata {
                    loop_start_index,
                    body_start_index,
                    body_size,
                    parent_loop: parent_loop_index,
                    source_location: loop_location,
                });

                instructions.push(Instruction::Loop(body_with_check));
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
            assert_eq!(body.len(), 3); // LoopCheck + ++
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

    // Strategy: Generate valid BrainFuck programs
    fn valid_bf_source() -> impl Strategy<Value = String> {
        prop::collection::vec(bf_instruction(), 0..100).prop_flat_map(|instrs| {
            // Build a string with balanced brackets
            let mut result = String::new();
            let mut depth = 0;

            for instr in instrs {
                match instr.as_str() {
                    "[" => {
                        result.push('[');
                        depth += 1;
                    }
                    "]" => {
                        if depth > 0 {
                            result.push(']');
                            depth -= 1;
                        }
                    }
                    c => result.push_str(c),
                }
            }

            // Close any remaining brackets
            for _ in 0..depth {
                result.push(']');
            }

            Just(result)
        })
    }

    // Strategy: Generate balanced bracket sequences
    fn balanced_brackets_source() -> impl Strategy<Value = String> {
        prop::collection::vec(bf_instruction(), 0..50).prop_map(|instrs| {
            let mut result = String::new();
            let mut depth = 0;

            for instr in instrs {
                match instr.as_str() {
                    "[" => {
                        result.push('[');
                        depth += 1;
                    }
                    "]" => {
                        if depth > 0 {
                            result.push(']');
                            depth -= 1;
                        }
                    }
                    c => result.push_str(c),
                }
            }

            // Close remaining brackets
            for _ in 0..depth {
                result.push(']');
            }

            result
        })
    }

    // Strategy: Generate BF instructions
    fn bf_instruction() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("+".to_string()),
            Just("-".to_string()),
            Just(">".to_string()),
            Just("<".to_string()),
            Just(".".to_string()),
            Just(",".to_string()),
            Just("[".to_string()),
            Just("]".to_string()),
        ]
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

// Phase 2: Loop metadata collection tests
#[cfg(test)]
mod loop_metadata_tests {
    use super::*;

    #[test]
    fn test_simple_loop_metadata() {
        // Simple: +[>+<-]
        let source = "+[>+<-]";
        let (_instructions, debug_info) = parse_with_debug(source).unwrap();

        // Verify loop metadata was collected
        assert_eq!(debug_info.loop_count(), 1);

        let metadata = debug_info.get_loop_metadata(1).unwrap();
        assert_eq!(metadata.loop_start_index, 1); // '[' is at index 1
        assert_eq!(metadata.body_start_index, 2); // Body starts after '[' (LoopCheck at index 2)
        assert_eq!(metadata.body_size, 5); // LoopCheck + >+<- = 5 instructions
        assert_eq!(metadata.parent_loop, None); // Top-level loop
        assert_eq!(metadata.source_location.line, 1);
        assert_eq!(metadata.source_location.column, 2);
    }

    #[test]
    fn test_nested_loop_metadata() {
        // Nested: +[>+[<.>-]<-]
        // Index mapping (with LoopCheck):
        // 0: +
        // 1: [ (outer)
        // 2: LoopCheck (outer body start)
        // 3: >
        // 4: +
        // 5: [ (inner)
        // 6: LoopCheck (inner body start)
        // 7: <
        // 8: .
        // 9: >
        // 10: -
        // 11: <
        // 12: -
        let source = "+[>+[<.>-]<-]";
        let (_instructions, debug_info) = parse_with_debug(source).unwrap();

        // Verify both loops collected
        assert_eq!(debug_info.loop_count(), 2);

        // Outer loop
        let outer = debug_info.get_loop_metadata(1).unwrap();
        assert_eq!(outer.loop_start_index, 1);
        assert_eq!(outer.body_start_index, 2);
        assert_eq!(outer.body_size, 11); // LoopCheck + >+[<.>-]<- = 11 instructions
        assert_eq!(outer.parent_loop, None);

        // Inner loop
        let inner = debug_info.get_loop_metadata(5).unwrap();
        assert_eq!(inner.loop_start_index, 5);
        assert_eq!(inner.body_start_index, 6);
        assert_eq!(inner.body_size, 5); // LoopCheck + <.>- = 5 instructions
        assert_eq!(inner.parent_loop, Some(1)); // Parent is outer loop
    }

    #[test]
    fn test_triple_nested_loop_metadata() {
        // Triple nested: +++[>+[>+[>+<-]<-]<-]
        let source = "+++[>+[>+[>+<-]<-]<-]";
        let (_instructions, debug_info) = parse_with_debug(source).unwrap();

        // Verify all three loops collected
        assert_eq!(debug_info.loop_count(), 3);

        // Outer loop (index 3)
        let outer = debug_info.get_loop_metadata(3).unwrap();
        assert_eq!(outer.loop_start_index, 3);
        assert_eq!(outer.body_start_index, 4);
        assert_eq!(outer.body_size, 17); // LoopCheck + >+[>+[>+<-]<-]<- = 17 instructions
        assert_eq!(outer.parent_loop, None);

        // Middle loop (index 7)
        let middle = debug_info.get_loop_metadata(7).unwrap();
        assert_eq!(middle.loop_start_index, 7);
        assert_eq!(middle.body_start_index, 8);
        assert_eq!(middle.body_size, 11); // LoopCheck + >+[>+<-]<- = 11 instructions
        assert_eq!(middle.parent_loop, Some(3)); // Parent is outer

        // Inner loop (index 11)
        let inner = debug_info.get_loop_metadata(11).unwrap();
        assert_eq!(inner.loop_start_index, 11);
        assert_eq!(inner.body_start_index, 12);
        assert_eq!(inner.body_size, 5); // LoopCheck + >+<- = 5 instructions
        assert_eq!(inner.parent_loop, Some(7)); // Parent is middle
    }

    #[test]
    fn test_sibling_loops_metadata() {
        // Two sibling loops: +[>+<-]+[>-<+]
        // Index mapping:
        // 0: +
        // 1: [ (first loop)
        // 2-5: >+<- (body of first loop)
        // 6: +
        // 7: [ (second loop)
        // 8-11: >-<+ (body of second loop)
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
        let second = debug_info.get_loop_metadata(8).unwrap();
        assert_eq!(second.loop_start_index, 8);
        assert_eq!(second.body_size, 5); // LoopCheck + >-<+ = 5 instructions
        assert_eq!(second.parent_loop, None); // Also top-level
    }

    #[test]
    fn test_empty_loop_metadata() {
        // Empty loop: []
        let source = "[]";
        let (_instructions, debug_info) = parse_with_debug(source).unwrap();

        assert_eq!(debug_info.loop_count(), 1);

        let metadata = debug_info.get_loop_metadata(0).unwrap();
        assert_eq!(metadata.loop_start_index, 0);
        assert_eq!(metadata.body_start_index, 1);
        assert_eq!(metadata.body_size, 1); // Only LoopCheck (empty body)
        assert_eq!(metadata.parent_loop, None);
    }
}
