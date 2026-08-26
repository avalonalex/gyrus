# Changelog

gyrus was a private repository until 0.3.0, and no earlier version was ever
published or tagged. This file is a record of how the project got here rather
than an upgrade guide — there is nobody to upgrade. Versions are the ones the
manifests carried at the time.

The format loosely follows [Keep a Changelog](https://keepachangelog.com/).

## Unreleased

The two terminal interfaces the hook system was built for.

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

### Changed

- **`docs/architecture.md` stopped predicting the debugger and started
  describing it.** The two things such a debugger would still want are named
  there: a `HookDecision` that substitutes an instruction, and any way to write
  to the tape from a hook.
- **`scripts/check-readme-commands.py` covers the new binaries**, so their
  documented flags rot no more quietly than the others'.

### Removed

- **`PRD/tui_debugger_and_tutorial.md`**, 1,343 lines, deleted rather than
  archived now that the thing it designed exists. That is the rule the
  directory runs on.

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
