//! Generated programs, the optimizer against the tree-walker.
//!
//! The tree-walker executes the AST as written: no fusion, no folded loops,
//! nothing clever. That makes it the reference, and every optimizer fold is a
//! claim that the fast path computes what the reference computes. This test
//! puts that claim to randomly generated programs rather than to examples
//! chosen by whoever wrote the fold.
//!
//! `gyrus-jit` has a harness of the same shape that adds the JIT as a third
//! engine. This one deliberately does not need it: the optimizer lives in this
//! crate, so `cargo test -p gyrus` should be able to falsify it on its own.
//! Before this existed, changing `optimizer.rs` and running the library's own
//! tests exercised the fold against nothing but hand-written cases.
//!
//! Both engines run through `StringIo`, so a program's input and output are
//! plain bytes and the comparison is exact.

use gyrus::codegen::compile_string;
use gyrus::io::StringIo;
use gyrus::random::{
    IdiomaticConfig, RandomProgramConfig, generate_idiomatic_program, generate_random_program,
};
use gyrus::{
    BfError, ExecutionConfig, ExecutionConfigBuilder, interpret_optimized_with_io,
    interpret_with_io, optimize_with_cell_model, parse,
};
use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;

/// Enough to falsify a fold in this crate's own suite. The deeper sweep --
/// four times the seeds, with the JIT as a third engine -- is
/// `gyrus-jit/tests/generated.rs`; duplicating its depth here would only make
/// `cargo test -p gyrus` slower without testing anything it does not.
const SEEDS: u64 = 100;

/// Whatever a `,` reads, both engines read the same thing.
const INPUT: &str = "hello, world";

/// Step budgets are not comparable between the engines -- the tree-walker
/// counts source instructions and the optimized interpreter counts fused ones,
/// so the same number means less work on one side than the other. A run that
/// exhausts either budget is therefore skipped rather than compared; the budget
/// exists only so a generated infinite loop fails by name instead of hanging.
const STEPS: u64 = 500_000;

/// The tree-walker's budget, scaled up.
///
/// It counts source instructions where the optimized interpreter counts fused
/// ones, so the same number buys it far less work. Giving both the same budget
/// does not make the test stricter -- it just starves the reference first and
/// skips the comparison, losing exactly the long-running programs that are
/// most worth comparing.
const REFERENCE_SCALE: u64 = 64;

/// What a run came to, in the terms the two engines share.
#[derive(Debug, PartialEq)]
enum Outcome {
    Output(Vec<u8>),
    OutOfBounds {
        attempted: isize,
        output: Vec<u8>,
    },
    Overflow(Vec<u8>),
    Underflow(Vec<u8>),
    Io(Vec<u8>),
    /// Not comparable: see `STEPS`.
    Limit,
}

fn outcome(result: Result<(), BfError>, output: Vec<u8>) -> Outcome {
    match result {
        Ok(()) => Outcome::Output(output),
        Err(BfError::MemoryOutOfBounds { attempted, .. }) => {
            Outcome::OutOfBounds { attempted, output }
        }
        Err(BfError::CellOverflow { .. }) => Outcome::Overflow(output),
        Err(BfError::CellUnderflow { .. }) => Outcome::Underflow(output),
        Err(BfError::IoError { .. }) => Outcome::Io(output),
        Err(BfError::StepLimitExceeded { .. } | BfError::ExecutionTimeout { .. }) => Outcome::Limit,
        Err(other) => panic!("unexpected error {other:?}"),
    }
}

type Build = fn(u64) -> ExecutionConfig;

/// Both memory models against both cell models: a fold that is only valid
/// under wrapping cells has to be caught here, not by a reader.
fn configurations() -> [(&'static str, Build); 4] {
    [
        ("fixed/wrapping", |max| {
            ExecutionConfigBuilder::new()
                .with_memory_size(32)
                .with_max_steps(max)
                .build()
        }),
        ("fixed/checked", |max| {
            ExecutionConfigBuilder::new()
                .with_memory_size(32)
                .with_checked_cells()
                .with_max_steps(max)
                .build()
        }),
        ("unbounded/wrapping", |max| {
            ExecutionConfigBuilder::new()
                .with_unbounded_memory(8, 64)
                .unwrap()
                .with_max_steps(max)
                .build()
        }),
        ("unbounded/checked", |max| {
            ExecutionConfigBuilder::new()
                .with_unbounded_memory(8, 64)
                .unwrap()
                .with_checked_cells()
                .with_max_steps(max)
                .build()
        }),
    ]
}

/// Run one program through both engines under one configuration.
///
/// Returns `None` when the comparison is not meaningful -- either engine hit
/// its budget -- and `Some((reference, optimized))` otherwise.
fn compare(source: &str, build: Build) -> Option<(Outcome, Outcome)> {
    let instructions = parse(source).expect("the generator emits balanced programs");

    let reference = {
        let (mut i, mut o) = (StringIo::new(INPUT), StringIo::empty());
        let config = build(STEPS * REFERENCE_SCALE);
        let r = interpret_with_io(&instructions, config, &mut i, &mut o, None);
        outcome(r.map(|_| ()), o.output_bytes().to_vec())
    };

    let config = build(STEPS);
    let program = optimize_with_cell_model(&instructions, *config.cell_model());
    let optimized = {
        let (mut i, mut o) = (StringIo::new(INPUT), StringIo::empty());
        let r = interpret_optimized_with_io(&program, config, &mut i, &mut o);
        outcome(r.map(|_| ()), o.output_bytes().to_vec())
    };

    if reference == Outcome::Limit || optimized == Outcome::Limit {
        return None;
    }
    Some((reference, optimized))
}

fn check_all(programs: &[String], what: &str) {
    let (mut compared, mut skipped) = (0usize, 0usize);
    for (seed, source) in programs.iter().enumerate() {
        for (name, build) in configurations() {
            match compare(source, build) {
                None => skipped += 1,
                Some((reference, optimized)) => {
                    compared += 1;
                    assert_eq!(
                        optimized, reference,
                        "{what} program {seed} under {name} disagrees with the tree-walker\n  \
                         source: {source:?}"
                    );
                }
            }
        }
    }
    eprintln!("{what}: compared {compared} runs, skipped {skipped} at the step budget");
    // Most runs must actually be compared. A looser floor lets a future change
    // to the generator or the budget quietly gut the coverage while the test
    // still reports success.
    let possible = programs.len() * configurations().len();
    assert!(
        compared * 4 >= possible * 3,
        "{what}: only {compared} of {possible} runs compared -- the budget is starving the test"
    );
}

/// Uniform instruction soup. Good at finding crashes and boundary handling,
/// less good at reaching the optimizer's folds, since a random program rarely
/// spells `[->+<]`.
#[test]
fn random_programs_agree_with_the_tree_walker() {
    let shape = RandomProgramConfig::default();
    let programs: Vec<String> = (0..SEEDS)
        .map(|seed| generate_random_program(&mut StdRng::seed_from_u64(seed), &shape))
        .collect();
    check_all(&programs, "random");
}

/// Programs built out of the idioms the optimizer actually rewrites -- clears,
/// sets, multiplies in both rotations, strided scans. These are the ones that
/// exercise a fold rather than stepping around it, and the reason the
/// generator has an idiomatic mode at all.
#[test]
fn idiomatic_programs_agree_with_the_tree_walker() {
    let shape = IdiomaticConfig {
        tape: 32,
        fragments: 24,
        max_depth: 3,
    };
    let programs: Vec<String> = (0..SEEDS)
        .map(|seed| generate_idiomatic_program(&mut StdRng::seed_from_u64(seed), &shape))
        .collect();
    check_all(&programs, "idiomatic");
}

/// Programs whose correct output is known in advance.
///
/// Everything above compares engines to each other, which proves they agree.
/// Agreement is not correctness: a fold that is wrong in the same way on both
/// sides passes every differential in this file. `compile_string` closes that
/// gap -- it emits a program that prints a given string, so the right answer
/// is known by construction rather than by asking another engine.
///
/// The compiled programs are also worth running for their shape: codegen
/// builds values with multiply loops and clears, which is precisely what the
/// optimizer folds, so these exercise the folds against an independent oracle.
#[test]
fn compiled_programs_print_the_string_they_were_built_from() {
    let mut rng = StdRng::seed_from_u64(0xC0DE);
    for case in 0..SEEDS {
        // Printable ASCII, including the characters that are also BrainFuck
        // instructions -- codegen has to emit those as data, not as code.
        let len = rng.random_range(1..=40usize);
        let text: String = (0..len)
            .map(|_| char::from(rng.random_range(0x20..=0x7Eu8)))
            .collect();

        let program = compile_string(&text);
        let instructions = parse(&program)
            .unwrap_or_else(|e| panic!("case {case}: codegen emitted unparsable BrainFuck: {e}"));

        // Its own configurations rather than the differential's above, which
        // use a deliberately tiny 32-cell tape to provoke boundary errors.
        // Codegen walks rightwards as it builds a string, so a 40-character
        // string runs off a 32-cell tape -- a fact about the tape, not about
        // codegen.
        //
        // Wrapping only, and not for convenience: codegen *targets* wrapping
        // cells. Its table reaches 255 by decrementing a zero cell --
        // `table[0][255]` is `"-"`, asserted in codegen's own unit tests -- so
        // a compiled program raises a checked-cell underflow at its first
        // instruction by construction.
        let configurations: [(&str, Build); 2] = [
            ("fixed/wrapping", |max| {
                ExecutionConfigBuilder::new()
                    .with_memory_size(30_000)
                    .with_max_steps(max)
                    .build()
            }),
            ("unbounded/wrapping", |max| {
                ExecutionConfigBuilder::new()
                    .with_unbounded_memory(64, 30_000)
                    .unwrap()
                    .with_max_steps(max)
                    .build()
            }),
        ];
        for (name, build) in configurations {
            let config = build(STEPS);
            let program = optimize_with_cell_model(&instructions, *config.cell_model());
            let (mut i, mut o) = (StringIo::new(""), StringIo::empty());
            let result = interpret_optimized_with_io(&program, config, &mut i, &mut o);
            let got = o.output_bytes().to_vec();

            assert!(
                result.is_ok(),
                "case {case} under {name}: compiled program for {text:?} failed: {result:?}"
            );
            assert_eq!(
                String::from_utf8_lossy(&got),
                text,
                "case {case} under {name}: compiled program printed the wrong string"
            );
        }
    }
}
