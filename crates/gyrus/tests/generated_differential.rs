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

use gyrus::io::StringIo;
use gyrus::random::{
    IdiomaticConfig, RandomProgramConfig, generate_idiomatic_program, generate_random_program,
};
use gyrus::{
    BfError, ExecutionConfig, ExecutionConfigBuilder, interpret_optimized_with_io,
    interpret_with_io, optimize_with_cell_model, parse,
};
use rand::SeedableRng;
use rand::rngs::StdRng;

const SEEDS: u64 = 200;

/// Whatever a `,` reads, both engines read the same thing.
const INPUT: &str = "hello, world";

/// Step budgets are not comparable between the engines -- the tree-walker
/// counts source instructions and the optimized interpreter counts fused ones,
/// so the same number means less work on one side than the other. A run that
/// exhausts either budget is therefore skipped rather than compared; the budget
/// exists only so a generated infinite loop fails by name instead of hanging.
const STEPS: u64 = 500_000;

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

type Build = fn() -> ExecutionConfig;

/// Both memory models against both cell models: a fold that is only valid
/// under wrapping cells has to be caught here, not by a reader.
fn configurations() -> [(&'static str, Build); 4] {
    [
        ("fixed/wrapping", || {
            ExecutionConfigBuilder::new()
                .with_memory_size(32)
                .with_max_steps(STEPS)
                .build()
        }),
        ("fixed/checked", || {
            ExecutionConfigBuilder::new()
                .with_memory_size(32)
                .with_checked_cells()
                .with_max_steps(STEPS)
                .build()
        }),
        ("unbounded/wrapping", || {
            ExecutionConfigBuilder::new()
                .with_unbounded_memory(8, 64)
                .unwrap()
                .with_max_steps(STEPS)
                .build()
        }),
        ("unbounded/checked", || {
            ExecutionConfigBuilder::new()
                .with_unbounded_memory(8, 64)
                .unwrap()
                .with_checked_cells()
                .with_max_steps(STEPS)
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
        let r = interpret_with_io(&instructions, build(), &mut i, &mut o, None);
        outcome(r.map(|_| ()), o.output_bytes().to_vec())
    };

    let config = build();
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
    assert!(
        compared > programs.len(),
        "{what}: only {compared} comparable runs -- the budget is starving the test"
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
