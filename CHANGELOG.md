# Changelog

gyrus was a private repository until 0.3.0, and no earlier version was ever
published or tagged. This file is a record of how the project got here rather
than an upgrade guide — there is nobody to upgrade. Versions are the ones the
manifests carried at the time.

The format loosely follows [Keep a Changelog](https://keepachangelog.com/).

## 0.4.0 — 2026-08-26

The two terminal interfaces the hook system was built for, and a debugger that
learned to stop on what a program *does* rather than only on where it is.

### Added

- **A terminal debugger** (`gyrus-debug`). Source, tape, output, and watched
  cells on screen at once; step, step over, step out, continue, run to cursor,
  and restart. It is built entirely on the library's public surface — an
  `ExecutionHook` plus its own `BfInput`/`BfOutput` — and adding it changed
  nothing in `gyrus`, which is the claim `docs/architecture.md` had been making
  about the hook system since 0.2.0.
- **Breakpoints are source positions, not lines.** `hello_world.bf` is one line
  of 106 instructions, so a line breakpoint would be nearly useless on it. The
  cursor snaps to the nearest instruction on its line, and `--break` takes
  `LINE` or `LINE:COLUMN`.
- **"Step over" and "step out" are instruction ranges**, taken from the loop
  metadata the parser already records. Loop depth cannot express either: at a
  `[`, the depth is the same on the iteration about to start as it is once the
  loop has finished, so a depth-based rule stops on the next iteration instead
  of after the loop.
- **The debugger stops at `[` through `after_instruction`.** The interpreter
  runs the `LoopCheck` at the head of a loop body itself and dispatches only
  that hook point for it — before the check executes. Without the special case,
  `[` would be the one instruction a debugger could never stop on.
- **Program input is queued rather than read from the terminal**, which the
  interface is using. A `,` with an empty queue stops execution and says so,
  even mid-`continue`; resuming without supplying anything is how you choose
  EOF. A restart replays what was already consumed.
- **An interactive tutorial** (`gyrus-tutorial`): thirteen lessons, numbered 0
  to 12, from `+` to the halting problem. Each explains an idea, hands over a
  program that demonstrates it, and asks for a variation.
- **The tutorial records every step and scrubs in both directions.** Walking
  backwards through `[->+<]` is what makes it legible, and it is affordable
  because a lesson tape is sixteen cells and runs are capped at 20,000 steps —
  which is the opposite trade from the debugger, and the reason the debugger
  does not offer it.
- **Three tests keep the lessons honest**: every answer must satisfy its own
  lesson's check, every starting program must parse and run, and no starting
  program may already be the answer. The prose and the checks are separate
  pieces of data, and editing one is exactly the change nobody re-runs by hand.
- **A shared widget crate** (`gyrus-tui`): source panel with syntax colors and
  breakpoint markers, hex memory dump with an ASCII sidebar, a labelled tape
  strip for teaching, output, watches, status, help and result overlays, and a
  terminal guard whose panic hook restores the screen. Widgets only — nothing
  in it knows about breakpoints or lesson progress.
- **Breakpoints you can commit.** A `@` in the source is a breakpoint. Every
  BrainFuck implementation ignores every character that is not one of the eight
  commands, so a marked program still runs identically everywhere — a marked
  program is not a special build, and there is a test holding that. Markers are
  read by default, which the design argued against: it wanted an opt-in flag on
  the strength of three bundled programs containing `@`, and never checked
  whether those markers would *fire*. Two of the three cannot, so the real
  misfire rate is one program in fifty-two.
- **Stops on what the program prints**, not only on where it is.
  `--break-output any`, `--break-output W`, `--break-output '\n'`, or `w out W`
  from inside. This is the question a positional breakpoint cannot ask, and
  usually the one you have: the output is wrong at some character, and you want
  the tape as it was just before that character was produced. Execution stops
  *before* the `.`, so the cell holding it is still there.
- **A watch that never fires says so.** It otherwise looks exactly like one
  that is broken — the program runs to the end either way — and those are
  different findings. "Never printed" is the answer when a character is missing
  from the output, and it is also how you discover the shell ate your backslash.
- **Slow motion.** `s` runs at one to fifty instructions a second, `+` and `-`
  moving the ladder during the run or before it starts. It is a speed limit on
  running rather than a way of running, so a paced run-to-cursor is still
  heading for the cursor and the header says so. The gap between instructions
  is spent waiting for a keystroke, not sleeping: at one instruction a second a
  sleep would ignore the keyboard for a second at a time.
- **The debugger says when a program is waiting for input.** A `,` with nothing
  queued reads `needs input` in the header, and the key hints lead with
  `i type input` — a state rather than a status message the next keypress
  clears.
- **A user manual** (`docs/manual.md`), organised by what you are trying to do
  rather than by what each flag is called, and linking onward rather than
  restating.

### Changed

- **`docs/architecture.md` stopped predicting the debugger and started
  describing it.** The two things such a debugger would still want are named
  there: a `HookDecision` that substitutes an instruction, and any way to write
  to the tape from a hook.
- **`scripts/check-readme-commands.py` covers the new binaries**, so their
  documented flags rot no more quietly than the others'.
- **The `*` line-comment rule has one home.** Whether a `+` executes is a fact
  about BrainFuck, and it had three encodings: the highlighter's, the TUI's,
  and the one breakpoint markers needed. It is
  `gyrus::syntax::{CharClass, LineScanner, classify_line}` now. The parser
  keeps its own scan-ahead — restructuring the hot path was not worth the risk
  — but a test holds the two together: every character the classifier calls
  code must be one the parser numbered, and every character it calls comment
  must not be.
- **`--input` means what `echo` means.** It appends a newline when the text
  does not end in one. Programs that read a number read until a newline, so
  without it `--input 1234567` stopped one byte short of starting and looked
  like the flag had been ignored — while the interactive prompt appended one,
  so the two ways of supplying input disagreed. `--input-file` stays
  byte-exact.
- **`w` takes either kind of watch**, spelled the way the panel displays it
  back: `3` for a cell, `out W` for output. A bare number is still a cell, so
  `5` watches cell 5 and `out 5` stops on the digit.
- **The debugger fits an 80×24 terminal.** Status fields drop whole rather than
  clipping — `30000 cells` used to render as `3000`, which does not look
  truncated, it looks like a smaller tape. `? help` and `q quit` are held back
  from that trimming, the key list scrolls and says how many rows are below,
  and its descriptions wrap instead of being cut mid-word.
- **A sixth guarded claim.** The README's debugger screenshot is generated, not
  taken: `scripts/capture-debugger-svg.py` drives the real binary in a pty and
  renders the bytes it writes to an SVG — text in, text out, no binary blob —
  and `--check` fails in CI when it drifts. Panel titles, the status row and
  the key hints have each changed since, and a stale image would still have
  looked plausible. It has caught three real drifts already.

### Removed

- **`PRD/tui_debugger_and_tutorial.md`**, 1,343 lines, and
  **`PRD/source-breakpoint-markers.md`**, 194 more — deleted rather than
  archived now that the things they designed exist. That is the rule the
  directory runs on, and it has a second effect worth naming: a shipped design
  is often wrong by the time the code exists, and deleting it is how that stops
  mattering.

## 0.3.0 — 2026-08-25

First public release. Two threads: a compiler, and getting the repository fit
to be read by someone who was not there.

### Added

- **A Cranelift JIT** (`gyrus --jit`). Compiles the whole optimized program to
  one native function rather than detecting hot loops. Roughly 3x over the
  optimized interpreter on mandelbrot and 1.5x on hanoi — and slower on programs
  that finish in a few milliseconds, because compile time is part of the run.
- **The JIT is held to the interpreters, not merely tested.** It produces the
  same bytes and the same error as the optimized interpreter, which is held in
  turn to the tree-walker. Where debug info exists it reports a *source
  location*, which the optimized interpreter cannot do: every failure site is a
  cold exit block that knows its own instruction.
- **A program generator** (`gyrus::random`, behind the off-by-default `random`
  feature; `gyrus-tool generate`). Two modes: uniform instruction soup, which
  finds crashes, and idiomatic programs built from the shapes the optimizer
  rewrites, which find optimizer bugs. It drives a differential harness over
  400 seeds under every memory and cell model combination.
- **Strided seek folding** — `[>>>>]` becomes one `SeekRight(4)`. mandelbrot
  spends 47% of its executed instructions inside 124 such loops.
- **`Set` fusion** — `[-]+++` becomes a single store. hanoi has 324 of them.
- **Loop rotation**, so a multiply loop folds wherever its decrement sits rather
  than only when it comes first.
- **`MIT LICENSE`**, and `programs/third-party/` with per-file attribution in
  `CREDITS.md`: author, source URL, and license for every borrowed program,
  including the GPL one and Cristofani's terms quoted verbatim.
- **Five scripts that check claims the docs make** — MSRV, documented commands,
  relative links, examples actually running, and tape-access discipline. Each
  exists because the claim it checks had already been wrong once.

### Changed

- **Renamed from `FerrousCortex` to `gyrus`** across crates, binaries, import
  paths, and the repository itself. A gyrus is a fold of the cerebral cortex,
  and reads as *gyre* — a loop.
- **The tape bound is about access, not cursor position.** Reading or writing a
  cell outside the tape is an error; moving the cursor outside it is not.
  Movement became plain arithmetic on a signed cursor and can no longer fail.
  Worth 8–11% on its own.
- **Folding is cell-model-aware.** Multiply loops are not folded under checked
  cells, because the fold would swallow an overflow the program should have
  reported. `OptimizedProgram` carries the model it was built for, and
  `interpret_optimized` rejects a mismatch rather than running folds that do not
  hold.
- **A fused run executes in one step** instead of being re-expanded into
  individual operations at run time.
- **Limit checking is free when no limits are set**, and the clock is sampled
  rather than read on every step.
- **Statistics are opt-in in the JIT** (`Statistics::Cheap` / `Full`), because
  counting costs; `--verbose` asks for them.
- **Bracket errors are returned, not printed.** The parser no longer writes to
  stderr behind the caller's back.
- **Test helpers left the public API** before it became public.
- **Documentation cut from roughly 22,800 lines to 8,500**, and reorganized:
  `docs/` for what exists, `PRD/` for what does not, and a PRD is deleted when
  its feature ships rather than archived. Test counts and line counts were
  removed rather than corrected — they go stale every commit, and a stale
  number is worse than none.
- **Workspace metadata unified.** One version across all crates, shared
  `[workspace.package]` fields, dependencies declared at breaking-change
  granularity, MSRV declared as 1.95 (Cranelift's floor) and verified by script,
  and the development toolchain pinned in `rust-toolchain.toml`.

### Fixed

- **14 bugs across the interpreter, optimizer, and hooks**, found by a
  systematic hunt rather than by symptoms.
- **`fuse_sets` folded past 255 under checked cells**, hiding an overflow that
  should have been reported. No differential test caught it, because every
  engine ran the same optimized program; the generated harness now includes the
  tree-walker as a third, independent engine.
- **A seek is bounded by the cells it walks**, and the cell model is enforced at
  run time rather than assumed at fold time.

### Removed

- **`benchmarks/mandelbrot`**, a 460 KB compiled binary that had been tracked in
  git, along with the Rust "reference implementation" beside it. That program
  claimed to produce the same output as `mandelbrot.bf` and did not: a parameter
  sweep reproduced at most 52% of the reference cells, because Bosman's program
  uses a fixed-point scheme that would have to be reverse engineered out of 11 KB
  of dense BrainFuck to port faithfully. The thing worth benchmarking is
  BrainFuck execution, so `benchmarks/` now holds golden outputs instead.
- **Publishing to crates.io.** `publish = false` is set deliberately: this is a
  learning project, not a dependency anyone should take on, and a registry name
  is a commitment to maintain. Use it as a path or git dependency.

## 0.2.0 — 2025-10-20 (never released)

The version that turned a working interpreter into a library with seams.

### Added

- **A Cargo workspace**: `gyrus` (library), `gyrus-cli` (the `gyrus` binary),
  and `gyrus-tool` (development workflows — minify, validate, view, debug-info,
  optimize, compile, generate).
- **The hook system.** `ExecutionHook` with five hook points, a `HookManager`,
  immutable `HookContext` snapshots, and `HookDecision` for execution control.
  Statistics, limits, warnings, and debug-info tracking were all migrated onto
  it, so instrumentation stopped being special-cased inside the interpreter. It
  costs nothing when no hook is registered, and it is the foundation a debugger
  will attach to.
- **Debug symbols**: an instruction-to-source mapping, so a runtime failure at
  instruction 5042 becomes a line, a column, and a caret.
- **The optimizer** and its `OptimizedProgram` IR — run fusion plus clear, scan,
  and multiply loop recognition, each instruction carrying the source range it
  came from.
- **A profiler** (`--trace`) that attributes execution to loops and prints a
  heatmap of where the time went.
- **Syntax highlighting**, with loop nesting depth, used in error output as well
  as in `gyrus-tool view`.
- **`codegen`**: compiles a string into a BrainFuck program that prints it.
- **Cell models** (`U8Wrapping`, `U8Checked`), orthogonal to the memory models,
  with cell-model-aware validation.
- **An I/O abstraction** (`BfInput` / `BfOutput`, with `StdIo`, `StringIo`,
  `DebugIo`), which is what makes the test suite fast and deterministic.
- **A program corpus** and test manifest, property-based tests, and criterion
  benchmarks.

### Changed

- The interpreter was broken up into `parser`, `interpreter/`, `config/`, and
  `hooks/` rather than one large module.
- Cell overflow and underflow warnings were removed: wrapping is standard
  BrainFuck behavior, and real programs depend on it.

## 0.1.0 — 2025-10-19 (never released)

A BrainFuck interpreter that took error messages seriously from the start.

### Added

- Recursive descent parser with line, column, and offset tracked for every
  position, producing a nested AST rather than a jump table.
- Rich errors: source context, a caret at the offending instruction, and
  structured error types via `thiserror` rather than strings.
- Bracket validation as a pre-parse pass that reports *every* unmatched bracket
  in one go, not the first one.
- Execution limits — step count and wall-clock timeout.
- Memory models: fixed (bounds-checked) and unbounded (grows to a limit).
- Configurable EOF behavior: zero, -1, no change, or error.
- A validation pass for empty loops, extreme nesting, and inefficient idioms.
- Minification, and `*` line comments so programs can be documented without
  accidental instructions.
- Execution statistics: steps, loop iterations, peak memory, I/O counts.
