//! The program corpus under the tree-walking interpreter.
//!
//! Every case comes from `programs/test_manifest.toml` by way of the
//! `gyrus-corpus` crate, which is also what drives `gyrus-jit`'s corpus test.
//! That shared source is the point: these cases used to be hand-written here
//! to *mirror* the manifest, and the mirror drifted in both directions --
//! eleven programs ended up tested by one engine and not the other.
//!
//! Adding a program now means adding it to the manifest, once, and both
//! engines pick it up.

use gyrus::{
    BfError, ExecutionConfigBuilder, interpret_with_io,
    io::{DebugIo, StringIo},
    parse, parse_with_debug,
};
use gyrus_corpus::Case;
use std::fs;
use std::path::Path;

/// The case run through the tree-walker: what it did, and what it emitted
/// before doing it.
fn run(case: &Case) -> (Result<(), BfError>, Vec<u8>) {
    let source = case.source();
    let instructions = parse(&source).unwrap_or_else(|e| panic!("{}: parse: {e}", case.name));
    let mut input = StringIo::new(&case.input);
    let mut output = StringIo::empty();
    let result = interpret_with_io(
        &instructions,
        case.config(false),
        &mut input,
        &mut output,
        None,
    );
    (result.map(|_| ()), output.output_bytes().to_vec())
}

#[test]
fn every_manifest_case_behaves_as_declared() {
    let cases = gyrus_corpus::cases();
    for case in &cases {
        // A program the manifest says will not parse is checked and finished
        // with: there is nothing to run.
        if case.expects_parse_error() {
            let source = case.source();
            assert!(
                parse(&source).is_err(),
                "{}: expected a parse error",
                case.name
            );
            continue;
        }

        let (result, output) = run(case);
        case.check("tree-walker", &result, &output);
    }
}

/// The manifest is expectations; this is inventory. A program can sit in
/// `programs/` with no manifest entry -- benchmark inputs do -- but the
/// directories themselves should not quietly empty out.
#[test]
fn the_corpus_directories_are_populated() {
    let programs = gyrus_corpus::workspace_root().join("programs");
    // These are the directories that exist. Naming one that does not is the
    // failure this test is for: it used to look for `advanced/`, which moved
    // under `third-party/` long ago, and counted zero without complaining
    // because the check was guarded by `if dir.exists()`.
    let expected = [
        ("basic", 3),
        ("errors", 3),
        ("tests", 3),
        ("warnings", 3),
        ("third-party/advanced", 10),
        ("third-party/utilities", 5),
        // Referenced by no manifest case and no other test, so this is the
        // only thing standing between it and quietly emptying out.
        ("debug", 1),
    ];

    for (dir, least) in expected {
        let path = programs.join(dir);
        assert!(path.is_dir(), "programs/{dir} is missing");
        let count = fs::read_dir(&path)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "bf"))
            .count();
        assert!(
            count >= least,
            "programs/{dir} has {count} .bf files, expected at least {least}"
        );
    }
}

/// A run stopped part-way still knows where it was.
///
/// This is the assertion that matters about debug symbols, and it needs a
/// program that actually fails: an earlier version of this test ran four
/// programs that finish in under a thousand steps against a five-thousand-step
/// budget, so its error arm was unreachable and the only live assertion
/// followed from parsing having succeeded. It would have passed with the
/// source-location lookup returning `None` for every error.
#[test]
fn a_failed_run_reports_where_it_was() {
    let root = gyrus_corpus::workspace_root();
    // Each of these fails a different way, and each must still name a line.
    for (program, steps) in [
        ("errors/infinite_loop.bf", 500),
        ("tests/deep_nesting.bf", 20),
    ] {
        let source = fs::read_to_string(root.join("programs").join(program)).unwrap();
        let (instructions, debug_info) = parse_with_debug(&source).unwrap();
        let (mut i, mut o) = (DebugIo::new(), DebugIo::new());
        let result = interpret_with_io(
            &instructions,
            ExecutionConfigBuilder::new()
                .with_memory_size(30_000)
                .with_max_steps(steps)
                .build(),
            &mut i,
            &mut o,
            Some(&debug_info),
        );
        match result {
            Err(BfError::StepLimitExceeded {
                source_location, ..
            }) => assert!(
                source_location.is_some(),
                "{program}: stopped by the step limit without a source location"
            ),
            other => panic!("{program}: expected the step limit to stop it, got {other:?}"),
        }
    }
}

/// Debug symbols against real programs rather than fragments: parsing with
/// debug info and running to completion keeps the table consistent.
#[test]
fn debug_symbols_survive_real_programs() {
    let root = gyrus_corpus::workspace_root();
    for program in [
        "basic/simple.bf",
        "basic/hello_world.bf",
        "basic/line_comments.bf",
        "tests/deep_nesting.bf",
    ] {
        let path = root.join("programs").join(program);
        assert!(path.is_file(), "{program} is missing");
        let source = fs::read_to_string(&path).unwrap();
        let (instructions, debug_info) =
            parse_with_debug(&source).unwrap_or_else(|e| panic!("{program}: {e}"));

        let mut input = DebugIo::new();
        let mut output = DebugIo::new();
        let result = interpret_with_io(
            &instructions,
            ExecutionConfigBuilder::new()
                .with_memory_size(30_000)
                .with_max_steps(5_000)
                .build(),
            &mut input,
            &mut output,
            Some(&debug_info),
        );

        // Either outcome is fine; what matters is that an interrupted run
        // still knows where it was.
        if let Err(e) = result {
            assert!(
                matches!(
                    e,
                    BfError::StepLimitExceeded { .. } | BfError::ExecutionTimeout { .. }
                ),
                "{program}: unexpected failure {e:?}"
            );
        }
        assert!(
            !debug_info.is_empty(),
            "{program}: parsed with debug info but the table is empty"
        );
    }
}

/// The manifest names files; the files have to be there. Cheap to check, and
/// it fails with the case's name rather than mid-run.
#[test]
fn every_manifest_case_names_a_real_file() {
    let programs = gyrus_corpus::workspace_root().join("programs");
    for case in gyrus_corpus::cases() {
        let path = programs.join(&case.file);
        assert!(
            path.is_file(),
            "{}: manifest points at {}, which does not exist",
            case.name,
            Path::new(&case.file).display()
        );
    }
}
