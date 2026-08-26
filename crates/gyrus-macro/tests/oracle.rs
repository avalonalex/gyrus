//! Macro programs whose answer is known by construction.
//!
//! Every other test of an execution engine in this repository proves that two
//! engines *agree*. Agreement is not correctness: a fold that is wrong the
//! same way on both sides passes all of them. `compile_string` closes that gap
//! for strings -- it emits a program that prints a given string, so the right
//! answer is known in advance -- and the design says a macro system
//! generalises it to arbitrary programs. This is that generalisation, and it
//! is the reason the preprocessor was worth building rather than a thing the
//! preprocessor happens to allow.
//!
//! The trick is to write the same computation twice, in two languages that
//! share no code. [`Op`] is a handful of operations with an obvious meaning;
//! [`Program::expected`] applies them to a `[u8]` in Rust, and
//! [`Program::render`] emits a `.bfm` that applies them to a BrainFuck tape.
//! If the expander, the parser, the optimizer or either interpreter is wrong,
//! the two answers differ. Nothing here asks another engine what it thinks.
//!
//! What it exercises, incidentally but not accidentally: macros invoking
//! macros, parameters passed on to further invocations, nested loops inside a
//! macro body inside a loop, and cell arithmetic that wraps. Those are the
//! shapes the optimizer folds.

use gyrus::{
    ExecutionConfigBuilder, interpret_optimized_with_io, interpret_with_io, io::StringIo,
    optimize_with_cell_model, parse,
};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

/// Named cells the generated programs use. The two beyond the variables are
/// the working cells the macro library needs; the generator never names them,
/// which is what keeps the model's account of them true.
const VARIABLES: usize = 4;

/// The macro library the generated programs are written against.
///
/// Written in the macro language, while [`Program::expected`] is written in
/// Rust. That is the whole point: two implementations of the same operations
/// with nothing in common, so a mistake would have to be made twice, the same
/// way, to go unnoticed.
const PRELUDE: &str = "\
@var v0 at 0
@var v1 at 1
@var v2 at 2
@var v3 at 3
@var scratch at 4
@var counter at 5

@macro clear(cell) {
    @to cell
    [-]
}

@macro set(cell, value) {
    @clear(cell)
    +{value}
}

* dst = dst + src, wrapping. src is left as it was; scratch ends at zero.
@macro add_to(src, dst) {
    @clear(scratch)
    @to src
    [
        @to dst
        +
        @to scratch
        +
        @to src
        -
    ]
    @to scratch
    [
        @to src
        +
        @to scratch
        -
    ]
}

@macro copy(src, dst) {
    @clear(dst)
    @add_to(src, dst)
}

* into = x * y, wrapping, by adding y to it x times. x and y are preserved.
@macro multiply(x, y, into) {
    @clear(into)
    @copy(x, counter)
    @to counter
    [
        @add_to(y, into)
        @to counter
        -
    ]
}

@macro print(cell) {
    @to cell
    .
}
";

/// One operation, with one meaning.
#[derive(Debug, Clone, Copy)]
enum Op {
    Set(usize, u8),
    Clear(usize),
    /// `dst += src`, wrapping. `src != dst`, because the idiom that implements
    /// it reads and writes the two cells in one loop.
    AddTo(usize, usize),
    /// `dst = src`. `src != dst`, for the same reason.
    Copy(usize, usize),
    /// `into = x * y`, wrapping. `into` differs from both, because the macro
    /// clears it before reading them.
    Multiply(usize, usize, usize),
    Print(usize),
}

#[derive(Default)]
struct Outcome {
    output: Vec<u8>,
    /// Whether any operation exceeded a cell and wrapped.
    wrapped: bool,
}

struct Program(Vec<Op>);

impl Program {
    /// What the program prints, worked out in Rust, and whether any cell
    /// wrapped on the way.
    ///
    /// The second half is not decoration. This repository's own note on
    /// generated programs says a generator can only falsify what it can
    /// express, after a guard that was wrong survived every seed because no
    /// generated program ever came near a cell boundary. Reporting it is what
    /// lets a test insist these do.
    fn expected(&self) -> Outcome {
        let mut cells = [0u8; VARIABLES];
        let mut outcome = Outcome::default();
        for op in &self.0 {
            match *op {
                Op::Set(cell, value) => cells[cell] = value,
                Op::Clear(cell) => cells[cell] = 0,
                Op::AddTo(src, dst) => {
                    let (sum, carried) = cells[dst].overflowing_add(cells[src]);
                    outcome.wrapped |= carried;
                    cells[dst] = sum;
                }
                Op::Copy(src, dst) => cells[dst] = cells[src],
                Op::Multiply(x, y, into) => {
                    let (product, carried) = cells[x].overflowing_mul(cells[y]);
                    outcome.wrapped |= carried;
                    cells[into] = product;
                }
                Op::Print(cell) => outcome.output.push(cells[cell]),
            }
        }
        outcome
    }

    /// The same program as `.bfm` source.
    fn render(&self) -> String {
        let mut source = String::from(PRELUDE);
        source.push('\n');
        for op in &self.0 {
            let line = match *op {
                Op::Set(cell, value) => format!("@set(v{cell}, {value})"),
                Op::Clear(cell) => format!("@clear(v{cell})"),
                Op::AddTo(src, dst) => format!("@add_to(v{src}, v{dst})"),
                Op::Copy(src, dst) => format!("@copy(v{src}, v{dst})"),
                Op::Multiply(x, y, into) => format!("@multiply(v{x}, v{y}, v{into})"),
                Op::Print(cell) => format!("@print(v{cell})"),
            };
            source.push_str(&line);
            source.push('\n');
        }
        source
    }
}

/// A program of `length` operations, ending in enough prints to be worth
/// checking.
///
/// The prints are forced rather than left to chance: a program that happens to
/// print nothing passes any comparison of outputs, and a suite of those would
/// look exactly like a suite that works.
fn generate(rng: &mut StdRng, length: usize) -> Program {
    let mut ops = Vec::with_capacity(length + VARIABLES);
    let cell = |rng: &mut StdRng| rng.random_range(0..VARIABLES);
    let other = |rng: &mut StdRng, avoid: &[usize]| loop {
        let candidate = rng.random_range(0..VARIABLES);
        if !avoid.contains(&candidate) {
            return candidate;
        }
    };

    for _ in 0..length {
        // Multiplication is rare: it is the only operation whose cost is a
        // product rather than a sum, and a program of nothing else runs for a
        // long time to prove the same thing.
        let op = match rng.random_range(0..10u8) {
            0..=2 => Op::Set(cell(rng), rng.random()),
            3 => Op::Clear(cell(rng)),
            4..=5 => {
                let src = cell(rng);
                Op::AddTo(src, other(rng, &[src]))
            }
            6 => {
                let src = cell(rng);
                Op::Copy(src, other(rng, &[src]))
            }
            7 => {
                // Bounded operands: the macro adds `y` to a cell `x` times, so
                // 255 by 255 is most of a million BrainFuck steps for one
                // operation, and the wrapping it would prove is proved by the
                // additions already.
                let (x, y) = (cell(rng), cell(rng));
                let into = other(rng, &[x, y]);
                ops.push(Op::Set(x, rng.random_range(0..=20)));
                ops.push(Op::Set(y, rng.random_range(0..=20)));
                Op::Multiply(x, y, into)
            }
            _ => Op::Print(cell(rng)),
        };
        ops.push(op);
    }

    for cell in 0..VARIABLES {
        ops.push(Op::Print(cell));
    }
    Program(ops)
}

/// Run an expansion under both interpreters in `gyrus`, and return what each
/// printed.
///
/// Both, because the oracle is worth as much to the optimizer as to the tree
/// walker -- more, since the optimizer is the one that rewrites the multiply
/// loops and clears these programs are full of.
fn run_both_engines(brainfuck: &str) -> (Vec<u8>, Vec<u8>) {
    let instructions = parse(brainfuck).expect("the expansion parses");
    let config = || {
        ExecutionConfigBuilder::new()
            .with_memory_size(30_000)
            .with_max_steps(50_000_000)
            .build()
    };

    let (mut input, mut walked) = (StringIo::empty(), StringIo::empty());
    interpret_with_io(&instructions, config(), &mut input, &mut walked, None)
        .expect("the tree-walker finishes");

    let optimized = optimize_with_cell_model(&instructions, *config().cell_model());
    let (mut input, mut folded) = (StringIo::empty(), StringIo::empty());
    interpret_optimized_with_io(&optimized, config(), &mut input, &mut folded)
        .expect("the optimized interpreter finishes");

    (
        walked.output_bytes().to_vec(),
        folded.output_bytes().to_vec(),
    )
}

fn check(program: &Program, name: &str) {
    let source = program.render();
    let expansion = gyrus_macro::expand(&source)
        .unwrap_or_else(|e| panic!("{name}: {}", e.format_with_source(&source)));

    let expected = program.expected().output;
    assert!(
        !expected.is_empty(),
        "{name}: a program that prints nothing proves nothing"
    );

    let (walked, folded) = run_both_engines(expansion.brainfuck());
    assert_eq!(
        walked, expected,
        "{name}: the tree-walker disagrees with the answer worked out in Rust\n{source}"
    );
    assert_eq!(
        folded, expected,
        "{name}: the optimized interpreter disagrees with the answer worked out in Rust\n{source}"
    );
}

/// A program small enough to check by hand, so the model itself is anchored.
///
/// Everything else here trusts [`Program::expected`] to say what the answer
/// is. This one does not: 8 times 9 is 72, which is `H`, and 7 times 15 is
/// 105, which is `i`. If the Rust model and the macro library were wrong in
/// the same way, this is the test that would notice.
#[test]
fn a_program_whose_answer_was_worked_out_by_hand() {
    let program = Program(vec![
        Op::Set(0, 8),
        Op::Set(1, 9),
        Op::Multiply(0, 1, 2),
        Op::Print(2),
        Op::Set(0, 7),
        Op::Set(1, 15),
        Op::Multiply(0, 1, 2),
        Op::Print(2),
    ]);
    assert_eq!(program.expected().output, b"Hi");
    check(&program, "by hand");
}

#[test]
fn generated_programs_print_what_they_were_built_to_print() {
    for seed in 0..64u64 {
        let mut rng = StdRng::seed_from_u64(seed);
        let program = generate(&mut rng, 24);
        check(&program, &format!("seed {seed}"));
    }
}

/// The generated programs reach a cell boundary, and often.
///
/// Without this, a change to the weights or the value range could quietly
/// stop them ever wrapping, and the suite would look exactly as green while
/// testing strictly less. That has happened here before, to the generator in
/// `gyrus`: a fold whose guard was wrong survived every seed because no
/// generated program came near 255, and a reading found it rather than a run.
#[test]
fn the_generated_programs_exercise_the_cell_boundary() {
    let wrapping = (0..64u64)
        .filter(|seed| {
            let mut rng = StdRng::seed_from_u64(*seed);
            generate(&mut rng, 24).expected().wrapped
        })
        .count();
    assert!(
        wrapping >= 16,
        "only {wrapping} of 64 generated programs wrap a cell; they are not \
         exercising the boundary any more"
    );
}
