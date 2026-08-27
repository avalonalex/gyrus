# Testing

How gyrus is tested, and where each kind of test lives.

Back to the [README](../README.md). For running the suite and the rest of the
development loop, see [Development](development.md).

Deliberately absent: test counts, coverage percentages, and status tables. They
change with every commit, and a stale number is worse than no number — this
file used to open with "136 total tests" long after there were far more. Run
`cargo test --workspace` for the current picture.

## The shape of the suite

| Where | What it covers |
|---|---|
| `crates/gyrus/src/**` | Unit tests beside the code they test, including `interpreter/tests.rs` |
| `crates/gyrus/src/test_utils.rs` | Helpers shared by those tests — `#[cfg(test)]` and private, so it is not API |
| `crates/gyrus/tests/program_corpus.rs` | The manifest's cases, run end to end through the tree-walker |
| `crates/gyrus/tests/generated_differential.rs` | Optimizer against the tree-walker on generated programs, and compiled strings against their known output |
| `crates/gyrus/tests/property_debug_symbols.rs` | Proptest over debug-symbol invariants |
| `crates/gyrus-macro/src/**` | The expander: expansion, the origin map, the loop rules, and located errors |
| `crates/gyrus-macro/tests/oracle.rs` | Generated macro programs whose answer is computed in Rust |
| `crates/gyrus-macro/tests/round_trip.rs` | The bundled `.bfm` programs, expanded and run |
| `crates/gyrus-macro/tests/source_locations.rs` | That a runtime error names the `.bfm` line somebody wrote |
| `crates/gyrus-corpus/` | The manifest, parsed once and shared by both corpus suites |
| `crates/gyrus-jit/tests/corpus.rs` | The same corpus under the JIT, driven from the manifest |
| `crates/gyrus-jit/tests/differential.rs` | JIT against the optimized interpreter on the bundled programs |
| `crates/gyrus-jit/tests/generated.rs` | JIT against the interpreter on generated programs, every configuration |
| `crates/gyrus-tui/src/**` | Widgets rendered into ratatui's `TestBackend` and read back as text, plus the scroll and layout arithmetic |
| `crates/gyrus-debug/src/**` | The rule for when execution stops, and the map between source positions and instruction indices |
| `crates/gyrus-tutorial/src/**` | The editor, and three tests that hold the lesson table together |
| `crates/gyrus/benches/` | Criterion micro-benchmarks for the interpreter and the parser |
| `scripts/` | Checks that guard claims the code cannot make on its own |

## Differential testing is the backbone

gyrus has four execution paths — tree-walking, optimized, JIT, and tracing —
that must agree. Almost nothing else in the project can be checked by looking
at it; an optimizer that is subtly wrong still produces plausible output. So
the engines are held against each other rather than against expectations
written by hand:

- **`generated_differential.rs`** (in `gyrus`) runs the optimizer against the
  tree-walker, which executes the AST as written and is therefore the
  reference. It needs no JIT, which is the point: the optimizer lives in
  `gyrus`, so `cargo test -p gyrus` can falsify a fold on its own.
- **`differential.rs`** runs the JIT and the optimized interpreter on the same
  source and input and requires the same bytes out, and the same error where
  there is one. The optimized interpreter is in turn held to the tree-walker,
  so agreement chains back to the simplest implementation.
- **`generated.rs`** does the same over randomly generated programs — 400 seeds
  under both memory models and both cell models. Step budgets are not
  comparable between engines (optimized instructions on one side, loop
  iterations on the other), so a run the interpreter cannot finish inside its
  budget is skipped rather than compared; every other outcome, including *which*
  error at *what* position, has to match.
- **`scripts/benchmark.sh`** diffs every timed run against a golden output in
  `benchmarks/expected/` and, for the fast programs, checks the optimized and
  `--debug` interpreters byte for byte. This makes the benchmark script a
  differential test that happens to also report timings: a number that improves
  while the output moves is a bug, and the script fails rather than printing it.
  Re-record with `--record` only after confirming the new output is correct.

## The program corpus

`programs/` holds real BrainFuck: Hello World and friends in `basic/`,
deliberate failures in `errors/`, programs that trigger a specific warning in
`warnings/`, runtime edge cases (EOF handling, deep nesting, loops that never
end) in `tests/`, a debug-symbol demonstration in `debug/`, and a borrowed
collection in `third-party/` — mandelbrot, hanoi, a quine, factor, rot13, life,
and the utilities, each credited in
[`third-party/CREDITS.md`](../programs/third-party/CREDITS.md).

`programs/test_manifest.toml` declares what each case should do: input,
expected output, expected exit, and any configuration the program needs
(memory size, step limit, timeout, EOF behavior).

Both corpus suites read it through the `gyrus-corpus` crate, so adding a case
to the manifest gives the tree-walker, the optimized interpreter, and the JIT a
test at once. That sharing is deliberate: the two suites used to keep separate
ideas of the corpus, with the tree-walker's hand-written to *mirror* the
manifest, and the mirror drifted in both directions — eleven programs ended up
tested by one engine and not the other.

Two things the manifest cannot express as a plain expected output:

- **Programs that never terminate on purpose** (rot13, fibonacci) declare
  `expected_output_prefix` and an expected `limit` error. The step budget is the
  Ctrl-C a human would type.
- **Runs stopped by a limit are not compared byte-for-byte across engines.**
  `max_steps` counts optimized instructions in the interpreter and loop
  iterations in the JIT, so the two stop at different points in a program that
  emits forever — rot13 under a 100,000-step budget leaves 32,897 trailing NULs
  on one side and 3,146 on the other, after identical real output. The prefix is
  what is comparable, and both engines are held to it.

Programs with binary output are compared as raw bytes. Everything runs through
`StringIo` rather than real stdin/stdout, so the suite is fast and
deterministic, and a program's input is just a string in the manifest.

A success case must say what it produces — `expected_output`,
`expected_output_prefix`, or `output_is_source` for the quine, whose output is
its own source. A case that only asserts it exited cleanly is a program nobody
is checking, which is what `factor` and `collatz` were: both ran, neither had
its answer looked at.

## Property-based tests

Proptest is used where a property should hold for *every* program rather than
for a chosen few. The parser carries most of them (parsing never panics on
arbitrary input, valid programs always parse, parsing is deterministic,
balanced brackets always parse, comments never change validity), with more over
codegen and the interpreter, and `property_debug_symbols.rs` covering the
instruction-to-source mapping.

```bash
cargo test proptest                       # just the property tests
PROPTEST_CASES=1000 cargo test proptest   # more cases than the default 100
```

A failing case is minimized by proptest and written to a regressions file —
`crates/gyrus/proptest-regressions/` for the in-module tests, alongside the test
for the integration ones. Those files are committed, which is the point: the
shrunk counterexample becomes a permanent regression test.

## Generated programs

`gyrus::random` generates BrainFuck for fuzzing and for the differential
harness. It is a real feature rather than a test helper, so it sits behind the
off-by-default `random` feature — the crate's only optional dependency:

```bash
cargo test -p gyrus --features random     # exercise the generator
cargo run -p gyrus-tool -- generate       # the same generator, from the CLI
```

It produces two kinds of program: uniformly random instruction soup, which is
good at finding crashes, and *idiomatic* programs built from recognizable
patterns (clear loops, copies, multiplies, scans), which are good at finding
optimizer bugs because they contain the shapes the optimizer rewrites.

**A generator can only falsify what it can express.** The idiomatic mode emits
a clear followed by a run of 200-315 `+`, for one specific reason. Run fusion
caps an `Add` at 255, so a longer run becomes `Add(255)` then `Add(rest)` —
after the clear, `Set(255)` followed by `Add(rest)`. Folding those two together
is valid only while the sum stays under 256; past that, under checked cells,
the overflow is the thing the program existed to report. That guard was once
wrong, and no number of seeds could catch it, because every other fragment
writes single-digit values and no generated program ever came near a cell
boundary. A review found it instead. With the boundary fragment, reintroducing
the same bug fails the suite in under two seconds.

Worth remembering when adding a fold: ask whether the generator can produce the
shape that would prove it wrong.

## An oracle, not just agreement

Every differential above proves the engines *agree*. Agreement is not
correctness — a fold wrong in the same way on both sides passes all of them.

There are two oracles, and they close the gap from different directions.

`compile_string` turns a string into a BrainFuck program that prints it, so the
right answer is known by construction rather than by asking another engine, and
`compiled_programs_print_the_string_they_were_built_from` checks it. The
compiled programs are worth running for their shape too: codegen builds values
with multiply loops and clears, exactly what the optimizer folds.

`crates/gyrus-macro/tests/oracle.rs` generalises that from strings to programs,
which is the reason the macro preprocessor was worth building rather than
something it happens to allow. The same computation is written twice, in two
languages that share no code: a handful of operations — set, clear, add, copy,
multiply, print — applied to a `[u8]` in Rust, and applied to a BrainFuck tape
by a macro library written in `.bfm`. If the expander, the parser, the
optimizer or either interpreter is wrong, the two answers differ, and nothing
in it asks another engine what it thinks.

Two properties it holds itself to, both from lessons recorded below:

- **A program that prints nothing proves nothing**, so every generated program
  ends by printing every cell, and the check refuses an empty expectation.
- **A generator can only falsify what it can express**, so a test insists that
  the generated programs reach a cell boundary — 35 of 64 seeds wrap today, and
  the assertion fails below 16. Without it, a change to the weights could
  quietly stop them ever wrapping while the suite stayed green.

Two properties of codegen the test has to respect, both real rather than
workarounds:

- **It targets wrapping cells.** Its table reaches 255 by decrementing a zero
  cell, so a compiled program raises a checked-cell underflow at its first
  instruction. The oracle runs under wrapping only.
- **It walks rightwards** as it builds, so it needs a realistic tape rather
  than the deliberately tiny one the differentials use to provoke boundary
  errors.

## Benchmarks

```bash
cargo bench                          # criterion, HTML reports in target/criterion/
cargo bench --bench interpreter
cargo bench --bench parser
cargo bench -- simple_arithmetic     # one benchmark by name

scripts/benchmark.sh                 # end-to-end timings, output verified
scripts/benchmark.sh --full          # include the slow --debug runs
scripts/benchmark.sh --profile PROG  # loop profile via --trace
```

The criterion suites cover arithmetic, nested loops, pointer movement, I/O,
Hello World, hanoi, and mandelbrot for the interpreter; simple, nested, long,
and comment-heavy sources for the parser. `hanoi` and `mandelbrot` are embedded
via `include_str!` so the benchmark binary does not depend on the working
directory.

Micro-benchmarks and `benchmark.sh` answer different questions: criterion tells
you whether a function got slower, `benchmark.sh` tells you whether a program
got slower *and still prints the right thing*.

## Checks that guard claims

Some facts about this repository are claims nobody exercises day to day, so they
rot silently. Each of these has been wrong at least once, which is why it is a
script now rather than a good intention:

```bash
scripts/check-msrv.sh              # the workspace builds on its declared MSRV
scripts/check-readme-commands.py   # every flag the docs use really exists
scripts/check-doc-links.py         # every relative Markdown link resolves
scripts/check-examples.sh          # every example runs, not just compiles
scripts/check-tape-access.py       # the tape is indexed only where the contract is enforced
scripts/check-bfm-pseudocode.py    # every .bfm with a loop says what the loop is for
scripts/check-mandelbrot-claims.py # the measurements the macro design rests on
```

`check-examples.sh` runs each example rather than only building it, because
building is not enough: when `MemoryAddress` became signed,
`hooks_execution_tracer` still compiled and panicked on its first instruction,
and nothing noticed because nothing ran it.

`check-readme-commands.py` needs `cargo build --release --workspace` first.

When adding a claim to the docs, ask whether a script could check it. If it
could, write the script — an unexecuted claim is one that will eventually be
false.

## Adding a test

- **A language or interpreter behavior**: a unit test beside the code, using
  the helpers in `test_utils.rs` (`run_bf`, `run_bf_expect_ok`,
  `run_bf_expect_err`, `assert_bf_equivalent`, and the ready-made configs).
- **A whole program**: add it under `programs/`, describe it in
  `test_manifest.toml`, and add the matching case to `program_corpus.rs`.
- **Something that should hold for every program**: a proptest, not fifty
  hand-written cases.
- **An optimizer or JIT change**: nothing hand-written is as good as the
  differential harness. If a new pattern is recognized, make sure the generator
  can produce it, so `generated.rs` exercises it on every run.
- **A widget**: render it into a `TestBackend` and assert on the text that comes
  back. Asserting on the buffer as strings catches the things that actually go
  wrong — a marker in the wrong column, a title that no longer fits — and
  ignores styling, which no assertion should be pinned to.
- **A tutorial lesson**: nothing, if you are only adding to the table in
  `lesson.rs`. The three tests there already cover every entry: the answer must
  satisfy the check, the starter must parse and run, and the starter must not
  already be the answer.
