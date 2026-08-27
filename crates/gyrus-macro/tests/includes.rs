//! `@include`, the one directive that needs a filesystem.
//!
//! These are integration tests rather than unit tests because the unit of
//! behaviour is a *directory*: what a path resolves against, what happens when
//! two files name a third, and what a cycle does are all questions about files
//! on disk. `expand` takes text and cannot answer any of them.

use std::path::PathBuf;

use gyrus_macro::{MacroError, expand, expand_file};

/// A directory of `.bfm` files, removed when the test ends.
///
/// Named after the test rather than randomly, so a failure leaves something a
/// person can go and look at, and the next run of that test clears it.
struct Fixture {
    dir: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("gyrus-include-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temporary directory");
        Fixture { dir }
    }

    fn write(&self, relative: &str, text: &str) -> PathBuf {
        let path = self.dir.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("a directory for the file");
        }
        std::fs::write(&path, text).expect("writing the file");
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn expanded(path: &std::path::Path) -> String {
    expand_file(path)
        .unwrap_or_else(|failure| panic!("{}", failure.report()))
        .brainfuck()
        .to_string()
}

#[test]
fn a_library_declares_and_the_program_invokes() {
    let fixture = Fixture::new("library");
    fixture.write("lib.bfm", "@define STEP 3\n@macro bump {\n+{STEP}\n}\n");
    let main = fixture.write("main.bfm", "@include \"lib.bfm\"\n@bump\n+{STEP}\n");

    assert_eq!(expanded(&main), "++++++");
}

/// The rule the design rests on. The map holds one position per emitted byte
/// against one text, so an instruction in another file could only report a
/// line of the file that included it or a line number belonging to a file the
/// reader is not looking at.
#[test]
fn an_included_file_declares_and_does_not_emit() {
    let fixture = Fixture::new("emits");
    fixture.write("lib.bfm", "@define X 1\n+++\n");
    let main = fixture.write("main.bfm", "@include \"lib.bfm\"\n");

    let failure = expand_file(&main).unwrap_err();
    assert!(
        matches!(
            failure.error,
            MacroError::IncludedFileEmits {
                instruction: '+',
                ..
            }
        ),
        "{:?}",
        failure.error
    );
    // Against the included file's own text, not the one being expanded.
    assert!(failure.file().is_some_and(|path| path.ends_with("lib.bfm")));
    assert!(failure.source().contains("+++"), "{}", failure.source());
}

/// Even a macro invoked at the top level of one: it is the emitting that is
/// refused, not the writing of instructions.
#[test]
fn an_included_file_may_not_invoke_a_macro_either() {
    let fixture = Fixture::new("invokes");
    fixture.write("lib.bfm", "@macro three {\n+++\n}\n@three\n");
    let main = fixture.write("main.bfm", "@include \"lib.bfm\"\n");

    assert!(matches!(
        expand_file(&main).unwrap_err().error,
        MacroError::IncludedFileEmits { .. }
    ));
}

/// Two libraries sharing a third, which is what guards exist for in languages
/// that read a file as many times as it is named.
#[test]
fn a_file_named_twice_is_read_once() {
    let fixture = Fixture::new("diamond");
    fixture.write("base.bfm", "@define B 2\n");
    fixture.write("left.bfm", "@include \"base.bfm\"\n@define L 1\n");
    fixture.write("right.bfm", "@include \"base.bfm\"\n@define R 1\n");
    let main = fixture.write(
        "main.bfm",
        "@include \"left.bfm\"\n@include \"right.bfm\"\n+{B}\n+{L}\n+{R}\n",
    );

    // `@define B 2` twice would be a redefinition, so reaching this at all is
    // the assertion; the count says every one of the three arrived.
    assert_eq!(expanded(&main), "++++");
}

#[test]
fn a_cycle_terminates_rather_than_being_detected() {
    let fixture = Fixture::new("cycle");
    fixture.write("a.bfm", "@include \"b.bfm\"\n@define A 1\n");
    fixture.write("b.bfm", "@include \"a.bfm\"\n@define B 1\n");
    let main = fixture.write("main.bfm", "@include \"a.bfm\"\n+{A}\n+{B}\n");

    assert_eq!(expanded(&main), "++");
}

#[test]
fn a_path_is_relative_to_the_file_that_wrote_it() {
    let fixture = Fixture::new("relative");
    fixture.write("deep/near.bfm", "@define N 4\n");
    // `sibling.bfm` names its neighbour, not the neighbour of the file that
    // included it. Running from anywhere has to give the same answer.
    fixture.write("deep/sibling.bfm", "@include \"near.bfm\"\n");
    let main = fixture.write("main.bfm", "@include \"deep/sibling.bfm\"\n+{N}\n");

    assert_eq!(expanded(&main), "++++");
}

#[test]
fn an_error_inside_an_included_file_names_that_file_and_shows_its_lines() {
    let fixture = Fixture::new("error-inside");
    fixture.write("lib.bfm", "@define A 1\n@var thing at oops\n");
    let main = fixture.write("main.bfm", "@include \"lib.bfm\"\n");

    let failure = expand_file(&main).unwrap_err();
    let report = failure.report();
    assert!(report.contains("lib.bfm"), "{report}");
    assert!(report.contains("oops"), "{report}");
    // The line is the included file's own, not a position in the buffer the
    // expander happens to hold both files in.
    assert_eq!(failure.error.location().line, 2);
}

#[test]
fn source_given_as_text_has_nothing_to_resolve_against() {
    let error = expand("@include \"lib.bfm\"\n").unwrap_err();
    assert!(
        matches!(error, MacroError::IncludeWithoutAFile { .. }),
        "{error:?}"
    );
    assert!(
        error
            .hint()
            .is_some_and(|hint| hint.contains("Expand a file"))
    );
}

#[test]
fn a_missing_file_says_which_one() {
    let fixture = Fixture::new("missing");
    let main = fixture.write("main.bfm", "@include \"nowhere.bfm\"\n");

    let failure = expand_file(&main).unwrap_err();
    let MacroError::IncludeUnreadable { path, .. } = &failure.error else {
        panic!("{:?}", failure.error);
    };
    assert!(path.ends_with("nowhere.bfm"), "{}", path.display());
}

#[test]
fn an_include_inside_a_macro_body_is_refused() {
    let fixture = Fixture::new("inside-macro");
    fixture.write("lib.bfm", "@define A 1\n");
    let main = fixture.write("main.bfm", "@macro m {\n@include \"lib.bfm\"\n}\n@m\n");

    assert!(matches!(
        expand_file(&main).unwrap_err().error,
        MacroError::DeclarationInsideMacro { .. }
    ));
}

#[test]
fn a_path_is_quoted_and_the_quote_is_closed() {
    let fixture = Fixture::new("quoting");
    for (source, what) in [
        ("@include lib.bfm\n", "unquoted"),
        ("@include \"lib.bfm\n", "unclosed"),
        ("@include \"\"\n", "empty"),
    ] {
        let main = fixture.write("main.bfm", source);
        let failure = expand_file(&main).unwrap_err();
        assert!(
            matches!(failure.error, MacroError::MalformedDirective { .. }),
            "{what}: {:?}",
            failure.error
        );
    }
}

/// A quoted path is prose to every reader but the one that resolves it, and a
/// `{` inside one must not open anything on the way past.
#[test]
fn a_skipped_branch_passes_over_a_quoted_path() {
    let fixture = Fixture::new("skipped-path");
    let main = fixture.write(
        "main.bfm",
        "@ifdef MISSING\n@include \"lib{1}.bfm\"\n@endif\n+\n",
    );

    assert_eq!(expanded(&main), "+");
}

/// The claim the emission ban buys: an instruction from a library reports the
/// line that invoked it, in the file somebody is reading.
#[test]
fn every_byte_still_comes_from_the_file_being_expanded() {
    let fixture = Fixture::new("origins");
    fixture.write("lib.bfm", "@macro three {\n+++\n}\n");
    let main = fixture.write("main.bfm", "@include \"lib.bfm\"\n@three\n");

    let expansion = expand_file(&main).unwrap_or_else(|f| panic!("{}", f.report()));
    let root_lines = std::fs::read_to_string(&main).unwrap().lines().count();
    for offset in 0..expansion.brainfuck().len() {
        let origin = expansion.origin(offset).expect("every byte has an origin");
        assert_eq!(origin.line, 2, "byte {offset} came from the library");
        assert!(origin.line <= root_lines, "outside the file being expanded");
    }
}
