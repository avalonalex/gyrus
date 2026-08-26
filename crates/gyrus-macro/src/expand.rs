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
//! `@` followed by anything else is an error rather than a comment, and so are
//! `{` and `}` outside a repeat count. Reserving them is what lets `@var`,
//! `@to` and `@macro` arrive later without changing what an existing `.bfm`
//! means.
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

/// Instructions a repeat count may follow. Brackets are deliberately absent:
/// `[{3}` would mean three loop openings, which is not a thing anyone means.
const REPEATABLE: [char; 6] = ['+', '-', '<', '>', '.', ','];

/// Expand `.bfm` source into BrainFuck, keeping the origin of every byte.
pub fn expand(source: &str) -> Result<Expansion, MacroError> {
    Scanner::new(source).run()
}

struct Scanner {
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
                '@' => self.directive()?,
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

        let source: String = self.chars.iter().collect();
        Ok(Expansion::new(source, self.out, self.origins))
    }

    // ---- character-level helpers -------------------------------------------

    fn peek(&self) -> Option<char> {
        self.chars.get(self.at.offset).copied()
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
            body.parse::<u64>()
                .map_err(|_| MacroError::ExpansionTooLarge {
                    count: u64::MAX,
                    limit: REPEAT_LIMIT,
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
            return Err(MacroError::ExpansionTooLarge {
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

        if let Some((_, first)) = self.constants.get(&name) {
            return Err(MacroError::Redefinition {
                name,
                first: *first,
                location,
            });
        }
        self.constants.insert(name.clone(), (value, location));
        self.defined.push(name);

        // The rest of the line is a comment. Without this a stray instruction
        // after the value would be emitted, which reads as the definition
        // having done something it did not.
        self.skip_line();
        Ok(())
    }
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
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
            let before = &source[..source
                .char_indices()
                .nth(origin.offset)
                .map(|(i, _)| i)
                .unwrap()];
            assert_eq!(
                origin.line,
                before.matches('\n').count() + 1,
                "line disagrees with offset at byte {offset}"
            );
            assert_eq!(
                origin.column,
                before.chars().count()
                    - before
                        .rfind('\n')
                        .map_or(0, |i| before[..i].chars().count() + 1)
                    + 1,
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
        // A star comment, and characters that are not instructions at all.
        assert_eq!(expanded("+ * this ->>> is prose\nhello +"), "++");
    }

    #[test]
    fn the_rest_of_a_define_line_is_a_comment() {
        // Without this the trailing '+' would be emitted, and the definition
        // would appear to have done something.
        assert_eq!(expanded("@define A 1 * a note +++\n+{A}"), "+");
    }

    #[test]
    fn origins_are_exact_for_every_shape_the_expander_emits() {
        assert_origins_are_exact("@define X 65\n+{X}.\n");
        assert_origins_are_exact("+{3}[>{2}-{4}<{2}]>.\n");
        assert_origins_are_exact("* prose\n\n  +  \n  [ - ]  \n,.\n");
    }

    #[test]
    fn an_undefined_name_is_reported_where_it_is_used() {
        let error = expand("@define A 1\n+{B}").unwrap_err();
        let MacroError::UndefinedSymbol {
            name,
            location,
            defined,
        } = error
        else {
            panic!("expected an undefined symbol, got {error:?}");
        };
        assert_eq!(name, "B");
        assert_eq!((location.line, location.column), (2, 2));
        assert_eq!(defined, vec!["A".to_string()]);
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
        // The alternative -- treating '@var' as a comment -- would mean a .bfm
        // written today silently changes meaning when @var arrives.
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
        // Scanning on for a '}' would swallow the program and blame a line far
        // below the mistake.
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
                MacroError::ExpansionTooLarge {
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
