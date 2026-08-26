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
//! @var counter at 0        * a named cell
//! @var letter  at 1
//!
//! @to counter              * the expander emits the movement
//! +{STEP}                  * repeat an instruction
//!
//! [>]                      * a scan: the expander loses track of the cursor
//! @here letter             * tell it where the scan landed, emitting nothing
//! ```
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
//! `@macro` and the conditionals are designed but not built; they are rejected
//! by name rather than treated as comments, so a `.bfm` written today cannot
//! change meaning when they arrive.
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

mod error;
mod expand;
mod source_map;

pub use error::{Kind, MacroError};
pub use expand::{REPEAT_LIMIT, expand};
pub use source_map::Expansion;
