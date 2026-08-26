//! Runtime errors name the `.bfm`, not the expansion.
//!
//! This is the criterion the whole feature is for. A preprocessor that only
//! repeated characters would be a shell script; what makes this one worth
//! putting inside gyrus is that a program which expands to a wall of
//! BrainFuck still reports its failures at the line and column somebody wrote.
//!
//! So these assert exact positions, never `is_some()`. A location that is
//! never checked against a number is a location nobody is checking -- the
//! corpus suite already carries that lesson, in a test whose error arm was
//! unreachable for months.

use gyrus::{
    BfError, DebugInfo, ExecutionConfig, ExecutionConfigBuilder, SourceLocation, interpret_with_io,
    io::StringIo, parse_with_debug,
};
use gyrus_macro::Expansion;

/// Expand, remap, run. Returns the failure and the expansion that produced it.
fn run(source: &str, config: ExecutionConfig) -> (BfError, Expansion) {
    let expansion =
        gyrus_macro::expand(source).unwrap_or_else(|e| panic!("{}", e.format_with_source(source)));
    let (instructions, expanded) = parse_with_debug(expansion.brainfuck()).expect("parses");
    let debug_info = expansion.remap(&expanded);
    let error = execute(&instructions, config, &debug_info).expect_err("expected a failure");
    (error, expansion)
}

fn execute(
    instructions: &[gyrus::Instruction],
    config: ExecutionConfig,
    debug_info: &DebugInfo,
) -> Result<(), BfError> {
    let (mut input, mut output) = (StringIo::empty(), StringIo::empty());
    interpret_with_io(
        instructions,
        config,
        &mut input,
        &mut output,
        Some(debug_info),
    )
    .map(|_| ())
}

/// The rendered message without its colors.
///
/// `gyrus` highlights source context a character at a time, so every character
/// of the macro line arrives wrapped in its own escape sequence and no line of
/// the original survives as a contiguous substring. Stripping is what lets a
/// test assert on the text a reader sees rather than on the bytes.
fn plain(rendered: &str) -> String {
    let mut out = String::new();
    let mut chars = rendered.chars();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        // CSI ... final byte in @-~; this is all gyrus emits.
        if chars.next() == Some('[') {
            for c in chars.by_ref() {
                if ('@'..='~').contains(&c) {
                    break;
                }
            }
        }
    }
    out
}

fn location(error: &BfError) -> SourceLocation {
    match error {
        BfError::CellOverflow {
            source_location, ..
        }
        | BfError::CellUnderflow {
            source_location, ..
        }
        | BfError::MemoryOutOfBounds {
            source_location, ..
        }
        | BfError::StepLimitExceeded {
            source_location, ..
        } => source_location.expect("the error carries no source location at all"),
        other => panic!("unexpected error {other:?}"),
    }
}

#[test]
fn a_cell_overflow_points_at_the_macro_line_that_caused_it() {
    // 300 increments under checked cells: the 256th is the one that fails, and
    // all 300 were written as a single '+' at line 2, column 1.
    let source = "@define OVERSHOOT 300\n+{OVERSHOOT}\n";
    let (error, _) = run(
        source,
        ExecutionConfigBuilder::new()
            .with_memory_size(30_000)
            .with_checked_cells()
            .build(),
    );
    let at = location(&error);
    assert_eq!((at.line, at.column), (2, 1), "{error:?}");
}

#[test]
fn an_out_of_bounds_access_points_at_the_instruction_that_made_it() {
    // Movement is free under the tape contract; the '+' is what reaches off
    // the end of an eight-cell tape. It sits at line 2, column 7.
    let source = "@define FAR 40\n>{FAR}+\n";
    let (error, _) = run(
        source,
        ExecutionConfigBuilder::new().with_memory_size(8).build(),
    );
    let at = location(&error);
    assert_eq!((at.line, at.column), (2, 7), "{error:?}");
}

#[test]
fn a_run_stopped_by_its_step_limit_still_knows_where_it_was() {
    let source = "@define WIDTH 12\n+{WIDTH}\n[]\n";
    let (error, _) = run(
        source,
        ExecutionConfigBuilder::new()
            .with_memory_size(30_000)
            .with_max_steps(200)
            .build(),
    );
    let at = location(&error);
    // Stopped inside the loop on line 3, which is the only place 200 steps can
    // land given twelve increments precede it.
    assert_eq!(at.line, 3, "{error:?}");
}

#[test]
fn the_rendered_message_shows_macro_source_and_not_the_expansion() {
    let source = "@define OVERSHOOT 300\n+{OVERSHOOT}\n";
    let (error, expansion) = run(
        source,
        ExecutionConfigBuilder::new()
            .with_memory_size(30_000)
            .with_checked_cells()
            .build(),
    );

    let rendered = plain(&error.format_with_source(expansion.source()));
    assert!(
        rendered.contains("+{OVERSHOOT}"),
        "the macro line is missing from:\n{rendered}"
    );
    assert!(
        rendered.contains("@define OVERSHOOT 300"),
        "the definition is missing from:\n{rendered}"
    );
    assert!(
        !rendered.contains("++++++++++"),
        "the expansion leaked into the message:\n{rendered}"
    );
    // The caret sits under the '+' that was written, at column 1 of line 2.
    let caret = rendered.lines().find(|l| l.contains('^')).expect("a caret");
    assert_eq!(caret.trim_end(), "       ^", "caret line was {caret:?}");
}

/// The remap is load-bearing, and this is what proves it.
///
/// Without it the same failure reports a column deep inside a single line of
/// generated BrainFuck. Asserting only that the remapped location exists would
/// pass whether or not `remap` did anything at all.
#[test]
fn without_the_remap_the_same_failure_reports_expansion_coordinates() {
    let source = "@define OVERSHOOT 300\n+{OVERSHOOT}\n";
    let expansion = gyrus_macro::expand(source).expect("expands");
    let (instructions, expanded) = parse_with_debug(expansion.brainfuck()).expect("parses");

    let config = || {
        ExecutionConfigBuilder::new()
            .with_memory_size(30_000)
            .with_checked_cells()
            .build()
    };

    let raw = execute(&instructions, config(), &expanded).expect_err("expected a failure");
    let raw_at = location(&raw);
    // One line of 300 '+', so the failure lands at line 1, column 256.
    assert_eq!((raw_at.line, raw_at.column), (1, 256), "{raw:?}");

    let remapped_info = expansion.remap(&expanded);
    let remapped =
        execute(&instructions, config(), &remapped_info).expect_err("expected a failure");
    let remapped_at = location(&remapped);
    assert_eq!(
        (remapped_at.line, remapped_at.column),
        (2, 1),
        "{remapped:?}"
    );
}
