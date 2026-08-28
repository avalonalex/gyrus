//! # gyrus-macro
//!
//! A macro preprocessor for BrainFuck. It reads `.bfm` source, expands it to
//! ordinary BrainFuck, and -- the part that matters -- hands back a map from
//! every byte of the expansion to the position in the `.bfm` that wrote it.
//!
//! ## Why the map comes first
//!
//! A preprocessor that only repeated characters would be a shell script. The
//! reason to build this one inside gyrus is that a program which expands to
//! four thousand characters of BrainFuck and then reports a cell overflow at
//! column 3,847 of the *expansion* is exactly the experience gyrus exists to
//! replace. So the origin map is not a later phase: it is the signature of the
//! function that emits a byte, and there is no path here that emits one
//! without it.
//!
//! ## Located errors need nothing from `gyrus`
//!
//! `Expansion::remap` rewrites the `DebugInfo` that `parse_with_debug`
//! produced for the expansion so that it names the `.bfm` instead. Everything
//! that requires -- `DebugInfo::with_source`, `record`, `lookup`, `len` -- is
//! already public, so located runtime errors, loop call stacks included, come
//! out in macro coordinates without a line changing in the interpreter. That
//! mirrors how the debugger was built on the hook system.
//!
//! One thing a remapped table does *not* carry is loop metadata, which no
//! foreign crate can construct. Nothing in the error path reads it;
//! `gyrus-debug` does, so stepping through a `.bfm` will need a change in
//! `gyrus`. [`Expansion::remap`] says what kind.
//!
//! ## What it understands today
//!
//! ```text
//! @define STEP 9           * a named constant
//! @define BANG '!'         * or a character, or 0x21
//! @var counter             * a named cell, and the expander picks which
//! @var letter  at 1        * or say which, when it matters
//!
//! @to counter              * the expander emits the movement
//! +{STEP}                  * repeat an instruction
//!
//! [>]                      * a scan: the expander loses track of the cursor
//! @here letter             * tell it where the scan landed, emitting nothing
//!
//! @stride 9                * or say the tape is records of nine cells,
//! @field marker at 0       * name their parts, and walk an array of them
//! @here marker             * without ever naming a cell at all
//! [
//!     @to marker            * a directive starts its line, here as anywhere
//!     >{9}
//! ]
//! @here counter            * and back to an absolute cell when it is over
//!
//! @macro bump(by) {        * a body, expanded in place wherever it is used
//!     @to letter
//!     +{by}
//! }
//! @bump(STEP)
//!
//! @ifdef TRACE             * and code that is absent rather than skipped
//!     @to letter
//!     .
//! @endif
//!
//! @repeat 3 {              * a count around a body, not around one `+`
//!     @to letter
//!     .
//! }
//!
//! @macro loop(cell, body) {   * a body is an argument too, so a loop
//!     @to cell                * can be a macro like anything else
//!     [
//!         @body
//!         @to cell
//!     ]
//! }
//! @loop(counter) {
//!     @to counter
//!     -
//! }
//! ```
//!
//! That block expands; `the_documented_example_expands` reads it out of this
//! file and runs it, because a front page nobody executes is a front page that
//! drifts. It is missing one directive, and for a reason worth stating:
//!
//! ```text
//! @include "lib/ascii.bfm"   * another file's declarations, read here
//! ```
//!
//! `@include` needs a file to resolve its path against, so it works through
//! [`expand_at`] and not [`expand`] -- which is also why it cannot appear in a
//! block expanded from text.
//!
//! An included file **declares**: `@define`, `@var`, `@field`, `@stride`,
//! `@macro`. It may not emit BrainFuck, nor move the cursor with `@here`. The map below holds one position per
//! emitted byte against one text, and a second file cannot be written in it,
//! so an instruction from a library could only report a line of the file that
//! included it or a line number belonging to a file the reader is not looking
//! at. Refusing to emit is the third option, and it costs a library nothing: a
//! macro is how you ship instructions, and its bytes name the invocation.
//!
//! Naming cells is the part that earns the feature. Manual pointer arithmetic
//! is what makes hand-written BrainFuck unmaintainable past a few dozen cells,
//! and it is the one abstraction an expander can provide that a comment
//! cannot: move a variable and the program still works.
//!
//! Tracking the cursor statically runs into loops, which is the whole design
//! problem -- see [`expand`] for the three rules, why `[>]` is allowed to lose
//! the position rather than be refused, and why `@here` is trusted rather than
//! checked. `@here` is the only construct here that can silently produce a
//! wrong program.
//!
//! A `{` after an invocation's arguments hands the macro a body, as its last
//! argument, and the macro expands it with `@name`. A block carries the scope
//! it was written in, so one written inside a macro can name that macro's
//! parameters. `@repeat` is the same shape with a count instead of a name, and
//! is a directive rather than a macro because expansion has no loop of its own
//! to count with.
//!
//! The vocabulary is closed: every directive named above is built, so there is
//! nothing left that a `.bfm` written today could come to mean differently.
//!
//! ## Example
//!
//! ```rust
//! use gyrus::{ExecutionConfigBuilder, interpret_with_io, io::StringIo, parse_with_debug};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let expansion = gyrus_macro::expand("@define CHAR_A 65\n+{CHAR_A}.\n")?;
//! assert_eq!(expansion.brainfuck(), "+++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++.");
//!
//! // Errors from the run below will name the .bfm, not the line above.
//! let (instructions, expanded) = parse_with_debug(expansion.brainfuck())?;
//! let debug_info = expansion.remap(&expanded);
//!
//! let (mut input, mut output) = (StringIo::empty(), StringIo::empty());
//! interpret_with_io(
//!     &instructions,
//!     ExecutionConfigBuilder::new().with_memory_size(30_000).build(),
//!     &mut input,
//!     &mut output,
//!     Some(&debug_info),
//! )?;
//! assert_eq!(output.output_string(), "A");
//! # Ok(())
//! # }
//! ```

mod directive;
mod error;
mod expand;
mod lex;
mod source_map;

use std::path::Path;

/// The extension macro source is written with.
pub const EXTENSION: &str = "bfm";

/// Whether a path names macro source.
///
/// Shared rather than tested wherever it is wanted, because getting it wrong
/// is silent: macro source read as BrainFuck is not an error, it is a
/// *different program* -- every directive becomes a comment and `+{200}`
/// collapses to one `+` -- so it runs, prints something else, and exits zero.
/// Every tool that takes a BrainFuck file needs the same answer, and the
/// comparison ignores case because a filesystem that does not would otherwise
/// make one file behave two ways depending on how its name was typed.
pub fn is_macro_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(EXTENSION))
}

/// Either a BrainFuck error or a macro error.
///
/// The two are separate types on purpose -- `gyrus` knows nothing about macros
/// -- and a program that handles both needs somewhere to put them. This crate
/// is the only one that can name both, so it is that somewhere. Without it a
/// binary reaches for `exit` from inside a function whose signature promises
/// to return its failures, which both of them did.
#[derive(Debug)]
pub enum ProgramError {
    Bf(gyrus::BfError),
    /// A macro error, with the file and text its caret is drawn against.
    Macro(MacroFailure),
}

impl From<gyrus::BfError> for ProgramError {
    fn from(error: gyrus::BfError) -> Self {
        ProgramError::Bf(error)
    }
}

impl From<MacroFailure> for ProgramError {
    fn from(failure: MacroFailure) -> Self {
        ProgramError::Macro(failure)
    }
}

impl ProgramError {
    /// The message to print: a macro error rendered against the macro source
    /// with a caret, a BrainFuck error with its hint and its cause.
    pub fn report(&self) -> String {
        match self {
            ProgramError::Bf(error) => error.format_detailed(),
            ProgramError::Macro(failure) => failure.report(),
        }
    }
}

pub use error::{Kind, MacroError, MacroFailure, StrayKind, Wanted};
pub use expand::{INCLUDE_DEPTH_LIMIT, REPEAT_LIMIT, expand, expand_at};
pub use source_map::Expansion;

#[cfg(test)]
mod tests {
    /// The tour of the language at the top of this file, expanded.
    ///
    /// Read out of the source rather than repeated here: a copy would be
    /// checked and the documentation would not. It had drifted -- `@here`
    /// inside a record left the cursor record-relative, so the `@to` two lines
    /// below it could not resolve, and the crate's own front page was a
    /// program the crate rejects.
    #[test]
    fn the_documented_example_expands() {
        let source = include_str!("lib.rs");
        let start = source
            .find("//! ## What it understands today")
            .expect("the section is still there");
        let block: String = source[start..]
            .lines()
            .skip_while(|line| !line.starts_with("//! ```"))
            .skip(1)
            .take_while(|line| !line.starts_with("//! ```"))
            .map(|line| format!("{}\n", line.strip_prefix("//! ").unwrap_or("")))
            .collect();
        assert!(block.contains("@macro bump"), "the block was not found");

        if let Err(error) = crate::expand(&block) {
            panic!("{}", error.format_with_source(&block));
        }
    }
}
