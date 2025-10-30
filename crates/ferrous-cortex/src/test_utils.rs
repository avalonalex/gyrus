//! Test utilities for BrainFuck interpreter testing.
//!
//! This module provides helper functions and utilities for writing tests.
//! Only available in test builds.

use crate::config::{EofBehavior, ExecutionConfig, ExecutionConfigBuilder, MemoryModel};
use crate::error::BfError;
use crate::interpreter::interpret_with_io;
use crate::io::StringIo;
use crate::parser::parse;
use crate::stats::ExecutionStats;

/// Run a BrainFuck program with string input and capture output.
///
/// # Examples
///
/// ```
/// use ferrous_cortex::test_utils::run_bf;
///
/// let (output, stats) = run_bf("++++++++[>++++<-]>.", "").unwrap();
/// assert_eq!(output, " "); // ASCII 32
/// ```
pub fn run_bf(source: &str, input: &str) -> Result<(String, ExecutionStats), BfError> {
    let instructions = parse(source)?;
    let config = ExecutionConfig::default();
    let mut input_io = StringIo::new(input);
    let mut output_io = StringIo::empty();

    let stats = interpret_with_io(&instructions, config, &mut input_io, &mut output_io, None)?;
    Ok((output_io.output_string(), stats))
}

/// Run a BrainFuck program with custom config.
///
/// # Examples
///
/// ```
/// use ferrous_cortex::test_utils::{run_bf_with_config, configs};
///
/// let (output, stats) = run_bf_with_config(
///     "+[>+]",
///     "",
///     configs::with_step_limit(100)
/// );
/// assert!(output.is_err()); // Should hit step limit
/// ```
pub fn run_bf_with_config(
    source: &str,
    input: &str,
    config: ExecutionConfig,
) -> Result<(String, ExecutionStats), BfError> {
    let instructions = parse(source)?;
    let mut input_io = StringIo::new(input);
    let mut output_io = StringIo::empty();

    let stats = interpret_with_io(&instructions, config, &mut input_io, &mut output_io, None)?;
    Ok((output_io.output_string(), stats))
}

/// Run and expect success.
///
/// # Panics
///
/// Panics if execution fails.
pub fn run_bf_expect_ok(source: &str, input: &str) -> (String, ExecutionStats) {
    run_bf(source, input).expect("BF execution should succeed")
}

/// Run and expect failure.
///
/// # Panics
///
/// Panics if execution succeeds.
pub fn run_bf_expect_err(source: &str, input: &str) -> BfError {
    run_bf(source, input).expect_err("BF execution should fail")
}

/// Assert that two BrainFuck programs produce the same output given the same input.
///
/// # Panics
///
/// Panics if the programs produce different output.
pub fn assert_bf_equivalent(source1: &str, source2: &str, input: &str) {
    let (output1, _) = run_bf_expect_ok(source1, input);
    let (output2, _) = run_bf_expect_ok(source2, input);
    assert_eq!(
        output1, output2,
        "Programs should produce identical output.\nProgram 1: {}\nProgram 2: {}",
        source1, source2
    );
}

/// Common test configurations.
pub mod configs {
    use super::*;

    /// Tiny memory configuration (10 cells).
    pub fn tiny_memory() -> ExecutionConfig {
        ExecutionConfigBuilder::new().with_memory_size(10).build()
    }

    /// Small memory configuration (100 cells).
    pub fn small_memory() -> ExecutionConfig {
        ExecutionConfigBuilder::new().with_memory_size(100).build()
    }

    /// Configuration with step limit.
    pub fn with_step_limit(limit: u64) -> ExecutionConfig {
        ExecutionConfigBuilder::new()
            .with_memory_size(30000)
            .with_max_steps(limit)
            .build()
    }

    /// Configuration with timeout.
    pub fn with_timeout(ms: u64) -> ExecutionConfig {
        ExecutionConfigBuilder::new()
            .with_memory_size(30000)
            .with_timeout_ms(ms)
            .build()
    }

    /// Configuration with unbounded memory.
    ///
    /// # Panics
    ///
    /// Panics if initial > max.
    pub fn unbounded_memory(initial: usize, max: usize) -> ExecutionConfig {
        ExecutionConfigBuilder::new()
            .with_unbounded_memory(initial, max)
            .expect("Invalid unbounded memory configuration")
            .build()
    }

    /// Configuration with specific EOF behavior.
    pub fn with_eof_behavior(behavior: EofBehavior) -> ExecutionConfig {
        ExecutionConfigBuilder::new()
            .with_memory_size(30000)
            .with_eof_behavior(behavior)
            .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_bf_simple() {
        let (output, stats) = run_bf("+++.", "").unwrap();
        assert_eq!(output, "\u{3}"); // ASCII 3
        assert_eq!(stats.total_steps.get(), 4);
    }

    #[test]
    fn test_run_bf_with_input() {
        let (output, _) = run_bf(",.", "A").unwrap();
        assert_eq!(output, "A");
    }

    #[test]
    fn test_run_bf_expect_ok() {
        let (output, _) = run_bf_expect_ok("++++++++[>++++<-]>.", "");
        assert_eq!(output, " "); // ASCII 32
    }

    #[test]
    #[should_panic(expected = "BF execution should succeed")]
    fn test_run_bf_expect_ok_panics_on_error() {
        run_bf_expect_ok("[", ""); // Unmatched bracket
    }

    #[test]
    fn test_run_bf_expect_err() {
        let err = run_bf_expect_err("[", "");
        assert!(matches!(err, BfError::UnmatchedOpenBracket { .. }));
    }

    #[test]
    #[should_panic(expected = "BF execution should fail")]
    fn test_run_bf_expect_err_panics_on_success() {
        run_bf_expect_err("+++.", ""); // Valid program
    }

    #[test]
    fn test_assert_bf_equivalent() {
        // These should produce the same output
        assert_bf_equivalent("+++.", "+++++--.", ""); // 3 = 5 - 2
    }

    #[test]
    #[should_panic(expected = "Programs should produce identical output")]
    fn test_assert_bf_equivalent_fails_on_difference() {
        assert_bf_equivalent("+++.", "++.", ""); // Different output
    }

    #[test]
    fn test_config_tiny_memory() {
        let config = configs::tiny_memory();
        // Just verify it creates a valid config
        assert!(matches!(config.memory_model(), MemoryModel::Fixed(_)));
    }

    #[test]
    fn test_config_with_step_limit() {
        let err = run_bf_with_config("+[>+]", "", configs::with_step_limit(100)).unwrap_err();
        assert!(matches!(err, BfError::StepLimitExceeded { .. }));
    }

    #[test]
    fn test_config_eof_behavior() {
        let config = configs::with_eof_behavior(EofBehavior::SetZero);
        assert_eq!(config.eof_behavior(), EofBehavior::SetZero);
    }
}
