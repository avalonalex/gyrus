# PRD: Source Breakpoint Markers

**Status**: Not Started
**Last Updated**: 2026-08-26
**Priority**: Low — small, self-contained, and worth doing for the ergonomics

## Summary

Let a character in the source — `@` by default — mark a breakpoint, so that
`gyrus-debug` picks it up at load. Every BrainFuck interpreter ignores every
character that is not one of the eight commands, so a marked program still runs
identically everywhere, including under `gyrus` itself.

The feature is **opt-in**, behind `--markers`, for a reason the corpus makes
plain: three of the fifty-two bundled programs already contain `@`, and none of
those occurrences means "stop here".

## Motivation

`gyrus-debug` has two ways to set a breakpoint today, and both are transient:

- `b` at the cursor, which lives as long as the session does.
- `--break LINE[:COLUMN]`, which needs you to know the line and the column.

Neither survives closing the debugger, and neither travels. A marker in the
source does both:

- **It sits where you are already reading.** Deciding to stop at a particular
  `[` is a thing you conclude while looking at that `[`, not while composing a
  command line about it.
- **It is committed with the program.** "Run the debugger and look at what
  happens at the marker" is a reproducible instruction to a colleague, or to
  yourself in a month.
- **It costs nothing anywhere else.** `gyrus`, `gyrus --jit`, and every other
  BrainFuck implementation treat it as a comment. A marked program is not a
  special build.

`--break 412:7` is the thing this replaces, and the argument against it is that
column 7 of line 412 is a fact about the file that nobody carries in their head.

## Requirements

**Functional**

1. `gyrus-debug --markers FILE` sets a breakpoint at each marker in `FILE`.
2. A marker binds to the **next instruction at or after it**, not the nearest.
3. `--markers=CHAR` picks a different character.
4. Markers inside `*` line comments are ignored.
5. Markers combine with `--break` rather than replacing it.
6. Marker breakpoints are ordinary breakpoints once set: `b` toggles them, `B`
   clears them, and the source panel marks them the same way.
7. A marker with no instruction after it is reported, not silently dropped.
8. `CHAR` may not be one of `> < + - . , [ ]` or `*`.

**Non-functional**

9. Off unless asked for. See [Why opt-in](#why-opt-in-the-corpus-says-so).
10. No effect on any other binary, and no change to what `gyrus` prints for a
    marked program — verified by a test, not asserted.
11. The debugger never writes to the source file.

## Design

### Why opt-in: the corpus says so

The obvious design is to read markers always. The bundled programs say not to.
Three of fifty-two contain `@`, in three different shapes, and **none** of them
would be caught by ignoring `*` line comments:

| Program | Where | What an always-on scan would do |
|---|---|---|
| `calc.bf` | `+@<<<<<<<<<` — three times, in dense code | Three breakpoints in the middle of a working program |
| `char.bf` | `@` alone on the line after the program | A marker with no instruction after it |
| `pi.bf` | An email address inside a `[ … ]` block comment | A breakpoint inside the comment-loop idiom |

`pi.bf` is the instructive one. The comment is not a `*` line comment; it is the
classic idiom of putting prose inside a loop whose cell is zero, so it never
runs. Those characters are *parsed*, and no comment rule excludes them.

So an always-on scan misfires on about 6% of real programs, which is far too
often for a feature whose whole appeal is that you can stop thinking about it.
`--markers` costs one flag and removes the problem entirely.

Ignoring `*` line comments is still worth doing on top, because it is nearly
free and it catches the most likely accident in a *newly written* program —
`* mail me at foo@bar.com` in a header comment.

### Which instruction a marker binds to

The next instruction at or after the marker, scanning forward across lines.

This differs deliberately from `--break LINE:COLUMN`, which snaps to the
*nearest* instruction on that line, before or after. That rule is right for a
cursor, which lands wherever the arrow keys left it. It is wrong for a marker,
which someone typed immediately before the thing they want to stop at:

```
+++@[->+<]        breaks at the [
+++[->+<]@        breaks at whatever follows the loop
+++[->+<]         a marker at the end of the file has nothing to bind to
```

### Which character

`@` by default, `--markers=CHAR` to change it. The corpus argues for `@` over
the obvious alternatives:

- `#` appears in only one bundled program, which is better — but `#` already
  means something else in several BrainFuck implementations, where it prints
  the tape and *continues*. Reusing it for "stop" invites a reader to expect the
  wrong behavior.
- `!` appears in eight bundled programs, and some implementations use it to
  separate a program from its input.

`@` has no established meaning in any dialect worth honoring, and the flag makes
the default a preference rather than a commitment.

### Read-only, in this version

The debugger does not write markers back. `b` toggles a session breakpoint and
the file is untouched.

Writing back is tempting and is the natural second version, but it is a separate
feature with its own problems: inserting a character shifts the position of
every instruction after it, which invalidates the debug symbols the session is
running against. Doing it safely means re-parsing mid-session, and that is more
than this feature is worth. If it is built later, it should be an explicit
"save breakpoints" action, never a side effect of `b`.

### Interaction with the rest of the toolchain

- **`gyrus`, `gyrus --jit`, `--debug`, `--trace`**: no change. Markers are
  comments to the parser and always were.
- **`gyrus-tool minify`**: strips markers, because it strips comments and a
  marker is a comment. This is correct and should be documented rather than
  fixed — `minify`'s contract is minimal BrainFuck source, and a marker is not
  that. The round-trip property (`parse → minify → parse` yields an identical
  AST) is unaffected, since markers never reach the AST.
- **`gyrus-tool validate`**: no new warning by default. A trailing marker with
  nothing after it is reported by the debugger, where it matters.
- **`gyrus-tutorial`**: markers mean nothing there, and the lesson editor should
  not treat them specially.

### Where the code goes

`Program::from_source` (`crates/gyrus-debug/src/program.rs`) already walks the
source once and builds the position-to-index maps both directions. The scan
belongs beside it, producing a `Vec<Position>` that `main.rs` feeds to
`Session::set_breakpoint`. Binding a marker to the next instruction is a
`range((line, column)..).next()` on the `indices` map that is already there.

**This is the moment to move the `*` line-comment rule into the library.** The
rule — `*` starts a comment that runs to the end of the line — is currently
written out three times: in `crates/gyrus/src/parser.rs`, in
`crates/gyrus/src/syntax.rs`, and in `crates/gyrus-tui/src/theme.rs`. The marker
scan needs it too, and a fourth copy of a *language* rule is one too many. It
should become one public function in `gyrus::syntax`, which the other three call.

That refactor is a prerequisite rather than a nice-to-have, and it is the larger
half of this work.

## Implementation Plan

1. **Move the `*` rule into `gyrus::syntax`** as a public per-character
   classifier, and make `SyntaxHighlighter::highlight` and
   `gyrus_tui::classify_line` call it. No behavior change; the existing syntax
   tests are the check.
2. **Scan for markers** in `Program::from_source`, behind a parameter, returning
   the bound positions and the unbindable ones.
3. **Wire up the flag**: `--markers[=CHAR]` on `gyrus-debug`, rejecting the eight
   commands and `*`, defaulting to `@` when given bare.
4. **Report at startup**: the count in the status line, and a warning naming the
   line of any marker with nothing after it.
5. **Document** in `docs/debugger.md`, next to `--break`.

## Success Criteria

- `gyrus-debug --markers programs/…` stops at each marker, in source order.
- A marker binds to the instruction *after* it, shown by a test over the three
  shapes above.
- Markers inside `*` line comments are ignored.
- `--markers` on `calc.bf` sets exactly the three breakpoints its `@`s imply,
  and `char.bf` reports one unbindable marker rather than failing.
- **`gyrus` prints the same bytes for a marked and an unmarked program.** A
  differential test, not a claim — the whole premise is that a marked program is
  not a special build.
- `--no-markers` is unnecessary, because the feature is off by default.

## Dependencies

- Step 1 changes `crates/gyrus/src/syntax.rs`, which nothing else in this design
  touches. It can land on its own and is worth doing regardless.
- Nothing else. No new dependency, no change to the hook system, no change to
  the interpreter.
