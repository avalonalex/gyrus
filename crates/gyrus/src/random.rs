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
//! Two generators: [`generate_random_program`] flips coins over the commands,
//! for the parser and the error paths; [`generate_idiomatic_program`] composes
//! the idioms real programs are written in -- clears, multiplies, scans,
//! counted loops -- and keeps its programs on the tape and terminating, so
//! that engines can be compared on what they *compute*.
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

/// Configuration for idiomatic program generation. See
/// [`generate_idiomatic_program`].
#[derive(Debug, Clone)]
pub struct IdiomaticConfig {
    /// Cells the program may use; every move stays inside `0..tape`.
    pub tape: usize,
    /// Top-level fragments to emit.
    pub fragments: usize,
    /// Maximum nesting of counted loops.
    pub max_depth: usize,
}

impl Default for IdiomaticConfig {
    fn default() -> Self {
        Self {
            tape: 32,
            fragments: 20,
            max_depth: 3,
        }
    }
}

/// Generate a program made of the idioms real BrainFuck is written in.
///
/// [`generate_random_program`] flips coins over the eight commands, which
/// exercises the parser and the error paths but almost never produces a
/// clear loop, a multiply loop, a scan or a counted loop -- the shapes the
/// optimizer folds and the JIT compiles specially -- and most of what it
/// produces walks off a small tape within a few steps. This generator
/// composes those idioms, keeps the cursor on the tape by construction
/// (tracking where it can be as an interval, since a scan's landing cell is
/// not known statically), and bounds every loop it writes, so its programs
/// complete and their output can be compared between engines.
///
/// Fragments: `[-]` clears, optionally with a value after; runs of `+`/`-`
/// and moves; `[->+++>+<<]`-style multiplies with the decrement first or
/// last and up to three targets; `[>>>]`-style scans with a zero cell
/// planted within reach; `[-]++++[ ... - ]` counted loops whose bodies never
/// touch the counter; and I/O. One fragment in ten is written in a way that
/// checked cells reject (`[-]---`), so that path is exercised too.
///
/// ```
/// use gyrus::random::{IdiomaticConfig, generate_idiomatic_program};
/// use gyrus::parse;
///
/// let program = generate_idiomatic_program(&mut rand::rng(), &IdiomaticConfig::default());
/// assert!(parse(&program).is_ok());
/// ```
pub fn generate_idiomatic_program<R: Rng>(rng: &mut R, config: &IdiomaticConfig) -> String {
    let mut g = Idioms {
        rng,
        config,
        out: String::new(),
        lo: 0,
        hi: 0,
        wall: 0,
    };
    for _ in 0..config.fragments {
        g.fragment(0);
    }
    g.out
}

struct Idioms<'a, R: Rng> {
    rng: &'a mut R,
    config: &'a IdiomaticConfig,
    out: String,
    /// Where the cursor can be: exact after balanced fragments, an interval
    /// after a scan. Both ends always inside the tape.
    lo: usize,
    hi: usize,
    /// Inside a counted loop's body, the leftmost cell the body may use --
    /// one past the counter, which the body must not touch.
    wall: usize,
}

impl<R: Rng> Idioms<'_, R> {
    fn room_right(&self) -> usize {
        self.config.tape - 1 - self.hi
    }

    fn room_left(&self) -> usize {
        self.lo - self.wall
    }

    fn exact(&self) -> bool {
        self.lo == self.hi
    }

    /// Emit a move; the caller has checked the room.
    fn shift(&mut self, delta: isize) {
        let c = if delta > 0 { '>' } else { '<' };
        for _ in 0..delta.unsigned_abs() {
            self.out.push(c);
        }
        self.lo = (self.lo as isize + delta) as usize;
        self.hi = (self.hi as isize + delta) as usize;
    }

    fn repeat(&mut self, c: char, n: usize) {
        for _ in 0..n {
            self.out.push(c);
        }
    }

    fn fragment(&mut self, depth: usize) {
        match self.rng.random_range(0..10) {
            0 | 1 => self.set(),
            2 | 3 => self.run(),
            4 | 5 => self.multiply(),
            6 => self.scan(),
            7 if depth < self.config.max_depth && self.exact() => self.counted_loop(depth),
            7 => self.run(),
            8 => self.out.push('.'),
            _ => self.out.push(','),
        }
    }

    /// `[-]`, usually followed by a value.
    fn set(&mut self) {
        self.out.push_str("[-]");
        match self.rng.random_range(0..10) {
            0 => {}
            // Written with `-`: fine wrapping, an underflow under checked cells.
            1 => {
                let n = self.rng.random_range(1..=5);
                self.repeat('-', n);
            }
            _ => {
                let n = self.rng.random_range(1..=12);
                self.repeat('+', n);
            }
        }
    }

    /// A few arithmetic runs and moves, staying on the tape.
    fn run(&mut self) {
        for _ in 0..self.rng.random_range(1..=4) {
            if self.rng.random_bool(0.5) {
                let c = if self.rng.random_bool(0.6) { '+' } else { '-' };
                let n = self.rng.random_range(1..=9);
                self.repeat(c, n);
            } else {
                let right = self.room_right().min(3) as i64;
                let left = self.room_left().min(3) as i64;
                let delta = self.rng.random_range(-left..=right) as isize;
                if delta != 0 {
                    self.shift(delta);
                }
            }
        }
    }

    /// `[->+++>+<<]`, with the decrement first or last, sometimes `+` on the
    /// source. Balanced, so the cursor interval is unchanged.
    fn multiply(&mut self) {
        let room = self.room_right().min(4);
        if room == 0 {
            return self.run();
        }
        let count = self.rng.random_range(1..=room.min(3));
        let mut offsets: Vec<usize> = (1..=room).collect();
        // A random subset, in order.
        while offsets.len() > count {
            let k = self.rng.random_range(0..offsets.len());
            offsets.remove(k);
        }
        let source = if self.rng.random_bool(0.9) { '-' } else { '+' };
        let first = self.rng.random_bool(0.5);
        self.out.push('[');
        if first {
            self.out.push(source);
        }
        let mut pos = 0;
        for off in &offsets {
            self.repeat('>', off - pos);
            pos = *off;
            let c = if self.rng.random_bool(0.8) { '+' } else { '-' };
            let n = self.rng.random_range(1..=5);
            self.repeat(c, n);
        }
        self.repeat('<', pos);
        if !first {
            self.out.push(source);
        }
        self.out.push(']');
    }

    /// `[>>>]` with a zero planted within reach, so it stops on the tape.
    /// Where it stops depends on the cells between, so the cursor becomes an
    /// interval. Not used inside a counted loop's body, where the cursor
    /// must stay exact.
    fn scan(&mut self) {
        if self.wall > 0 {
            return self.run();
        }
        let right = self.rng.random_bool(0.6);
        let room = if right {
            self.room_right()
        } else {
            self.room_left()
        };
        let stride = self.rng.random_range(1..=3.min(room.max(1)));
        if stride > room {
            return self.run();
        }
        let reach = stride * self.rng.random_range(1..=(room / stride).min(3));
        let (there, back) = if right { ('>', '<') } else { ('<', '>') };
        // Plant the zero.
        self.repeat(there, reach);
        self.out.push_str("[-]");
        self.repeat(back, reach);
        // Scan.
        self.out.push('[');
        self.repeat(there, stride);
        self.out.push(']');
        if right {
            self.hi += reach;
        } else {
            self.lo -= reach;
        }
    }

    /// `[-]+++[ body - ]`: runs exactly `n` times because the body never
    /// touches the counter, which it keeps to the left of a wall.
    fn counted_loop(&mut self, depth: usize) {
        if self.room_right() < 2 {
            return self.run();
        }
        let n = self.rng.random_range(1..=6);
        self.out.push_str("[-]");
        self.repeat('+', n);
        self.out.push('[');
        let counter = self.lo;
        let outer_wall = self.wall;
        self.shift(1);
        self.wall = counter + 1;
        for _ in 0..self.rng.random_range(1..=3) {
            self.fragment(depth + 1);
        }
        debug_assert!(self.exact(), "a loop body keeps the cursor exact");
        self.shift(counter as isize - self.lo as isize);
        self.wall = outer_wall;
        self.out.push_str("-]");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    /// Idiomatic programs parse, keep their cursor on the tape they were
    /// written for, and -- under wrapping cells, where nothing they write is
    /// an error -- complete.
    #[test]
    fn idiomatic_programs_complete_on_their_tape() {
        use crate::ExecutionConfigBuilder;
        use crate::io::StringIo;
        let config = IdiomaticConfig::default();
        let mut completed = 0;
        for seed in 0..200 {
            let mut rng = StdRng::seed_from_u64(seed);
            let program = generate_idiomatic_program(&mut rng, &config);
            let instructions = parse(&program).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
            let run = ExecutionConfigBuilder::new()
                .with_memory_size(config.tape)
                .with_max_steps(2_000_000)
                .build();
            let (mut i, mut o) = (StringIo::new("input"), StringIo::empty());
            match crate::interpret_with_io(&instructions, run, &mut i, &mut o, None) {
                Ok(_) => completed += 1,
                Err(e) => panic!("seed {seed} failed: {e}\n{program}"),
            }
        }
        assert_eq!(completed, 200);
    }

    #[test]
    fn idiomatic_generation_is_deterministic_for_a_seed() {
        let config = IdiomaticConfig::default();
        let a = generate_idiomatic_program(&mut StdRng::seed_from_u64(42), &config);
        let b = generate_idiomatic_program(&mut StdRng::seed_from_u64(42), &config);
        assert_eq!(a, b);
    }

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
