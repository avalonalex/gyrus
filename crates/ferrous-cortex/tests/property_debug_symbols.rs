//! Property-based tests for debug symbol tracking
//!
//! These tests verify invariants that should hold for ALL programs,
//! not just specific test cases.

use ferrous_cortex::{io::{DebugIo, StringIo}, *};
use proptest::prelude::*;

/// Generate a random sequence of non-bracket BF commands
fn arb_bf_commands() -> impl Strategy<Value = Vec<char>> {
    let bf_chars = prop::sample::select(vec!['+', '-', '>', '<', '.', ',', ' ', '\n']);
    prop::collection::vec(bf_chars, 0..10)
}

/// Generate random valid BrainFuck programs with balanced brackets
///
/// Uses a recursive strategy to ensure all brackets are properly balanced:
/// - Base case: just commands (no loops)
/// - Recursive case: commands, then optionally a loop containing a valid program
fn arb_bf_program() -> impl Strategy<Value = String> {
    let leaf = arb_bf_commands().prop_map(|cmds| cmds.iter().collect::<String>());

    leaf.prop_recursive(
        3,  // depth: max nesting level
        10, // size: desired number of nodes
        3,  // items per collection
        |inner| {
            (arb_bf_commands(), inner, any::<bool>()).prop_map(|(cmds, inner_program, add_loop)| {
                let mut s = cmds.iter().collect::<String>();
                // Randomly add a loop with the inner program
                if !inner_program.is_empty() && add_loop {
                    s.push('[');
                    s.push_str(&inner_program);
                    s.push(']');
                }
                s
            })
        }
    )
}

proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(50))]

    /// Property: Source locations must be within source bounds
    #[test]
    fn prop_source_locations_in_bounds(source in arb_bf_program()) {
        if let Ok((_instructions, debug_info)) = parse_with_debug(&source) {
            // Try to look up locations for first 100 instruction indices
            for instruction_idx in 0..100 {
                if let Some(loc) = debug_info.lookup(instruction_idx) {
                    prop_assert!(
                        loc.offset < source.len(),
                        "Location offset {} out of bounds (source len: {})",
                        loc.offset,
                        source.len()
                    );

                    prop_assert!(
                        loc.line >= 1,
                        "Line numbers should be 1-indexed, got {}",
                        loc.line
                    );

                    prop_assert!(
                        loc.column >= 1,
                        "Column numbers should be 1-indexed, got {}",
                        loc.column
                    );
                }
            }
        }
    }

    /// Property: Parsing with debug should never panic
    #[test]
    fn prop_parse_with_debug_never_panics(source in "\\PC*") {
        // This property test just ensures parsing doesn't panic
        // Even on invalid input, we should get a Result::Err, not a panic
        let _ = parse_with_debug(&source);
    }

    /// Property: All terminations (success, error, or step limit) should have debug info when appropriate
    ///
    /// This test embraces the halting problem: we don't care if the program completes,
    /// hits the step limit, or errors. We only care that when something goes wrong,
    /// we have debug information.
    ///
    /// Now that StepLimitExceeded has source location, hitting the limit is a GOOD outcome!
    #[test]
    fn prop_terminations_have_debug_info(source in arb_bf_program()) {
        let (instructions, debug_info) = parse_with_debug(&source).unwrap();

        // Aggressive limits to force quick termination
        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_max_steps(1000)  // Lower limit for faster tests
            .build();

        // Use DebugIo to provide infinite input and discard output
        // This prevents programs from blocking on ',' instruction
        let mut input = DebugIo::new();
        let mut output = DebugIo::new();
        let result = interpret_with_io(&instructions, config, &mut input, &mut output, Some(&debug_info));

        match result {
            Ok(_) => {
                // Program completed successfully - no debug info needed
            }
            Err(error) => {
                // Any runtime error should have source location when debug info is provided
                match error {
                    BfError::MemoryOutOfBounds { source_location, .. } => {
                        prop_assert!(
                            source_location.is_some(),
                            "MemoryOutOfBounds error without source location"
                        );
                    }
                    BfError::CellOverflow { source_location, .. } => {
                        prop_assert!(
                            source_location.is_some(),
                            "CellOverflow error without source location"
                        );
                    }
                    BfError::CellUnderflow { source_location, .. } => {
                        prop_assert!(
                            source_location.is_some(),
                            "CellUnderflow error without source location"
                        );
                    }
                    BfError::StepLimitExceeded { source_location, .. } => {
                        // NEW: Step limit now captures source location!
                        prop_assert!(
                            source_location.is_some(),
                            "StepLimitExceeded should have source location when debug info provided"
                        );
                    }
                    BfError::ExecutionTimeout { .. } => {
                        // Timeout is a meta-error, doesn't have source location
                    }
                    BfError::IoError { .. } | BfError::FileError { .. } => {
                        // I/O errors are system errors, don't require source locations
                    }
                    BfError::UnmatchedOpenBracket { .. }
                    | BfError::UnmatchedCloseBracket { .. }
                    | BfError::MultipleBracketErrors { .. } => {
                        // Parse errors shouldn't happen here (we already parsed)
                    }
                    BfError::ConfigurationError { .. } | BfError::ExecutionPaused { .. } => {
                        // Meta-errors, don't require source locations
                    }
                    _ => {
                        // Future error types - should have source locations if they're runtime errors
                    }
                }
            }
        }
    }
}

// Unit tests for specific debug symbol behavior
#[cfg(test)]
mod unit_tests {
    use super::*;

    /// Test that step limit properly terminates execution and we can track stats
    #[test]
    fn test_step_limit_terminates_with_stats() {
        // Infinite loop program
        let source = "+[>+]";
        let (instructions, debug_info) = parse_with_debug(source).unwrap();

        // Very low step limit to ensure quick termination
        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_max_steps(50)
            .build();

        let mut input = StringIo::empty();
        let mut output = StringIo::empty();

        let result = interpret_with_io(
            &instructions,
            config,
            &mut input,
            &mut output,
            Some(&debug_info),
        );

        // Should hit step limit (note: limit is checked AFTER step execution, so actual_steps = limit + 1)
        match result {
            Err(BfError::StepLimitExceeded {
                actual_steps,
                limit,
                source_location,
                instruction_index,
                ..
            }) => {
                assert_eq!(actual_steps.get(), 51); // Executed one more before detecting limit
                assert_eq!(limit, 50);

                // NEW: Verify we have debug information about where we stopped!
                assert!(
                    source_location.is_some(),
                    "Step limit error should have source location when debug info provided"
                );
                let loc = source_location.unwrap();
                assert_eq!(loc.line, 1); // Single line program
                assert!(loc.column >= 1); // Valid column
                assert!(loc.offset < source.len()); // Within source bounds
                // instruction_index should be in the loop body (> or + instruction)
                assert!(
                    instruction_index == 2 || instruction_index == 3,
                    "Should be at > or + in loop body, got {}",
                    instruction_index
                );
            }
            other => panic!("Expected StepLimitExceeded, got: {:?}", other),
        }
    }

    /// Test that when execution is aborted by step limit, stats are still available
    #[test]
    fn test_step_limit_preserves_execution_state() {
        // Program that moves right repeatedly: >>>>>>>>>
        let source = ">".repeat(100);
        let (instructions, debug_info) = parse_with_debug(&source).unwrap();

        // Limit to 10 steps
        let config = ExecutionConfigBuilder::new()
            .with_memory_size(1000)
            .with_max_steps(10)
            .build();

        let mut input = StringIo::empty();
        let mut output = StringIo::empty();

        // Even though it hits the limit, we should know where we stopped
        let result = interpret_with_io(
            &instructions,
            config,
            &mut input,
            &mut output,
            Some(&debug_info),
        );

        assert!(result.is_err());
        match result {
            Err(BfError::StepLimitExceeded {
                actual_steps,
                source_location,
                ..
            }) => {
                // Limit is checked after step execution, so we get 11 steps for a limit of 10
                assert_eq!(actual_steps.get(), 11);

                // Verify debug information captured where execution stopped
                assert!(
                    source_location.is_some(),
                    "Step limit should capture where execution stopped"
                );
            }
            other => panic!("Expected StepLimitExceeded, got: {:?}", other),
        }
    }

    /// Test that runtime errors have source locations when debug info is provided
    #[test]
    fn test_runtime_error_has_source_location() {
        // Program that will cause memory overflow
        let source = ">".repeat(200);
        let (instructions, debug_info) = parse_with_debug(&source).unwrap();

        let config = ExecutionConfigBuilder::new()
            .with_memory_size(100)
            .with_max_steps(1000)
            .build();

        let mut input = StringIo::empty();
        let mut output = StringIo::empty();

        let result = interpret_with_io(
            &instructions,
            config,
            &mut input,
            &mut output,
            Some(&debug_info),
        );

        match result {
            Err(BfError::MemoryOutOfBounds {
                source_location, ..
            }) => {
                assert!(
                    source_location.is_some(),
                    "MemoryOutOfBounds should have source location when debug info provided"
                );

                let loc = source_location.unwrap();
                assert!(loc.offset < source.len());
                assert!(loc.line >= 1);
                assert!(loc.column >= 1);
            }
            other => panic!("Expected MemoryOutOfBounds, got: {:?}", other),
        }
    }
}
