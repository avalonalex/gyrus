//! Random BrainFuck program generation.
//!
//! Generates syntactically valid BrainFuck — brackets are always balanced,
//! because the generator emits loops as whole `[...]` units rather than
//! emitting `[` and `]` independently and hoping they match.
//!
//! This backs `gyrus-tool generate`, and is useful for fuzzing the parser and
//! optimizer, producing benchmark inputs, and generating teaching examples.
//! A generated program is valid, not meaningful: it will parse, but it has no
//! intended output and may well loop forever. Run generated programs under a
//! step limit or timeout.
//!
//! Requires the `random` feature.
//!
//! ```
//! use gyrus::random::{RandomProgramConfig, generate_random_program};
//! use gyrus::parse;
//!
//! let program = generate_random_program(&mut rand::rng(), &RandomProgramConfig::default());
//! assert!(parse(&program).is_ok());
//! ```

use rand::Rng;

/// Configuration for random program generation.
#[derive(Debug, Clone)]
pub struct RandomProgramConfig {
    /// Maximum nesting depth for loops.
    pub max_depth: usize,
    /// Upper bound on the number of commands emitted per nesting level.
    pub avg_commands: usize,
    /// Probability of opening a loop at each level (0.0 to 1.0).
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
/// use gyrus::random::{RandomProgramConfig, generate_random_program};
/// use gyrus::parse;
///
/// let config = RandomProgramConfig {
///     max_depth: 2,
///     avg_commands: 5,
///     loop_probability: 0.5,
/// };
/// let program = generate_random_program(&mut rand::rng(), &config);
/// assert!(parse(&program).is_ok());
/// ```
pub fn generate_random_program<R: Rng>(rng: &mut R, config: &RandomProgramConfig) -> String {
    generate_recursive(rng, config, 0)
}

fn generate_recursive<R: Rng>(rng: &mut R, config: &RandomProgramConfig, depth: usize) -> String {
    let mut result = String::new();

    // Generate random commands
    let num_commands = rng.random_range(0..config.avg_commands);
    for _ in 0..num_commands {
        let cmd = match rng.random_range(0..8) {
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
    if depth < config.max_depth && rng.random_bool(config.loop_probability) {
        result.push('[');
        result.push_str(&generate_recursive(rng, config, depth + 1));
        result.push(']');
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    /// The generator's one hard promise: whatever it emits, `parse` accepts.
    #[test]
    fn generated_programs_always_parse() {
        let config = RandomProgramConfig::default();
        for seed in 0..200 {
            let mut rng = StdRng::seed_from_u64(seed);
            let program = generate_random_program(&mut rng, &config);
            assert!(
                parse(&program).is_ok(),
                "seed {seed} produced an unparseable program: {program:?}"
            );
        }
    }

    #[test]
    fn respects_max_depth() {
        let config = RandomProgramConfig {
            max_depth: 2,
            avg_commands: 4,
            loop_probability: 1.0,
        };
        for seed in 0..50 {
            let mut rng = StdRng::seed_from_u64(seed);
            let program = generate_random_program(&mut rng, &config);
            let mut depth = 0usize;
            let mut deepest = 0usize;
            for c in program.chars() {
                match c {
                    '[' => {
                        depth += 1;
                        deepest = deepest.max(depth);
                    }
                    ']' => depth -= 1,
                    _ => {}
                }
            }
            assert_eq!(depth, 0, "unbalanced brackets at seed {seed}");
            assert!(
                deepest <= config.max_depth,
                "seed {seed} nested {deepest} deep"
            );
        }
    }

    #[test]
    fn zero_loop_probability_emits_no_loops() {
        let config = RandomProgramConfig {
            max_depth: 5,
            avg_commands: 10,
            loop_probability: 0.0,
        };
        let mut rng = StdRng::seed_from_u64(7);
        let program = generate_random_program(&mut rng, &config);
        assert!(!program.contains('['), "expected no loops, got {program:?}");
    }

    /// Generation is a pure function of the seed, so a failing fuzz case can be
    /// reproduced from its seed alone.
    #[test]
    fn generation_is_deterministic_for_a_seed() {
        let config = RandomProgramConfig::default();
        let a = generate_random_program(&mut StdRng::seed_from_u64(42), &config);
        let b = generate_random_program(&mut StdRng::seed_from_u64(42), &config);
        assert_eq!(a, b);
    }
}
