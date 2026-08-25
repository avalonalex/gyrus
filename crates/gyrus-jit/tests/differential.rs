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
/// reported with the cursor that did it.
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

/// Not just the same variant: the same hint, bound, dump and message. The
/// one field left out is `instruction_index`, where the interpreter reports
/// its step count and the JIT the instruction -- see `Site` in the JIT.
#[test]
fn out_of_tape_errors_are_word_for_word_the_interpreters() {
    let workspace = workspace();
    let overflow =
        std::fs::read_to_string(workspace.join("programs/errors/memory_overflow.bf")).unwrap();
    // The sample walks a few hundred cells right; a tape one short of that
    // makes its final `+` the overrun, whatever the exact count.
    let reach = overflow.matches('>').count() - overflow.matches('<').count();
    let cases: Vec<(String, usize)> = vec![
        ("<+".to_string(), 5),
        (">>>>>+".to_string(), 5),
        ("+++[->>>>>+<<<<<]".to_string(), 5),
        (overflow, reach),
    ];
    for (src, size) in cases {
        let build = move || ExecutionConfigBuilder::new().with_memory_size(size).build();
        let (interp, jit) = both(&src, "", build);
        let (Err(a), Err(b)) = (interp, jit) else {
            panic!("{src}: expected both to fail");
        };
        let (
            BfError::MemoryOutOfBounds {
                instruction_index: ia,
                attempted: a1,
                max: a2,
                memory_dump: a3,
                hint: a4,
                ..
            },
            BfError::MemoryOutOfBounds {
                instruction_index: ib,
                attempted: b1,
                max: b2,
                memory_dump: b3,
                hint: b4,
                ..
            },
        ) = (&a, &b)
        else {
            panic!("{src}: {a:?} vs {b:?}");
        };
        assert_eq!((a1, a2, a4), (b1, b2, b4), "{src}");
        assert_eq!(format!("{a3:?}"), format!("{b3:?}"), "{src}: memory dump");
        let scrub = |text: String, index: &gyrus::InstructionIndex| {
            text.replace(&format!("instruction {index}"), "instruction ?")
        };
        assert_eq!(
            scrub(a.format_detailed(), ia),
            scrub(b.format_detailed(), ib),
            "{src}: formatted"
        );
    }
}

/// What the JIT can do that the optimized interpreter cannot: with debug
/// info, the error points at the instruction that did it.
#[test]
fn with_debug_info_the_error_has_a_source_location() {
    // Line 2, column 3 is the `+` that writes cell -1.
    let src = "++\n<<+";
    let (instructions, debug_info) = gyrus::parse_with_debug(src).unwrap();
    let program = optimize(&instructions);
    let config = ExecutionConfigBuilder::new().with_memory_size(5).build();
    let (mut i, mut o) = (StringIo::empty(), StringIo::empty());
    let err = gyrus_jit::run(&program, &config, &mut i, &mut o, Some(&debug_info)).unwrap_err();
    let BfError::MemoryOutOfBounds {
        source_location: Some(loc),
        attempted,
        ..
    } = err
    else {
        panic!("expected a located out-of-bounds error, got {err:?}");
    };
    assert_eq!(attempted, -2);
    assert_eq!((loc.line, loc.column), (2, 3));
    // And it renders with the source context, caret and all.
    let (instructions, debug_info) = gyrus::parse_with_debug(src).unwrap();
    let (mut i, mut o) = (StringIo::empty(), StringIo::empty());
    let err = gyrus_jit::run(
        &optimize(&instructions),
        &config,
        &mut i,
        &mut o,
        Some(&debug_info),
    )
    .unwrap_err();
    let rendered = err.format_with_source(src);
    assert!(rendered.contains("At line 2, column 3"), "{rendered}");
}

/// An I/O failure is the same error, down to the operation named.
#[test]
fn io_errors_are_the_interpreters() {
    let build = || {
        ExecutionConfigBuilder::new()
            .with_memory_size(5)
            .with_eof_behavior(EofBehavior::Error)
            .build()
    };
    let (interp, jit) = both(",", "", build);
    let (Err(a), Err(b)) = (interp, jit) else {
        panic!("expected both to fail")
    };
    assert_eq!(a.to_string(), b.to_string());
    let (
        BfError::IoError {
            operation: oa,
            source: sa,
            ..
        },
        BfError::IoError {
            operation: ob,
            source: sb,
            ..
        },
    ) = (&a, &b)
    else {
        panic!("{a:?} vs {b:?}");
    };
    assert_eq!(oa, ob);
    assert_eq!((sa.kind(), sa.to_string()), (sb.kind(), sb.to_string()));
}
