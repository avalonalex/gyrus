//! The lexical rules of `.bfm`, in one place.
//!
//! Four things read this language, and they are not the same reader: the
//! expander walks it with a [`SourceLocation`](gyrus::SourceLocation) it
//! advances a character at a time; `skip_body` walks to a macro body's closing
//! brace without interpreting anything on the way; the "defined below this
//! line" hint walks ahead of the cursor over text the expander has not
//! reached; and `skip_branch` walks a conditional's untaken branch to its
//! `@endif` without expanding a character of it.
//!
//! What they must agree about is small and was written out three times, with
//! four differences between the copies:
//!
//! - where a `*` comment ends: one stopped at the end of the line, and one
//!   also at a body's closing brace -- see [`comment`];
//! - that the third reader did not know comments existed at all, so a `{` in
//!   prose silently switched the hint off for the rest of the file;
//! - how far a character literal *reaches*, and whether a newline ends one.
//!   What is inside one is decoded elsewhere, by `expand`'s `character`, which
//!   is a fifth reader this module does not yet cover;
//! - whether a `{` opens something or is a repeat count;
//! - how far a `"quoted path"` reaches. A path is prose to every reader but
//!   the one that resolves it, and `@include "lib{1}.bfm"` in a branch that
//!   is skipped must not open a brace on the way past.
//!
//! A comment is prose and holds no literals: a `'` inside one is an
//! apostrophe. That is why [`comment`] does not consult [`literal`] while
//! every other rule here does.
//!
//! Each of those has cost a bug. The fourth reader arrived after this module
//! did and was written as a caller, which is the whole point of it.
//!
//! They are free functions rather than a cursor type because every rule here
//! is *context-parameterised, not stateful*: `comment` needs to know whether a
//! body is being read, and the two line predicates need the offset the current
//! scan began at. Those come from the expander's frame stack, not from the
//! text, so a `Lexer` owning an offset would still be handed them on every
//! call -- a wrapper rather than an abstraction. [`step`] is where the state
//! that *is* shared, the brace depth, gets its one derivation.

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
    chars[line_start(chars, from, boundary)..from]
        .iter()
        .all(|&c| is_blank(c))
}

/// A space or a tab: the whitespace that may precede something and still leave
/// it first on its line.
fn is_blank(c: char) -> bool {
    c == ' ' || c == '\t'
}

/// Where the line containing `from` begins, or `boundary` if that is later.
fn line_start(chars: &[char], from: usize, boundary: usize) -> usize {
    let floor = boundary.min(from);
    chars[floor..from]
        .iter()
        .rposition(|&c| c == '\n')
        .map_or(floor, |newline| floor + newline + 1)
}

/// Whether `at` sits on a line whose first non-blank character is `@`.
///
/// A character literal only appears where a *value* belongs, and on a line of
/// BrainFuck there are two such places: inside a repeat count, which
/// [`repeat_count`] reads, and among a directive's operands. Everywhere else a
/// `'` is an apostrophe.
///
/// Both halves have cost a bug. Treating every `'` as a literal made `it's }
/// fine` in a macro body hide the brace after it; treating none of them as one
/// made `@n('}')` count its quoted brace as the body's end. Skipping a
/// directive's whole line instead is no good either -- `@macro inner { + }`
/// carries braces that must still be counted.
pub(crate) fn on_directive_line(chars: &[char], at: usize, boundary: usize) -> bool {
    chars[line_start(chars, at, boundary)..]
        .iter()
        .find(|&&c| !is_blank(c))
        .is_some_and(|&c| c == '@')
}

/// The directive name at `at`, and where it ends.
///
/// `chars[at]` is the `@`. Shared by the two readers that want a name without
/// a cursor to carry: the lookahead asking whether a line declares something,
/// and the skip over a false conditional counting the ones that nest. The
/// expander reads the same name through its own `identifier`, which spells the
/// rule over the scanner's position instead -- what the two share, and what
/// this module is for, is which *characters* those are.
///
/// A slice rather than a `String`: the skip asks this of every line-start `@`
/// in a branch it is throwing away, and does it again on every invocation of
/// the body that branch is in.
pub(crate) fn spelling(chars: &[char], at: usize) -> (&[char], usize) {
    let start = at + 1;
    let mut end = start;
    if chars.get(start).copied().is_some_and(is_identifier_start) {
        while chars.get(end).copied().is_some_and(is_identifier_char) {
            end += 1;
        }
    }
    (&chars[start..end], end)
}

/// Whether a name read by [`spelling`] is `text`.
pub(crate) fn matches(name: &[char], text: &str) -> bool {
    name.iter().copied().eq(text.chars())
}

/// A character an identifier may begin with.
///
/// Narrower than the rest, and separate for that reason: `@3` is not a
/// directive, and a name could not start with a digit anywhere a number may be
/// written in its place.
pub(crate) fn is_identifier_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

/// A character an identifier may contain after its first.
pub(crate) fn is_identifier_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
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
    let end = chars[from..]
        .iter()
        .position(|&c| c == '\n')
        .map_or(chars.len(), |newline| from + newline);
    if !closing_brace_ends_it {
        return end;
    }
    // Decided once for the comment rather than once per character: the rule
    // is about the *last* thing on the line, so finding that is the whole
    // question. Asking it of every character made a long comment inside a
    // macro body -- re-read on every invocation -- measurably slower.
    match chars[from..end].iter().rposition(|&c| !is_blank(c)) {
        Some(last) if chars[from + last] == '}' => from + last,
        _ => end,
    }
}

/// Past a `".."` path beginning at `from`, and whether it was closed.
///
/// The same shape as [`literal`] with a different delimiter, and separate
/// because the two mean different things: what is inside a `'x'` is decoded as
/// a character, and what is inside a `"x"` is handed to the filesystem
/// verbatim. A backslash is *not* an escape here for that reason -- a Windows
/// path is full of them, and `"a\\b.bfm"` should name the file it looks like.
pub(crate) fn quoted(chars: &[char], from: usize) -> (usize, bool) {
    debug_assert_eq!(chars.get(from), Some(&'"'));
    let mut at = from + 1;
    while let Some(&c) = chars.get(at) {
        match c {
            '\n' => return (at, false),
            '"' => return (at + 1, true),
            _ => at += 1,
        }
    }
    (at, false)
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

/// How a value ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValueEnd {
    /// At the delimiter it was looking for, which is the only way it should
    /// end.
    Delimiter,
    /// At a newline or the end of the file: the delimiter was forgotten.
    Unclosed,
    /// Inside a character literal that was never closed. Told apart from
    /// `Unclosed` because "no closing quote" and "no closing '}'" send the
    /// reader to different ends of the same line.
    OpenLiteral,
}

/// Past a value beginning at `from`: everything up to the first character
/// `ends` accepts that is not inside a character literal.
///
/// Both places a value appears -- a repeat count and a directive's operands --
/// read one this way, so a literal is opaque to both. `+{'}'}` is the obvious
/// thing to write, and so is `@print(',')`.
pub(crate) fn value(chars: &[char], from: usize, ends: impl Fn(char) -> bool) -> (usize, ValueEnd) {
    let mut at = from;
    while let Some(&c) = chars.get(at) {
        if c == '\n' {
            return (at, ValueEnd::Unclosed);
        }
        if ends(c) {
            return (at, ValueEnd::Delimiter);
        }
        if c == '\'' {
            let (end, closed) = literal(chars, at);
            if !closed {
                return (end, ValueEnd::OpenLiteral);
            }
            at = end;
            continue;
        }
        at += 1;
    }
    (at, ValueEnd::Unclosed)
}

/// A `{...}` repeat count at `from`: where its text stops, where the count
/// itself stops, and how it ended.
///
/// Two offsets because the caller wants the text and the walkers want to be
/// past the brace. Returning only the second made the one caller that needs
/// the first subtract one, guarded by a match on the variant, in another file.
pub(crate) fn repeat_count(chars: &[char], from: usize) -> (usize, usize, ValueEnd) {
    debug_assert_eq!(chars.get(from), Some(&'{'));
    let (text_end, ending) = value(chars, from + 1, |c| c == '}');
    match ending {
        ValueEnd::Delimiter => (text_end, text_end + 1, ending),
        _ => (text_end, text_end, ending),
    }
}

/// What the character at `at` is, and where it ends.
///
/// The two walks over this language -- one finding a macro body's closing
/// brace, one looking ahead for a declaration -- classify characters
/// identically and differ only in what they do with a brace and when they
/// stop. Extracting the rules and leaving the classification in both is the
/// halfway point where the shared code stops looking duplicated while the
/// decisions still are: that copy diverged twice, the second time inside the
/// commit that extracted the rules.
pub(crate) enum Step {
    /// Past the construct at `at`; the depth is unchanged.
    Past(usize),
    /// A `{` that opens a macro body.
    Open,
    /// A `}` that closes one.
    Close,
}

pub(crate) fn step(chars: &[char], at: usize, boundary: usize, in_body: bool) -> Step {
    match chars[at] {
        '*' => Step::Past(comment(chars, at, in_body)),
        // Only where a value belongs. A directive's line is not skipped whole,
        // because `@macro inner { + }` carries braces that still count.
        '\'' if on_directive_line(chars, at, boundary) => Step::Past(literal(chars, at).0),
        '"' if on_directive_line(chars, at, boundary) => Step::Past(quoted(chars, at).0),
        '{' if is_repeat_count(chars, at) => Step::Past(repeat_count(chars, at).1),
        '{' => Step::Open,
        '}' => Step::Close,
        _ => Step::Past(at + 1),
    }
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
        // Text end, then past the brace.
        assert_eq!(
            repeat_count(&chars("{'}'}"), 0),
            (4, 5, ValueEnd::Delimiter)
        );
        assert_eq!(repeat_count(&chars("{65}"), 0), (3, 4, ValueEnd::Delimiter));
        assert_eq!(
            repeat_count(&chars("{65\nmore"), 0),
            (3, 3, ValueEnd::Unclosed)
        );
        // Told apart, because the two send the reader to different ends of the
        // same line.
        assert_eq!(
            repeat_count(&chars("{'a}\nmore"), 0),
            (4, 4, ValueEnd::OpenLiteral)
        );
    }

    #[test]
    fn a_value_stops_at_its_delimiter_and_not_inside_a_literal() {
        // The same reader the count uses, which is the point of it.
        let comma = chars("','),rest");
        assert_eq!(
            value(&comma, 0, |c| c == ',' || c == ')'),
            (3, ValueEnd::Delimiter)
        );
        let spaced = chars("' ' more");
        assert_eq!(
            value(&spaced, 0, char::is_whitespace),
            (3, ValueEnd::Delimiter)
        );
    }

    #[test]
    fn a_spelling_stops_where_the_name_does() {
        for (source, name, end) in [
            ("@ifdef X", "ifdef", 6),
            ("@end_if", "end_if", 7),
            ("@", "", 1),
            // The scanner's own reader refuses a leading digit, and these two
            // are one rule or they are a bug waiting to be written.
            ("@3endif", "", 1),
        ] {
            let text = chars(source);
            let (word, at) = spelling(&text, 0);
            assert!(matches(word, name), "{source}: {word:?}");
            assert_eq!(at, end, "{source}");
        }
    }

    #[test]
    fn a_quoted_path_reaches_its_closing_quote_and_no_further() {
        for (source, end, closed) in [
            ("@include \"lib.bfm\"", 18, true),
            // A brace inside a path is a character of a filename, and the
            // reader that skips a branch must not take it for anything else.
            ("@include \"lib{1}.bfm\"", 21, true),
            // A backslash is a path separator, not an escape: the quote after
            // it still closes.
            ("@include \"a\\b.bfm\"", 18, true),
            ("@include \"unclosed\n+", 18, false),
        ] {
            assert_eq!(quoted(&chars(source), 9), (end, closed), "{source:?}");
        }
    }

    /// The whole reason it is here rather than in the expander: `step` is what
    /// the two skipping walks share, and a `{` in a path would otherwise open
    /// a body in a branch nobody is expanding.
    #[test]
    fn a_step_passes_over_a_quoted_path_on_a_directive_line() {
        let line = chars("@include \"lib{1}.bfm\"\n+");
        assert!(matches!(step(&line, 9, 0, false), Step::Past(21)));
        // Off a directive line it is prose, and a `\"` means nothing.
        let prose = chars("+ a \"quote\" in prose\n");
        assert!(matches!(step(&prose, 4, 0, false), Step::Past(5)));
    }

    #[test]
    fn a_directive_line_is_where_a_literal_may_appear() {
        let directive = chars("@n(',')\n+ it's prose");
        assert!(on_directive_line(&directive, 3, 0), "inside the operands");
        assert!(
            !on_directive_line(&directive, 12, 0),
            "an apostrophe in prose"
        );
        // A body's first character counts as the start of a line, as it does
        // for a directive.
        let inline = chars("@clear ");
        assert!(on_directive_line(&inline, 2, 0));
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
