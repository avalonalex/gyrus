//! The map from expanded BrainFuck back to the `.bfm` that produced it.
//!
//! This is the whole feature. A macro that expands to four thousand characters
//! of BrainFuck and then reports a cell overflow at column 3,847 of the
//! *expansion* is precisely the experience gyrus exists to replace, so the map
//! is built during expansion rather than reconstructed afterwards -- there is
//! no code path here that emits a byte without recording where it came from.
//!
//! # How it reaches the error messages
//!
//! Located runtime errors need no change to `gyrus`. [`DebugInfo`] is a map
//! from step index to source location plus the source text, and all three of
//! `with_source`, `record` and `lookup` are public. So:
//!
//! 1. `parse_with_debug` on the expansion gives step index -> location in the
//!    *expanded* BrainFuck.
//! 2. That location's `offset` indexes [`Expansion::origin`], giving the
//!    location in the `.bfm`.
//! 3. [`Expansion::remap`] writes those into a fresh `DebugInfo` carrying the
//!    `.bfm` text.
//!
//! Runtime errors carry `source_location` and are rendered by
//! `format_with_source(&source)`, which takes the text separately -- so once
//! the table is remapped, every located error names the macro source for free.
//! Loop call stacks come along too, which is not obvious: `DebugTrackingHook`
//! builds them from a plain `debug_info.lookup(index)`.
//!
//! # What a remapped table loses
//!
//! Its *loop metadata*, and this crate cannot put it back. `DebugInfo`
//! exposes `record_loop_metadata`, but `LoopMetadata` lives in a private
//! module and is not re-exported, so no foreign crate can construct one --
//! `loop_count()` on a remapped table is zero.
//!
//! Nothing in the error path reads it, which is why located errors and loop
//! call stacks are unaffected. `gyrus-debug` does read it, for loop
//! navigation, so running a `.bfm` under the debugger will need a change in
//! `gyrus` -- either exporting `LoopMetadata` or, better, a `DebugInfo::remap`
//! that keeps the whole table consistent. That is not the only thing standing
//! between the debugger and `.bfm`: its position map holds one instruction per
//! position, and remapping is many-to-one, since every instruction of `+{65}`
//! shares a column.

use gyrus::{DebugInfo, SourceLocation};

/// Expanded BrainFuck, and where every byte of it came from.
#[derive(Debug, Clone)]
pub struct Expansion {
    source: String,
    brainfuck: String,
    /// One entry per byte of `brainfuck`. The expansion is pure ASCII
    /// BrainFuck, so a byte index and a character index are the same thing --
    /// which matters, because `gyrus`'s parser records `offset` as a character
    /// index.
    origins: Vec<SourceLocation>,
}

impl Expansion {
    pub(crate) fn new(source: String, brainfuck: String, origins: Vec<SourceLocation>) -> Self {
        // Not `debug_assert_eq!`: this is the crate's whole correctness
        // claim, and a release build is the one users run. An emit path added
        // later that forgot its origin would otherwise point every byte past
        // the divergence at a confidently wrong line. It is one comparison per
        // file.
        assert_eq!(
            brainfuck.len(),
            origins.len(),
            "every emitted byte must carry an origin"
        );
        Self {
            source,
            brainfuck,
            origins,
        }
    }

    /// The `.bfm` text this was expanded from.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The expanded program: pure BrainFuck, ready for `gyrus::parse`.
    pub fn brainfuck(&self) -> &str {
        &self.brainfuck
    }

    /// Where the byte at `offset` of the expansion was written in the `.bfm`.
    pub fn origin(&self, offset: usize) -> Option<SourceLocation> {
        self.origins.get(offset).copied()
    }

    /// A [`DebugInfo`] for the expansion, rewritten to name the `.bfm`.
    ///
    /// Pass the table `gyrus::parse_with_debug` produced for
    /// [`Self::brainfuck`]; what comes back has the same step indices pointing
    /// at macro-source positions, and carries the `.bfm` text so that
    /// `format_with_source` prints macro lines.
    ///
    /// Step indices are contiguous from zero -- the parser assigns them with a
    /// counter it increments once per instruction -- so walking `0..len()` is a
    /// complete traversal, and `DebugInfo` exposes no iterator to do it
    /// another way.
    pub fn remap(&self, expanded: &DebugInfo) -> DebugInfo {
        let mut remapped = DebugInfo::with_source(self.source.clone());
        for step in 0..expanded.len() {
            if let Some(location) = expanded.lookup(step)
                && let Some(origin) = self.origin(location.offset)
            {
                remapped.record(step, origin);
            }
        }
        remapped
    }
}
