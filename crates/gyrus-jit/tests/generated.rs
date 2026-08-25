//! Generated programs, JIT against the optimized interpreter, under every
//! configuration: both memory models, both cell models. The interpreter is
//! itself held to the tree-walker by the same kind of harness, so agreement
//! here is agreement with the reference.
//!
//! Step budgets differ between the engines (optimized instructions there,
//! loop iterations here), so a program the interpreter cannot finish within
//! its budget is skipped rather than compared; every other outcome --
//! completion, or which error with what position -- must match.

use gyrus::io::StringIo;
use gyrus::random::{RandomProgramConfig, generate_random_program};
use gyrus::{
    BfError, ExecutionConfig, ExecutionConfigBuilder, interpret_optimized_with_io,
    optimize_with_cell_model, parse,
};
use rand::SeedableRng;
use rand::rngs::StdRng;

const SEEDS: u64 = 400;
/// Optimized-instruction budget for the interpreter; runs that exhaust it
/// are skipped. Generous, so that skipping is rare.
const INTERPRETER_STEPS: u64 = 200_000;
/// Iteration budget for the JIT: never the binding limit for a program the
/// interpreter finished, since an iteration is at least one step.
const JIT_STEPS: u64 = 1_000_000;

type Build = Box<dyn Fn(u64) -> ExecutionConfig>;

fn configs() -> Vec<(&'static str, Build)> {
    vec![
        (
            "fixed/wrapping",
            Box::new(|max| {
                ExecutionConfigBuilder::new()
                    .with_memory_size(32)
                    .with_max_steps(max)
                    .build()
            }),
        ),
        (
            "fixed/checked",
            Box::new(|max| {
                ExecutionConfigBuilder::new()
                    .with_memory_size(32)
                    .with_checked_cells()
                    .with_max_steps(max)
                    .build()
            }),
        ),
        (
            "unbounded/wrapping",
            Box::new(|max| {
                ExecutionConfigBuilder::new()
                    .with_unbounded_memory(8, 64)
                    .unwrap()
                    .with_max_steps(max)
                    .build()
            }),
        ),
        (
            "unbounded/checked",
            Box::new(|max| {
                ExecutionConfigBuilder::new()
                    .with_unbounded_memory(8, 64)
                    .unwrap()
                    .with_checked_cells()
                    .with_max_steps(max)
                    .build()
            }),
        ),
    ]
}

/// What a run came to, in the terms both engines share.
#[derive(Debug, PartialEq)]
enum Outcome {
    Output(Vec<u8>),
    OutOfBounds { attempted: isize, output: Vec<u8> },
    Overflow(Vec<u8>),
    Underflow(Vec<u8>),
    Io(Vec<u8>),
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
        Err(BfError::StepLimitExceeded { .. }) | Err(BfError::ExecutionTimeout { .. }) => {
            Outcome::Limit
        }
        Err(other) => panic!("unexpected error {other:?}"),
    }
}

#[test]
fn generated_programs_agree_under_every_configuration() {
    // Two shapes: short and loop-light, so that most runs complete and every
    // configuration gets exercised; and deep and loop-heavy, for nested
    // balanced loops, long runs and the limit paths.
    let shapes = [
        RandomProgramConfig {
            max_depth: 4,
            avg_commands: 12,
            loop_probability: 0.5,
        },
        RandomProgramConfig {
            max_depth: 7,
            avg_commands: 24,
            loop_probability: 0.8,
        },
    ];
    let mut compared = 0;
    let mut skipped = 0;
    let mut disagreements = Vec::new();
    // Half as many of the deep ones: each takes several times longer.
    for (shape, seed) in shapes
        .iter()
        .enumerate()
        .flat_map(|(k, shape)| (0..SEEDS >> k).map(move |seed| (shape, seed)))
    {
        let mut rng = StdRng::seed_from_u64(seed);
        let source = generate_random_program(&mut rng, shape);
        let instructions = parse(&source).unwrap();
        for (name, build) in configs() {
            let program = optimize_with_cell_model(&instructions, *build(0).cell_model());
            let (mut i, mut o) = (StringIo::new("hello, world"), StringIo::empty());
            let interp = outcome(
                interpret_optimized_with_io(&program, build(INTERPRETER_STEPS), &mut i, &mut o)
                    .map(|_| ()),
                o.output_bytes().to_vec(),
            );
            if interp == Outcome::Limit {
                skipped += 1;
                continue;
            }
            let (mut i, mut o) = (StringIo::new("hello, world"), StringIo::empty());
            // Both statistics modes generate different code; alternate.
            let statistics = if seed % 2 == 0 {
                gyrus_jit::Statistics::Cheap
            } else {
                gyrus_jit::Statistics::Full
            };
            let jit = outcome(
                gyrus_jit::run_with(
                    &program,
                    &build(JIT_STEPS),
                    &mut i,
                    &mut o,
                    None,
                    statistics,
                )
                .map(|_| ()),
                o.output_bytes().to_vec(),
            );
            compared += 1;
            if jit != interp {
                disagreements.push(format!("seed {seed} {name}: {source:?}\n   interpreter {interp:?}\n   jit         {jit:?}"));
            }
        }
    }
    eprintln!("compared {compared} runs, skipped {skipped} at the interpreter's step budget");
    assert!(
        compared > SEEDS as usize * 2,
        "too few comparisons: {compared}"
    );
    assert!(
        disagreements.is_empty(),
        "{} disagreements:\n{}",
        disagreements.len(),
        disagreements.join("\n")
    );
}
