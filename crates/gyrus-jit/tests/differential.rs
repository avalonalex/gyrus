//! The JIT is held to the optimized interpreter, which is held to the
//! tree-walker: same bytes out, same error where there is one.

use gyrus::io::StringIo;
use gyrus::{
    BfError, EofBehavior, ExecutionConfigBuilder, interpret_optimized_with_io, optimize, parse,
};
use std::path::Path;

fn workspace() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

fn config() -> gyrus::ExecutionConfig {
    ExecutionConfigBuilder::new()
        .with_memory_size(30_000)
        .build()
}

/// Both engines on the same source and input: (output, error).
fn both(
    src: &str,
    input: &str,
    config: impl Fn() -> gyrus::ExecutionConfig,
) -> (Result<Vec<u8>, BfError>, Result<Vec<u8>, BfError>) {
    let program = optimize(&parse(src).unwrap());
    let run = |jit: bool| {
        let mut i = StringIo::new(input);
        let mut o = StringIo::empty();
        let result = if jit {
            gyrus_jit::run(&program, &config(), &mut i, &mut o, None)
        } else {
            interpret_optimized_with_io(&program, config(), &mut i, &mut o)
        };
        result.map(|_| o.output_bytes().to_vec())
    };
    (run(false), run(true))
}

/// The benchmark programs against their recorded outputs. mandelbrot is left
/// to `benchmark.sh`: under the test profile Cranelift itself is unoptimized
/// and the compile alone takes seconds.
#[test]
fn benchmark_programs_match_their_golden_outputs() {
    for name in [
        "hello_world",
        "99beer",
        "triangle",
        "squares",
        "bf2c",
        "hanoi",
    ] {
        let dir = if name == "hello_world" {
            "basic"
        } else {
            "third-party/advanced"
        };
        let source =
            std::fs::read_to_string(workspace().join(format!("programs/{dir}/{name}.bf"))).unwrap();
        let expected =
            std::fs::read(workspace().join(format!("benchmarks/expected/{name}.txt"))).unwrap();
        let program = optimize(&parse(&source).unwrap());
        let mut input = StringIo::empty();
        let mut output = StringIo::empty();
        gyrus_jit::run(&program, &config(), &mut input, &mut output, None).unwrap();
        assert_eq!(output.output_bytes(), &expected[..], "{name}");
    }
}

/// Every IR shape the translator has an arm for, against the interpreter.
#[test]
fn agrees_with_the_optimized_interpreter() {
    let programs = [
        "+++++.",               // Add, Output
        "+++++-----.",          // Sub to zero
        ">>+++<<.>>.",          // Right/Left
        "+++[-]+++.",           // Set
        "+++[-].",              // Zero
        "+++++[>+++<-]>.",      // MultiplyAdd, decrement last
        "+++++[->+++>+<<]>.>.", // two targets
        "+++++[>+<+]>.",        // source incremented: 251 iterations
        "+>+>+><<<[>]+.",       // SeekRight(1)
        "+>>+<<[>>]<.",         // SeekRight(2)
        ">>+>>+[<<]>.",         // SeekLeft(2)
        ",.,.,.",               // Input with bytes
        ",[.,]",                // echo loop
        "++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.>>.<-.<.+++.------.--------.>>+.>++.",
        "+[>+<-]>[<+>-]<.", // nested moves
        "[[]]+.",           // loops never entered
    ];
    for src in programs {
        let (interp, jit) = both(src, "abc", config);
        assert_eq!(jit.as_ref().unwrap(), interp.as_ref().unwrap(), "{src}");
    }
}

/// EOF is the runtime's decision, in both engines.
#[test]
fn eof_behaviours_agree() {
    for eof in [
        EofBehavior::SetZero,
        EofBehavior::SetNegOne,
        EofBehavior::NoChange,
        EofBehavior::Error,
    ] {
        let build = move || {
            ExecutionConfigBuilder::new()
                .with_memory_size(100)
                .with_eof_behavior(eof)
                .build()
        };
        let (interp, jit) = both("+++++,.", "", build);
        match (interp, jit) {
            (Ok(a), Ok(b)) => assert_eq!(a, b, "{eof:?}"),
            (Err(a), Err(b)) => assert_eq!(
                std::mem::discriminant(&a),
                std::mem::discriminant(&b),
                "{eof:?}: {a:?} vs {b:?}"
            ),
            (a, b) => panic!("{eof:?}: interpreter {a:?}, jit {b:?}"),
        }
    }
}

/// The tape contract: moving off the tape is fine, touching it is the error,
/// reported with the cursor that did it. (`instruction_index` is the
/// interpreter's step count and is not matched yet -- see the PRD's step 2.)
#[test]
fn out_of_tape_access_is_the_same_error() {
    let build = || ExecutionConfigBuilder::new().with_memory_size(5).build();
    for (src, attempted) in [
        ("<+", -1),                    // one left of the tape
        (">>>>>+", 5),                 // one past the end
        (">>>>>>>>>><<<<<<<<<<+.", 0), // out and back: no error, prints 1
        ("+>+>+>+>+<<<<[>]", 5),       // every cell non-zero: the seek runs off the end
        ("+>+>+>+>+[>>]", 6),          // strided seek from cell 4 lands on 6
        ("+++[->>>>>+<<<<<]", 5),      // multiply target off the tape
    ] {
        let (interp, jit) = both(src, "", build);
        match (interp, jit) {
            (Ok(a), Ok(b)) => assert_eq!(a, b, "{src}"),
            (
                Err(BfError::MemoryOutOfBounds { attempted: a, .. }),
                Err(BfError::MemoryOutOfBounds { attempted: b, .. }),
            ) => {
                assert_eq!(a, attempted, "{src}: interpreter");
                assert_eq!(b, attempted, "{src}: jit");
            }
            (a, b) => panic!("{src}: interpreter {a:?}, jit {b:?}"),
        }
    }
}

/// The multiply fold is only valid under wrapping cells, and the program
/// records the model it was built for; the spike refuses anything else
/// rather than run folds that do not hold.
#[test]
fn checked_cells_are_refused_for_now() {
    let program = gyrus::optimize_with_cell_model(
        &parse("+").unwrap(),
        gyrus::CellModel::U8Checked(gyrus::U8CheckedCells),
    );
    let config = ExecutionConfigBuilder::new()
        .with_memory_size(10)
        .with_checked_cells()
        .build();
    let (mut i, mut o) = (StringIo::empty(), StringIo::empty());
    assert!(matches!(
        gyrus_jit::run(&program, &config, &mut i, &mut o, None),
        Err(BfError::ConfigurationError { .. })
    ));
}
