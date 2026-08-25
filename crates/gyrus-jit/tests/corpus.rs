//! The program corpus, under the JIT, against `programs/test_manifest.toml`:
//! literally the same cases `program_corpus.rs` holds the tree-walker to --
//! both read them through `gyrus-corpus` -- with the optimized interpreter
//! beside the JIT for the byte-for-byte comparison, under both cell models.
//!
//! The manifest parser used to live here, and the tree-walker's suite kept a
//! hand-written imitation of it. Sharing the real thing is what stops the two
//! from drifting again.

use gyrus::io::StringIo;
use gyrus::{BfError, interpret_optimized_with_io, optimize_with_cell_model, parse};
use gyrus_corpus::Case;

/// Both engines' (result, output) on the case.
fn both(
    case: &Case,
    program: &gyrus::optimizer::OptimizedProgram,
    checked: bool,
) -> [(Result<(), BfError>, Vec<u8>); 2] {
    let run = |jit: bool| {
        let (mut i, mut o) = (StringIo::new(&case.input), StringIo::empty());
        let r = if jit {
            gyrus_jit::run(program, &case.config(checked), &mut i, &mut o, None)
        } else {
            interpret_optimized_with_io(program, case.config(checked), &mut i, &mut o)
        };
        (r.map(|_| ()), o.output_bytes().to_vec())
    };
    [run(false), run(true)]
}

#[test]
fn the_corpus_behaves_the_same_under_the_jit() {
    for case in &gyrus_corpus::cases() {
        let source = case.source();
        let parsed = parse(&source);
        if case.expects_parse_error() {
            assert!(parsed.is_err(), "{}: expected a parse error", case.name);
            continue;
        }
        let instructions = parsed.unwrap_or_else(|e| panic!("{}: {e}", case.name));
        for checked in [false, true] {
            let what = format!(
                "{} ({})",
                case.name,
                if checked { "checked" } else { "wrapping" }
            );
            let program =
                optimize_with_cell_model(&instructions, *case.config(checked).cell_model());
            let [(interp, out_i), (jit, out_j)] = both(case, &program, checked);

            // For a run that ends on its own, the bytes emitted are the same.
            //
            // For one the step limit stops, they are not, and cannot be:
            // `max_steps` counts optimized instructions in the interpreter and
            // loop iterations in the JIT, so the two engines stop at different
            // points in a program that emits forever. rot13 under a 100k budget
            // gives 32,897 trailing NULs on one side and 3,146 on the other,
            // both after the same correct output. What is comparable there is
            // the prefix, which the manifest states and both engines are held
            // to below -- the same reason `generated.rs` skips a comparison the
            // interpreter could not finish.
            let limited = case.expected_error_type.as_deref() == Some("limit");
            if !limited {
                assert_eq!(out_j, out_i, "{what}: output differs between engines");
            }
            match (&interp, &jit) {
                (Ok(()), Ok(())) => {}
                (Err(a), Err(b)) => assert_eq!(
                    std::mem::discriminant(a),
                    std::mem::discriminant(b),
                    "{what}: {a:?} vs {b:?}"
                ),
                (a, b) => panic!("{what}: interpreter {a:?}, jit {b:?}"),
            }
            // The manifest's expectations are for the model it was written
            // for, the default; under checked cells a program may fail where
            // it did not, and then only the agreement above is required.
            if checked {
                continue;
            }
            match (case.expected_exit.as_str(), &jit) {
                ("success", Ok(())) => {
                    if let Some(expected) = &case.expected_output {
                        assert_eq!(&out_j, expected, "{what}: output");
                    }
                }
                ("error", Err(e)) => match case.expected_error_type.as_deref() {
                    Some("limit") => assert!(
                        matches!(
                            e,
                            BfError::StepLimitExceeded { .. } | BfError::ExecutionTimeout { .. }
                        ),
                        "{what}: {e:?}"
                    ),
                    Some("runtime") => {
                        assert!(
                            matches!(e, BfError::MemoryOutOfBounds { .. }),
                            "{what}: {e:?}"
                        )
                    }
                    _ => {}
                },
                (want, got) => panic!("{what}: expected {want}, got {got:?}"),
            }
            // Checked whatever the exit was: a program the step limit stopped
            // still has to have emitted the right bytes first. That is the
            // only thing worth asserting about the interactive ones.
            if let Some(prefix) = &case.expected_output_prefix {
                for (engine, out) in [("interpreter", &out_i), ("jit", &out_j)] {
                    assert!(
                        out.starts_with(prefix),
                        "{what}: {engine} output does not start with the expected prefix\n  expected: {:?}\n  got:      {:?}",
                        String::from_utf8_lossy(prefix),
                        String::from_utf8_lossy(&out[..out.len().min(prefix.len() + 16)]),
                    );
                }
            }
        }
    }
}
