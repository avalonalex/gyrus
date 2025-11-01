//! Test utilities for BrainFuck interpreter testing.
//!
//! This module provides helper functions and utilities for writing tests.
//! Only available in test builds.

use crate::config::{EofBehavior, ExecutionConfig, ExecutionConfigBuilder};
use crate::error::BfError;
use crate::interpreter::interpret_with_io;
use crate::io::StringIo;
use crate::parser::parse;
use crate::stats::ExecutionStats;

// Only used in tests
#[cfg(test)]
use crate::config::MemoryModel;

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
/// let result = run_bf_with_config(
///     "+[>+]",
///     "",
///     configs::with_step_limit(100)
/// );
/// assert!(result.is_err()); // Should hit step limit
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

/// Random BrainFuck program generation.
///
/// This module provides utilities for generating random BrainFuck programs,
/// useful for fuzzing, benchmarking, testing, and educational purposes.
pub mod random {
    use rand::Rng;

    /// Configuration for random program generation.
    #[derive(Debug, Clone)]
    pub struct RandomProgramConfig {
        /// Maximum nesting depth for loops
        pub max_depth: usize,
        /// Average number of commands per sequence
        pub avg_commands: usize,
        /// Probability of adding a loop (0.0 to 1.0)
        pub loop_probability: f64,
    }

    impl Default for RandomProgramConfig {
        fn default() -> Self {
            Self {
                max_depth: 3,
                avg_commands: 10,
                loop_probability: 0.3,
            }
        }
    }

    /// Generate a random valid BrainFuck program with balanced brackets.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrous_cortex::test_utils::random::{generate_random_program, RandomProgramConfig};
    /// use ferrous_cortex::parse;
    ///
    /// let program = generate_random_program(&mut rand::thread_rng(), &RandomProgramConfig::default());
    /// // The generated program should always parse successfully
    /// assert!(parse(&program).is_ok());
    /// ```
    pub fn generate_random_program<R: Rng>(rng: &mut R, config: &RandomProgramConfig) -> String {
        generate_recursive(rng, config, 0)
    }

    fn generate_recursive<R: Rng>(
        rng: &mut R,
        config: &RandomProgramConfig,
        depth: usize,
    ) -> String {
        let mut result = String::new();

        // Generate random commands
        let num_commands = rng.gen_range(0..config.avg_commands);
        for _ in 0..num_commands {
            let cmd = match rng.gen_range(0..8) {
                0 => '+',
                1 => '-',
                2 => '>',
                3 => '<',
                4 => '.',
                5 => ',',
                6 => ' ',
                _ => '\n',
            };
            result.push(cmd);
        }

        // Maybe add a loop if we haven't reached max depth
        if depth < config.max_depth && rng.gen_bool(config.loop_probability) {
            result.push('[');
            result.push_str(&generate_recursive(rng, config, depth + 1));
            result.push(']');
        }

        result
    }
}

/// Proptest strategies for property-based testing.
///
/// These are only available in test builds since proptest is a dev-dependency.
#[cfg(test)]
pub mod proptest_strategies {
    use proptest::prelude::*;

    /// Generate a random sequence of non-bracket BF commands
    fn arb_bf_commands() -> impl Strategy<Value = Vec<char>> {
        let bf_chars = prop::sample::select(vec!['+', '-', '>', '<', '.', ',', ' ', '\n']);
        prop::collection::vec(bf_chars, 0..10)
    }

    /// Generate random valid BrainFuck programs with balanced brackets.
    ///
    /// Uses a recursive strategy to ensure all brackets are properly balanced.
    pub fn arb_bf_program() -> impl Strategy<Value = String> {
        let leaf = arb_bf_commands().prop_map(|cmds| cmds.iter().collect::<String>());

        leaf.prop_recursive(
            3,  // depth: max nesting level
            10, // size: desired number of nodes
            3,  // items per collection
            |inner| {
                (arb_bf_commands(), inner, any::<bool>()).prop_map(
                    |(cmds, inner_program, add_loop)| {
                        let mut s = cmds.iter().collect::<String>();
                        // Randomly add a loop with the inner program
                        if !inner_program.is_empty() && add_loop {
                            s.push('[');
                            s.push_str(&inner_program);
                            s.push(']');
                        }
                        s
                    },
                )
            },
        )
    }
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
