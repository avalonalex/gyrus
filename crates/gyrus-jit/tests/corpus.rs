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
            // For one a limit stopped, they are not, and cannot be: `max_steps`
            // counts optimized instructions in the interpreter and loop
            // iterations in the JIT, so the two stop at different points in a
            // program that emits forever. rot13 under a 100k budget gives
            // 32,897 trailing NULs on one side and 3,146 on the other, after
            // the same correct output. The prefix is what is comparable, and
            // `Case::check` holds both engines to it below.
            //
            // Read off what actually happened rather than off the manifest: a
            // case tagged `limit` that stopped failing would otherwise have its
            // byte comparison suppressed for good, and a real divergence with
            // it.
            let stopped = |r: &Result<(), BfError>| {
                matches!(
                    r,
                    Err(BfError::StepLimitExceeded { .. } | BfError::ExecutionTimeout { .. })
                )
            };
            if !stopped(&interp) && !stopped(&jit) {
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
            // for, the default; under checked cells a program may fail where it
            // did not, and then only the agreement above is required.
            if checked {
                continue;
            }
            // Both engines against the manifest, through the same assertions
            // the tree-walker's suite uses -- these used to be written out
            // separately here and had already diverged: `expected_output` was
            // only checked on the success path, so a case declaring an error
            // and an output went unchecked.
            case.check("interpreter", &interp, &out_i);
            case.check("jit", &jit, &out_j);
        }
    }
}
