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
//! - `@stride N` and `@field NAME at N` -- a record shape, and names for the
//!   parts of it. A field is an offset rather than a cell, so `@to` can reach
//!   it wherever the record happens to be, which is what makes an array walked
//!   by scan loops writable in names instead of angle brackets.
//! - `@ifdef NAME` / `@ifndef NAME` ... `@endif` -- expand what is between
//!   them, or skip it. A branch not taken is never expanded, so it may hold
//!   names and brackets that would be errors in one that is. The test is made
//!   where it is reached rather than where it is written, so a body decides it
//!   against the caller's scope, once per invocation.
//! - `@include "lib.bfm"` -- another file's declarations, read here. The path
//!   is relative to the file that wrote it, a file named twice is read once,
//!   and an included file *declares*: it may not emit BrainFuck, nor move the
//!   cursor. That last pair is the rule the source map rests on rather than a
//!   restriction left over from one; [`Scanner::include`] says why.
//! - `@macro NAME(a, b) { ... }`, invoked as `@NAME(1, 2)` -- a body expanded
//!   in place. An argument is evaluated in the caller's scope and bound to the
//!   parameter, so a number, a constant and a cell all pass the same way.
//! - **A body is an argument too.** A `{` after an invocation's arguments
//!   hands the macro a block, as its last argument, and the macro expands it
//!   with `@name`. That is what lets a loop be a macro: `@while(n) { ... }`.
//!   A block carries the scope it was written in, so a block written inside
//!   `@emit(ch)` can say `ch` even though it is expanded inside `@while`.
//!   The `{` goes on the invocation's own line, because a directive owns the
//!   rest of its line and nothing more -- an invocation with the brace on the
//!   next line is an invocation with one argument missing, and is told so.
//! - `@text "..."` -- the instructions that print it, from `gyrus`'s
//!   codegen: about ten a character, where setting a cell from empty costs a
//!   hundred. It empties the cells it walks over and puts the cursor back.
//! - `@repeat N { ... }` -- the body, N times, counted when the program is
//!   built. `OP{N}` repeats one instruction; nothing could repeat a line, and
//!   no macro can, because expansion has no loop of its own.
//! - `OP{N}` -- repeat `OP` N times, where `OP` is one of `+ - < > . ,` and `N`
//!   is a number or a defined name. `+{0}` is nothing, which is legal.
//! - `*` to end of line, and any character that is not a BrainFuck instruction:
//!   comments, dropped from the expansion. Inside a macro body a comment also
//!   ends at a `}` that closes it and ends the line; [`crate::lex::comment`]
//!   has why, and the two shapes that forced it.
//!
//! Wherever a number may be written, so may a character or a hexadecimal
//! number: `'A'`, `'\n'` and `0x41` are all 65. They go through one
//! classifier, so a repeat count, a `@define`, a `@var`'s cell and a macro
//! argument all understand them without any of them being told.
//!
//! # Two rules about what is reserved
//!
//! **A directive must start its line**, after optional blanks, and owns the
//! rest of it. Elsewhere `@` is prose, because BrainFuck comments are
//! free-form and programs in `programs/` already contain one -- `calc.bf` uses
//! it as a marker inside its instruction stream and `pi.bf` carries an email
//! address -- so reserving `@` everywhere would hard-error on converting them.
//! The one exception is an `@` that *spells* a directive and is delimited like
//! one, which is refused rather than read as prose: it would have looked like
//! a directive and silently done nothing. `@@` is a literal `@` anywhere.
//! Owning the line is the other half: an instruction written after a `@define`
//! is refused rather than dropped, because silently discarding code somebody
//! wrote is the one thing a preprocessor must not do.
//!
//! **`{` and `}` are reserved everywhere.** Unlike `@` this costs nothing --
//! no bundled program has either in its prose -- and it buys the error for the
//! likeliest typo of all, a space between an instruction and its count.
//!
//! The vocabulary is now closed: every name in it is built, so there is no
//! directive left that a `.bfm` could mean something different by later.
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
//! [`MACRO_DEPTH_LIMIT`] the nesting -- a cycle is caught as a cycle, but a long
//! enough chain of *different* macros is not, and expansion recurses -- and
//! [`INVOCATION_LIMIT`] the invocations, because macros that emit nothing
//! still cost time and a few doubling wrappers reach billions of them in half
//! a page.
//!
//! # Where the cursor is, and when the expander stops knowing
//!
//! `@to` needs a static cursor position, and BrainFuck loops are where that
//! breaks: at `[` the position is known, but at `]` the cursor is wherever the
//! body left it, after a number of iterations nobody knows. What survives a
//! loop depends on how far its body moves:
//!
//! 1. **Nowhere.** A body that returns the cursor changes nothing. The
//!    position after the loop is the position before it.
//! 2. **A whole number of records.** The *cell* is gone -- a scan stops
//!    wherever the data says -- but which *field* of a record the cursor is on
//!    is exactly what it was, because every iteration moved a whole one. This
//!    is the case `@stride` exists for, and the case nearly all of a large
//!    BrainFuck program is -- `scripts/check-mandelbrot-claims.py` measures
//!    how much of one.
//! 3. **Anywhere else.** Nothing survives. That is not an error by itself,
//!    because such loops are ordinary; the next `@to` is the error, and it
//!    names both itself and the loop that lost the position.
//!
//! A `@to` *inside* a moving body follows from the same three. One naming a
//! cell is wrong in any of them: its first iteration would emit the right
//! movement and every later one the wrong movement, which is the worst way for
//! this to fail. One naming a field is right in case 2 and wrong in case 3.
//! Either way it is reported at the `]`, which is the first point at which how
//! far the body moves is known.
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
//! case rule 3 exists for -- and, naming a field rather than a cell, it is
//! also how a program says it is inside a record at all. After `[<]` the
//! programmer knows where the cursor landed and the expander cannot. It is
//! trusted rather than checked -- the
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

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gyrus::SourceLocation;

use crate::directive::{Declaration, Directive};
use crate::error::{Kind, MacroError, MacroFailure, Wanted};
use crate::lex::{self, REPEATABLE, Step, ValueEnd, is_identifier_char};
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
/// A macro reached from inside itself is caught as a cycle, but a chain of a thousand
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

/// Expand `.bfm` source into BrainFuck, keeping the origin of every byte.
pub fn expand(source: &str) -> Result<Expansion, MacroError> {
    Scanner::new(source).run().map_err(|failure| failure.error)
}

/// Expand source that came from `path`, resolving `@include` against the
/// directory holding it.
///
/// The only entry point where `@include` can work: a path is relative to the
/// file that wrote it, so source handed over as text alone has nothing to
/// resolve against.
///
/// The text is passed in rather than read here on purpose. Every caller
/// already reads files and already says so in its own way when one cannot be
/// read -- and "cannot read the program you asked me to run" is not a macro
/// error, which is what this function's failures are.
///
/// That failure carries the text its caret is drawn against, which is not
/// always this file: an error inside an included file names that file and
/// renders its lines.
pub fn expand_at(source: &str, path: &Path) -> Result<Expansion, MacroFailure> {
    // Canonical, because a path is also a file's identity here: `./main.bfm`
    // and `main.bfm` are one file, and a library that includes the program
    // back has to be told it already has it. Falling back to the path as
    // given, since a file that cannot be canonicalised is one the caller has
    // somehow already read.
    let identity = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    Scanner::rooted(source, Some(identity)).run()
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
    /// An offset within a record, usable as a `@to` target once `@here` has
    /// said the cursor is inside one.
    Field(i64),
    /// A body to expand, usable as `@name(...)`.
    Macro(Rc<MacroDef>),
    /// A body written at a call site and handed to a macro, usable as
    /// `@name`. It carries the bindings that were in scope where it was
    /// written, which is what makes `@while(counter) { @to n - }` inside a
    /// macro able to say `n`.
    Block(Rc<BlockArgument>),
}

/// A block argument: a body, and the scope it was written in.
///
/// An argument is evaluated where it is written, which for a value means
/// looked up there and for a body means *expanded* there. So a block carries a
/// copy of the frame it was written in -- the enclosing macro's parameters and
/// what they were bound to -- and invoking it puts that frame back rather than
/// the frame of whatever macro is holding it.
///
/// Without that, a block passed to `@while` from inside `@invert(n)` could not
/// say `n`: it would be expanded with `@while`'s frame on top, which has no
/// such parameter. That shape -- a loop whose body uses the enclosing macro's
/// argument -- is most of what block parameters are for.
#[derive(Debug)]
struct BlockArgument {
    def: Rc<MacroDef>,
    arguments: Vec<(Symbol, SourceLocation)>,
}

impl Symbol {
    fn kind(&self) -> Kind {
        match self {
            Symbol::Constant(_) => Kind::Constant,
            Symbol::Variable(_) => Kind::Variable,
            Symbol::Field(_) => Kind::Field,
            Symbol::Macro(_) => Kind::Macro,
            Symbol::Block(_) => Kind::Macro,
        }
    }
}

/// Whether two positions are the same place, ignoring how each came to be
/// known. Used to decide whether a `@here` inside a loop body leaves the exit
/// ambiguous, where only the place matters.
fn same_place(a: Position, b: Position) -> bool {
    match (a, b) {
        (Position::Known(x), Position::Known(y)) => x == y,
        (Position::Relative { offset: x, .. }, Position::Relative { offset: y, .. }) => x == y,
        _ => false,
    }
}

/// What a `@to` was told to move to.
#[derive(Debug, Clone, Copy)]
enum Target {
    /// A cell of the tape, from `@var`.
    Cell(i64),
    /// An offset within a record, from `@field`.
    Field(i64),
}

/// Where the cursor is during expansion, when the expander can still say.
#[derive(Debug, Clone, Copy)]
enum Position {
    Known(i64),
    /// Which cell of the tape is unknown; which field of a record is not.
    ///
    /// This is what a scan leaves behind. `[>{STRIDE}]` stops wherever the
    /// data says, so the cell cannot be known -- but it moved by a whole
    /// record each time, so the *offset within* a record is exactly what it
    /// was. That is the position an array walked by scan loops is always in,
    /// which is most of what a large BrainFuck program does.
    Relative {
        offset: i64,
        /// Where the cursor came to be in a record: a `@here` naming a
        /// field, or the loop that walked whole records.
        entered: SourceLocation,
    },
    /// Lost by a loop whose body did not return the cursor where it found it.
    /// Carries that loop's `[`, so an error can name the cause and not only
    /// the symptom.
    Unknown(SourceLocation),
}

/// How deep `@include` may nest.
///
/// Small on purpose. A chain this long is a mistake rather than a design, and
/// the scan recurses, so the alternative to a limit is a stack overflow.
pub const INCLUDE_DEPTH_LIMIT: usize = 32;

/// One file's text, and the span of the shared buffer holding it.
///
/// Included files are *appended* to that buffer rather than scanned in a
/// nested pass, because a macro body is a span of it: `MacroDef` holds
/// offsets, so a macro defined in an included file has to live in the same
/// buffer as one defined beside its invocation, or `invoke` would need to know
/// which text it is reading. Appending means it does not.
///
/// A location therefore carries a *global* offset and a *file-local* line and
/// column, which is what lets an error name `lib.bfm` line 3 while the offset
/// still indexes one buffer.
struct SourceFile {
    /// Where it was read from, or `None` for source handed over as text.
    ///
    /// Canonical, because this is also the file's *identity*: including it
    /// twice by two different relative paths is one file, and has to be.
    path: Option<PathBuf>,
    /// The path as the `@include` wrote it, joined to the directory it was
    /// written in. What a message should say -- `lib/text.bfm` is the file the
    /// reader has open, and the canonical form is the same file spelled in a
    /// way they did not choose.
    named: Option<PathBuf>,
    text: String,
    start: usize,
    end: usize,
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
    /// Conditionals this body opened and has not closed. Popping the frame
    /// pops these with it, so a body cannot close its caller's and cannot
    /// leave one open without the pop noticing. An empty `Vec` allocates
    /// nothing, which matters on a path taken once per invocation.
    conditionals: Vec<(Directive, SourceLocation)>,
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
    /// Where the cursor was when the body was entered, for deciding whether a
    /// `@here` inside it leaves the exit ambiguous.
    entry_position: Position,
    /// Whether a `@here` inside this body said where the cursor is. A loop may
    /// run zero times, so such a claim holds only if the body also leaves the
    /// cursor where it found it.
    here_inside: bool,
    /// Whether the emitted `>` and `<` in this body are what actually happens.
    ///
    /// They are not, once a nested loop moves by something other than whole
    /// records: that loop runs a number of times nobody knows, so its
    /// contribution is unknown, while the emitted count sees its body once.
    /// Without this the rule below reads `[ @to a @to m [>] >{2} ]` as moving
    /// three cells and lets the `@to` through -- right on the first iteration
    /// and wrong on every one after, which is what it exists to prevent.
    movement_certain: bool,
    /// The first `@to` inside this body that named a cell. In a body that
    /// moves at all, such a `@to` is wrong from the second iteration.
    cell_to_inside: Option<SourceLocation>,
    /// The first `@to` inside this body that named a field. This one survives
    /// a body that moves by whole records, because the offset it is relative
    /// to is the same on every iteration.
    field_to_inside: Option<SourceLocation>,
}

struct Scanner {
    chars: Vec<char>,
    /// The root file, then every file included so far, in buffer order. Never
    /// empty: the source being expanded is the first entry.
    files: Vec<SourceFile>,
    /// What each `@text` compiled to, kept because a `@text` in a macro body
    /// is compiled once per invocation otherwise, and the answer cannot
    /// change.
    compiled: HashMap<String, Rc<str>>,
    /// How many `@include` scans are in flight. An included file declares and
    /// does not emit, so this is also "is emitting an error right now".
    including: usize,
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
    /// Conditionals opened at file scope whose `@endif` has not been reached.
    /// A branch that is *not* taken never gets here: it is skipped whole,
    /// `@endif` and all. A macro body's own are on its frame, which is what
    /// makes "an `@endif` in a body closes one the body opened" structural.
    conditionals: Vec<(Directive, SourceLocation)>,
    /// The size of a record, if the file declared one. What it buys is the
    /// rule that a loop moving by a whole number of records leaves the offset
    /// within a record untouched -- without it every scan would lose the
    /// position and need a `@here` after it.
    stride: Option<(i64, SourceLocation)>,
    /// Cells the *expander* picked, as opposed to cells somebody wrote a
    /// number for. Which cells are taken is derivable from `symbols` and is
    /// derived; which of them nobody chose deliberately is not.
    chosen: BTreeSet<i64>,
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
        Self::rooted(source, None)
    }

    /// The same, for source read from a file: `@include` resolves against the
    /// directory holding it, so a library names its neighbours the way the
    /// file that includes them does.
    fn rooted(source: &str, path: Option<PathBuf>) -> Self {
        let chars: Vec<char> = source.chars().collect();
        Self {
            compiled: HashMap::new(),
            files: vec![SourceFile {
                named: path.clone(),
                path,
                text: source.to_string(),
                start: 0,
                end: chars.len(),
            }],
            including: 0,
            chars,
            at: SourceLocation::start(),
            symbols: HashMap::new(),
            frames: Vec::new(),
            invocations: 0,
            conditionals: Vec::new(),
            stride: None,
            chosen: BTreeSet::new(),
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

    /// Expand the root file, and say which file an error points into.
    ///
    /// The two halves are separate because the answer to "which file" needs
    /// the scanner, and `?` would have dropped it.
    fn run(mut self) -> Result<Expansion, MacroFailure> {
        match self.expand_root() {
            Ok(expansion) => Ok(expansion),
            Err(error) => {
                let file = self.file_at(error.location().offset);
                Err(MacroFailure::new(
                    error,
                    // Named only when it is not the file the caller handed
                    // over: saying that one back to them adds nothing.
                    if file.start > 0 {
                        file.named.clone()
                    } else {
                        None
                    },
                    file.text.clone(),
                ))
            }
        }
    }

    fn expand_root(&mut self) -> Result<Expansion, MacroError> {
        let root_end = self.files[0].end;
        self.scan_until(root_end)?;

        if let Some(open) = self.open_brackets.first() {
            return Err(MacroError::UnmatchedOpenBracket {
                location: open.location,
            });
        }
        if let Some(&(directive, location)) = self.conditionals.first() {
            return Err(unclosed(directive, location));
        }

        // The map holds one position per emitted byte against one text, and
        // `include` is what keeps every position inside it. Checked rather
        // than trusted, because the failure is a caret pointing confidently at
        // the wrong line -- and only when a file was included, so a program
        // without one pays nothing.
        assert!(
            self.files.len() == 1 || self.origins.iter().all(|origin| origin.offset <= root_end),
            "an included file emitted, so the source map names a file it cannot show"
        );

        Ok(Expansion::new(
            // The root file's text, which is the one an origin can point
            // into: `include` is what keeps that true.
            std::mem::take(&mut self.files[0].text),
            std::mem::take(&mut self.out),
            std::mem::take(&mut self.origins),
        ))
    }

    /// Where the current scan stops: the end of the macro body being
    /// expanded, or of the file. Skipping a false branch needs a bound, and a
    /// conditional that ran past this one would be looking for its `@endif` in
    /// somebody else's text.
    fn scan_end(&self) -> usize {
        self.frames
            .last()
            .map_or_else(|| self.file().end, |frame| frame.def.body_end)
    }

    /// The file the cursor is in.
    ///
    /// Derived rather than tracked: the spans are disjoint and in order, so
    /// the offset says which file it is, and there is no field to keep in step
    /// with the cursor. Asked once per skip and once per hint, never per
    /// character.
    fn file(&self) -> &SourceFile {
        self.file_at(self.at.offset)
    }

    fn file_at(&self, offset: usize) -> &SourceFile {
        // `<=`, not `<`: an error may point one past the last character of a
        // file, and that position is still that file's. The separator between
        // files is what makes this unambiguous -- without it, one past the end
        // of a file is also the first character of the next.
        self.files
            .iter()
            .rev()
            .find(|file| offset >= file.start && offset <= file.end)
            .unwrap_or(&self.files[0])
    }

    /// The conditionals of the scope being expanded: the innermost macro
    /// body's, or the file's.
    fn conditionals(&mut self) -> &mut Vec<(Directive, SourceLocation)> {
        match self.frames.last_mut() {
            Some(frame) => &mut frame.conditionals,
            None => &mut self.conditionals,
        }
    }

    /// Expand characters up to `end`.
    ///
    /// Bounded rather than "to the end of the input" because a macro body is
    /// a span of this same source; see this module's documentation. `end` is
    /// always [`Scanner::scan_end`] -- passed rather than fetched because it
    /// is the loop's bound and does not change under it.
    fn scan_until(&mut self, end: usize) -> Result<(), MacroError> {
        while self.at.offset < end {
            let Some(c) = self.peek() else { break };
            match c {
                '*' => self.skip_comment(),
                // Before either `@` arm, so a literal one is literal wherever
                // it is written. `lex::on_directive_line` knows this too, or
                // the readers that skip unexpanded text would end a body in a
                // different place than the expander does.
                '@' if self.chars.get(self.at.offset + 1) == Some(&'@') => {
                    self.bump();
                    self.bump();
                }
                '@' if lex::at_line_start(&self.chars, self.at.offset, self.boundary()) => {
                    self.directive()?
                }
                '@' => self.stray_at()?,
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

    /// An `@` that is not first on its line, and so is not a directive.
    ///
    /// Nearly all of them are prose and stay prose: `@` is an ordinary
    /// character in BrainFuck, `calc.bf` uses one as a marker inside its
    /// instruction stream, and `pi.bf` carries its author's email address.
    /// Reserving the character outright would hard-error on converting any of
    /// them, which is the reason [`prose_may_contain_an_at_sign`] gives and
    /// still gives.
    ///
    /// What is refused is the one shape that is never prose: an `@` that
    /// spells a directive. `[ @to b + ]` used to expand to `[+]` -- an endless
    /// loop where a move was written -- and say nothing, which was the only
    /// place this language failed silently. Thirteen words are enough to tell
    /// that apart from an email address.
    ///
    /// A macro's name is deliberately not in that set. The thirteen are fixed
    /// and known before a file is read, so what counts as prose does not
    /// depend on what happens to be defined above it.
    ///
    /// `@@` is a literal `@` written on purpose. It emits nothing, because a
    /// literal `@` is a comment character like any other, and it is rarely
    /// needed -- prose that is only prose already passes untouched.
    fn stray_at(&mut self) -> Result<(), MacroError> {
        let (word, end) = lex::spelling(&self.chars, self.at.offset);
        // A directive is the whole word and then a space, a newline, or the
        // end of the file. Without that last condition `bob@here.org` is an
        // error, which is the sort of prose this rule exists to leave alone --
        // and `pi.bf` keeps its author's email address only because his
        // provider is not called `to`.
        let delimited = matches!(self.chars.get(end), None | Some(' ' | '\t' | '\n' | '\r'));
        if delimited && let Some(directive) = Directive::from_word(word) {
            return Err(MacroError::StrayAt {
                directive: directive.spelling().to_string(),
                location: self.at,
            });
        }
        self.bump();
        Ok(())
    }

    // ---- character-level helpers -------------------------------------------

    fn peek(&self) -> Option<char> {
        self.chars.get(self.at.offset).copied()
    }

    /// Advance to `offset`, which is on this line.
    ///
    /// Every lexical rule but the single-character step measures a span that
    /// stops at the newline, so counting newlines in it is a pass that always
    /// returns zero -- and it is a pass over a comment that a macro body is
    /// re-read for on every invocation, which measured 29% on a body with a
    /// long one.
    fn advance_on_line(&mut self, offset: usize) {
        debug_assert!(
            !self.chars[self.at.offset..offset.min(self.chars.len())].contains(&'\n'),
            "advance_on_line crossed a newline"
        );
        let offset = offset.min(self.chars.len());
        if offset > self.at.offset {
            self.at.column += offset - self.at.offset;
            self.at.offset = offset;
        }
    }

    /// Advance to `offset`, keeping the line and column right wherever it is.
    ///
    /// The lexical rules measure a span; this crosses it. Bumping through it
    /// character by character walked every comment and literal twice, once to
    /// measure and once to move.
    fn advance_to(&mut self, offset: usize) {
        let offset = offset.min(self.chars.len());
        if offset <= self.at.offset {
            return;
        }
        let span = &self.chars[self.at.offset..offset];
        let newlines = span.iter().filter(|c| **c == '\n').count();
        if newlines == 0 {
            self.at.column += span.len();
        } else {
            self.at.line += newlines;
            self.at.column = span.iter().rev().take_while(|c| **c != '\n').count() + 1;
        }
        self.at.offset = offset;
    }

    /// The offset the current scan began at: the innermost body's first
    /// character, or the current file's. Both are line starts as far as a
    /// directive is concerned, which is why an `@include`d file may open with
    /// one even though the character before it is another file's last.
    fn boundary(&self) -> usize {
        self.frames
            .last()
            .map_or_else(|| self.file().start, |frame| frame.boundary)
    }

    /// Whether a macro body is being expanded.
    fn inside_a_macro(&self) -> bool {
        !self.frames.is_empty()
    }

    /// One character of [`Self::advance_to`], kept separate because it is the
    /// scan loop's per-character step and needs none of the span arithmetic.
    /// The line and column rule is the same one.
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

    /// Past a `*` comment at the cursor, by the rule the body reader uses --
    /// which is the argument the two of them have to agree about.
    fn skip_comment(&mut self) {
        let end = lex::comment(&self.chars, self.at.offset, self.inside_a_macro());
        self.advance_on_line(end);
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
        lex::matches(&self.chars[start..self.at.offset], want)
    }

    /// A token at the cursor, ending at the first character `ends` accepts
    /// that is not inside a character literal.
    ///
    /// The quotes are why this is shared rather than written at each of the
    /// three places that read a value. `@define SPACE ' '` ends its token on a
    /// space, `@text(',')` on a comma and `+{'}'}` on a brace -- each of them
    /// the delimiter that reader would otherwise stop at, and each of them the
    /// obvious thing to write.
    fn token(&mut self, ends: impl Fn(char) -> bool) -> String {
        let start = self.at.offset;
        let (end, _) = lex::value(&self.chars, start, ends);
        self.advance_on_line(end);
        self.chars[start..end].iter().collect()
    }

    /// An identifier at the cursor: a letter or `_`, then letters, digits, `_`.
    /// Empty if the cursor is not on one.
    fn identifier(&mut self) -> String {
        let mut name = String::new();
        match self.peek() {
            Some(c) if lex::is_identifier_start(c) => {}
            _ => return name,
        }
        while let Some(c) = self.peek() {
            if lex::is_identifier_char(c) {
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
        let count = if lex::is_repeat_count(&self.chars, self.at.offset) {
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
        self.position = match self.position {
            Position::Known(at) => Position::Known(at.saturating_add(delta)),
            Position::Relative { offset, entered } => Position::Relative {
                // Reduced within the record, because the offset says *which
                // field*, not how far the cursor has travelled. Moving one
                // whole record forward from field 0 is field 0 again, and it
                // is that fact the scan rule depends on.
                //
                // A relative position implies a stride: it can only come from
                // a field, only `@field` makes one, and that refuses to run
                // before `@stride`, which refuses a size of zero. Saying so
                // with `expect` states the invariant; a fallback arm would
                // hide it and leave an offset outside the record if it ever
                // broke.
                offset: offset
                    .saturating_add(delta)
                    .rem_euclid(self.stride.expect("a relative position implies a stride").0),
                entered,
            },
            unknown => unknown,
        };
    }

    /// Emit `count` copies of an instruction, within the file's budget.
    ///
    /// The one place instructions are emitted in bulk, so the limit check and
    /// the movement accounting cannot be applied to some emitters and not
    /// others.
    fn emit_run(&mut self, c: char, count: u64, origin: SourceLocation) -> Result<(), MacroError> {
        // The one place output is produced, so the one place the rule has to
        // be stated: an included file declares and does not emit. Checked
        // before the override below, so the error names the instruction rather
        // than whatever invoked the macro holding it.
        if self.including > 0 {
            return Err(MacroError::IncludedFileEmits {
                what: format!("'{c}'"),
                location: origin,
            });
        }
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
        self.bracket_at(c, origin)
    }

    /// The same, for a bracket that is not in the source: everything about a
    /// loop except reading it. `@text` emits code nobody typed, and its
    /// multiplication loops have to open and close on the same stack as the
    /// ones somebody did type, or the balance rules would not see them.
    fn bracket_at(&mut self, c: char, origin: SourceLocation) -> Result<(), MacroError> {
        if c == '[' {
            self.open_brackets.push(OpenLoop {
                location: origin,
                net_at_entry: self.net,
                entry_position: self.position,
                here_inside: false,
                movement_certain: true,
                cell_to_inside: None,
                field_to_inside: None,
            });
            return self.emit_run(c, 1, origin);
        }

        let Some(open) = self.open_brackets.pop() else {
            return Err(MacroError::UnmatchedCloseBracket { location: origin });
        };
        self.emit_run(c, 1, origin)?;

        let stride = self.stride.map(|(size, _)| size);
        // How far the body moved the cursor, and whether that number means
        // anything.
        //
        // Usually it is the emitted movement, and `movement_certain` is the
        // difference between what was emitted and what happens: a nested loop
        // runs an unknown number of times, so only one that moves by whole
        // records (or not at all) leaves the count meaningful.
        //
        // But when the body both starts and ends at a *known* cell, the
        // movement is the difference between them whatever happened in
        // between -- that is what knowing a position means. This is the case a
        // `@here` after a scan creates, and it is the only way to use one of
        // the catalogue's pointer-walking idioms inside a loop: the snippet
        // makes the emitted count meaningless, and the `@here` after it makes
        // the count beside the point. A position is only ever unknown or
        // trusted, so there is nothing weaker being relied on here than
        // `@here` is already relied on for.
        let (moved, certain) = match (open.entry_position, self.position) {
            (Position::Known(entry), Position::Known(exit)) => (exit - entry, true),
            _ => (self.net - open.net_at_entry, open.movement_certain),
        };
        let balanced = certain && moved == 0;
        let whole_records = certain && stride.is_some_and(|size| moved % size == 0);

        // What this loop contributes to an enclosing body's count. Whole
        // records k times is still whole records; anything else is unknown,
        // and every loop still open has to stop trusting its own total.
        if !(balanced || whole_records) {
            for parent in &mut self.open_brackets {
                parent.movement_certain = false;
            }
        }

        // Rule 3 in this module's documentation: not knowable before this
        // bracket, which is why it is reported here. A `@to` naming a cell is
        // wrong in any body that moves; one naming a field survives a body
        // that moves by whole records, because the offset it is relative to is
        // the same on every iteration.
        let wrong = match (open.cell_to_inside, open.field_to_inside) {
            (Some(to), _) if !balanced => Some(to),
            (_, Some(to)) if !whole_records => Some(to),
            _ => None,
        };
        if let Some(to) = wrong {
            return Err(MacroError::MovingInsideUnbalancedLoop {
                location: to,
                loop_at: open.location,
            });
        }

        // A `@here` inside the body says where the cursor is *if the body
        // ran*. A loop may run zero times, so the claim holds only where the
        // body also leaves the cursor where it found it.
        let ambiguous = open.here_inside && !same_place(self.position, open.entry_position);

        self.position = match self.position {
            _ if ambiguous => Position::Unknown(open.location),
            here if balanced => here,
            // Rule 2: the cell is gone, the offset within a record is not.
            // From a known cell that offset is arithmetic; from a known offset
            // it is unchanged.
            Position::Known(cell) if whole_records => Position::Relative {
                offset: cell.rem_euclid(stride.expect("whole records needs a stride")),
                entered: open.location,
            },
            relative @ Position::Relative { .. } if whole_records => relative,
            // Only the loop that *first* lost the position is worth naming:
            // re-tagging on every later one would point at a symptom.
            Position::Known(_) | Position::Relative { .. } => Position::Unknown(open.location),
            unknown => unknown,
        };
        Ok(())
    }

    /// A `{...}` repeat count at the cursor, resolved to a number.
    fn repeat_count(&mut self) -> Result<u64, MacroError> {
        let open = self.at;
        let (text_end, end, ending) = lex::repeat_count(&self.chars, open.offset);
        let body: String = self.chars[open.offset + 1..text_end].iter().collect();
        self.advance_on_line(end);
        let body = body.trim();

        let bad = |detail: String| MacroError::BadRepeatCount {
            detail,
            location: open,
        };
        match ending {
            ValueEnd::Delimiter => {}
            // Which of the two is missing, asked of the reader that knows.
            ValueEnd::OpenLiteral => return Err(bad(format!("{body} has no closing quote"))),
            ValueEnd::Unclosed => return Err(bad("no closing '}'".to_string())),
        }

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
                Wanted::Constant,
                location,
                declared,
            )),
        }
    }

    /// A name used where a `@to` target is wanted: a cell, or an offset.
    fn resolve_target(&self, name: &str, location: SourceLocation) -> Result<Target, MacroError> {
        match self.lookup(name, location)? {
            (Symbol::Variable(cell), _) => Ok(Target::Cell(cell)),
            (Symbol::Field(offset), _) => Ok(Target::Field(offset)),
            (other, declared) => Err(MacroError::wrong_kind(
                name,
                other.kind(),
                Wanted::Target,
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
        // The same walk the body reader makes, differing only in what it does
        // with a brace and when it stops. Scanning line by line instead did
        // not know comments existed: a `{` in prose opened a body that never
        // closed, and every declaration below it stopped being advertised.
        // Bounded by the file rather than by the buffer: everything after
        // this file's text is another file, appended by an `@include`, and
        // "defined below this line" is advice about *this* one.
        let (start, end) = (self.file().start, self.file().end);
        let mut at = self.at.offset.min(end);
        let mut depth = 0usize;
        // Nor one inside a conditional, in either direction. Whether that
        // branch is taken depends on what is defined where it is reached,
        // which this scan is in no position to know, and "move your code below
        // a line that may never be expanded" is worse advice than none. A hint
        // is optional; a wrong one is not a lesser version of a right one.
        let mut conditionals = 0usize;
        while at < end {
            // Only a declaration that could take effect. One inside a macro
            // body is refused outright, so advertising it would send the
            // reader to move their code below something that never runs.
            if depth == 0 && self.chars[at] == '@' && lex::at_line_start(&self.chars, at, start) {
                let (word, after) = lex::spelling(&self.chars, at);
                match Directive::from_word(word) {
                    Some(d) if d.conditional().is_some() => conditionals += 1,
                    Some(Directive::Endif) => conditionals = conditionals.saturating_sub(1),
                    Some(d)
                        if conditionals == 0
                            && d.declaration().is_some()
                            && names(&self.chars, after, name) =>
                    {
                        return true;
                    }
                    _ => {}
                }
            }
            at = match lex::step(&self.chars, at, start, depth > 0) {
                Step::Past(next) => next,
                Step::Open => {
                    depth += 1;
                    at + 1
                }
                Step::Close => {
                    depth = depth.saturating_sub(1);
                    at + 1
                }
            };
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
            Some(Directive::Here) => self.here(location),
            Some(Directive::Stride) => self.stride(location),
            Some(Directive::Field) => self.field(location),
            Some(Directive::Macro) => self.macro_definition(location),
            Some(directive @ (Directive::Ifdef | Directive::Ifndef)) => {
                self.conditional(directive, location)
            }
            Some(Directive::Endif) => self.endif(location),
            Some(Directive::Include) => self.include(location),
            Some(Directive::Repeat) => self.repeat(location),
            Some(Directive::Text) => self.text(location),
            // Not a directive, so it may be a macro -- checked after the
            // directives, so no macro can shadow one, and a name that is
            // neither is reported as what it looks like.
            None => match self.binding(&name).map(|(symbol, _)| symbol.clone()) {
                Some(Symbol::Macro(def)) => self.invoke(name, def, location),
                Some(Symbol::Block(block)) => self.invoke_block(name, block, location),
                _ => Err(MacroError::UnknownDirective { name, location }),
            },
        }
    }

    /// The argument bound to `name`, if the body being expanded takes it as a
    /// parameter.
    ///
    /// Only the innermost frame's parameters are visible, not a chain: a body
    /// sees its own parameters and the file's names, never its caller's, which
    /// is what makes a macro readable in isolation.
    fn parameter(&self, name: &str) -> Option<&(Symbol, SourceLocation)> {
        self.frames.last().and_then(|frame| {
            let index = frame.def.params.iter().position(|param| param == name)?;
            frame.arguments.get(index)
        })
    }

    /// Whether `name` is a parameter here at all -- which `binding` cannot
    /// say, because it answers with the argument and an argument may be
    /// anything a name outside could have been.
    fn is_parameter(&self, name: &str) -> bool {
        self.parameter(name).is_some()
    }

    /// What a name is bound to here: the innermost body's parameters first,
    /// then the file's names.
    ///
    /// One copy of the precedence rule. Writing it twice is what made a
    /// parameter honoured inside a body by `+{name}` and ignored by `@name`,
    /// so one name meant two things in one body -- and the first fix for that
    /// was to copy the rule again rather than share it.
    fn binding(&self, name: &str) -> Option<&(Symbol, SourceLocation)> {
        self.parameter(name).or_else(|| self.symbols.get(name))
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

    /// `@ifdef NAME` and `@ifndef NAME` -- expand what follows, or skip it.
    ///
    /// "Defined" is any name in scope where the conditional is *expanded*: a
    /// constant, a cell, a field, or a macro. That is what makes this per
    /// invocation rather than once -- a body's conditional is decided at the
    /// call site, so the same macro can expand two ways in one file if a
    /// `@define` falls between the two calls.
    ///
    /// A parameter is refused rather than answered. Every parameter is bound
    /// on every invocation, since the arity has to match, so `@ifdef` on one
    /// is always true and `@ifndef` on one is a branch that cannot be reached
    /// -- and "was I given this?" is the reading it invites.
    fn conditional(
        &mut self,
        directive: Directive,
        location: SourceLocation,
    ) -> Result<(), MacroError> {
        let (name, at_name) = self.named_operand(directive, "expected a name to test")?;
        self.end_of_directive(directive)?;
        if self.is_parameter(&name) {
            return Err(MacroError::ParameterAlwaysDefined {
                name,
                location: at_name,
            });
        }
        // `conditional()` says which answer takes the branch, so comparing it
        // to what is defined is the whole rule, and there is no arm left over
        // for a directive that is not one of the two.
        if directive.conditional() == Some(self.binding(&name).is_some()) {
            self.conditionals().push((directive, location));
            return Ok(());
        }
        self.skip_branch(directive, location)
    }

    /// `@endif` -- close the conditional whose branch was taken.
    fn endif(&mut self, location: SourceLocation) -> Result<(), MacroError> {
        self.end_of_directive(Directive::Endif)?;
        // Whatever is open in *this* scope, which is all a pop can reach.
        if self.conditionals().pop().is_none() {
            return Err(MacroError::UnmatchedEndif { location });
        }
        Ok(())
    }

    /// Walk to the `@endif` that closes a branch not taken, and past it.
    ///
    /// A fourth reader of this language, and the first that did not have to be
    /// written as one: it steps with [`lex::step`], so it cannot disagree with
    /// the expander about where a comment ends, how far a literal reaches, or
    /// whether a `{` is a repeat count. Nothing here is expanded, so a skipped
    /// branch may hold an unbalanced bracket, a cell nobody declared, or a
    /// name that does not exist -- which is most of what a conditional is for.
    fn skip_branch(
        &mut self,
        directive: Directive,
        location: SourceLocation,
    ) -> Result<(), MacroError> {
        // Hoisted for the reason `scan_until` takes its bound as a parameter:
        // the frame stack cannot change under this loop, so both are read once
        // rather than per character.
        let (boundary, end, in_macro) = (self.boundary(), self.scan_end(), self.inside_a_macro());
        let mut open = 1usize;
        let mut braces = 0usize;
        while self.at.offset < end {
            let at = self.at.offset;
            // At brace depth zero only. A macro body inside the branch is text
            // this walk passes over whole, exactly as the expander would: its
            // contents are not read until it is invoked, so an `@endif` in one
            // closes nothing here. Without the depth, wrapping a definition in
            // a conditional changed what the definition meant.
            if braces == 0 && self.chars[at] == '@' && lex::at_line_start(&self.chars, at, boundary)
            {
                let (word, after) = lex::spelling(&self.chars, at);
                match Directive::from_word(word) {
                    Some(nested) if nested.conditional().is_some() => open += 1,
                    Some(Directive::Endif) => {
                        open -= 1;
                        if open == 0 {
                            self.advance_on_line(after);
                            return self.end_of_directive(Directive::Endif);
                        }
                    }
                    _ => {}
                }
            }
            match lex::step(&self.chars, at, boundary, in_macro || braces > 0) {
                Step::Past(next) => self.advance_to(next),
                Step::Open => {
                    braces += 1;
                    self.bump();
                }
                Step::Close => {
                    braces = braces.saturating_sub(1);
                    self.bump();
                }
            }
        }
        Err(unclosed(directive, location))
    }

    /// `@include "lib.bfm"` -- read another file's declarations here.
    ///
    /// An included file *declares*: `@define`, `@var`, `@field`, `@stride` and
    /// `@macro`. It may not emit BrainFuck, and it may not move the cursor --
    /// `@here` is the one way to do the second without doing the first. That
    /// is the rule the whole design rests on rather than a restriction left
    /// over from one.
    ///
    /// The map holds one position per emitted byte, against one text, and a
    /// second file cannot be written in it -- so either an instruction from a
    /// library reports a line of the file that included it, or it reports a
    /// line number belonging to a file the reader is not looking at. Both are
    /// the thing this crate exists to prevent. Refusing to emit is the third
    /// option, and it costs a library nothing: a macro is how you ship
    /// instructions, and its bytes name the invocation, which is a line of the
    /// program somebody wrote.
    ///
    /// A file is included once. Including it again is not an error and not a
    /// second copy -- it is nothing, which is what makes a diamond work
    /// without every library carrying a guard, and what makes a cycle
    /// terminate instead of needing to be detected.
    fn include(&mut self, location: SourceLocation) -> Result<(), MacroError> {
        self.refuse_inside_a_macro(Directive::Include, location)?;
        let named = self.quoted_operand(
            Directive::Include.spelling(),
            "path",
            "expected a quoted path, as in `@include \"lib.bfm\"`",
            location,
        )?;
        if named.is_empty() {
            return Err(malformed(
                Directive::Include.spelling(),
                "the path is empty".to_string(),
                location,
            ));
        }
        self.end_of_directive(Directive::Include)?;

        // Relative to the file that wrote the `@include`, not to the process's
        // working directory: a library names its neighbours the way the file
        // beside it does, so where a program is run from cannot change what it
        // means.
        let Some(directory) = self
            .file()
            .path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
        else {
            return Err(MacroError::IncludeWithoutAFile { location });
        };
        let named_path = directory.join(&named);
        let path = named_path
            .canonicalize()
            .map_err(|source| MacroError::IncludeUnreadable {
                path: named_path.clone(),
                detail: source.to_string(),
                location,
            })?;
        if self
            .files
            .iter()
            .any(|file| file.path.as_deref() == Some(&*path))
        {
            return Ok(());
        }
        if self.including >= INCLUDE_DEPTH_LIMIT {
            return Err(MacroError::IncludeTooDeep {
                limit: INCLUDE_DEPTH_LIMIT,
                location,
            });
        }
        let text =
            std::fs::read_to_string(&path).map_err(|source| MacroError::IncludeUnreadable {
                path: named_path.clone(),
                detail: source.to_string(),
                location,
            })?;

        // A newline between files, belonging to neither. Two things need it.
        // A reader that runs to the end of a line -- a value, a comment, a
        // literal, a path -- would otherwise run to the end of the *buffer*,
        // so a file with no final newline would have its last token joined to
        // the next file's first: `@define M 5` before a library read as
        // `'5@define'`. And it keeps the spans apart, so an offset one past
        // the end of a file is not also the first character of the next one,
        // which is the difference between naming the right file in an error
        // and naming the one after it.
        self.chars.push('\n');
        let start = self.chars.len();
        self.chars.extend(text.chars());
        let end = self.chars.len();
        self.files.push(SourceFile {
            path: Some(path),
            named: Some(named_path),
            text,
            start,
            end,
        });

        // Line one, column one, of a buffer position far from the start: the
        // offset is where the text is, and the line and column are where the
        // reader will look for it.
        let resume = std::mem::replace(&mut self.at, SourceLocation::new(1, 1, start));
        // Its own conditionals, for the reason a macro body has its own: an
        // `@endif` in a library is not the includer's to close, and one the
        // library leaves open is not the includer's to close either. Without
        // this a stray `@endif` in a library silently ended a conditional in
        // the program, which then reported *its* `@endif` as unmatched.
        let outer = std::mem::take(&mut self.conditionals);
        self.including += 1;
        let result = self.scan_until(end);
        self.including -= 1;
        let left_open = self.conditionals.first().copied();
        self.conditionals = outer;
        self.at = resume;

        result?;
        if let Some((directive, location)) = left_open {
            return Err(unclosed(directive, location));
        }
        Ok(())
    }

    /// The `"path"` an `@include` names.
    fn quoted_operand(
        &mut self,
        name: &str,
        noun: &str,
        expected: &str,
        location: SourceLocation,
    ) -> Result<String, MacroError> {
        self.skip_blanks();
        let opens = self.at.offset;
        if self.peek() != Some('"') {
            return Err(malformed(name, expected.to_string(), self.at));
        }
        let (end, closed) = lex::quoted(&self.chars, opens);
        if !closed {
            return Err(malformed(
                name,
                format!("the {noun} is never closed: a `\"` opens one and a `\"` ends it"),
                location,
            ));
        }
        let named: String = self.chars[opens + 1..end - 1].iter().collect();
        self.advance_on_line(end);
        Ok(named)
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
        let body_start = self.at.offset;
        // Bounded by the file, not by the buffer. A `}` in an included
        // library would otherwise close a body opened in the program that
        // included it -- silently, since a brace in prose is a brace to this
        // walk -- and everything between them would vanish into the body.
        let end = self.scan_end();
        let mut depth = 1usize;
        while self.at.offset < end {
            let at = self.at.offset;
            // `depth` is never zero here, so a body is always being read.
            match lex::step(&self.chars, at, body_start, true) {
                Step::Past(next) => self.advance_to(next),
                Step::Open => {
                    depth += 1;
                    self.bump();
                }
                Step::Close => {
                    depth -= 1;
                    self.bump();
                    if depth == 0 {
                        return Ok(at);
                    }
                }
            }
        }
        Err(malformed(
            Directive::Macro.spelling(),
            format!("the body of '{name}' is never closed with '}}'"),
            location,
        ))
    }

    /// `@name` or `@name(a, b)` -- expand a macro's body here.
    fn invoke(
        &mut self,
        name: String,
        def: Rc<MacroDef>,
        location: SourceLocation,
    ) -> Result<(), MacroError> {
        let mut arguments = self.argument_list(&name)?;

        // A `{` after the arguments hands the macro a body, as its last
        // argument. The same shape `@macro` itself uses to take one, so a
        // macro that loops reads the way the loop it stands for reads:
        //
        //     @while(counter) {
        //         @to out
        //         +
        //     }
        self.skip_blanks();
        if self.peek() == Some('{') {
            arguments.push((self.block_argument(&name, location)?, location));
        }

        if arguments.len() != def.params.len() {
            return Err(MacroError::ArgumentCount {
                name,
                expected: def.params.len(),
                actual: arguments.len(),
                location,
            });
        }
        self.end_of_directive(Directive::Macro)?;

        // A cycle is the same *invocation* reached from inside itself, not
        // the same name seen twice. Once a macro can take a body, a name
        // legitimately appears inside its own expansion -- `@while(n) { ...
        // @while(m) { ... } ... }` is two loops and not a recursion, and the
        // two are at different places in the source. A macro that really does
        // expand itself re-enters the same place, because its body is fixed.
        if self.frames.iter().any(|frame| frame.call_site == location) {
            let mut chain: Vec<String> =
                self.frames.iter().map(|frame| frame.name.clone()).collect();
            chain.push(name.clone());
            return Err(MacroError::CircularMacro {
                name,
                chain,
                location,
            });
        }
        self.guard_depth(location)?;

        self.enter(name, def, arguments, location)
    }

    /// `@text "..."` -- the instructions that print it, and nothing else.
    ///
    /// Printing a string was the most expensive thing a `.bfm` could do.
    /// `@say(out, 'B')` empties a cell and counts it up to 66, which is about
    /// a hundred instructions a character; that was most of why the corpus's
    /// `99bottles.bfm` was six and a half times the size of the program it
    /// matches byte for byte, and it is 2.2 times now. `gyrus`'s `codegen` has done this properly all along -- a
    /// table of the shortest way from any byte to any other, including
    /// multiplication loops, at around ten instructions a character -- and
    /// nothing connected the two.
    ///
    /// **What it costs the program is cells.** The generated code starts on
    /// the cell under the cursor and walks right onto as many as it finds
    /// cheaper, so those are emptied first and left holding whatever the
    /// printing left in them. The cursor comes back to where it started,
    /// which is the one thing a directive can promise here and the one thing
    /// a program needs.
    ///
    /// Escapes are the ones a `'x'` literal takes, from the same table: `\n`,
    /// `\t`, `\r`, `\0`, `\\` and `\'`. Anything else is refused rather than
    /// passed through, so a `\r` cannot quietly come out as two characters
    /// here while meaning a carriage return one line above.
    ///
    /// A `"` cannot appear at all, because the rule that finds the end of the
    /// text is the one `@include` uses on paths, where a backslash is a
    /// separator and not an escape.
    fn text(&mut self, location: SourceLocation) -> Result<(), MacroError> {
        // Before generating anything, so the refusal names the directive
        // rather than the first bracket of code nobody wrote.
        if self.including > 0 {
            return Err(MacroError::IncludedFileEmits {
                what: "'@text'".to_string(),
                location,
            });
        }
        let name = Directive::Text.spelling();
        let text = self.quoted_operand(
            name,
            "text",
            "expected quoted text, as in `@text \"Hello\"`",
            location,
        )?;
        self.end_of_directive(Directive::Text)?;

        let mut decoded = String::with_capacity(text.len());
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                decoded.push(c);
                continue;
            }
            let after = chars.next().ok_or_else(|| {
                malformed(name, "nothing follows the backslash".to_string(), location)
            })?;
            decoded.push(escape(after).map_err(|detail| malformed(name, detail, location))?);
        }

        if decoded.is_empty() {
            return Ok(());
        }
        let code = match self.compiled.get(&decoded) {
            Some(code) => Rc::clone(code),
            None => {
                let code: Rc<str> = gyrus::codegen::compile_string(&decoded).into();
                self.compiled.insert(decoded, Rc::clone(&code));
                code
            }
        };
        // How far right it goes, and where it stops: both are known now, which
        // is what lets the cursor be put back and the tracking stay honest.
        let (mut at, mut reach, mut lowest) = (0i64, 0i64, 0i64);
        for c in code.chars() {
            match c {
                '>' => at += 1,
                '<' => at -= 1,
                _ => {}
            }
            reach = reach.max(at);
            lowest = lowest.min(at);
        }

        // Two things are relied on and neither is written down in `codegen`:
        // that it never steps left of where it started, and that it does not
        // finish left of it either. Both hold because every multiplication
        // loop it builds is `[-d>+n<]>`. If one stopped holding, the cells to
        // the left would be used without being emptied -- the table needs
        // them at zero -- and the cursor would be left somewhere the expander
        // thinks it is not, which is worse than either.
        assert!(
            lowest >= 0 && at >= 0,
            "codegen walked left of where it started: reached {lowest}, ended {at}"
        );

        // Nothing but the cursor's own cell may be in the way. The generated
        // code assumes every cell it touches starts at zero, so those cells
        // are emptied below -- and emptying a cell somebody named is the kind
        // of wrong that produces a different answer rather than an error. How
        // far it reaches depends on the text, so a longer string would
        // silently take out more.
        if let Position::Known(from) = self.position {
            let taken: Vec<&String> = self
                .defined
                .iter()
                .filter(|name| match self.symbols.get(*name) {
                    Some((Symbol::Variable(cell), _)) => *cell > from && *cell <= from + reach,
                    _ => false,
                })
                .collect();
            if let Some(name) = taken.first() {
                return Err(MacroError::TextOverAName {
                    name: (*name).clone(),
                    reach: reach as usize,
                    location,
                });
            }
        }

        // Empty what it is about to use. The table assumes every cell it
        // touches starts at zero, and a program that has been running does not
        // owe it that.
        let mut prologue = String::new();
        for _ in 0..=reach {
            prologue.push_str("[-]>");
        }
        self.emit_generated(&prologue, location)?;
        self.emit_run('<', (reach + 1) as u64, location)?;
        self.emit_generated(&code, location)?;
        // Back to where it started. A run of one character, so it goes through
        // the bulk path rather than one call per step.
        self.emit_run('<', at as u64, location)
    }

    /// Instructions the expander made rather than read, all reporting the
    /// directive that made them.
    fn emit_generated(&mut self, code: &str, origin: SourceLocation) -> Result<(), MacroError> {
        for c in code.chars() {
            match c {
                '[' | ']' => self.bracket_at(c, origin)?,
                _ => self.emit_run(c, 1, origin)?,
            }
        }
        Ok(())
    }

    /// `@repeat N { ... }` -- the body, N times over.
    ///
    /// `OP{N}` repeats one instruction, which is most of what a program wants
    /// and not all of it: a record is nine cells, and walking one is nine
    /// `>`, but *filling* one is nine of something longer. A block cannot be
    /// repeated by a macro, because a macro has no way to count -- expansion
    /// has no loop of its own, which is the whole reason this is a directive
    /// and not a library.
    ///
    /// The count is settled when the program is built, so it takes a number or
    /// a `@define`, and never a cell.
    fn repeat(&mut self, location: SourceLocation) -> Result<(), MacroError> {
        self.skip_blanks();
        let at_count = self.at;
        let count = self.number_or_name(
            Directive::Repeat,
            || "expected a count, as in `@repeat 9 { > }`".to_string(),
            at_count,
        )?;
        if count > REPEAT_LIMIT {
            return Err(MacroError::RepeatTooLarge {
                count,
                limit: REPEAT_LIMIT,
                location: at_count,
            });
        }
        self.skip_blanks();
        if self.peek() != Some('{') {
            return Err(malformed(
                Directive::Repeat.spelling(),
                "expected '{' to open the body to repeat".to_string(),
                self.at,
            ));
        }
        let (body_start, body_end) = self.body_span(Directive::Repeat.spelling(), location)?;
        self.end_of_directive(Directive::Repeat)?;

        for _ in 0..count {
            self.scan_span(body_start, body_end)?;
        }
        Ok(())
    }

    /// Expand a span of the source in place, and come back.
    ///
    /// Not an invocation. The body is *here*, in the scope it is written in,
    /// so there is no frame to push -- and everything that tells a repeated
    /// body from a called one follows from that: whose line an emitted byte
    /// names, whether a `@var` in it counts as being inside a macro, what it
    /// spends from the invocation budget, and whether the word `repeat` turns
    /// up in a chain of macros that use each other.
    ///
    /// Making it a fake invocation got all four of those wrong at once, which
    /// is what the origin check said first: every byte of a repeated body
    /// pointed at the `@repeat` line rather than at the instruction on it.
    fn scan_span(&mut self, start: SourceLocation, end: usize) -> Result<(), MacroError> {
        let resume = std::mem::replace(&mut self.at, start);
        let result = self.scan_until(end);
        self.at = resume;
        result
    }

    /// The `{ ... }` a directive or an invocation opens, and where it ends.
    fn body_span(
        &mut self,
        name: &str,
        location: SourceLocation,
    ) -> Result<(SourceLocation, usize), MacroError> {
        self.bump(); // past '{'
        let body_start = self.at;
        let body_end = self.skip_body(name, location)?;
        Ok((body_start, body_end))
    }

    /// The `{ ... }` after an invocation's arguments.
    ///
    /// The body is not read now -- only found -- and what it is read *with*
    /// is settled now: the frame it was written in, so that a block can name
    /// the parameters of the macro that wrote it. At file scope there is no
    /// frame and there is nothing to carry.
    fn block_argument(
        &mut self,
        name: &str,
        location: SourceLocation,
    ) -> Result<Symbol, MacroError> {
        let (body_start, body_end) = self.body_span(name, location)?;
        let (params, arguments) = match self.frames.last() {
            Some(frame) => (frame.def.params.clone(), frame.arguments.clone()),
            None => (Vec::new(), Vec::new()),
        };
        Ok(Symbol::Block(Rc::new(BlockArgument {
            def: Rc::new(MacroDef {
                params,
                body_start,
                body_end,
            }),
            arguments,
        })))
    }

    /// A block handed to a macro, expanded where it was written.
    fn invoke_block(
        &mut self,
        name: String,
        block: Rc<BlockArgument>,
        location: SourceLocation,
    ) -> Result<(), MacroError> {
        self.end_of_directive(Directive::Macro)?;
        // No cycle check, and not for want of trying. The test that finds one
        // for a macro -- the same invocation twice on the stack -- does not
        // identify a block: `@body` sits at one fixed place inside the macro
        // that takes it, and two nested `@while`s re-enter that place with two
        // different bodies. Depth is what bounds a block that really does
        // reach itself.
        self.guard_depth(location)?;
        let (def, arguments) = (Rc::clone(&block.def), block.arguments.clone());
        self.enter(name, def, arguments, location)
    }

    /// The limits every invocation is subject to, whatever it is invoking.
    ///
    /// A cycle is caught before these, and catches only what it can name; a
    /// chain of macros long enough to be a mistake but not a loop is what
    /// these are for.
    fn guard_depth(&mut self, location: SourceLocation) -> Result<(), MacroError> {
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
        Ok(())
    }

    /// A push, a scan, a pop. What used to be saved and restored around
    /// this by hand lives in the frame, so there is no invariant left to
    /// break -- and no way for an early return to leak half of it.
    fn enter(
        &mut self,
        name: String,
        def: Rc<MacroDef>,
        arguments: Vec<(Symbol, SourceLocation)>,
        location: SourceLocation,
    ) -> Result<(), MacroError> {
        let (body_start, body_end) = (def.body_start, def.body_end);
        self.frames.push(Invocation {
            name,
            def,
            arguments,
            boundary: body_start.offset,
            call_site: location,
            resume: self.at,
            conditionals: Vec::new(),
        });
        self.at = body_start;

        let result = self.scan_until(body_end);
        let frame = self.frames.pop().expect("pushed above");
        self.at = frame.resume;

        result?;
        // A body that opens a conditional closes it, for the same reason the
        // skip is bounded: an `@endif` in whatever this body invokes, or in
        // whatever invoked it, is not this one's to find. Reported the way
        // `run` reports the file's own -- against the outermost one left open.
        if let Some(&(directive, location)) = frame.conditionals.first() {
            return Err(unclosed(directive, location));
        }
        Ok(())
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
            None | Some('\n') | Some('*') => {
                let cell = self.next_free_cell();
                self.chosen.insert(cell);
                cell
            }
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
                let cell = self.number_or_name(
                    Directive::Var,
                    || Declaration::Var.missing_value(&name),
                    at_value,
                )?;
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
                let cell = cell as i64;
                // A number may name a cell another number already named --
                // two names for one cell in different phases is a real thing
                // to want, and both of them said so. It may not take a cell
                // the expander picked: that choice was made on the
                // understanding the cell was free, and nobody saw it made.
                if self.chosen.contains(&cell) {
                    return Err(MacroError::CellAlreadyChosen {
                        cell,
                        other: self.variable_at(cell).unwrap_or_default(),
                        location: at_value,
                    });
                }
                cell
            }
        };

        self.end_of_directive(Directive::Var)?;
        self.declare(name, Symbol::Variable(cell), location);
        Ok(())
    }

    /// The name of a variable already at `cell`.
    fn variable_at(&self, cell: i64) -> Option<String> {
        self.symbols
            .iter()
            .find(|(_, (symbol, _))| matches!(symbol, Symbol::Variable(at) if *at == cell))
            .map(|(name, _)| name.clone())
    }

    /// `@stride N` -- how many cells a record occupies.
    ///
    /// One per file, because it changes what every loop in the file means.
    fn stride(&mut self, location: SourceLocation) -> Result<(), MacroError> {
        self.refuse_inside_a_macro(Directive::Stride, location)?;
        if let Some((_, first)) = self.stride {
            return Err(malformed(
                Directive::Stride.spelling(),
                format!("a record size was already declared at {first}"),
                location,
            ));
        }
        self.skip_blanks();
        let at_value = self.at;
        let size = self.number_or_name(
            Directive::Stride,
            || "expected a record size, as in `@stride 9`".to_string(),
            at_value,
        )?;
        if size == 0 || size > EXPANSION_LIMIT as u64 {
            return Err(malformed(
                Directive::Stride.spelling(),
                format!("{size} is not a record size a program could walk"),
                at_value,
            ));
        }
        self.end_of_directive(Directive::Stride)?;
        self.stride = Some((size as i64, location));
        Ok(())
    }

    /// `@field NAME at N` -- a name for an offset within a record.
    ///
    /// Not a cell: a field says *which part* of a record, and the record it
    /// belongs to is wherever the cursor happens to be. That is what makes it
    /// usable after a scan, which is the position an array walked by scan
    /// loops is always in.
    fn field(&mut self, location: SourceLocation) -> Result<(), MacroError> {
        self.refuse_inside_a_macro(Directive::Field, location)?;
        let Some((stride, _)) = self.stride else {
            return Err(malformed(
                Directive::Field.spelling(),
                "a record size has to be declared first, as in `@stride 9`".to_string(),
                location,
            ));
        };
        let name = self.declared_name(Declaration::Field, location)?;

        self.skip_blanks();
        let at_keyword = self.at;
        if !self.identifier_is("at") {
            return Err(malformed(
                Directive::Field.spelling(),
                format!("expected `at`, as in `@field {name} at 0`"),
                at_keyword,
            ));
        }
        self.skip_blanks();
        let at_value = self.at;
        let offset = self.number_or_name(
            Directive::Field,
            || Declaration::Field.missing_value(&name),
            at_value,
        )?;
        if offset >= stride as u64 {
            return Err(malformed(
                Directive::Field.spelling(),
                format!("offset {offset} is outside a record of {stride} cells"),
                at_value,
            ));
        }

        self.end_of_directive(Directive::Field)?;
        self.declare(name, Symbol::Field(offset as i64), location);
        Ok(())
    }

    /// The lowest cell no `@var` has named.
    ///
    /// Lowest rather than next-after-the-last, so that mixing the two spellings
    /// does not leave a hole: `@var scratch at 9` followed by three plain
    /// `@var`s gives cells 0, 1 and 2, not 10, 11 and 12.
    fn next_free_cell(&self) -> i64 {
        let taken: BTreeSet<i64> = self
            .symbols
            .values()
            .filter_map(|(symbol, _)| match symbol {
                Symbol::Variable(cell) => Some(*cell),
                _ => None,
            })
            .collect();
        // The first gap in a sorted set is the first position whose value has
        // moved past its index -- one pass, rather than a lookup per candidate
        // in a container that is already in order.
        taken
            .iter()
            .enumerate()
            .find(|(index, cell)| **cell != *index as i64)
            .map_or(taken.len() as i64, |(index, _)| index as i64)
    }

    /// The name a directive takes as its operand, and where it was written.
    fn named_operand(
        &mut self,
        directive: Directive,
        expected: &str,
    ) -> Result<(String, SourceLocation), MacroError> {
        self.skip_blanks();
        let at_name = self.at;
        let name = self.identifier();
        if name.is_empty() {
            return Err(malformed(
                directive.spelling(),
                expected.to_string(),
                at_name,
            ));
        }
        Ok((name, at_name))
    }

    /// The cell a `@to` or `@here` names, and its own name for the error.
    fn cell_operand(&mut self, directive: Directive) -> Result<(String, Target), MacroError> {
        let (name, at_name) = self.named_operand(
            directive,
            "expected the name of a cell or a field, from `@var` or `@field`",
        )?;
        let target = self.resolve_target(&name, at_name)?;
        self.end_of_directive(directive)?;
        Ok((name, target))
    }

    /// `@to NAME` -- move the cursor to a named cell, or to a named field of
    /// the record it is in.
    fn to(&mut self, location: SourceLocation) -> Result<(), MacroError> {
        let (name, target) = self.cell_operand(Directive::To)?;

        // A `@to` is inside every loop currently open, recorded by what it
        // named: naming a cell is wrong in any body that moves, naming a field
        // is wrong only in one that does not move by whole records.
        for open in &mut self.open_brackets {
            match target {
                Target::Cell(_) => open.cell_to_inside.get_or_insert(location),
                Target::Field(_) => open.field_to_inside.get_or_insert(location),
            };
        }

        let (here, destination) = match (target, self.position) {
            (Target::Cell(cell), Position::Known(here)) => (here, cell),
            (Target::Field(offset), Position::Relative { offset: here, .. }) => (here, offset),
            // Each kind needs the position the other one has.
            (Target::Cell(_), Position::Relative { entered, .. }) => {
                return Err(MacroError::OnlyOffsetKnown {
                    name,
                    location,
                    entered,
                });
            }
            (Target::Field(_), Position::Known(_)) => {
                return Err(MacroError::NotInARecord { name, location });
            }
            (_, Position::Unknown(lost_at)) => {
                return Err(MacroError::PositionUnknown {
                    name,
                    location,
                    lost_at,
                });
            }
        };

        // `abs_diff` is total: unlike a subtraction it cannot overflow for
        // any pair of i64, so the distance needs no reasoning about bounds.
        let step = if destination >= here { '>' } else { '<' };
        self.emit_run(step, here.abs_diff(destination), location)
    }

    /// `@here NAME` -- assert where the cursor is, emitting nothing.
    fn here(&mut self, location: SourceLocation) -> Result<(), MacroError> {
        let (_, target) = self.cell_operand(Directive::Here)?;
        // The other way to change where the cursor is. `@to` and every
        // instruction emit, and are refused by `emit_run`; this one emits
        // nothing and would move the includer's idea of the cursor without
        // moving the cursor -- so the program that included it emits movement
        // for a position it is not at.
        if self.including > 0 {
            return Err(MacroError::IncludedFileMovesTheCursor { location });
        }
        // Inside a loop body this is a claim about where the cursor is *if the
        // body ran*, and a loop may run no times at all. The `]` decides
        // whether that leaves the exit ambiguous.
        for open in &mut self.open_brackets {
            open.here_inside = true;
        }
        // Saying where the cursor is, where the expander already knows, is
        // either agreement or a mistake -- and it can tell which. Refusing the
        // mistake is what lets the `]` above believe a position instead of
        // counting movement: the claim can only be wrong where it cannot be
        // checked, which is after something that lost the position.
        if let (Target::Cell(cell), Position::Known(believed)) = (target, self.position)
            && cell != believed
        {
            return Err(MacroError::HereContradictsCursor {
                claimed: cell,
                believed,
                location,
            });
        }
        self.position = match target {
            Target::Cell(cell) => Position::Known(cell),
            // Which field of a record, not which cell of the tape -- which is
            // the point, since a scan stops wherever the data says.
            Target::Field(offset) => Position::Relative {
                offset,
                entered: location,
            },
        };
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
        directive: Directive,
        missing: impl Fn() -> String,
        at_value: SourceLocation,
    ) -> Result<u64, MacroError> {
        let token = self.token(char::is_whitespace);
        let spelling = directive.spelling();
        match classify(&token) {
            Operand::Number(value) => Ok(value),
            Operand::Name(named) => self.resolve(named, at_value),
            Operand::Empty => Err(malformed(spelling, missing(), at_value)),
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
                self.skip_comment();
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
        let value = self.number_or_name(
            Directive::Define,
            || Declaration::Define.missing_value(&name),
            at_value,
        )?;
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
        // Checked before parsing, because `from_str_radix` accepts a leading
        // sign: without this `0x+41` is 65, while the decimal `+5` is refused.
        if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
            return Operand::Bad(format!("'{token}' is not a hexadecimal number"));
        }
        return match u64::from_str_radix(digits, 16) {
            Ok(value) => Operand::Number(value),
            // The same distinction the decimal branch below makes, and for the
            // same reason: this is a hexadecimal number, just too large.
            Err(_) => Operand::Bad(format!("'{token}' does not fit in a 64-bit number")),
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
/// ASCII only. A cell holds a byte, and the point of writing a letter instead
/// of a number is that it says what it means -- but `'\u{e9}'` would mean the
/// byte 233, which is not what printing it produces, because that is Latin-1
/// and the file is UTF-8. Accepting it would make the rule "characters that
/// fit work", when only the half that round-trips does. A byte above 127 can
/// still be written as a number, where nobody expects it to be a letter.
/// What a backslash and a character mean together.
///
/// One table, because there are two readers of it: a `'x'` literal and the
/// text of a `@text`. They had different tables for one commit, and `\r` in a
/// `@text` came out as a backslash and an `r` while the same escape one line
/// above gave a carriage return -- which is the shape of bug `lex.rs` counts
/// its readers to avoid.
fn escape(after: char) -> Result<char, String> {
    match after {
        'n' => Ok('\n'),
        't' => Ok('\t'),
        'r' => Ok('\r'),
        '0' => Ok('\0'),
        '\\' => Ok('\\'),
        '\'' => Ok('\''),
        other => Err(format!("\\{other} is not an escape this understands")),
    }
}

fn character(token: &str) -> Result<u8, String> {
    let body = token
        .strip_prefix('\'')
        .ok_or("it does not start with a quote")?
        .strip_suffix('\'')
        .ok_or_else(|| match token.ends_with('\'') {
            // A well-formed literal with something stuck to it: the quote is
            // there, so saying it is missing sends the reader to the wrong end.
            true => "it has no closing quote".to_string(),
            false => "something follows its closing quote".to_string(),
        })?;
    let mut chars = body.chars();
    let value = match chars.next().ok_or("it is empty")? {
        '\\' => escape(chars.next().ok_or("nothing follows the backslash")?)?,
        plain => plain,
    };
    if chars.next().is_some() {
        return Err("it holds more than one character".to_string());
    }
    match u8::try_from(u32::from(value)) {
        Ok(byte) if byte.is_ascii() => Ok(byte),
        _ => Err(format!(
            "{value:?} is not ASCII, so the byte it would mean is not what printing it produces"
        )),
    }
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

/// `@ifdef` or `@ifndef` left open, from the directive rather than its name.
///
/// The sibling of `malformed`, and for the same reason: three sites construct
/// this, and each of them would otherwise spell the directive out itself.
fn unclosed(directive: Directive, location: SourceLocation) -> MacroError {
    MacroError::UnclosedConditional {
        directive: directive.spelling(),
        location,
    }
}

/// Whether the name a declaration takes, which begins after `after`, is
/// `name`. Whether the directive is a declaration at all is the caller's
/// question: it has already read the spelling to count conditionals.
fn names(chars: &[char], after: usize, name: &str) -> bool {
    let mut i = after;
    while chars.get(i).is_some_and(|c| *c == ' ' || *c == '\t') {
        i += 1;
    }
    name.chars()
        .enumerate()
        .all(|(k, c)| chars.get(i + k) == Some(&c))
        && !chars
            .get(i + name.chars().count())
            .is_some_and(|c| is_identifier_char(*c))
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(lex::is_identifier_start) && chars.all(is_identifier_char)
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

            // `@to`, `@text` and a macro invocation are the constructs that
            // emit instructions nobody wrote, so their bytes point at the
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
            // A `@text` emits whatever the shortest way to print its string
            // turns out to be, so there is nothing to say about which
            // instruction a byte of it was -- only that it is credited to the
            // directive, which is checked above.
            let directive = Directive::from_spelling(&spelling).expect("checked just above");
            if !directive.emits_only_movement() {
                continue;
            }
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
        // Refused rather than read as prose. It expanded to "+[]+" once, which
        // was the only place this language failed silently: what somebody
        // wrote as a directive did nothing, and nothing said so.
        //
        // The ']' is still a real bracket and not part of a swallowed comment
        // tail -- the error is about the '@', not an unmatched '[' that the
        // source plainly matches.
        let error = expand("+[@define A 1 ]+").unwrap_err();
        let MacroError::StrayAt { directive, .. } = &error else {
            panic!("expected a stray '@', got {error:?}");
        };
        assert_eq!(directive, "define");
    }

    #[test]
    fn a_non_directive_at_does_not_swallow_the_rest_of_its_line() {
        // The guard the shape above exists for, kept on a word that is not a
        // directive: the ']' is a real bracket, not part of a comment tail.
        // This used to report an unmatched '[' that the source plainly
        // matched, and the refusal above must not be how that comes back.
        assert_eq!(expanded("+[@foo A 1 ]+"), "+[]+");
    }

    #[test]
    fn an_at_sign_in_an_email_address_is_prose() {
        // The delimiter is what makes this safe. A directive is the whole word
        // and then a blank, a newline, or the end of the file; `@here.org` is
        // none of those, and `pi.bf` would otherwise be refused for the sake
        // of its author's provider being spelled one way rather than another.
        assert_eq!(expanded("+ mail bob@here.org\n"), "+.");
        assert_eq!(expanded("+ write bob@to.com\n"), "+.");
        assert_eq!(expanded("+ x@text.io\n+"), "+.+");
    }

    #[test]
    fn a_doubled_at_does_not_change_where_a_body_ends() {
        // `lex` decides whether a line's quotes are literals, and it decides
        // it for the readers that walk text the expander has not reached. A
        // line opening with '@@' is prose, so the apostrophe here is an
        // apostrophe -- and `skip_body` finds the same '}' the expander would.
        assert_eq!(expanded("@macro m {\n@@ it's fine }\n@m\n"), "");
        assert_eq!(expanded("@macro n {\n@@ \" }\n+\n@n\n"), "+");
    }

    #[test]
    fn a_macro_invoked_mid_line_is_still_prose() {
        // Named so the hole is findable. Only the thirteen directive
        // spellings are refused mid-line; a macro's name is not, because the
        // thirteen are known before a file is read and a macro's name is not,
        // so including them would make what counts as prose depend on what
        // happens to be defined above it.
        assert_eq!(expanded("@macro m {\n+\n}\n+ @m +\n"), "++");
    }

    #[test]
    fn a_stray_at_says_how_to_write_it_either_way() {
        let hint = expand("+ @to a\n")
            .unwrap_err()
            .hint()
            .expect("a stray '@' has a hint");
        assert!(hint.contains("takes a line of its own"), "{hint}");
        assert!(hint.contains("'@@'"), "{hint}");
    }

    #[test]
    fn a_doubled_at_is_a_literal_one_and_emits_nothing() {
        assert_eq!(expanded("+ @@ +"), "++");
        // Including where it would otherwise be refused: this is the way out
        // for prose that really does want to say '@to'.
        assert_eq!(expanded("+ @@to a\n+"), "++");
        // And at the start of a line, where an '@' would otherwise open a
        // directive. A bare '@' alone on a line is still refused -- `char.bf`
        // has one, and needs the doubling like anything else.
        assert_eq!(expanded("+\n@@\n+"), "++");
        assert!(expand("+\n@\n+").is_err());
        assert_eq!(expanded("+\n@@define A 1\n+"), "++");
    }

    #[test]
    fn a_comment_may_hold_a_directive_that_is_only_prose() {
        assert_eq!(expanded("* mention @to and @define here\n+"), "+");
        assert_eq!(expanded("+ * @stride 9 is written like this\n+"), "++");
    }

    #[test]
    fn a_quoted_operand_may_hold_an_at_sign() {
        // '@text "a@b"' prints an email address as readily as anything else,
        // and the '@' inside the quotes is never a directive.
        assert!(expand("@text \"a@b\"").is_ok());
    }

    #[test]
    fn a_branch_not_taken_may_hold_a_directive_mid_line() {
        // A skipped branch is stepped over, not expanded, and is documented as
        // holding whatever it likes.
        assert_eq!(
            expanded("@ifdef NOPE\nthis @to is never read\n@endif\n+"),
            "+"
        );
    }

    #[test]
    fn the_at_signs_in_the_bf_corpus_are_still_prose() {
        // The reason the rule is narrow. `calc.bf` uses '@' as a marker inside
        // its instruction stream and `pi.bf` carries an email address; both
        // would have to be edited before conversion if '@' were reserved
        // outright, and neither spells a directive.
        assert_eq!(expanded("+@<<<\n+"), "+<<<+");
        assert_eq!(expanded("+ [ by Felix (felix@t-online.de) ]\n+"), "+[-.]+");
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
        assert_origins_are_exact("@var page\n@to page\n@text \"Hi\"\n");
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
        assert_eq!((*found, *wanted), (Kind::Variable, Wanted::Constant));

        let as_cell = expand("@define X 3\n@to X\n").unwrap_err();
        let MacroError::WrongKind { found, wanted, .. } = &as_cell else {
            panic!("expected a kind error, got {as_cell:?}");
        };
        assert_eq!((*found, *wanted), (Kind::Constant, Wanted::Target));

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
    fn here_cannot_contradict_a_position_the_expander_knows() {
        // The body really does move by one each time, and `@here` says it did
        // not. The `]` used to catch that, by counting movement rather than
        // believing a position; it is caught at the `@here` itself now, which
        // names the disagreement rather than the loop two lines below it.
        let source = "@var a at 0\n@var b at 3\n+[\n>\n@here a\n@to b\n@to a\n]\n";
        let error = expand(source).unwrap_err();
        let MacroError::HereContradictsCursor {
            claimed, believed, ..
        } = error
        else {
            panic!("a body with net movement was accepted: {error:?}");
        };
        assert_eq!((claimed, believed), (0, 1));
    }

    /// The point of measuring balance by position: a loop that loses the
    /// cursor and says where it landed is a loop whose movement is known
    /// again, whatever it emitted on the way.
    ///
    /// This is what lets a pointer-walking idiom from the catalogue -- a
    /// division, a comparison -- be used inside a loop at all, which is where
    /// they are wanted. Before it, the scan poisoned the enclosing loop's
    /// movement count for good and every `@to` after it was refused.
    #[test]
    fn a_scan_that_says_where_it_landed_leaves_the_loop_balanced() {
        let source = "@var a at 0\n@var b at 3\n+[\n[>]\n@here a\n@to b\n@to a\n]\n";
        assert_eq!(expanded(source), "+[[>]>>><<<]");
    }

    /// And re-anchoring somewhere other than where the body began is still a
    /// body that moves, so a `@to` in it is still refused.
    #[test]
    fn a_body_that_lands_somewhere_else_is_still_unbalanced() {
        let source = "@var a at 0\n@var b at 3\n+[\n[>]\n@here b\n@to a\n@to b\n]\n";
        let error = expand(source).unwrap_err();
        assert!(
            matches!(error, MacroError::MovingInsideUnbalancedLoop { .. }),
            "{error:?}"
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
        // Round and round once more than a check by name would have gone: a
        // cycle is the same invocation reached from inside itself, and that
        // takes one more turn to show than the same name does. What it costs
        // in turns it gains in not calling two nested loops a recursion.
        assert_eq!(
            chain,
            &[
                "a".to_string(),
                "b".to_string(),
                "a".to_string(),
                "b".to_string()
            ]
        );
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
        for body in [
            "@define A 1",
            "@var a at 0",
            "@macro inner { + }",
            // With a trailing comment, so the body reader has to find the end
            // before the rule that forbids it can apply.
            "@macro inner { ++ * why }",
        ] {
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
        // And a brace in the middle of a sentence is not the one closing the
        // body, so this one-line body is genuinely never closed. The message
        // says so rather than blaming what follows the brace, which is what it
        // used to do when any `}` ended a comment.
        let error = expand("@macro two { ++ * a } brace\n@two\n").unwrap_err();
        let MacroError::MalformedDirective { detail, .. } = &error else {
            panic!("expected a malformed directive, got {error:?}");
        };
        assert!(detail.contains("never closed"), "{detail}");
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
        // Cycles are caught as cycles, which bounds nothing here: a thousand
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

    /// A character literal is ASCII, and the reason is not that a cell is
    /// small.
    ///
    /// `'\u{e9}'` fits in a byte perfectly well -- it is 233. But 233 is
    /// Latin-1 and this file is UTF-8, so printing that byte does not produce
    /// the character somebody wrote, and the whole point of writing a letter
    /// instead of a number is that it says what it means. Accepting it would
    /// make the rule "characters that fit work" when only the half that
    /// round-trips does, and the failure would be silent.
    #[test]
    fn a_character_literal_is_ascii() {
        for source in ["+{'\u{e9}'}\n", "+{'\u{20ac}'}\n"] {
            let error = expand(source).unwrap_err();
            let MacroError::BadRepeatCount { detail, .. } = &error else {
                panic!("{source:?}: expected a bad repeat count, got {error:?}");
            };
            assert!(detail.contains("is not ASCII"), "{detail}");
        }
        // The byte is still available, where nobody expects it to be a letter.
        assert_eq!(expanded("+{233}\n").len(), 233);
        assert_eq!(expanded("+{0xE9}\n").len(), 233);
    }

    #[test]
    fn a_quoted_brace_ends_a_macro_body_nowhere() {
        // `+{'}'}` is documented as the obvious thing to write and tested at
        // the top level, but the body skipper had its own repeat-count reader
        // that stopped inside the literal and took its closing quote for the
        // body's end.
        assert_eq!(expanded("@macro brace {\n+{'}'}\n}\n@brace\n").len(), 125);
        assert_eq!(
            expanded("@macro n(c) {\n+{c}\n}\n@macro m {\n@n('}')\n}\n@m\n").len(),
            125
        );
    }

    #[test]
    fn a_number_may_not_take_a_cell_the_expander_chose() {
        // Newly reachable: before a cell could be chosen, every cell was
        // somebody's own choice. `scan.bfm` is the live shape -- three chosen
        // cells and a terminator at 3 -- where one more `@var` would have
        // silently aliased the terminator and stopped the scan ever ending.
        let error = expand("@var a\n@var b at 0\n").unwrap_err();
        let MacroError::CellAlreadyChosen { cell, other, .. } = &error else {
            panic!("expected a taken cell, got {error:?}");
        };
        assert_eq!((*cell, other.as_str()), (0, "a"));

        // Two numbers naming one cell is allowed: both of them said so.
        assert_eq!(expanded("@var a at 2\n@var b at 2\n@to b\n"), ">>");
    }

    #[test]
    fn a_chosen_cell_fills_the_lowest_gap() {
        assert_eq!(expanded("@var a at 0\n@var b at 2\n@var c\n@to c\n"), ">");
        assert_eq!(expanded("@var a at 0\n@var b at 1\n@var c\n@to c\n"), ">>");
    }

    #[test]
    fn a_sign_does_not_sneak_into_a_hexadecimal() {
        // `from_str_radix` accepts one, so `0x+41` was 65 while the decimal
        // `+5` was refused -- two spellings of one number disagreeing about
        // whether a sign is legal.
        for source in ["+{0x+41}\n", "+{0x-1}\n", "+{0x}\n", "+{0xzz}\n"] {
            let error = expand(source).unwrap_err();
            let MacroError::BadRepeatCount { detail, .. } = &error else {
                panic!("{source:?}: expected a bad repeat count, got {error:?}");
            };
            assert!(detail.contains("not a hexadecimal"), "{source:?}: {detail}");
        }
    }

    #[test]
    fn a_hexadecimal_too_large_is_told_apart_from_a_malformed_one() {
        // The distinction the decimal branch already made, for the same
        // reason: this is a hexadecimal number, it is just too big.
        let error = expand("+{0xFFFFFFFFFFFFFFFFF}\n").unwrap_err();
        let MacroError::BadRepeatCount { detail, .. } = &error else {
            panic!("expected a bad repeat count, got {error:?}");
        };
        assert!(
            detail.contains("does not fit in a 64-bit number"),
            "{detail}"
        );
    }

    #[test]
    fn an_unclosed_quote_is_named_wherever_it_starts() {
        // Asked of the reader that knows, rather than guessed at from the
        // spelling: a leading space or a leading letter used to turn this
        // into a complaint about the brace.
        for source in ["+{'a}\n", "+{ 'a }\n", "+{A'}\n"] {
            let error = expand(source).unwrap_err();
            let MacroError::BadRepeatCount { detail, .. } = &error else {
                panic!("{source:?}: expected a bad repeat count, got {error:?}");
            };
            assert!(detail.contains("no closing quote"), "{source:?}: {detail}");
        }
    }

    #[test]
    fn something_stuck_to_a_literal_is_not_a_missing_quote() {
        let error = expand("+{'a'x}\n").unwrap_err();
        let MacroError::BadRepeatCount { detail, .. } = &error else {
            panic!("expected a bad repeat count, got {error:?}");
        };
        assert!(detail.contains("follows its closing quote"), "{detail}");
    }

    #[test]
    fn a_quoted_brace_does_not_hide_the_declarations_below_it() {
        // The "defined below this line" hint counts braces to know whether a
        // declaration is inside a macro body. A brace in a literal is data.
        let error = expand("+{B}\n@define OPEN '{'\n@define B 5\n").unwrap_err();
        let hint = error.hint().expect("a hint");
        assert!(hint.contains("defined below this line"), "{hint}");
    }

    #[test]
    fn origins_survive_the_new_spellings() {
        super::tests::assert_origins_are_exact("@var a\n@var b\n@to b\n+{'A'}\n");
    }
}

#[cfg(test)]
mod relative_addressing {
    use super::tests::expanded;
    use super::*;

    const RECORD: &str = "@stride 3\n@field marker at 0\n@field one at 1\n@field two at 2\n";

    #[test]
    fn a_field_is_reached_from_wherever_the_record_is() {
        // No `@var` and no absolute position anywhere: the cursor is on a
        // field of a record, and the moves are between fields.
        let source = format!("{RECORD}@here marker\n@to two\n@to one\n");
        assert_eq!(expanded(&source), ">><");
    }

    #[test]
    fn a_scan_over_records_keeps_the_field_it_is_on() {
        // The rule the whole thing rests on. The body moves by exactly one
        // record, so the offset it starts each iteration at is the same, and
        // a `@to` naming a field is right every time.
        let source = format!("{RECORD}@here marker\n[\n@to one\n.\n@to marker\n>{{3}}\n]\n");
        assert_eq!(expanded(&source), "[>.<>>>]");
    }

    #[test]
    fn an_offset_wraps_within_a_record() {
        // The offset says which field, not how far the cursor has travelled,
        // so a whole record forward from field 0 is field 0 again.
        let source = format!("{RECORD}@here marker\n>{{3}}\n@to one\n");
        assert_eq!(expanded(&source), ">>>>");
        let back = format!("{RECORD}@here two\n>\n@to two\n");
        assert_eq!(expanded(&back), ">>>");
    }

    #[test]
    fn a_loop_that_moves_by_part_of_a_record_still_loses_the_position() {
        // Two cells of a three-cell record: which field the cursor is on
        // changes every iteration, so a field is no more reachable than a
        // cell would be.
        let source = format!("{RECORD}@here marker\n[\n@to one\n@to marker\n>{{2}}\n]\n");
        let error = expand(&source).unwrap_err();
        assert!(
            matches!(error, MacroError::MovingInsideUnbalancedLoop { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn a_cell_is_not_reachable_from_a_record_and_the_reverse() {
        // Each kind needs the position the other one has, and says so.
        let to_a_cell = format!("{RECORD}@var v at 7\n@here marker\n@to v\n");
        let error = expand(&to_a_cell).unwrap_err();
        let MacroError::OnlyOffsetKnown { name, .. } = &error else {
            panic!("expected an offset-only position, got {error:?}");
        };
        assert_eq!(name, "v");

        let to_a_field = format!("{RECORD}@var v at 7\n@to v\n@to one\n");
        let error = expand(&to_a_field).unwrap_err();
        let MacroError::NotInARecord { name, .. } = &error else {
            panic!("expected a not-in-a-record error, got {error:?}");
        };
        assert_eq!(name, "one");
    }

    #[test]
    fn a_scan_still_loses_a_cell_even_when_it_keeps_a_field() {
        // Whole records preserve the *offset*. They say nothing about which
        // record, so the cell is gone -- and the error says exactly that
        // rather than the vaguer "position not known", because after such a
        // loop the offset genuinely is known.
        let source = format!("{RECORD}@var v at 7\n@to v\n[\n>{{3}}\n]\n@to v\n");
        let error = expand(&source).unwrap_err();
        assert!(
            matches!(error, MacroError::OnlyOffsetKnown { .. }),
            "{error:?}"
        );

        // And a field *is* reachable, without a `@here`: the cursor was at a
        // known cell and the body moved whole records, so which field it is on
        // is arithmetic. 7 is offset 1, so reaching field 2 is one step.
        let reachable = format!("{RECORD}@var v at 7\n@to v\n[\n>{{3}}\n]\n@to two\n");
        assert!(
            expanded(&reachable).ends_with('>'),
            "{}",
            expanded(&reachable)
        );
    }

    #[test]
    fn a_record_is_declared_once_and_before_its_fields() {
        assert!(matches!(
            expand("@field marker at 0\n").unwrap_err(),
            MacroError::MalformedDirective { .. }
        ));
        assert!(matches!(
            expand("@stride 3\n@stride 4\n").unwrap_err(),
            MacroError::MalformedDirective { .. }
        ));
        for source in ["@stride 0\n", "@stride\n", "@stride x\n"] {
            assert!(expand(source).is_err(), "{source:?} was accepted");
        }
    }

    #[test]
    fn a_field_lies_inside_its_record() {
        let error = expand("@stride 3\n@field far at 3\n").unwrap_err();
        let MacroError::MalformedDirective { detail, .. } = &error else {
            panic!("expected a malformed directive, got {error:?}");
        };
        assert!(detail.contains("outside a record"), "{detail}");
        // The last offset inside it is fine.
        assert!(expand("@stride 3\n@field last at 2\n").is_ok());
    }

    #[test]
    fn a_field_shares_the_one_namespace() {
        assert!(matches!(
            expand("@stride 3\n@var x at 0\n@field x at 1\n").unwrap_err(),
            MacroError::Redefinition { .. }
        ));
        let error = expand("@stride 3\n@field f at 1\n+{f}\n").unwrap_err();
        let MacroError::WrongKind { found, .. } = &error else {
            panic!("expected a kind error, got {error:?}");
        };
        assert_eq!(*found, Kind::Field);
    }

    /// Emitted moves are not movement once a nested loop is involved.
    ///
    /// A scan runs a number of times nobody knows, so a body containing one
    /// moves by an unknown amount however its `>` and `<` count up. Reading
    /// the emitted total as the real one let the whole-records rule admit a
    /// `@to` that is right on the first iteration and wrong on every one
    /// after -- exactly what that rule exists to prevent.
    #[test]
    fn a_nested_scan_makes_the_count_mean_nothing() {
        // Emitted: 1 - 1 + 1 + 2 = 3, a whole record. Actual: the scan's
        // length plus two, which is not.
        let source = format!("{RECORD}@here marker\n[\n@to one\n@to marker\n[>]\n>{{2}}\n]\n");
        let error = expand(&source).unwrap_err();
        assert!(
            matches!(error, MacroError::MovingInsideUnbalancedLoop { .. }),
            "{error:?}"
        );

        // The same for a body that emits nothing net. This one was wrong
        // before records existed at all: `moved == 0` was read as balanced.
        let cells = "@var v at 0\n@var w at 1\n@to v\n[\n@to w\n@to v\n[>]\n<\n]\n";
        let error = expand(cells).unwrap_err();
        assert!(
            matches!(error, MacroError::MovingInsideUnbalancedLoop { .. }),
            "{error:?}"
        );

        // A nested loop that moves by whole records is fine, because k of
        // them is still whole records.
        let nested = format!("{RECORD}@here marker\n[\n@to one\n@to marker\n[>{{3}}]\n>{{3}}\n]\n");
        assert!(expand(&nested).is_ok(), "{:?}", expand(&nested).err());
    }

    /// A `@here` inside a loop describes where the cursor is if the body ran.
    #[test]
    fn a_here_inside_a_body_does_not_escape_a_loop_that_may_not_run() {
        let source = format!(
            "{RECORD}@var v at 5\n@to v\n[\n@here marker\n@to two\n@to marker\n>{{3}}\n]\n@to one\n"
        );
        let error = expand(&source).unwrap_err();
        assert!(
            matches!(error, MacroError::PositionUnknown { .. }),
            "{error:?}"
        );

        // One that leaves the cursor where it found it is not ambiguous:
        // both the zero-iteration and the k-iteration answers agree.
        let agrees =
            format!("{RECORD}@here marker\n[\n@here marker\n@to one\n@to marker\n]\n@to two\n");
        assert!(expand(&agrees).is_ok(), "{:?}", expand(&agrees).err());
    }

    /// Rule 2 as the module documents it: from a *known cell* too, not only
    /// from a known offset. Which field cell 7 is on with a stride of three is
    /// arithmetic, and a loop over whole records does not change it.
    #[test]
    fn a_known_cell_becomes_a_known_field_across_a_scan() {
        let source = format!("{RECORD}@var v at 7\n@to v\n[\n>{{3}}\n]\n@to two\n");
        // 7 is offset 1; field `two` is offset 2; one step.
        assert!(
            expanded(&source).ends_with(">>>]>"),
            "{}",
            expanded(&source)
        );
    }

    #[test]
    fn a_to_wants_a_cell_or_a_field_and_says_both() {
        let error = expand("@define C 3\n@to C\n").unwrap_err();
        let MacroError::WrongKind { wanted, .. } = &error else {
            panic!("expected a kind error, got {error:?}");
        };
        assert_eq!(*wanted, Wanted::Target);
        assert!(error.to_string().contains("cell or field"), "{error}");
    }

    #[test]
    fn origins_survive_relative_movement() {
        super::tests::assert_origins_are_exact(&format!(
            "{RECORD}@here marker\n[\n@to one\n.\n@to marker\n>{{3}}\n]\n"
        ));
    }
}

#[cfg(test)]
mod one_lexer {
    use super::*;

    /// Constructs whose lexical extent is not obvious, and which the readers
    /// of this language got wrong at least once each.
    const AWKWARD: &[(&str, &str)] = &[
        ("a quoted brace", "+{'}'}"),
        ("a quoted quote", "+{'\\''}"),
        ("a brace in prose", "+ * a { brace\n"),
        ("a close brace in prose", "+ * a } brace\n"),
        ("a quote in prose", "+ * it's prose\n"),
        // An apostrophe with no comment in front of it. The body reader used
        // to take it for a literal and skip to the end of the line, hiding the
        // brace after it from the scan that has to find the body's end.
        ("a bare apostrophe", "+ it's prose\n"),
        ("an unrepeated brace count", "+{3}"),
        // Only the fourth reader can get this one wrong, and it is the same
        // question as the rest: an `@endif` inside a comment is prose, and
        // ends where the comment does.
        ("an @endif in prose", "+ * an @endif in prose\n"),
    ];

    /// The same construct, read in the four places that read this language.
    ///
    /// Each of these was a separate reader with its own idea of where a
    /// comment ends, how far a literal reaches, and whether a `{` is a repeat
    /// count -- and each difference between them cost a bug. One input set
    /// through all of them is the test that would have caught every one, and
    /// the reason to add a reader here rather than to test it on a list of
    /// its own: the next construct added above is checked against all four.
    #[test]
    fn the_four_readers_agree_about_every_awkward_construct() {
        for &(what, construct) in AWKWARD {
            // 1. At the top level, where the expander reads it.
            let plain = format!("{construct}\n");
            let direct = expand(&plain)
                .unwrap_or_else(|e| panic!("{what} at the top level: {e}"))
                .brainfuck()
                .to_string();

            // 2. Inside a macro body, where `skip_body` has to find the end.
            let wrapped = format!("@macro body {{\n{construct}\n}}\n@body\n");
            let through_macro = expand(&wrapped)
                .unwrap_or_else(|e| panic!("{what} inside a macro body: {e}"))
                .brainfuck()
                .to_string();
            assert_eq!(
                direct, through_macro,
                "{what} expands differently inside a macro body"
            );

            // 3. Before a declaration, where the "defined below" hint walks
            //    ahead of the cursor looking for one.
            let ahead = format!("+{{LATER}}\n{construct}\n@define LATER 3\n");
            let error = expand(&ahead).unwrap_err();
            let hint = error.hint().unwrap_or_default();
            assert!(
                hint.contains("defined below this line"),
                "{what} hid the declaration after it from the hint: {hint}"
            );

            // 4. Inside a branch not taken, which `skip_branch` walks to the
            //    `@endif` without expanding a character of it.
            let skipped = format!("@ifdef MISSING\n{construct}\n@endif\n.\n");
            assert_eq!(
                expand(&skipped)
                    .unwrap_or_else(|e| panic!("{what} inside a skipped branch: {e}"))
                    .brainfuck(),
                ".",
                "{what} left something behind in a branch that was not taken"
            );
        }
    }

    /// A `}` in prose is reserved wherever it is written, and both readers
    /// say so -- differently, because they are in different positions to.
    #[test]
    fn a_close_brace_in_prose_is_refused_at_the_top_level_and_ends_a_body() {
        // Outside a body there is nothing for it to close.
        assert!(matches!(
            expand("+ it's } prose\n").unwrap_err(),
            MacroError::StrayBrace { brace: '}', .. }
        ));
        // Inside one it is the end of the body, so what follows is left on the
        // `@macro` line -- which a directive owns.
        let error = expand("@macro m {\nit's } fine\n}\n@m\n").unwrap_err();
        let MacroError::MalformedDirective { detail, .. } = &error else {
            panic!("expected a malformed directive, got {error:?}");
        };
        assert!(detail.contains("rest of its line"), "{detail}");
    }

    /// The lookahead reader, given a macro body -- which it never was.
    ///
    /// This is where its idea of where a comment ends had drifted again, in
    /// the commit that set out to stop exactly that: the body's closing brace
    /// was swallowed by the trailing comment, so its depth never came back to
    /// zero and every declaration below was hidden.
    #[test]
    fn the_hint_sees_past_a_one_line_body_with_a_comment() {
        for body in [
            "@macro clear { [-] * clears it }",
            "@macro pair(a) { +{a} * doubles }",
            "@macro many {\n++ * why }",
        ] {
            let source = format!("+{{LATER}}\n{body}\n@define LATER 3\n");
            let hint = expand(&source).unwrap_err().hint().unwrap_or_default();
            assert!(hint.contains("defined below this line"), "{body}: {hint}");
        }
    }
}

#[cfg(test)]
/// Bodies handed to macros, and bodies repeated.
///
/// Both exist because the alternative was writing the loop out. A macro could
/// take a cell and a count but not a *body*, so every `[` in the corpus is
/// written where it is used, and `@repeat` had no way to exist at all: a macro
/// cannot count, because expansion has no loop of its own.
mod blocks {
    use super::tests::expanded;
    use super::*;

    const WHILE: &str = "@macro while(cell, body) {\n@to cell\n[\n@body\n@to cell\n]\n}\n";

    #[test]
    fn a_macro_can_take_a_body() {
        let source = format!("@var n at 0\n{WHILE}@to n\n+{{3}}\n@while(n) {{\n@to n\n-\n}}\n");
        assert_eq!(expanded(&source), "+++[-]");
    }

    /// The case the capture is for. `ch` belongs to `emit`, and the block that
    /// names it is expanded inside `while`, whose frame has no such parameter
    /// -- so a block that carried nothing would fail here, and this is the
    /// shape most uses of a block take.
    #[test]
    fn a_block_names_the_parameters_of_the_macro_that_wrote_it() {
        let source = format!(
            "@var n at 0\n{WHILE}\n@macro emit(step) {{\n@to n\n+{{2}}\n@while(n) {{\n@to n\n-{{step}}\n}}\n}}\n@emit(1)\n"
        );
        assert_eq!(expanded(&source), "++[-]");
    }

    #[test]
    fn a_block_may_be_expanded_more_than_once() {
        let source = "@macro twice(body) {\n@body\n@body\n}\n@twice {\n+\n}\n";
        assert_eq!(expanded(source), "++");
    }

    #[test]
    fn a_block_may_hold_another() {
        let source = format!(
            "@var n at 0\n@var m at 1\n{WHILE}@to n\n+\n@to m\n+\n@while(n) {{\n@while(m) {{\n@to m\n-\n}}\n@to n\n-\n}}\n"
        );
        assert_eq!(expanded(&source), "+>+<[>[-]<-]");
    }

    #[test]
    fn a_macro_wanting_a_body_says_so_when_it_does_not_get_one() {
        let source = format!("@var n at 0\n{WHILE}@while(n)\n");
        let error = expand(&source).unwrap_err();
        assert!(
            matches!(
                error,
                MacroError::ArgumentCount {
                    expected: 2,
                    actual: 1,
                    ..
                }
            ),
            "{error:?}"
        );
    }

    #[test]
    fn repeat_expands_its_body_that_many_times() {
        assert_eq!(expanded("@repeat 3 {\n+>\n}\n"), "+>+>+>");
        assert_eq!(expanded("@repeat 0 {\n+\n}\n.\n"), ".");
        assert_eq!(expanded("@define N 4\n@repeat N {\n>\n}\n"), ">>>>");
    }

    /// The reason it is a directive: a count is settled when the program is
    /// built, so it is a number or a `@define` and never a cell.
    #[test]
    fn repeat_counts_at_expansion_time_or_says_why_not() {
        let error = expand("@var n\n@repeat n {\n+\n}\n").unwrap_err();
        assert!(matches!(error, MacroError::WrongKind { .. }), "{error:?}");

        let error = expand("@repeat 3\n+\n").unwrap_err();
        assert!(
            matches!(error, MacroError::MalformedDirective { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn repeat_inside_a_macro_uses_that_macro_s_parameters() {
        let source = "@macro row(step) {\n@repeat 3 {\n+{step}\n}\n}\n@row(2)\n";
        assert_eq!(expanded(source), "++++++");
    }

    /// The bytes of a repeated body come from where they are written.
    ///
    /// A macro's bytes name the invocation, because a definition is somewhere
    /// else and used many times. A repeated body is *here*, once, so it names
    /// itself -- and the crate's own origin check is what says so.
    #[test]
    fn a_repeated_body_names_its_own_lines() {
        super::tests::assert_origins_are_exact("@repeat 3 {\n+\n}\n");
        super::tests::assert_origins_are_exact("@define N 2\n@repeat N {\n+>\n}\n");
    }

    /// Printed text costs about a tenth of what setting a cell costs, and the
    /// cursor comes back to where it started so a program can carry on.
    #[test]
    fn text_is_cheaper_than_setting_a_cell_at_a_time() {
        let spelled = expanded("@var out\n@to out\n+{72}\n.\n[-]+{105}\n.\n");
        let printed = expanded("@text \"Hi\"\n");
        assert!(
            printed.len() * 2 < spelled.len(),
            "printed {} against {} spelled out",
            printed.len(),
            spelled.len()
        );

        // The cursor comes back, so a cell named before the text still means
        // what it meant. `page` is declared last, which is what a text needs:
        // room after it that nobody has named.
        assert_eq!(
            expanded("@var a at 0\n@var page at 1\n@to page\n@text \"x\"\n@to a\n+\n"),
            format!(">{}<+", expanded("@text \"x\"\n"))
        );
    }

    /// A text runs over the cells after the cursor, and emptying one somebody
    /// named is a wrong answer rather than an error -- so it is an error.
    #[test]
    fn text_refuses_to_run_over_a_named_cell() {
        let error =
            expand("@var out at 0\n@var flag at 3\n@to out\n@text \"Hello world\"\n").unwrap_err();
        assert!(
            matches!(&error, MacroError::TextOverAName { name, .. } if name == "flag"),
            "{error:?}"
        );
    }

    #[test]
    fn text_says_what_it_wanted() {
        for source in ["@text\n", "@text Hello\n", "@text \"unclosed\n"] {
            let error = expand(source).unwrap_err();
            assert!(
                matches!(error, MacroError::MalformedDirective { .. }),
                "{source:?}: {error:?}"
            );
        }
    }

    #[test]
    fn repeat_is_bounded() {
        let error = expand("@repeat 2000000 {\n+\n}\n").unwrap_err();
        assert!(
            matches!(error, MacroError::RepeatTooLarge { .. }),
            "{error:?}"
        );
    }
}

#[cfg(test)]
mod conditionals {
    use super::tests::{assert_origins_are_exact, expanded};
    use super::*;

    #[test]
    fn a_branch_is_taken_or_skipped_by_whether_the_name_is_defined() {
        let defined = "@define D 1\n@ifdef D\n+\n@endif\n@ifndef D\n-\n@endif\n";
        assert_eq!(expanded(defined), "+");
        let undefined = "@ifdef D\n+\n@endif\n@ifndef D\n-\n@endif\n";
        assert_eq!(expanded(undefined), "-");
    }

    #[test]
    fn any_kind_of_name_counts_as_defined() {
        for declaration in [
            "@define D 1",
            "@var D",
            "@stride 2\n@field D at 0",
            "@macro D {\n+\n}",
        ] {
            let source = format!("{declaration}\n@ifdef D\n.\n@endif\n");
            assert_eq!(expanded(&source), ".", "{declaration}");
        }
    }

    /// The point of the feature: a branch not taken is never expanded, so it
    /// may hold what a taken one could not.
    #[test]
    fn a_skipped_branch_may_hold_what_would_otherwise_be_an_error() {
        for held in [
            "[",                        // an unbalanced bracket
            "]",                        // and the other way
            "@to nowhere",              // a name that does not exist
            "@define D 1\n@define D 2", // a redefinition
            "}",                        // a reserved character
            "+{OOPS}",                  // an undefined repeat count
            "@wibble",                  // not a directive at all
        ] {
            let source = format!("@ifdef MISSING\n{held}\n@endif\n+\n");
            assert_eq!(expanded(&source), "+", "{held:?} was not skipped");
        }
    }

    #[test]
    fn conditionals_nest() {
        let both = "@define A 1\n@define B 1\n@ifdef A\n+\n@ifdef B\n-\n@endif\n>\n@endif\n";
        assert_eq!(expanded(both), "+->");
        // The inner `@endif` does not close the outer conditional, so the
        // instructions after it are still skipped.
        let neither = "@ifdef MISSING\n+\n@ifdef ALSO_MISSING\n-\n@endif\n>\n@endif\n<\n";
        assert_eq!(expanded(neither), "<");
    }

    /// The case that makes this worth evaluating per invocation rather than
    /// once. Not a parameter -- see below -- but the scope the body is
    /// expanded *into*, which is the caller's and differs between calls.
    #[test]
    fn a_body_is_decided_where_it_is_expanded() {
        let source = "@macro m {\n@ifdef LATER\n+\n@endif\n-\n}\n@m\n@define LATER 1\n@m\n";
        assert_eq!(expanded(source), "-+-");
    }

    /// The reading `@ifdef what` invites is "was I given a `what`", and it
    /// cannot mean that: the arity has to match, so a parameter is bound on
    /// every invocation. Answering "yes, always" would leave the `@ifndef`
    /// arm as code that reads like a branch and can never be reached.
    #[test]
    fn a_parameter_is_refused_rather_than_always_answered_yes() {
        let source = "@macro emit(what) {\n@ifdef what\n+{what}\n@endif\n}\n@emit(3)\n";
        let error = expand(source).unwrap_err();
        assert!(
            matches!(&error, MacroError::ParameterAlwaysDefined { name, .. } if name == "what"),
            "{error:?}"
        );
        assert!(error.hint().is_some_and(|h| h.contains("@ifndef what")));
    }

    /// A macro body inside a branch not taken is text, not structure. The skip
    /// passes over it whole, exactly as the expander does at a definition, so
    /// nothing written inside it can end the branch early.
    #[test]
    fn a_definition_inside_a_skipped_branch_is_passed_over_whole() {
        for held in [
            "@macro m {\n@endif\n}",
            "@macro m {\n@ifdef X\n}",
            "@macro m {\n@endif\n@endif\n}",
        ] {
            let source = format!("@ifdef MISSING\n{held}\n@endif\n.\n");
            assert_eq!(expanded(&source), ".", "{held:?}");
        }
    }

    /// A conditional that *is* taken may hold a definition, which is most of
    /// what one is for at file scope.
    #[test]
    fn a_taken_branch_may_define_a_macro() {
        let source = "@define ON 1\n@ifdef ON\n@macro m { ++ }\n@endif\n@m\n";
        assert_eq!(expanded(source), "++");
    }

    /// The "defined below this line" hint may not point into a conditional:
    /// moving code below a branch that is skipped fixes nothing, and the
    /// reader would have followed the advice to get there.
    #[test]
    fn the_hint_does_not_advertise_a_declaration_inside_a_conditional() {
        let error = expand("@to X\n@ifdef MISSING\n@var X\n@endif\n").unwrap_err();
        let hint = error.hint().unwrap_or_default();
        assert!(!hint.contains("below this line"), "{hint:?}");
        // The hint it does give is the ordinary one, which is still true.
        assert!(
            matches!(error, MacroError::UndefinedSymbol { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn an_endif_closes_something_or_says_so() {
        assert!(matches!(
            expand("@endif\n").unwrap_err(),
            MacroError::UnmatchedEndif { .. }
        ));
        assert!(matches!(
            expand("@define D 1\n@ifdef D\n@endif\n@endif\n").unwrap_err(),
            MacroError::UnmatchedEndif { .. }
        ));
    }

    #[test]
    fn a_conditional_is_closed_or_says_so() {
        for source in ["@define D 1\n@ifdef D\n+\n", "@ifdef MISSING\n+\n"] {
            let error = expand(source).unwrap_err();
            let MacroError::UnclosedConditional { directive, .. } = &error else {
                panic!("{source:?}: expected an unclosed conditional, got {error:?}");
            };
            assert_eq!(*directive, "ifdef");
        }
    }

    /// A conditional opens and closes in one file or one body, because the
    /// skip has to stop somewhere.
    #[test]
    fn a_conditional_does_not_cross_a_macro_body() {
        // Opened inside, never closed there.
        let opens = expand("@define D 1\n@macro m {\n@ifdef D\n+\n}\n@m\n@endif\n").unwrap_err();
        assert!(
            matches!(opens, MacroError::UnclosedConditional { .. }),
            "{opens:?}"
        );
        // And a body's `@endif` does not close what its caller opened.
        let closes =
            expand("@define D 1\n@macro m {\n@endif\n}\n@ifdef D\n@m\n@endif\n").unwrap_err();
        assert!(
            matches!(closes, MacroError::UnmatchedEndif { .. }),
            "{closes:?}"
        );
    }

    #[test]
    fn a_conditional_says_what_it_wanted() {
        assert!(matches!(
            expand("@ifdef\n").unwrap_err(),
            MacroError::MalformedDirective { .. }
        ));
        assert!(matches!(
            expand("@define D 1\n@ifdef D extra\n@endif\n").unwrap_err(),
            MacroError::MalformedDirective { .. }
        ));
    }

    #[test]
    fn origins_survive_a_conditional() {
        assert_origins_are_exact("@define D 1\n@ifdef D\n+{3}\n@endif\n-\n");
        assert_origins_are_exact("@ifdef MISSING\n+{3}\n@endif\n-\n");
    }
}
