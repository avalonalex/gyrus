//! Expansion errors, reported against the `.bfm` source.
//!
//! Every variant carries a [`SourceLocation`] in the macro source, and
//! [`MacroError::format_with_source`] renders it the way `gyrus` renders a
//! parse error: the offending line, two lines either side, and a caret. That
//! symmetry is the point -- a macro error and a BrainFuck error should be the
//! same kind of message, because to the person reading them they are.

use std::io::Write;

use crate::directive::understood;
use gyrus::SourceLocation;
use thiserror::Error;

/// What a name was declared to be.
///
/// A pair of `&'static str` would do the same job in prose, but two of them
/// plus an advice string made `MacroError` large enough that returning it by
/// value was worth a lint. This is one byte and reads better at the use site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Constant,
    Variable,
    Macro,
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Kind::Constant => "constant",
            Kind::Variable => "variable",
            Kind::Macro => "macro",
        })
    }
}

#[non_exhaustive]
#[derive(Debug, Error)]
pub enum MacroError {
    #[error("Undefined symbol '{name}' at {location}")]
    UndefinedSymbol {
        name: String,
        location: SourceLocation,
        /// Everything defined at the point of use, for the "did you mean" line.
        defined: Vec<String>,
        /// Whether a later line defines it. Expansion is a single pass, so
        /// this is a common and otherwise baffling way to fail.
        defined_later: bool,
    },

    #[error("'@{name}' is not implemented yet at {location}")]
    PlannedDirective {
        name: String,
        location: SourceLocation,
    },

    #[error("Unknown directive '@{name}' at {location}")]
    UnknownDirective {
        name: String,
        location: SourceLocation,
    },

    #[error("Malformed @{directive} at {location}: {detail}")]
    MalformedDirective {
        directive: String,
        detail: String,
        location: SourceLocation,
    },

    #[error("'{name}' is already defined at {location}")]
    Redefinition {
        name: String,
        first: SourceLocation,
        location: SourceLocation,
    },

    #[error("Malformed repeat count at {location}: {detail}")]
    BadRepeatCount {
        detail: String,
        location: SourceLocation,
    },

    #[error("A repeat count cannot follow '{instruction}' at {location}")]
    RepeatNotRepeatable {
        instruction: char,
        location: SourceLocation,
    },

    #[error("Stray '{brace}' at {location}")]
    StrayBrace {
        brace: char,
        location: SourceLocation,
    },

    #[error("'{name}' is a {found}, not a {wanted}, at {location}")]
    WrongKind {
        name: String,
        found: Kind,
        wanted: Kind,
        location: SourceLocation,
        declared: SourceLocation,
    },

    #[error("The cursor's position is not known at {location}, so '@to {name}' cannot move to it")]
    PositionUnknown {
        name: String,
        location: SourceLocation,
        /// The `[` of the loop that lost it.
        lost_at: SourceLocation,
    },

    #[error("'@to' at {location} is inside a loop that does not put the cursor back")]
    MovingInsideUnbalancedLoop {
        location: SourceLocation,
        /// The `[` of the loop in question.
        loop_at: SourceLocation,
    },

    #[error("'@{name}' takes {expected} argument(s) at {location}, given {actual}")]
    ArgumentCount {
        name: String,
        expected: usize,
        actual: usize,
        location: SourceLocation,
    },

    #[error("'@{name}' expands itself at {location}")]
    CircularMacro {
        name: String,
        /// The invocations in flight, outermost first.
        chain: Vec<String>,
        location: SourceLocation,
    },

    #[error("'@{directive}' cannot appear inside a macro body, at {location}")]
    DeclarationInsideMacro {
        directive: &'static str,
        location: SourceLocation,
    },

    #[error("Unmatched '[' at {location}")]
    UnmatchedOpenBracket { location: SourceLocation },

    #[error("Unmatched ']' at {location}")]
    UnmatchedCloseBracket { location: SourceLocation },

    #[error("Repeat count too large: {count} at {location} exceeds the {limit} limit")]
    RepeatTooLarge {
        count: u64,
        limit: u64,
        location: SourceLocation,
    },

    #[error("Cell {cell} at {location} is past the {limit} the whole file may expand to")]
    CellTooFar {
        cell: u64,
        limit: usize,
        location: SourceLocation,
    },

    #[error(
        "Expansion is too large: {emitted} instructions at {location} exceeds the {limit} limit"
    )]
    ExpansionTooLarge {
        emitted: usize,
        limit: usize,
        location: SourceLocation,
    },
}

impl MacroError {
    /// A name used where the other kind belongs.
    pub(crate) fn wrong_kind(
        name: &str,
        found: Kind,
        wanted: Kind,
        location: SourceLocation,
        declared: SourceLocation,
    ) -> Self {
        MacroError::WrongKind {
            name: name.to_string(),
            found,
            wanted,
            location,
            declared,
        }
    }

    /// Where in the `.bfm` source this went wrong.
    pub fn location(&self) -> SourceLocation {
        match self {
            MacroError::UndefinedSymbol { location, .. }
            | MacroError::PlannedDirective { location, .. }
            | MacroError::UnknownDirective { location, .. }
            | MacroError::MalformedDirective { location, .. }
            | MacroError::Redefinition { location, .. }
            | MacroError::BadRepeatCount { location, .. }
            | MacroError::RepeatNotRepeatable { location, .. }
            | MacroError::StrayBrace { location, .. }
            | MacroError::WrongKind { location, .. }
            | MacroError::PositionUnknown { location, .. }
            | MacroError::MovingInsideUnbalancedLoop { location, .. }
            | MacroError::UnmatchedOpenBracket { location }
            | MacroError::UnmatchedCloseBracket { location }
            | MacroError::ArgumentCount { location, .. }
            | MacroError::CircularMacro { location, .. }
            | MacroError::DeclarationInsideMacro { location, .. }
            | MacroError::CellTooFar { location, .. }
            | MacroError::RepeatTooLarge { location, .. }
            | MacroError::ExpansionTooLarge { location, .. } => *location,
        }
    }

    /// What to try instead.
    ///
    /// Matched exhaustively on purpose. A `_ => None` arm compiles for every
    /// variant added later, so each one would ship hintless by default rather
    /// than by decision -- and with `@macro` and the conditionals still to
    /// come that is a standing invitation. The variants that genuinely have nothing
    /// to add carry their advice in `detail` and say so here.
    pub fn hint(&self) -> Option<String> {
        match self {
            MacroError::UndefinedSymbol {
                name,
                defined,
                defined_later,
                ..
            } => Some(if *defined_later {
                format!(
                    "'{name}' is defined below this line. Expansion is a single pass, \
                     so a @define has to come before every use of it."
                )
            } else {
                match nearest(name, defined) {
                    Some(near) => format!("Did you mean '{near}'?"),
                    None if defined.is_empty() => {
                        "Nothing is defined above this line. Add `@define NAME VALUE` or \
                         `@var NAME at N` before it."
                            .to_string()
                    }
                    None => format!("Defined above this line: {}.", defined.join(", ")),
                }
            }),
            MacroError::PlannedDirective { name, .. } => Some(format!(
                "@{name} is part of the design but not built yet. {}",
                understood()
            )),
            MacroError::UnknownDirective { .. } | MacroError::MalformedDirective { .. } => {
                Some(format!(
                    "{} A directive must start its line; an '@' anywhere else is \
                     an ordinary comment character.",
                    understood()
                ))
            }
            MacroError::Redefinition { first, .. } => Some(format!(
                "It was first defined at {first}. A symbol is defined once, so that a name means one thing."
            )),
            MacroError::RepeatNotRepeatable { .. } => Some(
                "Repeat counts apply to + - < > . and , -- repeating a bracket would not mean anything."
                    .to_string(),
            ),
            MacroError::StrayBrace { brace: '{', .. } => Some(
                "A repeat count attaches to the instruction before it, with nothing between: \
                 `+{3}`, not `+ {3}`."
                    .to_string(),
            ),
            MacroError::StrayBrace { .. } => {
                Some("There is no repeat count open here for a '}' to close.".to_string())
            }
            MacroError::WrongKind {
                name,
                found,
                declared,
                ..
            } => Some(format!(
                "'{name}' was declared a {found} at {declared}. {}.",
                // What the kind it *is* would be written as. Matching on the
                // kind that was wanted gave the same text only because two
                // kinds make one the negation of the other; a third would
                // have needed a special case here.
                match found {
                    Kind::Constant => format!("`+{{{name}}}` uses its value as a repeat count"),
                    Kind::Variable => format!("`@to {name}` moves the cursor to it"),
                    Kind::Macro => format!("`@{name}` expands it"),
                }
            )),
            MacroError::PositionUnknown { lost_at, .. } => Some(format!(
                "The loop at {lost_at} does not put the cursor back where it found it, so \
                 where it ends up depends on the data. That is ordinary BrainFuck -- `[>]` \
                 is a scan -- and it is only a problem for `@to`. Say `@here NAME` once you \
                 know where the scan landed."
            )),
            MacroError::MovingInsideUnbalancedLoop { loop_at, .. } => Some(format!(
                "The loop at {loop_at} leaves the cursor somewhere other than it found it, \
                 so this `@to` would emit the right movement on the first iteration and the \
                 wrong movement on every one after. Move the cursor with '>' and '<' inside \
                 a loop like this, or make the body put the cursor back."
            )),
            MacroError::UnmatchedOpenBracket { .. } => Some(
                "Every '[' needs a matching ']'. This is checked before expansion so the \
                 position is the one you wrote."
                    .to_string(),
            ),
            MacroError::UnmatchedCloseBracket { .. } => {
                Some("This ']' closes a loop that was never opened.".to_string())
            }
            MacroError::ArgumentCount {
                name, expected, ..
            } => Some(match expected {
                0 => format!("`@{name}` takes none, so write it without parentheses."),
                _ => format!(
                    "Its parameters were named when it was defined; `@{name}` needs {expected} \
                     of them, separated by commas."
                ),
            }),
            MacroError::CircularMacro { chain, .. } => Some(format!(
                "Expanding {} would not terminate. A macro cannot use itself, directly or \
                 through another.",
                chain
                    .iter()
                    .map(|n| format!("@{n}"))
                    .collect::<Vec<_>>()
                    .join(" -> ")
            )),
            MacroError::DeclarationInsideMacro { directive, .. } => Some(format!(
                "A macro body expands once per invocation, so an `@{directive}` in one would \
                 declare the same name again on the second. Move it above the macro."
            )),
            MacroError::CellTooFar { limit, .. } => Some(format!(
                "Reaching cell N costs N moves, so a cell past {limit} is one no program could \
                 move to. It usually means the constant it came from is not the number it \
                 was meant to be."
            )),
            MacroError::RepeatTooLarge { .. } => Some(
                "A repeat count this large is almost always a mistake in the constant it came from."
                    .to_string(),
            ),
            MacroError::ExpansionTooLarge { .. } => Some(
                "The whole file expands to more instructions than this. The map holds a source \
                 position per emitted byte, so an expansion this size costs far more memory than \
                 it looks like it should."
                    .to_string(),
            ),
            // Its `detail` is the hint: it names what was expected, in place.
            MacroError::BadRepeatCount { .. } => None,
        }
    }

    /// The error with its source line and a caret, the way a parse error reads.
    pub fn format_with_source(&self, source: &str) -> String {
        let mut out = format!("Error: {self}\n\n{}", context(source, self.location()));
        if let Some(hint) = self.hint() {
            out.push_str(&format!("\nHint: {hint}\n"));
        }
        out
    }
}

/// The closest defined name, by a cheap edit-distance-ish measure: same length
/// with one substitution, or a one-character insertion or deletion. Enough for
/// a typo, and it does not pretend to be more.
fn nearest<'a>(name: &str, defined: &'a [String]) -> Option<&'a str> {
    defined
        .iter()
        .find(|candidate| within_one_edit(name, candidate))
        .map(String::as_str)
}

fn within_one_edit(a: &str, b: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    match a.len().abs_diff(b.len()) {
        0 => a.iter().zip(&b).filter(|(x, y)| x != y).count() == 1,
        1 => {
            // Walk both, allowing exactly one skip in the longer one.
            let (long, short) = if a.len() > b.len() {
                (&a, &b)
            } else {
                (&b, &a)
            };
            let mut skipped = false;
            let mut j = 0;
            for c in long.iter() {
                if j < short.len() && short[j] == *c {
                    j += 1;
                } else if skipped {
                    return false;
                } else {
                    skipped = true;
                }
            }
            j == short.len()
        }
        _ => false,
    }
}

/// Two lines either side of the offending line, with a caret under the column.
///
/// Rendered by `gyrus`'s own `SyntaxHighlighter`, not by an imitation of it.
/// An earlier version hand-rolled a `{:5} | ` gutter, which matched the plain
/// `extract_source_context` -- but `format_with_source` calls the *highlighted*
/// variant, whose gutter is `   N | ` in colour, so a macro error and a runtime
/// error for the same file visibly disagreed. Going through the public
/// highlighter is what makes "they look like they came from the same program"
/// true rather than asserted.
fn context(source: &str, location: SourceLocation) -> String {
    use gyrus::syntax::SyntaxHighlighter;
    use termcolor::{Ansi, Color, ColorSpec, WriteColor};

    let highlighted = SyntaxHighlighter::new()
        .show_line_numbers(true)
        .highlight(source);

    let line_idx = location.line.saturating_sub(1);
    let total = highlighted.lines().len();
    let start = line_idx.saturating_sub(2);
    let end = (line_idx + 3).min(total);

    let mut buffer = Vec::new();
    if start < total {
        highlighted
            .write_ansi_range(&mut buffer, start, (line_idx + 1).min(total))
            .expect("writing to a Vec cannot fail");
    }

    // The gutter is "   N | " -- four columns plus three -- and the column is
    // 1-indexed, so the caret sits at column + 6. Same arithmetic as `gyrus`.
    let mut ansi = Ansi::new(&mut buffer);
    write!(ansi, "{}", " ".repeat(location.column + 6)).expect("writing to a Vec cannot fail");
    let mut caret = ColorSpec::new();
    caret.set_fg(Some(Color::Red)).set_bold(true);
    ansi.set_color(&caret)
        .expect("setting a colour cannot fail");
    write!(ansi, "^").expect("writing to a Vec cannot fail");
    ansi.reset().expect("resetting cannot fail");
    writeln!(ansi).expect("writing to a Vec cannot fail");

    if line_idx + 1 < end {
        highlighted
            .write_ansi_range(&mut buffer, line_idx + 1, end)
            .expect("writing to a Vec cannot fail");
    }

    String::from_utf8(buffer).expect("the highlighter emits UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rendering without its colours. `gyrus`'s highlighter emits an
    /// escape sequence per character, so no line of the source survives as a
    /// contiguous substring of the coloured output.
    fn strip_ansi(rendered: &str) -> String {
        let mut out = String::new();
        let mut chars = rendered.chars();
        while let Some(c) = chars.next() {
            if c != '\u{1b}' {
                out.push(c);
                continue;
            }
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

    #[test]
    fn a_stray_open_brace_names_the_space_that_caused_it() {
        let error = crate::expand("+ {3}\n").unwrap_err();
        let hint = error.hint().expect("a hint");
        assert!(hint.contains("nothing between"), "{hint}");
    }

    #[test]
    fn a_caret_lands_under_the_column_it_names() {
        let source = "@define A 65\n+{B}\n";
        let error = MacroError::UndefinedSymbol {
            name: "B".to_string(),
            location: SourceLocation::new(2, 3, 15),
            defined: vec!["A".to_string()],
            defined_later: false,
        };
        let rendered = strip_ansi(&error.format_with_source(source));
        let caret = rendered
            .lines()
            .find(|l| l.contains('^'))
            .expect("a caret line");
        // The gutter is `gyrus`'s own -- "   N | ", seven characters -- so a
        // caret for column 3 sits at column + 6.
        assert_eq!(caret.find('^'), Some(3 + 6), "caret line was {caret:?}");
        // And it lands under the character it names, on the rendered line.
        let line = rendered
            .lines()
            .find(|l| l.contains("+{B}"))
            .expect("the offending line");
        assert_eq!(line.chars().nth(3 + 6), Some('B'), "line was {line:?}");
        assert!(rendered.contains("Did you mean 'A'?"), "{rendered}");
    }

    #[test]
    fn a_planned_directive_says_so_rather_than_calling_itself_unknown() {
        let error = MacroError::PlannedDirective {
            name: "include".to_string(),
            location: SourceLocation::new(1, 1, 0),
        };
        let rendered = strip_ansi(&error.format_with_source("@include \"stdlib.bfm\"\n"));
        assert!(rendered.contains("not implemented yet"), "{rendered}");
        assert!(rendered.contains("@define"), "{rendered}");
    }

    #[test]
    fn one_edit_apart_is_a_suggestion_and_two_is_not() {
        assert!(within_one_edit("COUNTER", "COUNTERS")); // insertion
        assert!(within_one_edit("COUNTERS", "COUNTER")); // deletion
        assert!(within_one_edit("CHAR_A", "CHAR_B")); // substitution
        assert!(!within_one_edit("CHAR_A", "CHAR")); // two deletions
        assert!(!within_one_edit("A", "BC"));
        assert!(!within_one_edit("SAME", "SAME")); // identical is not a typo
    }
}
