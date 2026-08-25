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
        let name = &case.name;

        match (case.expected_exit.as_str(), &result) {
            ("success", Ok(())) => {}
            ("error", Err(e)) => match case.expected_error_type.as_deref() {
                Some("limit") => assert!(
                    matches!(
                        e,
                        BfError::StepLimitExceeded { .. } | BfError::ExecutionTimeout { .. }
                    ),
                    "{name}: expected a limit, got {e:?}"
                ),
                Some("runtime") => assert!(
                    matches!(e, BfError::MemoryOutOfBounds { .. }),
                    "{name}: expected a runtime error, got {e:?}"
                ),
                _ => {}
            },
            (want, got) => panic!("{name}: manifest expects {want}, got {got:?}"),
        }

        // Output is checked whatever the exit was: a program stopped by its
        // step limit still has to have emitted the right bytes first, which is
        // the whole point of testing the interactive ones this way.
        if let Some(expected) = &case.expected_output {
            assert_eq!(
                Bytes(&output),
                Bytes(expected),
                "{name}: output differs from the manifest"
            );
        }
        if let Some(prefix) = &case.expected_output_prefix {
            assert!(
                output.starts_with(prefix),
                "{name}: output does not start with the expected prefix\n  expected: {:?}\n  got:      {:?}",
                Bytes(prefix),
                Bytes(&output[..output.len().min(prefix.len() + 16)]),
            );
        }
    }
}

/// Bytes that print as text when they can, so a failed comparison is readable
/// rather than a wall of integers.
struct Bytes<'a>(&'a [u8]);

impl PartialEq for Bytes<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl std::fmt::Debug for Bytes<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", String::from_utf8_lossy(self.0))?;
        if self.0.len() > 40 {
            write!(f, " ({} bytes)", self.0.len())?;
        }
        Ok(())
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

/// Debug symbols against real programs rather than fragments: whatever the
/// run does -- finish, hit a limit, fail -- the mapping back to source has to
/// survive it.
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
