//! Expansion errors, reported against the `.bfm` source.
//!
//! Every variant carries a [`SourceLocation`] in the macro source, and
//! [`MacroError::format_with_source`] renders it the way `gyrus` renders a
//! parse error: the offending line, two lines either side, and a caret. That
//! symmetry is the point -- a macro error and a BrainFuck error should be the
//! same kind of message, because to the person reading them they are.

use gyrus::SourceLocation;
use thiserror::Error;

/// Directives the design has a plan for but the expander does not implement.
///
/// Naming them separately is worth five lines: `@var` is a reasonable thing to
/// type, and "unknown directive" would be a lie about why it failed.
pub(crate) const PLANNED: &[&str] = &["var", "to", "macro", "include", "ifdef", "ifndef", "endif"];

#[non_exhaustive]
#[derive(Debug, Error)]
pub enum MacroError {
    #[error("Undefined symbol '{name}' at {location}")]
    UndefinedSymbol {
        name: String,
        location: SourceLocation,
        /// Everything defined at the point of use, for the "did you mean" line.
        defined: Vec<String>,
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

    #[error("Unmatched '[' at {location}")]
    UnmatchedOpenBracket { location: SourceLocation },

    #[error("Unmatched ']' at {location}")]
    UnmatchedCloseBracket { location: SourceLocation },

    #[error("Expansion is too large: {count} repetitions at {location} exceeds the {limit} limit")]
    ExpansionTooLarge {
        count: u64,
        limit: u64,
        location: SourceLocation,
    },
}

impl MacroError {
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
            | MacroError::UnmatchedOpenBracket { location }
            | MacroError::UnmatchedCloseBracket { location }
            | MacroError::ExpansionTooLarge { location, .. } => *location,
        }
    }

    /// What to try instead.
    pub fn hint(&self) -> Option<String> {
        match self {
            MacroError::UndefinedSymbol { name, defined, .. } => Some(match nearest(name, defined) {
                Some(near) => format!("Did you mean '{near}'? Symbols are defined before use, with @define."),
                None if defined.is_empty() => {
                    "Nothing is defined yet. Add `@define NAME VALUE` above this line.".to_string()
                }
                None => format!("Defined so far: {}.", defined.join(", ")),
            }),
            MacroError::PlannedDirective { name, .. } => Some(format!(
                "@{name} is part of the design but not built yet. \
                 Today the expander understands @define and repeat counts like +{{N}}."
            )),
            MacroError::UnknownDirective { .. } => {
                Some("The expander understands @define. Write a literal '@' as a comment character anywhere else.".to_string())
            }
            MacroError::Redefinition { first, .. } => Some(format!(
                "It was first defined at {first}. A symbol is defined once, so that a name means one thing."
            )),
            MacroError::RepeatNotRepeatable { .. } => Some(
                "Repeat counts apply to + - < > . and , -- repeating a bracket would not mean anything."
                    .to_string(),
            ),
            MacroError::UnmatchedOpenBracket { .. } => {
                Some("Every '[' needs a matching ']'. This is checked before expansion so the position is the one you wrote.".to_string())
            }
            MacroError::UnmatchedCloseBracket { .. } => {
                Some("This ']' closes a loop that was never opened.".to_string())
            }
            MacroError::ExpansionTooLarge { .. } => Some(
                "A repeat count this large is almost always a mistake in the constant it came from."
                    .to_string(),
            ),
            _ => None,
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
/// The shape deliberately matches `gyrus`'s own source context -- five-column
/// line numbers, a `|` gutter -- so that a macro error and a BrainFuck error
/// look like they came from the same program, because they did.
fn context(source: &str, location: SourceLocation) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let line_idx = location.line.saturating_sub(1);
    let start = line_idx.saturating_sub(2);
    let end = (line_idx + 3).min(lines.len());

    let mut out = String::new();
    for (number, line) in lines.iter().enumerate().take(end).skip(start) {
        out.push_str(&format!("{:5} | {}\n", number + 1, line));
        if number == line_idx {
            let spaces = " ".repeat(location.column.saturating_sub(1));
            out.push_str(&format!("      | {spaces}^\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_caret_lands_under_the_column_it_names() {
        let source = "@define A 65\n+{B}\n";
        let error = MacroError::UndefinedSymbol {
            name: "B".to_string(),
            location: SourceLocation::new(2, 3, 15),
            defined: vec!["A".to_string()],
        };
        let rendered = error.format_with_source(source);
        let caret = rendered
            .lines()
            .find(|l| l.contains('^'))
            .expect("a caret line");
        // Column 3 of "+{B}" is the 'B', under a "      | " gutter.
        assert_eq!(caret, "      |   ^");
        assert!(rendered.contains("Did you mean 'A'?"), "{rendered}");
    }

    #[test]
    fn a_planned_directive_says_so_rather_than_calling_itself_unknown() {
        let error = MacroError::PlannedDirective {
            name: "var".to_string(),
            location: SourceLocation::new(1, 1, 0),
        };
        let rendered = error.format_with_source("@var x at 0\n");
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
