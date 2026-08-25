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
| `crates/gyrus/tests/program_corpus.rs` | Real BrainFuck programs, run end to end through the tree-walker |
| `crates/gyrus/tests/property_debug_symbols.rs` | Proptest over debug-symbol invariants |
| `crates/gyrus-jit/tests/corpus.rs` | The same corpus under the JIT, driven from the manifest |
| `crates/gyrus-jit/tests/differential.rs` | JIT against the optimized interpreter on the bundled programs |
| `crates/gyrus-jit/tests/generated.rs` | JIT against the interpreter on generated programs, every configuration |
| `crates/gyrus/benches/` | Criterion micro-benchmarks for the interpreter and the parser |
| `scripts/` | Checks that guard claims the code cannot make on its own |

## Differential testing is the backbone

gyrus has four execution paths — tree-walking, optimized, JIT, and tracing —
that must agree. Almost nothing else in the project can be checked by looking
at it; an optimizer that is subtly wrong still produces plausible output. So
the engines are held against each other rather than against expectations
written by hand:

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

**One caveat worth knowing before you add a case.** The JIT's `corpus.rs`
parses the manifest and runs what it finds there. `program_corpus.rs` — the
tree-walker's — does not: its cases are hand-written to *mirror* the manifest.
The two can drift, and adding a manifest entry does not automatically give the
tree-walker a test. Add both until that is fixed.

Programs that never terminate on purpose (rot13, fibonacci) are tested by
giving them a step limit and asserting on a prefix of the output — the limit
stands in for the Ctrl-C a human would type. Programs with binary output are
compared as raw bytes. Everything runs through `StringIo` rather than real
stdin/stdout, so the suite is fast and deterministic.

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
