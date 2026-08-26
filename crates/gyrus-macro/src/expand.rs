//! The expander: `.bfm` source in, pure BrainFuck plus an origin map out.
//!
//! A single pass over the characters. There is no separate lexer, parser and
//! code generator, because at this size they would be three names for the same
//! loop -- `gyrus`'s own parser and `gyrus-corpus`'s TOML reader are written
//! the same way, and for the same reason.
//!
//! # What it understands
//!
//! - `@define NAME VALUE` -- a named constant. `VALUE` is a decimal number or
//!   a name defined earlier. Definition precedes use: one pass, as `cpp` has.
//! - `OP{N}` -- repeat `OP` N times, where `OP` is one of `+ - < > . ,` and `N`
//!   is a number or a defined name. `+{0}` is nothing, which is legal.
//! - `*` to end of line, and any character that is not a BrainFuck instruction:
//!   comments, dropped from the expansion.
//!
//! # Two rules about what is reserved
//!
//! **A directive must start its line**, after optional blanks, and owns the
//! rest of it. Elsewhere `@` is an ordinary comment character, because
//! BrainFuck prose is free-form and three programs in `programs/` already
//! contain one -- reserving `@` everywhere would hard-error on converting them.
//! Owning the line is the other half: an instruction written after a `@define`
//! is refused rather than dropped, because silently discarding code somebody
//! wrote is the one thing a preprocessor must not do.
//!
//! **`{` and `}` are reserved everywhere.** Unlike `@` this costs nothing --
//! no bundled program has either in its prose -- and it buys the error for the
//! likeliest typo of all, a space between an instruction and its count.
//!
//! Between them, a `.bfm` written today cannot change meaning when `@var`,
//! `@to` and `@macro` arrive.
//!
//! # Brackets are checked here
//!
//! `gyrus`'s parser would catch an unbalanced `[`, but it would catch it in the
//! *expansion*, and bakes the rendered context into the error at parse time --
//! so the position a user saw would be a column in generated code. Checking
//! here costs a stack and a comparison, and is the difference between a
//! position they can act on and one they cannot.

use std::collections::HashMap;

use gyrus::SourceLocation;

use crate::error::{MacroError, PLANNED};
use crate::source_map::Expansion;

/// The most instructions one repeat count may emit.
///
/// Not a resource limit so much as a typo detector: `+{N}` where `N` came from
/// a constant that was meant to be 65 and is 65,000,000 should say so rather
/// than allocate. The tape is 30,000 cells by default, so a million of anything
/// is already far past useful.
pub const REPEAT_LIMIT: u64 = 1_000_000;

/// The most instructions one file may expand to, across every repeat count.
///
/// [`REPEAT_LIMIT`] alone bounds nothing: fifty legal `+{1000000}` lines are
/// fifty megabytes of BrainFuck and, because the origin map holds a
/// `SourceLocation` per emitted byte, more than a gigabyte of map. That is a
/// 550-byte input, and the next step for this crate is a command line reading
/// files it did not write.
pub const EXPANSION_LIMIT: usize = 1_000_000;

/// Instructions a repeat count may follow. Brackets are deliberately absent:
/// `[{3}` would mean three loop openings, which is not a thing anyone means.
const REPEATABLE: [char; 6] = ['+', '-', '<', '>', '.', ','];

/// Expand `.bfm` source into BrainFuck, keeping the origin of every byte.
pub fn expand(source: &str) -> Result<Expansion, MacroError> {
    Scanner::new(source).run()
}

struct Scanner {
    source: String,
    chars: Vec<char>,
    at: SourceLocation,
    constants: HashMap<String, (u64, SourceLocation)>,
    /// Insertion order, for the "did you mean" list. A HashMap alone would
    /// suggest names in an order that changes between runs.
    defined: Vec<String>,
    out: String,
    origins: Vec<SourceLocation>,
    open_brackets: Vec<SourceLocation>,
}

impl Scanner {
    fn new(source: &str) -> Self {
        Self {
            source: source.to_string(),
            chars: source.chars().collect(),
            at: SourceLocation::start(),
            constants: HashMap::new(),
            defined: Vec::new(),
            out: String::new(),
            origins: Vec::new(),
            open_brackets: Vec::new(),
        }
    }

    fn run(mut self) -> Result<Expansion, MacroError> {
        while let Some(c) = self.peek() {
            match c {
                '*' => self.skip_line(),
                '@' if self.at_line_start() => self.directive()?,
                '{' | '}' => {
                    return Err(MacroError::StrayBrace {
                        brace: c,
                        location: self.at,
                    });
                }
                '[' | ']' => self.bracket(c)?,
                c if REPEATABLE.contains(&c) => self.instruction(c)?,
                _ => self.bump(),
            }
        }

        if let Some(location) = self.open_brackets.first() {
            return Err(MacroError::UnmatchedOpenBracket {
                location: *location,
            });
        }

        Ok(Expansion::new(self.source, self.out, self.origins))
    }

    // ---- character-level helpers -------------------------------------------

    fn peek(&self) -> Option<char> {
        self.chars.get(self.at.offset).copied()
    }

    /// Whether only blanks precede the cursor on its line.
    ///
    /// Asked only when the cursor is on `@`, which is rare, so walking back to
    /// the newline is cheaper than a flag every `bump` would have to maintain.
    fn at_line_start(&self) -> bool {
        self.chars[..self.at.offset]
            .iter()
            .rev()
            .take_while(|&&c| c != '\n')
            .all(|&c| c == ' ' || c == '\t')
    }

    fn bump(&mut self) {
        if let Some(c) = self.peek() {
            if c == '\n' {
                self.at.line += 1;
                self.at.column = 1;
            } else {
                self.at.column += 1;
            }
            self.at.offset += 1;
        }
    }

    fn skip_line(&mut self) {
        while let Some(c) = self.peek() {
            if c == '\n' {
                return;
            }
            self.bump();
        }
    }

    fn skip_blanks(&mut self) {
        while matches!(self.peek(), Some(' ') | Some('\t')) {
            self.bump();
        }
    }

    fn emit(&mut self, c: char, origin: SourceLocation) {
        self.out.push(c);
        self.origins.push(origin);
    }

    /// An identifier at the cursor: a letter or `_`, then letters, digits, `_`.
    /// Empty if the cursor is not on one.
    fn identifier(&mut self) -> String {
        let mut name = String::new();
        match self.peek() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
            _ => return name,
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                name.push(c);
                self.bump();
            } else {
                break;
            }
        }
        name
    }

    // ---- the grammar --------------------------------------------------------

    fn instruction(&mut self, c: char) -> Result<(), MacroError> {
        let origin = self.at;
        self.bump();
        let count = if self.peek() == Some('{') {
            self.repeat_count()?
        } else {
            1
        };

        let total = self.out.len().saturating_add(count as usize);
        if total > EXPANSION_LIMIT {
            return Err(MacroError::ExpansionTooLarge {
                emitted: total,
                limit: EXPANSION_LIMIT,
                location: origin,
            });
        }

        for _ in 0..count {
            self.emit(c, origin);
        }
        Ok(())
    }

    fn bracket(&mut self, c: char) -> Result<(), MacroError> {
        let origin = self.at;
        self.bump();
        if self.peek() == Some('{') {
            return Err(MacroError::RepeatNotRepeatable {
                instruction: c,
                location: origin,
            });
        }
        if c == '[' {
            self.open_brackets.push(origin);
        } else if self.open_brackets.pop().is_none() {
            return Err(MacroError::UnmatchedCloseBracket { location: origin });
        }
        self.emit(c, origin);
        Ok(())
    }

    /// A `{...}` repeat count at the cursor, resolved to a number.
    fn repeat_count(&mut self) -> Result<u64, MacroError> {
        let open = self.at;
        self.bump(); // past '{'

        let mut body = String::new();
        loop {
            match self.peek() {
                Some('}') => {
                    self.bump();
                    break;
                }
                // A newline inside a repeat count means the '}' was forgotten;
                // scanning on would swallow the rest of the program looking
                // for one, and blame a line far below the mistake.
                None | Some('\n') => {
                    return Err(MacroError::BadRepeatCount {
                        detail: "no closing '}'".to_string(),
                        location: open,
                    });
                }
                Some(c) => {
                    body.push(c);
                    self.bump();
                }
            }
        }

        let body = body.trim();
        if body.is_empty() {
            return Err(MacroError::BadRepeatCount {
                detail: "empty. Write a number, as in +{65}, or a name defined with @define"
                    .to_string(),
                location: open,
            });
        }

        let count = if body.chars().all(|c| c.is_ascii_digit()) {
            // Reporting u64::MAX here would put a number in front of the user
            // that they never typed.
            body.parse::<u64>()
                .map_err(|_| MacroError::BadRepeatCount {
                    detail: format!("'{body}' does not fit in a 64-bit number"),
                    location: open,
                })?
        } else if is_identifier(body) {
            self.resolve(body, open)?
        } else {
            return Err(MacroError::BadRepeatCount {
                detail: format!("'{body}' is neither a number nor a name"),
                location: open,
            });
        };

        if count > REPEAT_LIMIT {
            return Err(MacroError::RepeatTooLarge {
                count,
                limit: REPEAT_LIMIT,
                location: open,
            });
        }
        Ok(count)
    }

    fn resolve(&self, name: &str, location: SourceLocation) -> Result<u64, MacroError> {
        self.constants
            .get(name)
            .map(|(value, _)| *value)
            .ok_or_else(|| MacroError::UndefinedSymbol {
                name: name.to_string(),
                location,
                defined: self.defined.clone(),
                defined_later: self.defined_later(name),
            })
    }

    /// Whether a later line defines `name`.
    ///
    /// Asked only on the way to an error, and worth the scan: single-pass
    /// define-before-use is an unusual constraint, and "nothing is defined
    /// yet" is a poor thing to read directly above a rendering of the
    /// definition two lines down.
    fn defined_later(&self, name: &str) -> bool {
        let rest: String = self.chars[self.at.offset.min(self.chars.len())..]
            .iter()
            .collect();
        rest.lines().any(|line| {
            let Some(tail) = line.trim_start().strip_prefix("@define") else {
                return false;
            };
            let tail = tail.trim_start();
            tail.strip_prefix(name)
                .is_some_and(|after| !after.starts_with(|c: char| is_identifier_char(c)))
        })
    }

    fn directive(&mut self) -> Result<(), MacroError> {
        let location = self.at;
        self.bump(); // past '@'
        let name = self.identifier();

        if name.is_empty() {
            return Err(MacroError::MalformedDirective {
                directive: String::new(),
                detail: "a directive name must follow '@'".to_string(),
                location,
            });
        }
        if name == "define" {
            return self.define(location);
        }
        if PLANNED.contains(&name.as_str()) {
            return Err(MacroError::PlannedDirective { name, location });
        }
        Err(MacroError::UnknownDirective { name, location })
    }

    fn define(&mut self, location: SourceLocation) -> Result<(), MacroError> {
        self.skip_blanks();
        let at_name = self.at;
        let name = self.identifier();
        if name.is_empty() {
            return Err(MacroError::MalformedDirective {
                directive: "define".to_string(),
                detail: "expected a name, as in `@define CHAR_A 65`".to_string(),
                location: at_name,
            });
        }

        // Before resolving the value, not after: a redefinition whose value is
        // also wrong should report the redefinition, or the user fixes the
        // value and only then learns the name was taken.
        if let Some((_, first)) = self.constants.get(&name) {
            return Err(MacroError::Redefinition {
                name,
                first: *first,
                location,
            });
        }

        self.skip_blanks();
        let at_value = self.at;
        let mut token = String::new();
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                break;
            }
            token.push(c);
            self.bump();
        }

        if token.is_empty() {
            return Err(MacroError::MalformedDirective {
                directive: "define".to_string(),
                detail: format!("expected a value for '{name}', as in `@define {name} 65`"),
                location: at_value,
            });
        }

        let value = if token.chars().all(|c| c.is_ascii_digit()) {
            token
                .parse::<u64>()
                .map_err(|_| MacroError::MalformedDirective {
                    directive: "define".to_string(),
                    detail: format!("'{token}' does not fit in a 64-bit number"),
                    location: at_value,
                })?
        } else if is_identifier(&token) {
            self.resolve(&token, at_value)?
        } else {
            return Err(MacroError::MalformedDirective {
                directive: "define".to_string(),
                detail: format!("'{token}' is neither a number nor a name"),
                location: at_value,
            });
        };

        // A directive owns the rest of its line, and says so rather than
        // dropping what follows. Skipping silently discarded any instruction
        // written after the value -- and a discarded ']' went on to blame a
        // '[' that the source plainly matched.
        self.skip_blanks();
        match self.peek() {
            None | Some('\n') => {}
            Some('*') => self.skip_line(),
            Some(c) => {
                return Err(MacroError::MalformedDirective {
                    directive: "define".to_string(),
                    detail: format!(
                        "'{c}' follows the value. A @define takes the rest of its line: \
                         move this to a line of its own, or start a comment with '*'"
                    ),
                    location: self.at,
                });
            }
        }

        self.constants.insert(name.clone(), (value, location));
        self.defined.push(name);
        Ok(())
    }
}

fn is_identifier_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(is_identifier_char)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expanded(source: &str) -> String {
        expand(source).expect("expands").brainfuck().to_string()
    }

    /// Every emitted byte points at a character that could have emitted it.
    ///
    /// This is the whole correctness claim of the origin map, and in this
    /// language it is exact rather than approximate: an instruction is only
    /// ever emitted from a literal instruction character, so the character the
    /// origin names must *be* the character emitted. A repeat count makes the
    /// mapping many-to-one -- all 65 of `+{65}` name the same `+` -- which is
    /// the correct answer, not a rounding of it.
    fn assert_origins_are_exact(source: &str) {
        let expansion = expand(source).expect("expands");
        let chars: Vec<char> = source.chars().collect();
        for (offset, emitted) in expansion.brainfuck().chars().enumerate() {
            let origin = expansion
                .origin(offset)
                .unwrap_or_else(|| panic!("byte {offset} has no origin"));
            assert_eq!(
                chars[origin.offset], emitted,
                "byte {offset} ({emitted}) points at {:?} in the macro source",
                chars[origin.offset]
            );
            // The line and column must agree with the offset, or a caret lands
            // on the wrong character even though the offset is right.
            let before: String = chars[..origin.offset].iter().collect();
            assert_eq!(
                origin.line,
                before.matches('\n').count() + 1,
                "line disagrees with offset at byte {offset}"
            );
            let column = before.chars().rev().take_while(|&c| c != '\n').count() + 1;
            assert_eq!(
                origin.column, column,
                "column disagrees with offset at byte {offset}"
            );
        }
    }

    #[test]
    fn a_constant_becomes_a_run_of_instructions() {
        assert_eq!(expanded("@define X 5\n+{X}"), "+++++");
    }

    #[test]
    fn a_literal_count_needs_no_constant() {
        assert_eq!(expanded(">{3}<{2}"), ">>><<");
    }

    #[test]
    fn every_repeatable_instruction_repeats() {
        assert_eq!(expanded("+{2}-{2}<{2}>{2}.{2},{2}"), "++--<<>>..,,");
    }

    #[test]
    fn a_count_of_zero_emits_nothing() {
        assert_eq!(expanded("+{0}-"), "-");
    }

    #[test]
    fn a_definition_may_name_an_earlier_one() {
        assert_eq!(expanded("@define A 3\n@define B A\n+{B}"), "+++");
    }

    #[test]
    fn comments_do_not_reach_the_expansion() {
        assert_eq!(expanded("+ * this ->>> is prose\nhello +"), "++");
    }

    #[test]
    fn a_star_comment_may_follow_a_define() {
        assert_eq!(expanded("@define A 1 * a note +++\n+{A}"), "+");
    }

    // --- what the review found -------------------------------------------

    #[test]
    fn an_instruction_after_a_define_is_refused_rather_than_dropped() {
        // This used to expand to "+", silently discarding three instructions
        // somebody wrote. Dropping code is the one thing a preprocessor must
        // not do quietly.
        let error = expand("@define A 1 +++\n+{A}").unwrap_err();
        let MacroError::MalformedDirective { detail, .. } = &error else {
            panic!("expected a malformed directive, got {error:?}");
        };
        assert!(detail.contains("rest of its line"), "{detail}");
    }

    #[test]
    fn a_directive_must_start_its_line() {
        // Not a directive here, so it is prose -- and the ']' that follows is
        // a real bracket rather than part of a swallowed comment tail. This
        // shape used to report an unmatched '[' that the source plainly
        // matched.
        assert_eq!(expanded("+[@define A 1 ]+"), "+[]+");
    }

    #[test]
    fn prose_may_contain_an_at_sign() {
        // calc.bf, pi.bf and char.bf all have one. Reserving '@' everywhere
        // would hard-error on converting any of them.
        assert_eq!(expanded("+ see @foo for details\n+"), "++");
    }

    #[test]
    fn a_name_defined_below_is_told_so_rather_than_that_nothing_is_defined() {
        let error = expand("+{A}\n@define A 5\n").unwrap_err();
        let hint = error.hint().expect("a hint");
        assert!(hint.contains("defined below this line"), "{hint}");
        assert!(hint.contains("single pass"), "{hint}");
    }

    #[test]
    fn the_whole_file_has_an_expansion_budget() {
        // Each count is legal on its own; together they are not. Fifty of
        // these was a 550-byte file and more than a gigabyte of origin map.
        let source = format!("+{{{REPEAT_LIMIT}}}\n+{{{REPEAT_LIMIT}}}\n");
        let error = expand(&source).unwrap_err();
        assert!(
            matches!(
                error,
                MacroError::ExpansionTooLarge {
                    limit: EXPANSION_LIMIT,
                    ..
                }
            ),
            "{error:?}"
        );
    }

    #[test]
    fn a_count_too_big_for_a_u64_reports_what_was_written() {
        // Never the u64::MAX it failed to parse into: that is a number the
        // user did not type.
        let error = expand("+{99999999999999999999999}\n").unwrap_err();
        let MacroError::BadRepeatCount { detail, .. } = &error else {
            panic!("expected a bad repeat count, got {error:?}");
        };
        assert!(detail.contains("99999999999999999999999"), "{detail}");
        assert!(!detail.contains("18446744073709551615"), "{detail}");
    }

    #[test]
    fn a_redefinition_is_reported_before_its_value_is_resolved() {
        // Otherwise the user fixes the bad value, re-runs, and only then
        // learns the name was already taken -- two round trips for one line.
        let error = expand("@define A 1\n@define A NOPE\n").unwrap_err();
        assert!(
            matches!(error, MacroError::Redefinition { .. }),
            "{error:?}"
        );
    }

    // --- everything else --------------------------------------------------

    #[test]
    fn origins_are_exact_for_every_shape_the_expander_emits() {
        assert_origins_are_exact("@define X 65\n+{X}.\n");
        assert_origins_are_exact("+{3}[>{2}-{4}<{2}]>.\n");
        assert_origins_are_exact("* prose\n\n  +  \n  [ - ]  \n,.\n");
        assert_origins_are_exact("+ prose with @ and no directive\n[-]\n");
    }

    #[test]
    fn an_undefined_name_is_reported_where_it_is_used() {
        let error = expand("@define A 1\n+{B}").unwrap_err();
        let MacroError::UndefinedSymbol {
            name,
            location,
            defined,
            defined_later,
        } = error
        else {
            panic!("expected an undefined symbol, got {error:?}");
        };
        assert_eq!(name, "B");
        assert_eq!((location.line, location.column), (2, 2));
        assert_eq!(defined, vec!["A".to_string()]);
        assert!(!defined_later);
    }

    #[test]
    fn a_name_is_defined_once() {
        let error = expand("@define A 1\n@define A 2").unwrap_err();
        let MacroError::Redefinition {
            first, location, ..
        } = error
        else {
            panic!("expected a redefinition, got {error:?}");
        };
        assert_eq!(first.line, 1);
        assert_eq!(location.line, 2);
    }

    #[test]
    fn a_planned_directive_is_refused_rather_than_ignored() {
        for directive in ["@var x at 0", "@to x", "@macro clear { [-] }"] {
            let error = expand(directive).unwrap_err();
            assert!(
                matches!(error, MacroError::PlannedDirective { .. }),
                "{directive}: got {error:?}"
            );
        }
    }

    #[test]
    fn an_unknown_directive_is_not_a_comment_either() {
        let error = expand("@wibble").unwrap_err();
        assert!(
            matches!(error, MacroError::UnknownDirective { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn braces_are_reserved() {
        assert!(matches!(
            expand("{X}").unwrap_err(),
            MacroError::StrayBrace { brace: '{', .. }
        ));
        assert!(matches!(
            expand("+ }").unwrap_err(),
            MacroError::StrayBrace { brace: '}', .. }
        ));
    }

    #[test]
    fn a_bracket_cannot_be_repeated() {
        let error = expand("[{3}").unwrap_err();
        assert!(
            matches!(
                error,
                MacroError::RepeatNotRepeatable {
                    instruction: '[',
                    ..
                }
            ),
            "{error:?}"
        );
    }

    #[test]
    fn an_unterminated_count_blames_its_own_line() {
        let error = expand("+{3\n-{2}\n").unwrap_err();
        let MacroError::BadRepeatCount { location, .. } = error else {
            panic!("expected a bad repeat count, got {error:?}");
        };
        assert_eq!(location.line, 1);
    }

    #[test]
    fn unbalanced_brackets_are_caught_in_macro_coordinates() {
        let open = expand("+\n[>+<-\n").unwrap_err();
        let MacroError::UnmatchedOpenBracket { location } = open else {
            panic!("expected an unmatched '[', got {open:?}");
        };
        assert_eq!((location.line, location.column), (2, 1));

        let close = expand("+\n+]\n").unwrap_err();
        let MacroError::UnmatchedCloseBracket { location } = close else {
            panic!("expected an unmatched ']', got {close:?}");
        };
        assert_eq!((location.line, location.column), (2, 2));
    }

    #[test]
    fn an_absurd_count_is_refused_rather_than_allocated() {
        let error = expand("@define OOPS 65000000\n+{OOPS}").unwrap_err();
        assert!(
            matches!(
                error,
                MacroError::RepeatTooLarge {
                    limit: REPEAT_LIMIT,
                    ..
                }
            ),
            "{error:?}"
        );
    }

    #[test]
    fn a_malformed_define_says_what_was_expected() {
        assert!(matches!(
            expand("@define\n").unwrap_err(),
            MacroError::MalformedDirective { .. }
        ));
        assert!(matches!(
            expand("@define A\n").unwrap_err(),
            MacroError::MalformedDirective { .. }
        ));
    }
}
