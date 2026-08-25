//! Every warning points at the thing it is about.
//!
//! This is the property the validator quietly failed for as long as it has
//! existed: it threaded one `SourceLocation::start()` into every warning, so a
//! program with an empty loop on line 4 and `[--]` on line 7 reported both at
//! line 1, column 1. Nothing caught it, because nothing asserted the location
//! meant anything.
//!
//! Every warning the validator can currently produce is about a loop, so the
//! character at the reported position must be `[`. That is a weak-sounding
//! claim which happens to be exactly strong enough: it fails for any drift
//! between the validator's index arithmetic and the parser's numbering, which
//! is the only way this can break.

use gyrus::{BfWarning, parse_with_debug, validate};
use proptest::prelude::*;

/// The character at a location, addressed the way `SourceLocation` counts:
/// `offset` is a character index into the source, not a byte index.
fn char_at(source: &str, offset: usize) -> Option<char> {
    source.chars().nth(offset)
}

fn location_of(warning: &BfWarning) -> gyrus::SourceLocation {
    match warning {
        BfWarning::EmptyLoop { location }
        | BfWarning::ExtremeNesting { location, .. }
        | BfWarning::SuspiciousPattern { location, .. }
        | BfWarning::DeadCode { location, .. } => *location,
        // `BfWarning` is non-exhaustive; a variant added later needs its own
        // arm here rather than a silent pass.
        other => panic!("unhandled warning variant: {other:?}"),
    }
}

/// Balanced-bracket programs, with the patterns the validator has opinions
/// about deliberately over-represented, plus whitespace and comments so the
/// location arithmetic has to survive them.
fn arb_program() -> impl Strategy<Value = String> {
    let leaf = prop::sample::select(vec![
        "+",
        "-",
        ">",
        "<",
        ".",
        ",",
        " ",
        "\n",
        "\n\n",
        "* a comment\n",
        "[]",
        "[-]",
        "[+]",
        "[--]",
        "[++]",
        "[>]",
        "[->+<]",
    ]);
    prop::collection::vec(leaf, 0..24).prop_map(|parts| parts.concat())
}

proptest! {
    #[test]
    fn every_warning_points_at_a_bracket(source in arb_program()) {
        let Ok((instructions, debug_info)) = parse_with_debug(&source) else {
            return Ok(());
        };
        for warning in validate(&instructions, &debug_info) {
            let location = location_of(&warning);
            let found = char_at(&source, location.offset);
            prop_assert_eq!(
                found,
                Some('['),
                "warning {:?} points at {:?} (line {}, column {}, offset {}), not '[', in {:?}",
                warning,
                found,
                location.line,
                location.column,
                location.offset,
                source
            );
        }
    }

    /// The reported line and column must agree with the offset. A location
    /// that is internally inconsistent is worse than one that is merely wrong,
    /// because two consumers will disagree about where it points.
    #[test]
    fn line_and_column_agree_with_the_offset(source in arb_program()) {
        let Ok((instructions, debug_info)) = parse_with_debug(&source) else {
            return Ok(());
        };
        for warning in validate(&instructions, &debug_info) {
            let location = location_of(&warning);
            let before: String = source.chars().take(location.offset).collect();
            let line = before.matches('\n').count() + 1;
            let column = before.chars().rev().take_while(|c| *c != '\n').count() + 1;
            prop_assert_eq!(location.line, line, "line disagrees with offset in {:?}", source);
            prop_assert_eq!(location.column, column, "column disagrees with offset in {:?}", source);
        }
    }
}
