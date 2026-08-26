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
//! - `@var NAME at N` -- a named cell, and `@to NAME` to move the cursor there.
//!   The expander tracks where the cursor is and emits the difference, which
//!   is the abstraction that earns the feature: manual pointer arithmetic is
//!   what makes hand-written BrainFuck unmaintainable past a few dozen cells.
//! - `@here NAME` -- tell the expander where the cursor is without moving it.
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
//! Between them, a `.bfm` written today cannot change meaning when `@macro`
//! and the conditionals arrive.
//!
//! # Where the cursor is, and when the expander stops knowing
//!
//! `@to` needs a static cursor position, and BrainFuck loops are where that
//! breaks: at `[` the position is known, but at `]` the cursor is wherever the
//! body left it, after a number of iterations nobody knows. Three rules, and
//! the first two are the whole design:
//!
//! 1. A loop whose body returns the cursor where it found it changes nothing.
//!    The position after it is the position before it.
//! 2. A loop whose body does not -- `[>]`, the ordinary scan idiom -- leaves
//!    the position *unknown*. That is not an error by itself, because such
//!    loops are entirely normal BrainFuck. The next `@to` is the error, and it
//!    names both itself and the loop that lost the position.
//! 3. A `@to` *inside* an unbalanced body is an error, reported at the `]`.
//!    Its first iteration would be right and every later one wrong, which is
//!    the worst way for this to fail. Whether the body balances is not known
//!    until the `]`, which is why the check lives there.
//!
//! `@here NAME` re-establishes a position without emitting anything, for the
//! case rule 2 exists for: after `[<]` the programmer knows where the cursor
//! landed and the expander cannot. It is trusted rather than checked -- the
//! one construct here that can silently produce a wrong program -- and it is
//! the price of `@to` and scan loops coexisting at all.
//!
//! A tracked position may go negative, and that is not an error. Movement off
//! the tape is legal under gyrus's tape contract; only *access* is checked.
//! The original design listed a `NegativePointer` error, which predates that
//! contract and would contradict it.
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

use crate::error::{Kind, MacroError, PLANNED};
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

/// The directives that declare a name, for the two helpers they share.
///
/// An enum rather than the directive's spelling: the helpers used to pick
/// their example text by matching on a `&str`, so `@define`'s wording was the
/// silent fallback for anything unrecognised, and a typo in a literal would
/// have been caught by nobody. Adding a variant here is a compile error until
/// every arm is filled in.
#[derive(Debug, Clone, Copy)]
enum Directive {
    Define,
    Var,
}

impl Directive {
    fn spelling(self) -> &'static str {
        match self {
            Directive::Define => "define",
            Directive::Var => "var",
        }
    }

    /// What to write where the name goes.
    fn name_example(self) -> &'static str {
        match self {
            Directive::Define => "`@define CHAR_A 65`",
            Directive::Var => "`@var counter at 0`",
        }
    }

    /// What is missing when the value is, and an example of supplying it.
    fn value_example(self, name: &str) -> (&'static str, String) {
        match self {
            Directive::Define => ("a value", format!("`@define {name} 65`")),
            Directive::Var => ("a cell", format!("`@var {name} at 0`")),
        }
    }
}

/// What a name means. One namespace, so a name means one thing.
#[derive(Debug, Clone, Copy)]
enum Symbol {
    /// A number, usable as a repeat count.
    Constant(u64),
    /// A cell, usable as a `@to` target.
    Variable(i64),
}

impl Symbol {
    fn kind(&self) -> Kind {
        match self {
            Symbol::Constant(_) => Kind::Constant,
            Symbol::Variable(_) => Kind::Variable,
        }
    }
}

/// Where the cursor is during expansion, when the expander can still say.
#[derive(Debug, Clone, Copy)]
enum Position {
    Known(i64),
    /// Lost by a loop whose body did not return the cursor where it found it.
    /// Carries that loop's `[`, so an error can name the cause and not only
    /// the symptom.
    Unknown(SourceLocation),
}

/// A `[` whose `]` has not been reached.
struct OpenLoop {
    location: SourceLocation,
    /// The running movement total when the body was entered. Balance is a
    /// property of how far the body moved the cursor, which is always known,
    /// and *not* of where the cursor ended up, which often is not. Comparing
    /// absolute positions instead made a balanced `[-]` after a scan look
    /// unbalanced -- so it stole the blame from the scan that actually lost
    /// the position, and it refused a net-zero body that used `@here`.
    net_at_entry: i64,
    /// The first `@to` inside this body. Its presence is what makes an
    /// unbalanced body an error rather than merely a loss of position.
    to_inside: Option<SourceLocation>,
}

struct Scanner {
    source: String,
    chars: Vec<char>,
    at: SourceLocation,
    symbols: HashMap<String, (Symbol, SourceLocation)>,
    /// Insertion order, for the "did you mean" list. A HashMap alone would
    /// suggest names in an order that changes between runs.
    defined: Vec<String>,
    position: Position,
    /// Net cursor movement so far. Unlike `position` this is never unknown:
    /// it counts emitted `>` and `<`, which the expander always knows, and
    /// `@here` deliberately does not touch it.
    net: i64,
    out: String,
    origins: Vec<SourceLocation>,
    open_brackets: Vec<OpenLoop>,
}

impl Scanner {
    fn new(source: &str) -> Self {
        Self {
            source: source.to_string(),
            chars: source.chars().collect(),
            at: SourceLocation::start(),
            symbols: HashMap::new(),
            defined: Vec::new(),
            position: Position::Known(0),
            net: 0,
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

        if let Some(open) = self.open_brackets.first() {
            return Err(MacroError::UnmatchedOpenBracket {
                location: open.location,
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

        // A literal '>' or '<' moves the cursor just as `@to` does, so the
        // tracked position has to follow it or the two cannot be mixed.
        self.emit_run(c, count, origin)
    }

    /// Record cursor movement: always in `net`, and in `position` if there
    /// still is one. Saturating rather than wrapping -- these are bounded far
    /// below i64 by `EXPANSION_LIMIT` and the cap on a `@var` cell, so
    /// saturation is unreachable, and a panic on input the expander did not
    /// write would not be.
    fn step(&mut self, delta: i64) {
        self.net = self.net.saturating_add(delta);
        if let Position::Known(at) = self.position {
            self.position = Position::Known(at.saturating_add(delta));
        }
    }

    /// Emit `count` copies of an instruction, within the file's budget.
    ///
    /// The one place instructions are emitted in bulk, so the limit check and
    /// the movement accounting cannot be applied to some emitters and not
    /// others.
    fn emit_run(&mut self, c: char, count: u64, origin: SourceLocation) -> Result<(), MacroError> {
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
        // Bounded by the check above, so the cast cannot lose anything.
        self.step(match c {
            '>' => count as i64,
            '<' => -(count as i64),
            _ => 0,
        });
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
            self.open_brackets.push(OpenLoop {
                location: origin,
                net_at_entry: self.net,
                to_inside: None,
            });
            self.emit(c, origin);
            return Ok(());
        }

        let Some(open) = self.open_brackets.pop() else {
            return Err(MacroError::UnmatchedCloseBracket { location: origin });
        };
        self.emit(c, origin);

        if self.net != open.net_at_entry {
            // Reported here rather than at the `@to` itself, because whether
            // the body balances is not known until this bracket. Its first
            // iteration would emit the right movement and every later one the
            // wrong movement, which is the worst way for this to fail.
            if let Some(to) = open.to_inside {
                return Err(MacroError::MovingInsideUnbalancedLoop {
                    location: to,
                    loop_at: open.location,
                });
            }
            // Only the loop that *first* lost the position is worth naming:
            // re-tagging on every later unbalanced loop would point the user
            // at a symptom rather than the cause.
            if matches!(self.position, Position::Known(_)) {
                self.position = Position::Unknown(open.location);
            }
        }

        // A `@to` in a nested body is inside this one too.
        if let (Some(to), Some(parent)) = (open.to_inside, self.open_brackets.last_mut())
            && parent.to_inside.is_none()
        {
            parent.to_inside = Some(to);
        }
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

    /// A name's entry, or an undefined-symbol error naming what is in scope.
    fn lookup(
        &self,
        name: &str,
        location: SourceLocation,
    ) -> Result<(Symbol, SourceLocation), MacroError> {
        self.symbols
            .get(name)
            .copied()
            .ok_or_else(|| MacroError::UndefinedSymbol {
                name: name.to_string(),
                location,
                defined: self.defined.clone(),
                defined_later: self.defined_later(name),
            })
    }

    /// A name used where a number is wanted.
    fn resolve(&self, name: &str, location: SourceLocation) -> Result<u64, MacroError> {
        match self.lookup(name, location)? {
            (Symbol::Constant(value), _) => Ok(value),
            (other, declared) => Err(MacroError::WrongKind {
                name: name.to_string(),
                found: other.kind(),
                wanted: Kind::Constant,
                location,
                declared,
            }),
        }
    }

    /// A name used where a cell is wanted.
    fn resolve_cell(&self, name: &str, location: SourceLocation) -> Result<i64, MacroError> {
        match self.lookup(name, location)? {
            (Symbol::Variable(cell), _) => Ok(cell),
            (other, declared) => Err(MacroError::WrongKind {
                name: name.to_string(),
                found: other.kind(),
                wanted: Kind::Variable,
                location,
                declared,
            }),
        }
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
            let trimmed = line.trim_start();
            let Some(tail) = trimmed
                .strip_prefix("@define")
                .or_else(|| trimmed.strip_prefix("@var"))
            else {
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
        match name.as_str() {
            "define" => return self.define(location),
            "var" => return self.var(location),
            "to" => return self.to(location),
            "here" => return self.here(),
            _ => {}
        }
        if PLANNED.contains(&name.as_str()) {
            return Err(MacroError::PlannedDirective { name, location });
        }
        Err(MacroError::UnknownDirective { name, location })
    }

    /// `@var NAME at N` -- name a cell.
    fn var(&mut self, location: SourceLocation) -> Result<(), MacroError> {
        let name = self.declared_name(Directive::Var, location)?;

        self.skip_blanks();
        let at_keyword = self.at;
        if self.identifier() != "at" {
            return Err(MacroError::MalformedDirective {
                directive: "var".to_string(),
                detail: format!("expected `at`, as in `@var {name} at 0`"),
                location: at_keyword,
            });
        }

        self.skip_blanks();
        let at_value = self.at;
        let cell = self.number_or_name(Directive::Var, &name, at_value)?;
        // Reaching cell N costs N moves, so a cell past the expansion budget
        // is one no program could ever move to. Refusing it here is also what
        // keeps every tracked position small enough that the arithmetic on
        // them cannot overflow.
        if cell > EXPANSION_LIMIT as u64 {
            return Err(MacroError::MalformedDirective {
                directive: "var".to_string(),
                detail: format!(
                    "cell {cell} is further along the tape than {EXPANSION_LIMIT} moves reach, \
                     which is the whole file's budget"
                ),
                location: at_value,
            });
        }
        let cell = cell as i64;

        self.end_of_directive("var")?;
        self.declare(name, Symbol::Variable(cell), location);
        Ok(())
    }

    /// `@to NAME` -- move the cursor to a named cell.
    fn to(&mut self, location: SourceLocation) -> Result<(), MacroError> {
        self.skip_blanks();
        let at_name = self.at;
        let name = self.identifier();
        if name.is_empty() {
            return Err(MacroError::MalformedDirective {
                directive: "to".to_string(),
                detail: "expected the name of a cell declared with @var".to_string(),
                location: at_name,
            });
        }
        let target = self.resolve_cell(&name, at_name)?;
        self.end_of_directive("to")?;

        // Inside a loop body, remember that a `@to` happened: if the body
        // turns out not to restore the cursor, this is the error, and the `]`
        // is where that becomes knowable.
        if let Some(open) = self.open_brackets.last_mut()
            && open.to_inside.is_none()
        {
            open.to_inside = Some(location);
        }

        let here = match self.position {
            Position::Known(here) => here,
            Position::Unknown(lost_at) => {
                return Err(MacroError::PositionUnknown {
                    name,
                    location,
                    lost_at,
                });
            }
        };

        // In i128, so that no pair of tracked positions can overflow the
        // subtraction. Both are bounded well below i64 in practice; this is
        // what makes that a fact rather than an assumption.
        let delta = i128::from(target) - i128::from(here);
        let (step, count) = if delta >= 0 {
            ('>', delta)
        } else {
            ('<', -delta)
        };
        let count = u64::try_from(count).unwrap_or(u64::MAX);
        self.emit_run(step, count, location)
    }

    /// `@here NAME` -- assert where the cursor is, emitting nothing.
    fn here(&mut self) -> Result<(), MacroError> {
        self.skip_blanks();
        let at_name = self.at;
        let name = self.identifier();
        if name.is_empty() {
            return Err(MacroError::MalformedDirective {
                directive: "here".to_string(),
                detail: "expected the name of a cell declared with @var".to_string(),
                location: at_name,
            });
        }
        let cell = self.resolve_cell(&name, at_name)?;
        self.end_of_directive("here")?;
        self.position = Position::Known(cell);
        Ok(())
    }

    // ---- pieces the directives share ---------------------------------------

    /// The name a declaration is declaring, checked for redefinition.
    ///
    /// Before any value is resolved, so a redefinition whose value is also
    /// wrong reports the redefinition rather than costing two round trips.
    fn declared_name(
        &mut self,
        directive: Directive,
        location: SourceLocation,
    ) -> Result<String, MacroError> {
        self.skip_blanks();
        let at_name = self.at;
        let name = self.identifier();
        if name.is_empty() {
            return Err(MacroError::MalformedDirective {
                directive: directive.spelling().to_string(),
                detail: format!("expected a name, as in {}", directive.name_example()),
                location: at_name,
            });
        }
        if let Some((_, first)) = self.symbols.get(&name) {
            return Err(MacroError::Redefinition {
                name,
                first: *first,
                location,
            });
        }
        Ok(name)
    }

    /// A decimal number or the name of a constant.
    fn number_or_name(
        &mut self,
        directive: Directive,
        name: &str,
        at_value: SourceLocation,
    ) -> Result<u64, MacroError> {
        let mut token = String::new();
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                break;
            }
            token.push(c);
            self.bump();
        }

        if token.is_empty() {
            let (wanted, example) = directive.value_example(name);
            return Err(MacroError::MalformedDirective {
                directive: directive.spelling().to_string(),
                detail: format!("expected {wanted} for '{name}', as in {example}"),
                location: at_value,
            });
        }

        if token.chars().all(|c| c.is_ascii_digit()) {
            token
                .parse::<u64>()
                .map_err(|_| MacroError::MalformedDirective {
                    directive: directive.spelling().to_string(),
                    detail: format!("'{token}' does not fit in a 64-bit number"),
                    location: at_value,
                })
        } else if is_identifier(&token) {
            self.resolve(&token, at_value)
        } else {
            Err(MacroError::MalformedDirective {
                directive: directive.spelling().to_string(),
                detail: format!("'{token}' is neither a number nor a name"),
                location: at_value,
            })
        }
    }

    /// A directive owns the rest of its line, and says so rather than dropping
    /// what follows. Skipping silently discarded any instruction written after
    /// it -- and a discarded ']' went on to blame a '[' the source matched.
    fn end_of_directive(&mut self, directive: &str) -> Result<(), MacroError> {
        self.skip_blanks();
        match self.peek() {
            None | Some('\n') => Ok(()),
            Some('*') => {
                self.skip_line();
                Ok(())
            }
            Some(c) => Err(MacroError::MalformedDirective {
                directive: directive.to_string(),
                detail: format!(
                    "'{c}' follows it. A @{directive} takes the rest of its line: \
                     move this to a line of its own, or start a comment with '*'"
                ),
                location: self.at,
            }),
        }
    }

    fn declare(&mut self, name: String, symbol: Symbol, at: SourceLocation) {
        self.symbols.insert(name.clone(), (symbol, at));
        self.defined.push(name);
    }

    fn define(&mut self, location: SourceLocation) -> Result<(), MacroError> {
        let name = self.declared_name(Directive::Define, location)?;
        self.skip_blanks();
        let at_value = self.at;
        let value = self.number_or_name(Directive::Define, &name, at_value)?;
        self.end_of_directive("define")?;
        self.declare(name, Symbol::Constant(value), location);
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
        let mut directions: std::collections::HashMap<usize, char> =
            std::collections::HashMap::new();
        for (offset, emitted) in expansion.brainfuck().chars().enumerate() {
            let origin = expansion
                .origin(offset)
                .unwrap_or_else(|| panic!("byte {offset} has no origin"));
            // `@to` is the one construct that emits an instruction not
            // written literally, so its bytes point at the directive that
            // generated them. Everything else still points at itself.
            if chars[origin.offset] == '@' {
                let head: String = chars[origin.offset..].iter().take(3).collect();
                assert_eq!(
                    head, "@to",
                    "byte {offset} points at a directive that does not emit"
                );
                assert!(
                    emitted == '>' || emitted == '<',
                    "@to emitted {emitted:?}, which is not a move"
                );
                // A single `@to` moves one way. Recording the direction per
                // directive occurrence is what makes this exact rather than
                // "points at some @to": a byte misattributed to a different
                // `@to` shows up here whenever the two move oppositely, and
                // a run split across directives shows up as a contradiction.
                if let Some(previous) = directions.insert(origin.offset, emitted) {
                    assert_eq!(
                        previous, emitted,
                        "the @to at offset {} is credited with both directions",
                        origin.offset
                    );
                }
            } else {
                assert_eq!(
                    chars[origin.offset], emitted,
                    "byte {offset} ({emitted}) points at {:?} in the macro source",
                    chars[origin.offset]
                );
            }
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

        // Every `@to` that emitted anything is accounted for exactly once, so
        // a run credited to the wrong directive cannot hide as a duplicate.
        let written = source.match_indices("@to").count();
        assert!(
            directions.len() <= written,
            "{} directives emitted moves but only {written} were written",
            directions.len()
        );
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
        for directive in [
            "@macro clear { [-] }",
            "@include \"lib.bfm\"",
            "@ifdef DEBUG",
        ] {
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

    // ---- pointer tracking ----------------------------------------------

    #[test]
    fn to_emits_the_difference_between_where_it_is_and_where_it_is_going() {
        assert_eq!(expanded("@var a at 0\n@var b at 3\n@to b\n"), ">>>");
        assert_eq!(
            expanded("@var a at 0\n@var b at 3\n@to b\n@to a\n"),
            ">>><<<"
        );
        // Already there: nothing to emit.
        assert_eq!(expanded("@var a at 2\n@var b at 2\n@to a\n@to b\n"), ">>");
    }

    #[test]
    fn a_literal_move_is_tracked_too() {
        // Otherwise `>` and `@to` could not be mixed in one program, which is
        // most of what a partial rewrite looks like.
        assert_eq!(expanded("@var a at 0\n@var b at 3\n>\n@to b\n"), ">>>");
        assert_eq!(
            expanded("@var a at 0\n@var b at 3\n>{5}\n@to b\n"),
            ">>>>><<"
        );
    }

    #[test]
    fn a_balanced_loop_leaves_the_position_where_it_found_it() {
        // `>+<-` returns the cursor, so the expander can keep counting.
        assert_eq!(
            expanded("@var a at 0\n@var b at 2\n+[>+<-]\n@to b\n"),
            "+[>+<-]>>"
        );
    }

    #[test]
    fn to_may_be_used_inside_a_loop_that_puts_the_cursor_back() {
        let source = "@var a at 0\n@var b at 2\n+[\n@to b\n+\n@to a\n-\n]\n";
        assert_eq!(expanded(source), "+[>>+<<-]");
    }

    #[test]
    fn an_unbalanced_loop_loses_the_position_without_being_an_error() {
        // `[>]` is the ordinary scan idiom. Losing track is the correct
        // outcome; refusing to expand would ban the idiom outright.
        assert_eq!(expanded("@var a at 0\n+[>]\n"), "+[>]");

        // It becomes an error only when something needs the position.
        let error = expand("@var a at 0\n@var b at 5\n+[>]\n@to b\n").unwrap_err();
        let MacroError::PositionUnknown {
            name,
            location,
            lost_at,
        } = &error
        else {
            panic!("expected an unknown position, got {error:?}");
        };
        assert_eq!(name, "b");
        assert_eq!(location.line, 4, "the error is at the @to");
        assert_eq!(lost_at.line, 3, "and it names the loop that lost it");
    }

    #[test]
    fn here_re_establishes_a_position_a_scan_lost() {
        // The escape hatch, and the reason it exists: after `[>]` the
        // programmer knows where the cursor landed and the expander cannot.
        // It emits nothing and is trusted.
        assert_eq!(
            expanded("@var a at 0\n@var b at 5\n@var c at 7\n+[>]\n@here b\n@to c\n"),
            "+[>]>>"
        );
    }

    #[test]
    fn to_inside_an_unbalanced_loop_is_refused_at_the_bracket() {
        // The first iteration would emit the right movement and every one
        // after it the wrong movement -- the worst way for this to fail, and
        // not knowable until the ']'.
        let source = "@var a at 0\n@var b at 2\n+[\n>\n@to b\n<-\n]\n";
        let error = expand(source).unwrap_err();
        let MacroError::MovingInsideUnbalancedLoop { location, loop_at } = &error else {
            panic!("expected a move inside an unbalanced loop, got {error:?}");
        };
        assert_eq!(location.line, 5, "the error names the @to");
        assert_eq!(loop_at.line, 3, "and the loop that made it wrong");
    }

    #[test]
    fn a_to_in_a_nested_body_is_inside_the_outer_one_too() {
        // The inner loop balances; the outer does not. The `@to` is still
        // inside something unbalanced, and has to be caught.
        let source = "@var a at 0\n@var b at 1\n+[\n@to b\n@to a\n[-]\n>\n]\n";
        let error = expand(source).unwrap_err();
        assert!(
            matches!(error, MacroError::MovingInsideUnbalancedLoop { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn a_name_means_one_thing() {
        let as_count = expand("@var x at 3\n+{x}\n").unwrap_err();
        let MacroError::WrongKind { found, wanted, .. } = &as_count else {
            panic!("expected a kind error, got {as_count:?}");
        };
        assert_eq!((*found, *wanted), (Kind::Variable, Kind::Constant));

        let as_cell = expand("@define X 3\n@to X\n").unwrap_err();
        let MacroError::WrongKind { found, wanted, .. } = &as_cell else {
            panic!("expected a kind error, got {as_cell:?}");
        };
        assert_eq!((*found, *wanted), (Kind::Constant, Kind::Variable));

        // And the two share a namespace, so this is a redefinition.
        assert!(matches!(
            expand("@define X 3\n@var X at 1\n").unwrap_err(),
            MacroError::Redefinition { .. }
        ));
    }

    #[test]
    fn a_var_says_what_it_expected() {
        for source in ["@var\n", "@var x\n", "@var x 0\n", "@var x at\n"] {
            assert!(
                matches!(
                    expand(source).unwrap_err(),
                    MacroError::MalformedDirective { .. }
                ),
                "{source:?} was accepted"
            );
        }
    }

    #[test]
    fn a_cell_may_be_named_by_a_constant() {
        assert_eq!(
            expanded("@define TOP 4\n@var a at 0\n@var b at TOP\n@to b\n"),
            ">>>>"
        );
    }

    #[test]
    fn moving_left_of_the_tape_is_not_an_error_here() {
        // Movement off the tape is legal under gyrus's tape contract -- only
        // access is checked -- so the expander does not invent a stricter
        // rule than the interpreter has. The design's `NegativePointer` error
        // predates that contract.
        assert_eq!(expanded("@var a at 0\n<{3}\n@to a\n"), "<<<>>>");
    }

    #[test]
    fn origins_survive_the_new_directives() {
        assert_origins_are_exact("@var a at 0\n@var b at 3\n@to b\n+\n@to a\n");
        assert_origins_are_exact("@var a at 0\n@var b at 2\n+[\n@to b\n+\n@to a\n-\n]\n");
    }
}

#[cfg(test)]
mod balance_tests {
    use super::*;

    fn expanded(source: &str) -> String {
        expand(source).expect("expands").brainfuck().to_string()
    }

    #[test]
    fn a_balanced_loop_after_a_scan_does_not_steal_the_blame() {
        // `[-]` is balanced. Judging balance by comparing absolute positions
        // made it look otherwise once the position was already unknown, so
        // the error named it instead of the scan that actually lost track --
        // and the hint said `[-]` does not put the cursor back, which is
        // false.
        let error = expand("@var a at 0\n@var b at 5\n+[>]\n+[-]\n@to b\n").unwrap_err();
        let MacroError::PositionUnknown { lost_at, .. } = &error else {
            panic!("expected an unknown position, got {error:?}");
        };
        assert_eq!(lost_at.line, 3, "the scan lost it, not the clear on line 4");
    }

    #[test]
    fn here_works_inside_a_loop_body() {
        // The escape hatch has to work where scan-then-use actually lives.
        // The body emits `>>` then `<<`: net zero, right on every iteration.
        let source = "@var a at 0\n@var b at 2\n+[>]\n+[\n@here a\n@to b\n@to a\n]\n";
        assert_eq!(expanded(source), "+[>]+[>><<]");
    }

    #[test]
    fn here_does_not_switch_off_the_unbalanced_loop_check() {
        // Balance is measured in movement, not in where the cursor is
        // believed to be, so `@here` cannot talk the `]` out of noticing that
        // the body really does move by one each time.
        let source = "@var a at 0\n@var b at 3\n+[\n>\n@here a\n@to b\n@to a\n]\n";
        let error = expand(source).unwrap_err();
        assert!(
            matches!(error, MacroError::MovingInsideUnbalancedLoop { .. }),
            "a body with net movement was accepted: {error:?}"
        );
    }

    #[test]
    fn no_input_makes_the_expander_panic_on_arithmetic() {
        // Both of these panicked in a debug build: the subtraction in `@to`
        // and the addition in `step`. A preprocessor must not panic on input
        // it did not write, and a cell nothing could reach is now refused
        // where it is declared.
        for source in [
            "@var big at 9223372036854775807\n<\n@to big\n",
            "@var big at 9223372036854775807\n@here big\n>\n",
        ] {
            let error = expand(source).unwrap_err();
            assert!(
                matches!(error, MacroError::MalformedDirective { .. }),
                "{source:?}: got {error:?}"
            );
        }
    }

    #[test]
    fn a_cell_beyond_the_expansion_budget_is_refused_where_it_is_declared() {
        let just_past = EXPANSION_LIMIT + 1;
        let error = expand(&format!("@var far at {just_past}\n")).unwrap_err();
        let MacroError::MalformedDirective { detail, .. } = &error else {
            panic!("expected a malformed directive, got {error:?}");
        };
        assert!(detail.contains("budget"), "{detail}");

        // And the last reachable cell is still fine.
        assert!(expand(&format!("@var edge at {EXPANSION_LIMIT}\n")).is_ok());
    }
}
