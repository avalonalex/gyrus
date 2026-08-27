//! The lexical rules of `.bfm`, in one place.
//!
//! Three things read this language, and they are not the same reader: the
//! expander walks it with a [`SourceLocation`](gyrus::SourceLocation) it
//! advances a character at a time; `skip_body` walks to a macro body's closing
//! brace without interpreting anything on the way; and the "defined below this
//! line" hint walks ahead of the cursor over text the expander has not reached.
//!
//! What they must agree about is small and was written out three times, with
//! four differences between the copies:
//!
//! - where a `*` comment ends -- one stopped at the end of the line, one also
//!   at a body's closing brace, and the third did not know comments existed,
//!   so a `{` in prose silently switched the hint off;
//! - how far a character literal reaches, and whether a newline ends one;
//! - whether a `{` opens something or is a repeat count.
//!
//! Each of those has cost a bug. They are free functions over `(chars,
//! offset)` rather than methods because the three callers do not share a
//! cursor -- only the text.

/// Instructions a repeat count may follow. Brackets are deliberately absent:
/// `[{3}` would mean three loop openings, which is not a thing anyone means.
pub(crate) const REPEATABLE: [char; 6] = ['+', '-', '<', '>', '.', ','];

/// Whether only blanks separate `from` from the start of its line, or from
/// `boundary` -- the offset the current scan began at, which is a macro body's
/// first character while one is being expanded.
///
/// A directive must start its line, and the start of a body counts as one, or
/// a one-line `@macro reset { @clear }` reads its own invocation as prose.
pub(crate) fn at_line_start(chars: &[char], from: usize, boundary: usize) -> bool {
    chars[boundary.min(from)..from]
        .iter()
        .rev()
        .take_while(|&&c| c != '\n')
        .all(|&c| c == ' ' || c == '\t')
}

/// Past a `*` comment beginning at `from`.
///
/// To the end of the line -- or, inside a macro body, to a `}` that closes it
/// *and ends the line*. Both halves of that are needed. Without the exception,
/// a one-line `@macro clear { [-] * clears it }` -- the shape the
/// documentation uses -- reports its own body as never closed. Without "and
/// ends the line", a `}` written in prose closes the body where it stands, so
/// `* clears the } cell` inside a multi-line body ends it three lines early.
///
/// A brace that closes a body is the last thing on its line; one in a sentence
/// is not. That is the whole distinction, and stating it here is what lets the
/// expander and the body reader agree about every comment that is not this
/// one deliberate case.
pub(crate) fn comment(chars: &[char], from: usize, closing_brace_ends_it: bool) -> usize {
    let mut at = from;
    while let Some(&c) = chars.get(at) {
        if c == '\n' {
            break;
        }
        if closing_brace_ends_it && c == '}' && ends_the_line(chars, at + 1) {
            break;
        }
        at += 1;
    }
    at
}

/// Whether only blanks separate `from` from the end of its line.
fn ends_the_line(chars: &[char], from: usize) -> bool {
    chars[from.min(chars.len())..]
        .iter()
        .take_while(|&&c| c != '\n')
        .all(|&c| c == ' ' || c == '\t')
}

/// Past a character literal beginning at `from`, and whether it was closed.
///
/// A literal never spans a line. Without that an unclosed quote swallows the
/// rest of the file looking for its pair and then blames a line far below the
/// one it is on.
pub(crate) fn literal(chars: &[char], from: usize) -> (usize, bool) {
    debug_assert_eq!(chars.get(from), Some(&'\''));
    let mut at = from + 1;
    while let Some(&c) = chars.get(at) {
        match c {
            '\n' => return (at, false),
            '\\' if chars.get(at + 1).is_some_and(|&next| next != '\n') => at += 2,
            '\'' => return (at + 1, true),
            _ => at += 1,
        }
    }
    (at, false)
}

/// Whether the `{` at `at` is a repeat count rather than a brace.
///
/// It is one exactly when it abuts an instruction -- which is also why `+ {3}`
/// is refused rather than read as a count: the space is the difference.
pub(crate) fn is_repeat_count(chars: &[char], at: usize) -> bool {
    chars.get(at) == Some(&'{')
        && at > 0
        && chars.get(at - 1).is_some_and(|c| REPEATABLE.contains(c))
}

/// Past a `{...}` repeat count beginning at `from`, and whether it was closed.
///
/// The body may hold a character literal, and that literal may hold the brace
/// that would otherwise end the count: `+{'}'}` is the obvious thing to write.
pub(crate) fn repeat_count(chars: &[char], from: usize) -> (usize, bool) {
    debug_assert_eq!(chars.get(from), Some(&'{'));
    let mut at = from + 1;
    while let Some(&c) = chars.get(at) {
        match c {
            '\n' => return (at, false),
            '}' => return (at + 1, true),
            '\'' => at = literal(chars, at).0,
            _ => at += 1,
        }
    }
    (at, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(text: &str) -> Vec<char> {
        text.chars().collect()
    }

    #[test]
    fn a_comment_ends_at_the_line_or_at_a_body_brace_that_ends_it() {
        // A brace in the middle of a sentence is prose, in a body or out of one.
        let prose = chars("* a } brace\nnext");
        assert_eq!(comment(&prose, 0, false), 11);
        assert_eq!(
            comment(&prose, 0, true),
            11,
            "a brace in a sentence is not the end"
        );

        // A brace that ends the line is the one closing the body.
        let closes = chars("* clears it }\nnext");
        assert_eq!(comment(&closes, 0, true), 12);
        assert_eq!(
            comment(&closes, 0, false),
            13,
            "outside a body it is still prose"
        );

        // Trailing blanks do not change which it is.
        let spaced = chars("* clears it }  \nnext");
        assert_eq!(comment(&spaced, 0, true), 12);
    }

    #[test]
    fn a_literal_never_spans_a_line() {
        assert_eq!(literal(&chars("'a'x"), 0), (3, true));
        assert_eq!(literal(&chars("'\\''"), 0), (4, true));
        assert_eq!(literal(&chars("'\\n'"), 0), (4, true));
        // Unclosed: stops at the newline and says so, rather than running on.
        assert_eq!(literal(&chars("'a\nmore"), 0), (2, false));
        assert_eq!(literal(&chars("'a"), 0), (2, false));
    }

    #[test]
    fn a_brace_is_a_count_only_when_it_abuts_an_instruction() {
        assert!(is_repeat_count(&chars("+{3}"), 1));
        assert!(
            !is_repeat_count(&chars("+ {3}"), 2),
            "a space is the difference"
        );
        assert!(
            !is_repeat_count(&chars("[{3}"), 1),
            "brackets are not repeatable"
        );
        assert!(!is_repeat_count(&chars("{3}"), 0), "nothing precedes it");
    }

    #[test]
    fn a_count_holds_a_literal_that_holds_its_own_brace() {
        assert_eq!(repeat_count(&chars("{'}'}"), 0), (5, true));
        assert_eq!(repeat_count(&chars("{65}"), 0), (4, true));
        assert_eq!(repeat_count(&chars("{65\nmore"), 0), (3, false));
    }

    #[test]
    fn a_line_start_tolerates_blanks_and_a_body_boundary() {
        let c = chars("  @to x");
        assert!(at_line_start(&c, 2, 0));
        let mid = chars("+ @to x");
        assert!(!at_line_start(&mid, 2, 0));
        // The body's first character counts as a line start.
        assert!(at_line_start(&mid, 2, 2));
    }
}
