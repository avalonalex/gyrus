//! The expander: `.bfm` source in, pure BrainFuck plus an origin map out.
//!
//! A single pass over the characters. There is no separate lexer, parser and
//! code generator, because at this size they would be three names for the same
//! loop -- `gyrus`'s own parser and `gyrus-corpus`'s TOML reader are written
//! the same way, and for the same reason.
//!
//! # What it understands
//!
//! - `@define NAME VALUE` -- a named constant. Definition precedes use: one
//!   pass, as `cpp` has.
//! - `@var NAME at N` -- a named cell, and `@to NAME` to move the cursor
//!   there: the expander tracks where the cursor is and emits the difference.
//!   `@var NAME` alone has a cell chosen for it, which is the half that makes
//!   a layout maintainable -- the point of naming cells is to stop counting
//!   them, and `at N` still makes you count them once.
//! - `@here NAME` -- tell the expander where the cursor is without moving it.
//! - `@macro NAME(a, b) { ... }`, invoked as `@NAME(1, 2)` -- a body expanded
//!   in place. An argument is evaluated in the caller's scope and bound to the
//!   parameter, so a number, a constant and a cell all pass the same way.
//! - `OP{N}` -- repeat `OP` N times, where `OP` is one of `+ - < > . ,` and `N`
//!   is a number or a defined name. `+{0}` is nothing, which is legal.
//! - `*` to end of line, and any character that is not a BrainFuck instruction:
//!   comments, dropped from the expansion.
//!
//! Wherever a number may be written, so may a character or a hexadecimal
//! number: `'A'`, `'\n'` and `0x41` are all 65. They go through one
//! classifier, so a repeat count, a `@define`, a `@var`'s cell and a macro
//! argument all understand them without any of them being told.
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
//! Between them, a `.bfm` written today cannot change meaning when the
//! conditionals arrive.
//!
//! # Macros are expanded in place
//!
//! A macro is not a subroutine: each invocation emits the whole body, so the
//! cursor tracking, the loop rules and the expansion budget apply to it with
//! no special case -- which is why `@to` works inside a body and why a body
//! that moves the cursor makes its enclosing loop unbalanced like any other
//! movement would.
//!
//! A body is a *span of this source* rather than a copy, so invoking a macro
//! is moving the cursor and scanning to the closing brace. Every position
//! inside a body is therefore a real position in the file, which is what lets
//! a macro error point into the definition. Emitted bytes are the other way
//! round: they name the invocation, because the map holds one position per
//! byte and a definition used twenty times would not say which of them failed.
//!
//! A body sees its own parameters and the file's names, never its caller's,
//! and cannot declare anything -- a `@define` inside one would run again on
//! the second invocation and collide with itself.
//!
//! Three limits bound expansion, and each exists because the others do not
//! see the case it covers. [`EXPANSION_LIMIT`] counts emitted instructions,
//! [`MACRO_DEPTH_LIMIT`] the nesting -- a cycle is caught by name, but a long
//! enough chain of *different* macros is not, and expansion recurses -- and
//! [`INVOCATION_LIMIT`] the invocations, because macros that emit nothing
//! still cost time and a few doubling wrappers reach billions of them in half
//! a page.
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
//! These three rules are not novel, and it is worth knowing they were reached
//! before: Frans Faase's bfmacro
//! (<https://www.cs.tufts.edu/~couch/bfmacro/bfmacro/>), which the design cites
//! as its inspiration, tracks the pointer statically through balanced loops,
//! gives `[>]` as its own example of the case that defeats it, makes its `to`
//! an error afterwards, and provides an `at` directive to re-anchor. Arriving
//! at the same answer independently is reassurance rather than a coincidence:
//! there is not much room in this problem.
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
use std::rc::Rc;

use gyrus::SourceLocation;

use crate::directive::{Declaration, Directive};
use crate::error::{Kind, MacroError};
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

/// How deep macro invocations may nest.
///
/// A macro that uses itself is caught by name, but a chain of a thousand
/// different macros is not -- and expansion recurses, so a deep enough chain
/// aborts the process with a stack overflow instead of reporting anything.
/// Nothing legible needs more than a handful of levels.
pub const MACRO_DEPTH_LIMIT: usize = 64;

/// How many macro invocations one file may expand.
///
/// [`EXPANSION_LIMIT`] bounds emitted instructions, which bounds nothing here:
/// twenty-two macros that each invoke the previous one twice are five hundred
/// bytes of source, four million invocations, twelve seconds, and no output at
/// all. Thirty levels is an hour. The budget has to count the work, not just
/// the result.
pub const INVOCATION_LIMIT: u64 = 100_000;

/// Instructions a repeat count may follow. Brackets are deliberately absent:
/// `[{3}` would mean three loop openings, which is not a thing anyone means.
const REPEATABLE: [char; 6] = ['+', '-', '<', '>', '.', ','];

/// Expand `.bfm` source into BrainFuck, keeping the origin of every byte.
pub fn expand(source: &str) -> Result<Expansion, MacroError> {
    Scanner::new(source).run()
}

/// A macro definition: its parameters, and the span its body occupies.
#[derive(Debug)]
struct MacroDef {
    params: Vec<String>,
    /// Where the body starts, and the offset one past its last character.
    body_start: SourceLocation,
    body_end: usize,
}

/// What a name means. One namespace, so a name means one thing.
#[derive(Debug, Clone)]
enum Symbol {
    /// A number, usable as a repeat count.
    Constant(u64),
    /// A cell, usable as a `@to` target.
    Variable(i64),
    /// A body to expand, usable as `@name(...)`.
    Macro(Rc<MacroDef>),
}

impl Symbol {
    fn kind(&self) -> Kind {
        match self {
            Symbol::Constant(_) => Kind::Constant,
            Symbol::Variable(_) => Kind::Variable,
            Symbol::Macro(_) => Kind::Macro,
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

/// One macro invocation being expanded.
struct Invocation {
    name: String,
    /// What was invoked, so a parameter resolves by position.
    def: Rc<MacroDef>,
    /// The arguments, in the definition's parameter order.
    ///
    /// A `HashMap` keyed by parameter name was allocated and filled per
    /// invocation, cloning every parameter name into it, to be searched by a
    /// handful of lookups. A body has a handful of parameters, so position is
    /// cheaper and simpler. That, and not formatting an error message per
    /// invocation, is most of why 100,000 invocations went from 73.6ms to
    /// 45.8ms, and 6.4ms from 11.0ms with no arguments to bind.
    arguments: Vec<(Symbol, SourceLocation)>,
    /// Where the body starts: a directive must begin its line, and the start
    /// of a body counts as one, or `@macro reset { @clear }` would read its
    /// own invocation as prose.
    boundary: usize,
    /// Where the invocation was written. The outermost frame's is what an
    /// emitted byte reports.
    call_site: SourceLocation,
    /// Where to resume once the body has been scanned.
    resume: SourceLocation,
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
    /// One entry per macro invocation in flight, innermost last.
    ///
    /// This was four fields -- the parameter frames, the names being expanded,
    /// the line-start boundary and the origin override -- pushed and popped
    /// together by hand around one `scan_until`. Every one of them is a
    /// function of this stack, so each is an accessor now rather than an
    /// invariant somebody has to maintain: the depth is `len`, the cycle test
    /// is over the names, the boundary is the innermost frame's, and the
    /// origin override is `first`, which is what "the outermost invocation"
    /// means. The flag that used to compute that last one went with it.
    frames: Vec<Invocation>,
    /// Invocations so far, against [`INVOCATION_LIMIT`].
    invocations: u64,
    /// Cells a `@var` has already named, so that one written without `at` can
    /// be given one nobody is using.
    cells_taken: std::collections::BTreeSet<i64>,
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
            frames: Vec::new(),
            invocations: 0,
            cells_taken: std::collections::BTreeSet::new(),
            defined: Vec::new(),
            position: Position::Known(0),
            net: 0,
            // Most of a `.bfm` is instructions, so its length is a decent
            // first guess and spares the early doublings.
            out: String::with_capacity(source.len().min(EXPANSION_LIMIT)),
            origins: Vec::with_capacity(source.len().min(EXPANSION_LIMIT)),
            open_brackets: Vec::new(),
        }
    }

    fn run(mut self) -> Result<Expansion, MacroError> {
        self.scan_until(self.chars.len())?;

        if let Some(open) = self.open_brackets.first() {
            return Err(MacroError::UnmatchedOpenBracket {
                location: open.location,
            });
        }

        Ok(Expansion::new(self.source, self.out, self.origins))
    }

    /// Expand characters up to `end`.
    ///
    /// Bounded rather than "to the end of the input" because a macro body is
    /// a span of this same source; see this module's documentation.
    fn scan_until(&mut self, end: usize) -> Result<(), MacroError> {
        while self.at.offset < end {
            let Some(c) = self.peek() else { break };
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
        Ok(())
    }

    // ---- character-level helpers -------------------------------------------

    fn peek(&self) -> Option<char> {
        self.chars.get(self.at.offset).copied()
    }

    /// Whether only blanks precede the cursor on its line.
    ///
    /// Asked only when the cursor is on `@`, which is rare, so walking back to
    /// the newline is cheaper than a flag every `bump` would have to maintain.
    /// The offset the current scan began at: zero, or the innermost body's
    /// first character.
    fn boundary(&self) -> usize {
        self.frames.last().map_or(0, |frame| frame.boundary)
    }

    /// Whether a macro body is being expanded.
    fn inside_a_macro(&self) -> bool {
        !self.frames.is_empty()
    }

    fn at_line_start(&self) -> bool {
        let boundary = self.boundary();
        self.chars[boundary.min(self.at.offset)..self.at.offset]
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

    /// Whether the identifier at the cursor is exactly `want`, consuming it
    /// either way. Spares the `String` that comparing `identifier()` against
    /// a keyword allocates and drops.
    fn identifier_is(&mut self, want: &str) -> bool {
        let start = self.at.offset;
        while self.peek().is_some_and(is_identifier_char) {
            self.bump();
        }
        self.chars[start..self.at.offset]
            .iter()
            .copied()
            .eq(want.chars())
    }

    /// A token at the cursor, ending at the first character `ends` accepts
    /// that is not inside a character literal.
    ///
    /// The quotes are why this is shared rather than written at each of the
    /// three places that read a value. `@define SPACE ' '` ends its token on a
    /// space, `@print(',')` on a comma and `+{'}'}` on a brace -- each of them
    /// the delimiter that reader would otherwise stop at, and each of them the
    /// obvious thing to write.
    fn token(&mut self, ends: impl Fn(char) -> bool) -> String {
        let mut token = String::new();
        let mut quoted = false;
        let mut escaped = false;
        while let Some(c) = self.peek() {
            // A newline ends a token whatever the quoting, so an unclosed
            // quote cannot swallow the rest of the file looking for its pair
            // and then blame a line far below the one it is on.
            if c == '\n' || (!quoted && ends(c)) {
                break;
            }
            match c {
                '\\' if quoted => escaped = !escaped,
                '\'' if !escaped => quoted = !quoted,
                _ => escaped = false,
            }
            token.push(c);
            self.bump();
        }
        token
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
        // Bytes made inside a macro say they came from the invocation, not
        // from the definition. The map holds one position per byte and a
        // macro gives a byte two, so this is a choice: the invocation is the
        // line of the program the reader wrote, and a definition used twenty
        // times would not say which of them failed. `first` is what "the
        // outermost invocation" means, so no origin points inside a body.
        let origin = self.frames.first().map_or(origin, |frame| frame.call_site);
        let total = self.out.len().saturating_add(count as usize);
        if total > EXPANSION_LIMIT {
            return Err(MacroError::ExpansionTooLarge {
                emitted: total,
                limit: EXPANSION_LIMIT,
                location: origin,
            });
        }
        // Filled in bulk rather than a byte at a time. `origins` holds a
        // `SourceLocation` -- 24 bytes -- per emitted byte, and growing it
        // from nothing by doubling costs nineteen reallocations and 25 MB of
        // copying to reach the limit. Measured about 3x faster on
        // `+{1000000}` (2.97ms to 0.95ms) and on a long `@to`, and 1.2x on a
        // file of single instructions, so it is not a trade.
        let n = count as usize;
        self.origins.resize(self.origins.len() + n, origin);
        self.out.extend(std::iter::repeat_n(c, n));
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
            return self.emit_run(c, 1, origin);
        }

        let Some(open) = self.open_brackets.pop() else {
            return Err(MacroError::UnmatchedCloseBracket { location: origin });
        };
        self.emit_run(c, 1, origin)?;

        if self.net != open.net_at_entry {
            // Rule 3 in this module's documentation: not knowable before
            // this bracket, which is why it is reported here.
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

        Ok(())
    }

    /// A `{...}` repeat count at the cursor, resolved to a number.
    fn repeat_count(&mut self) -> Result<u64, MacroError> {
        let open = self.at;
        self.bump(); // past '{'

        let body = self.token(|c| c == '}' || c == '\n');
        // A newline inside a repeat count means the '}' was forgotten;
        // scanning on would swallow the rest of the program looking for one,
        // and blame a line far below the mistake.
        if self.peek() != Some('}') {
            // Which of the two is missing, since an unclosed quote reaches
            // the end of the line without the brace being the problem.
            let detail = match body.starts_with('\'') && !body.ends_with('\'') {
                true => format!("{body} has no closing quote"),
                false => "no closing '}'".to_string(),
            };
            return Err(MacroError::BadRepeatCount {
                detail,
                location: open,
            });
        }
        self.bump();
        let body = body.trim();
        let bad = |detail: String| MacroError::BadRepeatCount {
            detail,
            location: open,
        };
        let count = match classify(body) {
            Operand::Number(value) => value,
            Operand::Name(named) => self.resolve(named, open)?,
            Operand::Empty => {
                return Err(bad(
                    "empty. Write a number, as in +{65}, or a name defined with @define"
                        .to_string(),
                ));
            }
            Operand::Bad(detail) => return Err(bad(detail)),
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
        self.binding(name)
            .cloned()
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
            (other, declared) => Err(MacroError::wrong_kind(
                name,
                other.kind(),
                Kind::Constant,
                location,
                declared,
            )),
        }
    }

    /// A name used where a cell is wanted.
    fn resolve_cell(&self, name: &str, location: SourceLocation) -> Result<i64, MacroError> {
        match self.lookup(name, location)? {
            (Symbol::Variable(cell), _) => Ok(cell),
            (other, declared) => Err(MacroError::wrong_kind(
                name,
                other.kind(),
                Kind::Variable,
                location,
                declared,
            )),
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
        // Only declarations that could actually take effect. One inside a
        // macro body is refused outright, so advertising it would send the
        // reader to move their code below a definition that will never run.
        //
        // A plain loop rather than `.any()` over a closure mutating its own
        // capture: the depth has to be read before the line updates it, and
        // stating that as an order of statements is clearer than as a value
        // computed at the top of a closure and returned at the bottom.
        let mut depth = 0usize;
        for line in rest.lines() {
            if depth == 0 && declares(line.trim_start(), name) {
                return true;
            }
            depth = brace_depth_after(line, depth);
        }
        false
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
        match Directive::from_spelling(&name) {
            Some(Directive::Define) => self.define(location),
            Some(Directive::Var) => self.var(location),
            Some(Directive::To) => self.to(location),
            Some(Directive::Here) => self.here(),
            Some(Directive::Macro) => self.macro_definition(location),
            Some(_) => Err(MacroError::PlannedDirective { name, location }),
            // Not a directive, so it may be a macro -- checked after the
            // directives, so no macro can shadow one, and a name that is
            // neither is reported as what it looks like.
            None => match self.macro_named(&name) {
                Some(def) => self.invoke(name, def, location),
                None => Err(MacroError::UnknownDirective { name, location }),
            },
        }
    }

    /// What a name is bound to here: the innermost body's parameters first,
    /// then the file's names.
    ///
    /// One copy of the precedence rule. Writing it twice is what made a
    /// parameter honoured inside a body by `+{name}` and ignored by `@name`,
    /// so one name meant two things in one body -- and the first fix for that
    /// was to copy the rule again rather than share it.
    ///
    /// Only the innermost frame's parameters are visible, not a chain: a body
    /// sees its own parameters and the file's names, never its caller's, which
    /// is what makes a macro readable in isolation.
    fn binding(&self, name: &str) -> Option<&(Symbol, SourceLocation)> {
        self.frames
            .last()
            .and_then(|frame| {
                let index = frame.def.params.iter().position(|param| param == name)?;
                frame.arguments.get(index)
            })
            .or_else(|| self.symbols.get(name))
    }

    /// The macro a name means here, parameters included.
    fn macro_named(&self, name: &str) -> Option<Rc<MacroDef>> {
        match self.binding(name)? {
            (Symbol::Macro(def), _) => Some(Rc::clone(def)),
            _ => None,
        }
    }

    /// A declaration inside a macro body would run again on the next
    /// invocation and collide with itself, so it is refused where it is
    /// written rather than on the second call.
    fn refuse_inside_a_macro(
        &self,
        directive: Directive,
        location: SourceLocation,
    ) -> Result<(), MacroError> {
        if !self.inside_a_macro() {
            return Ok(());
        }
        Err(MacroError::DeclarationInsideMacro {
            directive: directive.spelling(),
            location,
        })
    }

    /// `@macro NAME { body }` or `@macro NAME(a, b) { body }`.
    fn macro_definition(&mut self, location: SourceLocation) -> Result<(), MacroError> {
        self.refuse_inside_a_macro(Directive::Macro, location)?;
        self.skip_blanks();
        let at_name = self.at;
        let name = self.identifier();
        if name.is_empty() {
            return Err(malformed(
                Directive::Macro.spelling(),
                "expected a name, as in `@macro clear { [-] }`".to_string(),
                at_name,
            ));
        }
        self.refuse_a_directive_name(&name, Directive::Macro, at_name)?;
        if let Some((_, first)) = self.symbols.get(&name) {
            return Err(MacroError::Redefinition {
                name,
                first: *first,
                location,
            });
        }

        let params = self.parameter_list(&name)?;

        self.skip_blanks();
        if self.peek() != Some('{') {
            return Err(malformed(
                Directive::Macro.spelling(),
                format!("expected '{{' to open the body of '{name}'"),
                self.at,
            ));
        }
        self.bump();
        let body_start = self.at;
        let body_end = self.skip_body(&name, location)?;
        // Like every other directive. Without this `@macro m { + }+++` dropped
        // three instructions somebody wrote, and `@macro m { + } @c` dropped a
        // whole invocation.
        self.end_of_directive(Directive::Macro)?;

        self.declare(
            name,
            Symbol::Macro(Rc::new(MacroDef {
                params,
                body_start,
                body_end,
            })),
            location,
        );
        Ok(())
    }

    /// A parenthesised, comma-separated list, or nothing if there is no `(`.
    ///
    /// Shared by a definition's parameters and an invocation's arguments,
    /// which were the same loop written twice and had already drifted: one
    /// skipped blanks before the `(` and the other did not, so a space was
    /// legal at a call site and an error about a missing `{` at a definition.
    /// Writing it once also stopped both from accepting a trailing comma.
    fn paren_list<T>(
        &mut self,
        directive: &str,
        what: impl Fn() -> String,
        mut item: impl FnMut(&mut Self) -> Result<T, MacroError>,
    ) -> Result<Vec<T>, MacroError> {
        let mut items = Vec::new();
        self.skip_blanks();
        if self.peek() != Some('(') {
            return Ok(items);
        }
        self.bump();
        self.skip_blanks();
        if self.peek() == Some(')') {
            self.bump();
            return Ok(items);
        }
        loop {
            self.skip_blanks();
            items.push(item(self)?);
            self.skip_blanks();
            match self.peek() {
                Some(',') => self.bump(),
                Some(')') => {
                    self.bump();
                    return Ok(items);
                }
                _ => {
                    return Err(malformed(
                        directive,
                        format!("expected ',' or ')' in {}", what()),
                        self.at,
                    ));
                }
            }
        }
    }

    /// `(a, b)` after a macro's name, or nothing.
    fn parameter_list(&mut self, name: &str) -> Result<Vec<String>, MacroError> {
        let params = self.paren_list(
            Directive::Macro.spelling(),
            || format!("the parameters of '{name}'"),
            |s| {
                let at_param = s.at;
                let param = s.identifier();
                if param.is_empty() {
                    return Err(malformed(
                        Directive::Macro.spelling(),
                        "expected a parameter name".to_string(),
                        at_param,
                    ));
                }
                Ok((param, at_param))
            },
        )?;

        // Checked after the list rather than inside it, so the list parser
        // knows nothing about what it is collecting.
        for (index, (param, at)) in params.iter().enumerate() {
            if params[..index].iter().any(|(seen, _)| seen == param) {
                return Err(malformed(
                    Directive::Macro.spelling(),
                    format!("'{name}' already has a parameter called '{param}'"),
                    *at,
                ));
            }
        }
        Ok(params.into_iter().map(|(param, _)| param).collect())
    }

    /// Walk to the `}` that closes a body, and return its offset.
    ///
    /// It has to read the body the way the scanner will: a `*` comment can
    /// contain a brace, and `+{3}` is a repeat count rather than a nested
    /// body. Getting either wrong would end the body in the wrong place.
    fn skip_body(&mut self, name: &str, location: SourceLocation) -> Result<usize, MacroError> {
        let mut depth = 1usize;
        // Set once per character at the foot of the loop, rather than at the
        // end of each arm: `*`, `{` and `}` are not repeatable, so all five
        // assignments were the same expression about the character that
        // entered the arm.
        let mut after_instruction = false;
        loop {
            let Some(c) = self.peek() else {
                return Err(malformed(
                    Directive::Macro.spelling(),
                    format!("the body of '{name}' is never closed with '}}'"),
                    location,
                ));
            };
            match c {
                '*' => {
                    // To the end of the line, or to the brace that closes the
                    // body, whichever comes first. Otherwise a one-line
                    // `@macro clear { [-] * clears it }` -- the natural thing
                    // to write, and the shape the docs use -- reports its own
                    // body as never closed.
                    while self
                        .peek()
                        .is_some_and(|c| c != '\n' && !(c == '}' && depth == 1))
                    {
                        self.bump();
                    }
                }
                '{' if after_instruction => {
                    // A repeat count, not a nested brace.
                    while self.peek().is_some_and(|c| c != '}' && c != '\n') {
                        self.bump();
                    }
                    if self.peek() == Some('}') {
                        self.bump();
                    }
                }
                '{' => {
                    depth += 1;
                    self.bump();
                }
                '}' => {
                    depth -= 1;
                    let end = self.at.offset;
                    self.bump();
                    if depth == 0 {
                        return Ok(end);
                    }
                }
                _ => self.bump(),
            }
            after_instruction = REPEATABLE.contains(&c);
        }
    }

    /// `@name` or `@name(a, b)` -- expand a macro's body here.
    fn invoke(
        &mut self,
        name: String,
        def: Rc<MacroDef>,
        location: SourceLocation,
    ) -> Result<(), MacroError> {
        let arguments = self.argument_list(&name)?;
        if arguments.len() != def.params.len() {
            return Err(MacroError::ArgumentCount {
                name,
                expected: def.params.len(),
                actual: arguments.len(),
                location,
            });
        }
        self.end_of_directive(Directive::Macro)?;

        if self.frames.iter().any(|frame| frame.name == name) {
            let mut chain: Vec<String> =
                self.frames.iter().map(|frame| frame.name.clone()).collect();
            chain.push(name.clone());
            return Err(MacroError::CircularMacro {
                name,
                chain,
                location,
            });
        }
        // Cycles are caught by name; depth and volume are not.
        if self.frames.len() >= MACRO_DEPTH_LIMIT {
            return Err(MacroError::MacroTooDeep {
                limit: MACRO_DEPTH_LIMIT,
                location,
            });
        }
        self.invocations += 1;
        if self.invocations > INVOCATION_LIMIT {
            return Err(MacroError::TooManyInvocations {
                limit: INVOCATION_LIMIT,
                location,
            });
        }

        // A push, a scan, a pop. What used to be saved and restored around
        // this by hand lives in the frame, so there is no invariant left to
        // break -- and no way for an early return to leak half of it.
        let (body_start, body_end) = (def.body_start, def.body_end);
        self.frames.push(Invocation {
            name,
            def,
            arguments,
            boundary: body_start.offset,
            call_site: location,
            resume: self.at,
        });
        self.at = body_start;

        let result = self.scan_until(body_end);

        self.at = self.frames.pop().expect("pushed above").resume;
        result
    }

    /// `(65, counter)` after an invocation, resolved in the caller's scope.
    fn argument_list(&mut self, name: &str) -> Result<Vec<(Symbol, SourceLocation)>, MacroError> {
        // Reported against the invoked name rather than `macro`: `@set(9x)`
        // said "Malformed @macro", and `@macro` appears nowhere in a program
        // that only invokes one. `what` is a closure because it is used only
        // on the error path, and was being formatted on every invocation.
        self.paren_list(
            name,
            || format!("the arguments of '@{name}'"),
            |s| {
                let at_argument = s.at;
                let token = s.token(|c| c == ',' || c == ')' || c.is_whitespace());
                // A number is a constant; a name is whatever it already means, so
                // a cell or another macro passes as readily as a count.
                let symbol = match classify(&token) {
                    Operand::Number(value) => Symbol::Constant(value),
                    Operand::Name(named) => s.lookup(named, at_argument)?.0,
                    Operand::Empty => {
                        return Err(malformed(
                            name,
                            "expected an argument".to_string(),
                            at_argument,
                        ));
                    }
                    Operand::Bad(detail) => return Err(malformed(name, detail, at_argument)),
                };
                Ok((symbol, at_argument))
            },
        )
    }

    /// `@var NAME at N` -- name a cell. Or `@var NAME`, and one is chosen.
    ///
    /// Choosing is the half that makes a layout maintainable: the point of
    /// naming cells is to stop counting them, and `at N` still makes you count
    /// them once. Writing the number remains available, because a program that
    /// cares where a cell sits -- one laying out a string to scan over, say --
    /// has to be able to say so.
    fn var(&mut self, location: SourceLocation) -> Result<(), MacroError> {
        self.refuse_inside_a_macro(Directive::Var, location)?;
        let name = self.declared_name(Declaration::Var, location)?;
        self.skip_blanks();

        let cell = match self.peek() {
            // Nothing follows the name, so choose a cell for it.
            None | Some('\n') | Some('*') => self.next_free_cell(),
            _ => {
                let at_keyword = self.at;
                if !self.identifier_is("at") {
                    return Err(malformed(
                        Directive::Var.spelling(),
                        format!(
                            "expected `at`, as in `@var {name} at 0` -- or nothing after the \
                             name, to have a cell chosen"
                        ),
                        at_keyword,
                    ));
                }
                self.skip_blanks();
                let at_value = self.at;
                let cell = self.number_or_name(Declaration::Var, &name, at_value)?;
                // Reaching cell N costs N moves, so a cell past the expansion
                // budget is one no program could move to. Refusing it here is
                // also what keeps every tracked position small enough that the
                // arithmetic on them cannot overflow.
                if cell > EXPANSION_LIMIT as u64 {
                    return Err(MacroError::CellTooFar {
                        cell,
                        limit: EXPANSION_LIMIT,
                        location: at_value,
                    });
                }
                cell as i64
            }
        };

        self.end_of_directive(Directive::Var)?;
        self.cells_taken.insert(cell);
        self.declare(name, Symbol::Variable(cell), location);
        Ok(())
    }

    /// The lowest cell no `@var` has named.
    ///
    /// Lowest rather than next-after-the-last, so that mixing the two spellings
    /// does not leave a hole: `@var scratch at 9` followed by three plain
    /// `@var`s gives cells 0, 1 and 2, not 10, 11 and 12.
    fn next_free_cell(&self) -> i64 {
        (0..)
            .find(|cell| !self.cells_taken.contains(cell))
            .expect("i64 is not exhausted")
    }

    /// The cell a `@to` or `@here` names, and its own name for the error.
    fn cell_operand(&mut self, directive: Directive) -> Result<(String, i64), MacroError> {
        self.skip_blanks();
        let at_name = self.at;
        let name = self.identifier();
        if name.is_empty() {
            return Err(malformed(
                directive.spelling(),
                "expected the name of a cell declared with @var".to_string(),
                at_name,
            ));
        }
        let cell = self.resolve_cell(&name, at_name)?;
        self.end_of_directive(directive)?;
        Ok((name, cell))
    }

    /// `@to NAME` -- move the cursor to a named cell.
    fn to(&mut self, location: SourceLocation) -> Result<(), MacroError> {
        let (name, target) = self.cell_operand(Directive::To)?;

        // A `@to` is inside every loop currently open, so every open frame
        // records it. Setting only the innermost and lifting it to the parent
        // at each `]` reached the same answer, but stated one rule in two
        // places. Nesting is shallow, so the walk costs nothing.
        for open in &mut self.open_brackets {
            open.to_inside.get_or_insert(location);
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
        let (_, cell) = self.cell_operand(Directive::Here)?;
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
        declaring: Declaration,
        location: SourceLocation,
    ) -> Result<String, MacroError> {
        self.skip_blanks();
        let at_name = self.at;
        let name = self.identifier();
        if name.is_empty() {
            return Err(MacroError::MalformedDirective {
                directive: declaring.directive().spelling().to_string(),
                detail: format!("expected a name, as in {}", declaring.name_example()),
                location: at_name,
            });
        }
        self.refuse_a_directive_name(&name, declaring.directive(), at_name)?;
        if let Some((_, first)) = self.symbols.get(&name) {
            return Err(MacroError::Redefinition {
                name,
                first: *first,
                location,
            });
        }
        Ok(name)
    }

    /// A name spelled like a directive is refused where it is declared.
    ///
    /// Directives are checked before macros, so a macro called `to` could be
    /// defined and then never invoked -- every `@to` would be read as the
    /// directive and fail with a message about `@var`, in a program that has
    /// none. Refusing all such declarations, not only macros, keeps one rule.
    fn refuse_a_directive_name(
        &self,
        name: &str,
        directive: Directive,
        location: SourceLocation,
    ) -> Result<(), MacroError> {
        if Directive::from_spelling(name).is_none() {
            return Ok(());
        }
        Err(malformed(
            directive.spelling(),
            format!("'{name}' is the name of a directive, so `@{name}` would never reach this"),
            location,
        ))
    }

    /// A decimal number or the name of a constant.
    fn number_or_name(
        &mut self,
        declaring: Declaration,
        name: &str,
        at_value: SourceLocation,
    ) -> Result<u64, MacroError> {
        let token = self.token(char::is_whitespace);
        let spelling = declaring.directive().spelling();
        match classify(&token) {
            Operand::Number(value) => Ok(value),
            Operand::Name(named) => self.resolve(named, at_value),
            Operand::Empty => Err(malformed(spelling, declaring.missing_value(name), at_value)),
            Operand::Bad(detail) => Err(malformed(spelling, detail, at_value)),
        }
    }

    /// A directive owns the rest of its line, and says so rather than dropping
    /// what follows. Skipping silently discarded any instruction written after
    /// it -- and a discarded ']' went on to blame a '[' the source matched.
    fn end_of_directive(&mut self, directive: Directive) -> Result<(), MacroError> {
        self.skip_blanks();
        match self.peek() {
            None | Some('\n') => Ok(()),
            // The brace that closes the body we are inside. `scan_until`
            // stops at it, so leaving it unconsumed is what ends the body --
            // and without this a one-line `@macro reset { @clear }` would
            // read its own closing brace as junk after the invocation.
            // Asked of `expanding`, which records being inside a body, rather
            // than of `boundary`, which only happens to today.
            Some('}') if self.inside_a_macro() => Ok(()),
            Some('*') => {
                self.skip_line();
                Ok(())
            }
            Some(c) => Err(MacroError::MalformedDirective {
                directive: directive.spelling().to_string(),
                detail: format!(
                    "'{c}' follows it. A @{} takes the rest of its line: \
                     move this to a line of its own, or start a comment with '*'",
                    directive.spelling()
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
        self.refuse_inside_a_macro(Directive::Define, location)?;
        let name = self.declared_name(Declaration::Define, location)?;
        self.skip_blanks();
        let at_value = self.at;
        let value = self.number_or_name(Declaration::Define, &name, at_value)?;
        self.end_of_directive(Directive::Define)?;
        self.declare(name, Symbol::Constant(value), location);
        Ok(())
    }
}

/// What a token written where a value belongs turns out to be.
///
/// The three-way decision was written out at each of the three places that
/// make it -- a repeat count, a declaration's value, an argument -- and they
/// had already drifted on how to word the same failure.
enum Operand<'a> {
    Number(u64),
    /// Borrowed from the token the caller already holds. Owning it meant a
    /// heap copy per name resolved, and a body is re-scanned per invocation.
    Name(&'a str),
    /// Nothing was written. Each site says what it wanted there, because only
    /// the site knows.
    Empty,
    /// Not a value, and why. The wording lives here rather than at each of
    /// the three sites that report it -- which is where it drifted before.
    Bad(String),
}

fn classify(token: &str) -> Operand<'_> {
    if token.is_empty() {
        return Operand::Empty;
    }
    // `'A'` and `0x41` mean 65, and read better than it wherever a byte is
    // what is meant. Both land here rather than at the sites that want a
    // number, so a repeat count, a `@define`, a `@var`'s cell and a macro
    // argument all understand them without any of them being told.
    if token.starts_with('\'') {
        return match character(token) {
            Ok(byte) => Operand::Number(u64::from(byte)),
            Err(why) => Operand::Bad(format!("{token} is not a character: {why}")),
        };
    }
    if let Some(digits) = token
        .strip_prefix("0x")
        .or_else(|| token.strip_prefix("0X"))
    {
        return match u64::from_str_radix(digits, 16) {
            Ok(value) if !digits.is_empty() => Operand::Number(value),
            _ => Operand::Bad(format!("'{token}' is not a hexadecimal number")),
        };
    }
    if token.chars().all(|c| c.is_ascii_digit()) {
        return match token.parse() {
            Ok(value) => Operand::Number(value),
            // Never the u64::MAX it failed to parse into: that is a number
            // the user did not type.
            Err(_) => Operand::Bad(format!("'{token}' does not fit in a 64-bit number")),
        };
    }
    if is_identifier(token) {
        return Operand::Name(token);
    }
    Operand::Bad(format!("'{token}' is neither a number nor a name"))
}

/// The byte a `'x'` literal means.
///
/// One byte, because a cell holds one: `'\u{e9}'` is 233 and fits, and a
/// character that does not is a mistake rather than something to truncate --
/// which is the same rule the test manifest applies to its expected output.
fn character(token: &str) -> Result<u8, String> {
    let body = token
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
        .ok_or("it has no closing quote")?;
    let mut chars = body.chars();
    let value = match chars.next().ok_or("it is empty")? {
        '\\' => match chars.next().ok_or("nothing follows the backslash")? {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            '0' => '\0',
            '\\' => '\\',
            '\'' => '\'',
            other => return Err(format!("\\{other} is not an escape this understands")),
        },
        plain => plain,
    };
    if chars.next().is_some() {
        return Err("it holds more than one character".to_string());
    }
    u8::try_from(u32::from(value)).map_err(|_| format!("{value:?} does not fit in a cell"))
}

/// A `MalformedDirective`, without the five-line struct literal it was written
/// as at eight new sites -- each of which spelled the directive out as a
/// string, in the one crate whose `directive` module exists because that
/// vocabulary had been spelled out five times and drifted.
fn malformed(directive: &str, detail: String, location: SourceLocation) -> MacroError {
    MacroError::MalformedDirective {
        directive: directive.to_string(),
        detail,
        location,
    }
}

/// Whether a line declares `name`, for the "defined below" hint.
fn declares(trimmed: &str, name: &str) -> bool {
    Directive::ALL
        .into_iter()
        .filter(|d| d.declaration().is_some())
        .filter_map(|d| trimmed.strip_prefix(&format!("@{}", d.spelling())))
        .any(|tail| {
            tail.trim_start()
                .strip_prefix(name)
                .is_some_and(|after| !after.starts_with(is_identifier_char))
        })
}

/// Brace depth after a line, for the same hint. Crude but sufficient for one:
/// a repeat count's brace sits against an instruction, and a body's does not.
fn brace_depth_after(line: &str, mut depth: usize) -> usize {
    let mut previous = ' ';
    for c in line.chars() {
        match c {
            '{' if !REPEATABLE.contains(&previous) => depth += 1,
            '}' if depth > 0 => depth -= 1,
            _ => {}
        }
        previous = c;
    }
    depth
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

    pub(super) fn expanded(source: &str) -> String {
        expand(source)
            .unwrap_or_else(|e| panic!("{}", e.format_with_source(source)))
            .brainfuck()
            .to_string()
    }

    /// Every emitted byte points at a character that could have emitted it.
    ///
    /// This is the whole correctness claim of the origin map, and in this
    /// language it is exact rather than approximate: an instruction is only
    /// ever emitted from a literal instruction character, so the character the
    /// origin names must *be* the character emitted. A repeat count makes the
    /// mapping many-to-one -- all 65 of `+{65}` name the same `+` -- which is
    /// the correct answer, not a rounding of it.
    pub(super) fn assert_origins_are_exact(source: &str) {
        let expansion = expand(source).expect("expands");
        let chars: Vec<char> = source.chars().collect();
        // Which names are macros, read as the expander reads them rather than
        // by searching the file for the text "@macro NAME". `Directive::emits`
        // exists so this helper does not spell the vocabulary out a second
        // time, and a substring search would match any `@foo` whose name
        // happened to appear after "@macro " anywhere in the source.
        let macros: Vec<String> = source
            .lines()
            .filter_map(|line| line.trim_start().strip_prefix("@macro "))
            .map(|rest| {
                rest.trim_start()
                    .chars()
                    .take_while(|c| is_identifier_char(*c))
                    .collect()
            })
            .collect();
        let mut directions: std::collections::HashMap<usize, char> =
            std::collections::HashMap::new();

        for (offset, emitted) in expansion.brainfuck().chars().enumerate() {
            let origin = expansion
                .origin(offset)
                .unwrap_or_else(|| panic!("byte {offset} has no origin"));

            // First, because it holds for every byte however it was emitted.
            // It used to sit at the foot of the loop, past a `continue`, so
            // no byte a macro emitted was checked against it at all.
            let before: String = chars[..origin.offset].iter().collect();
            assert_eq!(
                origin.line,
                before.matches('\n').count() + 1,
                "line disagrees with offset at byte {offset}"
            );
            assert_eq!(
                origin.column,
                before.chars().rev().take_while(|&c| c != '\n').count() + 1,
                "column disagrees with offset at byte {offset}"
            );

            // `@to` and a macro invocation are the two constructs that emit
            // instructions nobody wrote, so their bytes point at the
            // directive. Everything else still points at itself.
            if chars[origin.offset] != '@' {
                assert_eq!(
                    chars[origin.offset], emitted,
                    "byte {offset} ({emitted}) points at {:?} in the macro source",
                    chars[origin.offset]
                );
                continue;
            }

            let spelling: String = chars[origin.offset + 1..]
                .iter()
                .take_while(|c| is_identifier_char(**c))
                .collect();
            if macros.contains(&spelling) {
                // A macro emits whatever its body does, so nothing more can
                // be said about which instruction it was.
                continue;
            }
            assert!(
                Directive::from_spelling(&spelling).is_some_and(Directive::emits),
                "byte {offset} points at @{spelling}, which does not emit"
            );
            assert!(
                emitted == '>' || emitted == '<',
                "@to emitted {emitted:?}, which is not a move"
            );
            // A single `@to` moves one way. Recording the direction per
            // directive occurrence is what makes this exact rather than
            // "points at some @to": a byte misattributed to a different `@to`
            // shows up whenever the two move oppositely, and a run split
            // across directives shows up as a contradiction.
            if let Some(previous) = directions.insert(origin.offset, emitted) {
                assert_eq!(
                    previous, emitted,
                    "the @to at offset {} is credited with both directions",
                    origin.offset
                );
            }
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
        for directive in ["@include \"lib.bfm\"", "@ifdef DEBUG", "@endif"] {
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
        // Rule 3: not knowable until the ']'.
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
        // `@var x` alone is not in this list any more: a cell is chosen for
        // it. What is left is a name missing, a cell missing after `at`, and
        // an `at` missing before one.
        for source in ["@var\n", "@var x 0\n", "@var x at\n"] {
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
        // The shapes the loop rules are about, which used to live in a second
        // test module that could not reach this helper.
        assert_origins_are_exact("@var a at 0\n@var b at 5\n+[>]\n@here b\n+\n");
        assert_origins_are_exact("@var a at 0\n@var b at 2\n+[>]\n+[\n@here a\n@to b\n@to a\n]\n");
    }

    // ---- what the loop rules guarantee ---------------------------------

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
                matches!(error, MacroError::CellTooFar { .. }),
                "{source:?}: got {error:?}"
            );
        }
    }

    #[test]
    fn a_cell_beyond_the_expansion_budget_is_refused_where_it_is_declared() {
        let just_past = EXPANSION_LIMIT + 1;
        let error = expand(&format!("@var far at {just_past}\n")).unwrap_err();
        // A structured variant rather than free text inside a "malformed
        // directive": the syntax was fine, the value was out of range -- and
        // the shared malformed-directive hint, which explains that directives
        // start their line, was advice for a different mistake.
        let MacroError::CellTooFar { cell, limit, .. } = &error else {
            panic!("expected a cell out of range, got {error:?}");
        };
        assert_eq!((*cell, *limit), (just_past as u64, EXPANSION_LIMIT));

        // And the last reachable cell is still fine.
        assert!(expand(&format!("@var edge at {EXPANSION_LIMIT}\n")).is_ok());
    }
}

#[cfg(test)]
mod macro_tests {
    use super::tests::{assert_origins_are_exact, expanded};
    use super::*;

    #[test]
    fn a_macro_without_parameters_expands_its_body() {
        assert_eq!(expanded("@macro clear { [-] }\n@clear\n"), "[-]");
        // And as many times as it is written.
        assert_eq!(expanded("@macro clear { [-] }\n@clear\n@clear\n"), "[-][-]");
    }

    #[test]
    fn a_parameter_is_a_constant_inside_the_body() {
        assert_eq!(expanded("@macro set(v) { [-]+{v} }\n@set(3)\n"), "[-]+++");
    }

    #[test]
    fn several_parameters_bind_in_order() {
        let source = "@macro pair(a, b) { +{a}>+{b} }\n@pair(2, 3)\n";
        assert_eq!(expanded(source), "++>+++");
    }

    #[test]
    fn an_argument_may_be_a_name_from_the_caller() {
        // A number, a constant, and a cell all pass the same way -- a
        // parameter binds whatever the argument already means.
        let source = "@define THREE 3\n@macro set(v) { +{v} }\n@set(THREE)\n";
        assert_eq!(expanded(source), "+++");

        let with_cell = "@var a at 0\n@var b at 2\n@macro go(cell) {\n@to cell\n}\n@go(b)\n";
        assert_eq!(expanded(with_cell), ">>");
    }

    #[test]
    fn a_body_may_span_lines_and_hold_directives() {
        let source = "\
@var counter at 0
@var letter at 1
@macro bump(step) {
    @to letter
    +{step}
    @to counter
    -
}
+{2}[
@bump(3)
]
";
        assert_eq!(expanded(source), "++[>+++<-]");
    }

    #[test]
    fn a_macro_may_use_another() {
        let source = "@macro clear { [-] }\n@macro reset { @clear }\n@reset\n";
        assert_eq!(expanded(source), "[-]");
    }

    #[test]
    fn a_macro_that_uses_itself_is_refused_with_the_chain() {
        let direct = expand("@macro loop_forever { @loop_forever }\n@loop_forever\n").unwrap_err();
        assert!(
            matches!(direct, MacroError::CircularMacro { .. }),
            "{direct:?}"
        );

        let indirect = expand("@macro a { @b }\n@macro b { @a }\n@a\n").unwrap_err();
        let MacroError::CircularMacro { chain, .. } = &indirect else {
            panic!("expected a cycle, got {indirect:?}");
        };
        assert_eq!(chain, &["a".to_string(), "b".to_string(), "a".to_string()]);
    }

    #[test]
    fn the_wrong_number_of_arguments_says_how_many_it_wanted() {
        let error = expand("@macro set(v) { +{v} }\n@set(1, 2)\n").unwrap_err();
        let MacroError::ArgumentCount {
            expected, actual, ..
        } = &error
        else {
            panic!("expected an arity error, got {error:?}");
        };
        assert_eq!((*expected, *actual), (1, 2));

        let none_given = expand("@macro set(v) { +{v} }\n@set\n").unwrap_err();
        assert!(
            matches!(
                none_given,
                MacroError::ArgumentCount {
                    expected: 1,
                    actual: 0,
                    ..
                }
            ),
            "{none_given:?}"
        );
    }

    #[test]
    fn a_name_that_is_no_macro_is_still_an_unknown_directive() {
        assert!(matches!(
            expand("@wibble\n").unwrap_err(),
            MacroError::UnknownDirective { .. }
        ));
    }

    #[test]
    fn a_macro_shares_the_one_namespace() {
        assert!(matches!(
            expand("@define X 1\n@macro X { + }\n").unwrap_err(),
            MacroError::Redefinition { .. }
        ));
        // And using one where a number belongs says which it is.
        let error = expand("@macro X { + }\n+{X}\n").unwrap_err();
        let MacroError::WrongKind { found, .. } = &error else {
            panic!("expected a kind error, got {error:?}");
        };
        assert_eq!(*found, Kind::Macro);
    }

    #[test]
    fn a_declaration_inside_a_body_is_refused_where_it_is_written() {
        // It would run again on the second invocation and collide with
        // itself, so the second call is the wrong place to find out.
        for body in ["@define A 1", "@var a at 0", "@macro inner { + }"] {
            let source = format!("@macro outer {{\n{body}\n}}\n@outer\n");
            let error = expand(&source).unwrap_err();
            assert!(
                matches!(error, MacroError::DeclarationInsideMacro { .. }),
                "{body}: got {error:?}"
            );
        }
    }

    #[test]
    fn a_body_ends_at_its_own_brace_and_not_a_repeat_counts() {
        // `+{3}` is a repeat count; its '}' must not close the body.
        assert_eq!(expanded("@macro three { +{3} }\n@three\n"), "+++");
        // A trailing comment in a one-line body is the natural thing to write
        // -- it is the shape the docs use -- and it must not swallow the
        // brace, which used to report the body as never closed.
        assert_eq!(
            expanded("@macro clear { [-] * clears it }\n@clear\n"),
            "[-]"
        );
        // Which is to say the comment ends at that brace. Anything after it
        // is still on the @macro line, and a directive owns its line.
        let error = expand("@macro two { ++ * a } brace\n@two\n").unwrap_err();
        assert!(
            matches!(error, MacroError::MalformedDirective { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn a_body_that_is_never_closed_says_so() {
        let error = expand("@macro open {\n+++\n").unwrap_err();
        let MacroError::MalformedDirective { detail, .. } = &error else {
            panic!("expected a malformed directive, got {error:?}");
        };
        assert!(detail.contains("never closed"), "{detail}");
    }

    #[test]
    fn bytes_from_a_macro_point_at_the_invocation() {
        // The map holds one position per byte and a macro gives a byte two.
        // The invocation is the line the reader wrote, and a definition used
        // twenty times would not say which of them failed.
        let source = "@macro five { +{5} }\n@five\n";
        let expansion = expand(source).expect("expands");
        assert_eq!(expansion.brainfuck(), "+++++");
        for offset in 0..5 {
            let origin = expansion.origin(offset).expect("an origin");
            assert_eq!(
                (origin.line, origin.column),
                (2, 1),
                "byte {offset} does not point at the invocation"
            );
        }

        // Nested invocations keep the outermost, so no origin points into a
        // body.
        let nested =
            expand("@macro inner { ++ }\n@macro outer { @inner }\n@outer\n").expect("expands");
        for offset in 0..2 {
            assert_eq!(nested.origin(offset).expect("an origin").line, 3);
        }
    }

    #[test]
    fn the_cursor_is_tracked_through_a_body() {
        // A macro is expanded inline, so movement inside one counts towards
        // the loop balance like any other. Nothing special is needed for it.
        let unbalanced = "@var a at 0\n@var b at 1\n@macro step { > }\n+[\n@step\n@to b\n]\n";
        let error = expand(unbalanced).unwrap_err();
        assert!(
            matches!(error, MacroError::MovingInsideUnbalancedLoop { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn origins_survive_macros() {
        assert_origins_are_exact("@macro five { +{5} }\n@five\n@five\n");
        assert_origins_are_exact("@var a at 0\n@var b at 3\n@macro go {\n@to b\n}\n@go\n");
    }

    // ---- limits, scope, and what a body may not do ---------------------

    #[test]
    fn a_deep_chain_of_distinct_macros_is_refused_rather_than_exhausting_the_stack() {
        // Cycles are caught by name, which bounds nothing here: a thousand
        // different macros each using the next is not a cycle, and expansion
        // recurses, so this aborted the process instead of reporting.
        let mut source = String::from("@macro m0 { + }\n");
        for level in 1..=MACRO_DEPTH_LIMIT + 5 {
            source.push_str(&format!("@macro m{level} {{ @m{} }}\n", level - 1));
        }
        source.push_str(&format!("@m{}\n", MACRO_DEPTH_LIMIT + 5));

        let error = expand(&source).unwrap_err();
        assert!(
            matches!(
                error,
                MacroError::MacroTooDeep {
                    limit: MACRO_DEPTH_LIMIT,
                    ..
                }
            ),
            "{error:?}"
        );

        // And a chain inside the limit still expands.
        let mut shallow = String::from("@macro m0 { + }\n");
        for level in 1..=8 {
            shallow.push_str(&format!("@macro m{level} {{ @m{} }}\n", level - 1));
        }
        shallow.push_str("@m8\n");
        assert_eq!(expanded(&shallow), "+");
    }

    #[test]
    fn macros_that_emit_nothing_still_have_a_budget() {
        // The emitted-instruction budget cannot see this: twenty macros each
        // invoking the previous one twice is a million invocations, and no
        // output at all, so nothing was counting.
        let mut source = String::from("@macro m0 { }\n");
        for level in 1..=20 {
            source.push_str(&format!(
                "@macro m{level} {{\n@m{0}\n@m{0}\n}}\n",
                level - 1
            ));
        }
        source.push_str("@m20\n");

        let error = expand(&source).unwrap_err();
        assert!(
            matches!(
                error,
                MacroError::TooManyInvocations {
                    limit: INVOCATION_LIMIT,
                    ..
                }
            ),
            "{error:?}"
        );
    }

    #[test]
    fn a_parameter_wins_over_a_file_level_macro_at_an_invocation_too() {
        // It already won inside a repeat count, so one name meant two
        // different things in one body.
        let source = "@macro clear {\n[-]\n}\n@macro m(clear) {\n+{clear}\n}\n@m(3)\n";
        assert_eq!(expanded(source), "+++");

        // And a macro passed as an argument is invokable through it, which
        // falls out of resolving the name the same way everywhere.
        let higher_order = "@macro two {\n++\n}\n@macro apply(f) {\n@f\n}\n@apply(two)\n";
        assert_eq!(expanded(higher_order), "++");
    }

    #[test]
    fn a_macro_owns_the_rest_of_its_line() {
        // `@macro m { + }+++` used to expand to four instructions: the body,
        // plus three that were written after the brace and silently absorbed.
        let error = expand("@macro m { + }+++\n@m\n").unwrap_err();
        assert!(
            matches!(error, MacroError::MalformedDirective { .. }),
            "{error:?}"
        );
        // A whole invocation vanished the same way, which was worse.
        assert!(expand("@macro m { + } @c\n@m\n").is_err());
    }

    #[test]
    fn a_macro_cannot_be_named_after_a_directive() {
        // Directives are checked first, so such a macro could be defined and
        // then never invoked -- every `@to` read as the directive, failing
        // with a message about `@var` in a program that has none.
        for name in ["to", "here", "define", "var", "macro", "include"] {
            let error = expand(&format!("@macro {name} {{ + }}\n")).unwrap_err();
            assert!(
                matches!(error, MacroError::MalformedDirective { .. }),
                "@macro {name}: got {error:?}"
            );
        }
        // The same rule for the other declarations, so there is one rule.
        assert!(expand("@define to 5\n").is_err());
        assert!(expand("@var here at 0\n").is_err());
    }

    #[test]
    fn a_space_before_the_parameters_is_allowed_as_it_is_at_a_call() {
        // It was legal at the call site and an error about a missing '{' at
        // the definition, because the two lists were parsed by two loops.
        assert_eq!(expanded("@macro m (a) {\n+{a}\n}\n@m(3)\n"), "+++");
        assert_eq!(expanded("@macro m(a) {\n+{a}\n}\n@m (3)\n"), "+++");
    }

    #[test]
    fn a_trailing_comma_is_refused_in_both_lists() {
        assert!(expand("@macro m(a,) {\n+{a}\n}\n").is_err());
        assert!(expand("@macro m(a) {\n+{a}\n}\n@m(1,)\n").is_err());
    }

    #[test]
    fn the_defined_below_hint_does_not_point_inside_a_macro_body() {
        // A declaration in a body is refused outright, so advertising one
        // sent the reader to move their code below something that can never
        // run.
        let error = expand("+{A}\n@macro m {\n@define A 5\n}\n").unwrap_err();
        let hint = error.hint().expect("a hint");
        assert!(
            !hint.contains("defined below"),
            "the hint points at a dead end: {hint}"
        );
    }
}

#[cfg(test)]
mod readable_constants {
    use super::tests::expanded;
    use super::*;

    #[test]
    fn a_var_without_a_cell_is_given_one() {
        // The point of naming cells is to stop counting them, and `at N` still
        // makes you count them once.
        assert_eq!(expanded("@var a\n@var b\n@var c\n@to c\n"), ">>");
    }

    #[test]
    fn a_chosen_cell_never_lands_on_one_already_named() {
        // Lowest free rather than next-after-the-last, so mixing the two
        // spellings leaves no hole.
        assert_eq!(expanded("@var scratch at 9\n@var a\n@var b\n@to b\n"), ">");
        assert_eq!(expanded("@var a\n@var pinned at 1\n@var b\n@to b\n"), ">>");
    }

    #[test]
    fn a_character_is_the_byte_it_stands_for() {
        assert_eq!(expanded("@define A 'A'\n+{A}\n").len(), 65);
        assert_eq!(expanded("+{'A'}\n").len(), 65);
        assert_eq!(expanded("@define NEWLINE '\\n'\n+{NEWLINE}\n").len(), 10);
        assert_eq!(expanded("@define TAB '\\t'\n+{TAB}\n").len(), 9);
        // 'é' is 233, which fits in a cell.
        assert_eq!(expanded("+{'\u{e9}'}\n").len(), 233);
    }

    #[test]
    fn hexadecimal_is_the_number_it_spells() {
        assert_eq!(expanded("@define MASK 0x10\n+{MASK}\n").len(), 16);
        assert_eq!(expanded("+{0xff}\n").len(), 255);
        assert_eq!(expanded("+{0XFF}\n").len(), 255);
    }

    #[test]
    fn a_quote_holds_the_delimiter_the_reader_would_have_stopped_at() {
        // Each of these is the obvious thing to write, and each ends its token
        // on the character the reader would otherwise have stopped at.
        assert_eq!(expanded("@define SPACE ' '\n+{SPACE}\n").len(), 32);
        assert_eq!(expanded("+{'}'}\n").len(), 125);
        assert_eq!(
            expanded("@macro n(c) {\n+{c}\n}\n@n(',')\n").len(),
            usize::from(b',')
        );
    }

    #[test]
    fn a_character_and_a_number_may_be_written_wherever_the_other_may() {
        // They go through one classifier, so nothing had to be told about
        // them twice.
        assert_eq!(expanded("@var a at 0\n@var b at 0x2\n@to b\n"), ">>");
        assert_eq!(
            expanded("@define N 3\n@macro m(c) {\n+{c}\n}\n@m(0xA)\n").len(),
            10
        );
    }

    #[test]
    fn a_malformed_literal_says_what_is_wrong_with_it() {
        for (source, expected) in [
            ("+{'ab'}\n", "more than one character"),
            ("+{''}\n", "it is empty"),
            ("+{'\\q'}\n", "is not an escape"),
            ("+{'a}\n", "no closing quote"),
            ("+{0x}\n", "not a hexadecimal"),
            ("+{0xzz}\n", "not a hexadecimal"),
        ] {
            let error = expand(source).unwrap_err();
            let MacroError::BadRepeatCount { detail, .. } = &error else {
                panic!("{source:?}: expected a bad repeat count, got {error:?}");
            };
            assert!(
                detail.contains(expected),
                "{source:?}: {detail:?} does not mention {expected:?}"
            );
        }
    }

    #[test]
    fn a_character_too_wide_for_a_cell_is_refused() {
        let error = expand("+{'\u{20ac}'}\n").unwrap_err();
        let MacroError::BadRepeatCount { detail, .. } = &error else {
            panic!("expected a bad repeat count, got {error:?}");
        };
        assert!(detail.contains("does not fit in a cell"), "{detail}");
    }

    #[test]
    fn origins_survive_the_new_spellings() {
        super::tests::assert_origins_are_exact("@var a\n@var b\n@to b\n+{'A'}\n");
    }
}
