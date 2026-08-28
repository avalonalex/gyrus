# Development

Building gyrus, running its gates, and the checks that keep this repository's
claims honest.

Back to the [README](../README.md). For what the test suite actually contains,
see [Testing](testing.md).

## Building

```bash
cargo build                    # whole workspace, debug
cargo build --release          # what you want for anything timed
cargo build -p gyrus           # just the library
cargo run -p gyrus-cli -- programs/basic/hello_world.bf
cargo run -p gyrus-tool -- view programs/basic/simple.bf --line-numbers
cargo run -p gyrus-debug -- programs/basic/simple.bf
cargo run -p gyrus-tutorial
```

The two terminal binaries take over the screen, so `cargo run` on them wants a
real terminal — piping their output somewhere is not useful.

`rust-toolchain.toml` pins the compiler, so `cargo` picks the right one on its
own and local builds cannot drift from CI. That pin is a different fact from
`rust-version` in `Cargo.toml`: the pin is the compiler this repository is
developed and gated against, while `rust-version` is the oldest compiler a
consumer needs. Both are declared, and `scripts/check-msrv.sh` is what keeps the
second one true.

The pin exists because the lint surface genuinely moves — the same tree reported
one clippy warning on 1.93.1 and four on 1.97.1. Since CI gates on
`-D warnings`, an unpinned toolchain turns unrelated changes red for no reason.
Bumping it is a deliberate one-line edit followed by fixing whatever the newer
lints find.

Dependencies are declared at breaking-change granularity (`"2"`, not
`"2.0.17"`), so `cargo update` picks up compatible upgrades on its own and
`Cargo.lock` records exactly what is in use.

## The gates

Everything below has to pass before a change lands:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check

scripts/check-msrv.sh              # the workspace builds on its declared MSRV,
                                   #   and the README badge says the same number
scripts/check-readme-commands.py   # every flag the docs use really exists
scripts/check-doc-links.py         # every relative Markdown link resolves
scripts/check-examples.sh          # every example runs, not just compiles
scripts/check-tape-access.py       # the tape is indexed only where the contract is enforced
scripts/check-bfm-pseudocode.py    # every .bfm with a loop says what the loop is for
scripts/check-macro-language.py    # every example in the .bfm reference expands to what it says
scripts/capture-debugger-svg.py --check   # the README's screenshot is what the debugger draws
```

`check-readme-commands.py` needs `cargo build --release --workspace` first.

The six scripts guard claims that rot quietly, and each of them exists because
the claim it checks was wrong at least once — or, in the screenshot's case,
because it is the kind of claim that would rot in total silence:

- **MSRV** was declared 1.85 by inference from the edition. It was actually 1.88
  — let-chains — and is now 1.95, which is Cranelift's floor. The script reads
  the number out of `Cargo.toml` rather than restating it, so it cannot drift
  from what it checks.
- **Documented commands** included `gyrus --validate` and `gyrus --minify` long
  after both became `gyrus-tool` subcommands. The script extracts every
  documented invocation and checks its flags against clap's `--help`.
- **Doc links** broke when five files moved during a documentation cleanup, two
  of them having been broken beforehand.
- **Examples** are run, not just built. Building is already covered by clippy and
  it is not enough: when `MemoryAddress` became signed,
  `hooks_execution_tracer` still compiled and panicked on its first instruction,
  because nothing ran it.
- **Tape access** enforces the one structural requirement of the tape contract —
  every read and write goes through `VmState::cell`/`cell_at`, because that is
  where the bound lives. A site that genuinely needs a direct index says why
  with a `// tape-access-ok:` note.

- **The debugger screenshot** is generated rather than taken. Panel titles, the
  status row, and the key hints have each changed at least once since the
  debugger was written, and a stale image would still look plausible.
  `capture-debugger-svg.py` drives the real binary in a pty and renders the
  bytes it writes to an SVG — text in, text out, so the result is diffable and
  no binary blob enters the repository.

When adding a claim to the docs, ask whether a script could check it. If it
could, write the script: an unexecuted claim is one that will eventually be
false.

## Tests

```bash
cargo test --workspace                    # everything
cargo test -- --nocapture                 # with output
cargo test proptest                       # just the property tests
PROPTEST_CASES=1000 cargo test proptest   # more cases than the default 100
cargo test -p gyrus --features random     # the program generator
```

The suite is unit tests beside the code, a corpus of real BrainFuck programs run
end to end, property tests over invariants, and a differential harness holding
the JIT, the optimized interpreter, and the tree-walker to identical output.
[Testing](testing.md) covers what each of those does and where to add to them.

## Benchmarks

```bash
scripts/benchmark.sh                 # time each mode, verify output against benchmarks/expected/
scripts/benchmark.sh --full          # include the slow --debug runs
scripts/benchmark.sh --profile PROG  # loop profile via --trace
cargo bench                          # criterion micro-benchmarks
```

`GYRUS_JIT_DUMP=path` writes the emitted bytes to `path` and prints the address
they were mapped at, alongside a `path.srcloc` table of code offset to AST
index. A sampling profiler sees only addresses inside JIT'd code; those two
files are what turn a sample into a BrainFuck construct:

```bash
GYRUS_JIT_DUMP=/tmp/code.bin samply record -s -r 20000 -- \
    ./target/release/gyrus --jit programs/third-party/advanced/mandelbrot.bf
```

Mapping the leaf frames through that table is how the JIT's time was first
attributed by construct -- 49% of mandelbrot is seek loops -- and how it was
established that those loops are latency-bound rather than
instruction-bound. See [Performance](performance.md).

`GYRUS_JIT_DISASM=1` makes `gyrus --jit` print the machine code Cranelift
emitted, on stderr, before running. It is how you find out what a translation
choice actually costs rather than guessing -- and, as often, what it does not
cost: see the shared-exit experiment in
[Performance](performance.md), where
removing a third of the emitted instructions made the program slower.

`benchmark.sh` is a differential test that also reports timings: it diffs every
run against a golden output, so a number that improves while the output moves
fails the script instead of being printed. Only re-record with `--record` after
confirming the new output is correct.

## Documentation

The rules, in short: `docs/` describes what exists, `PRD/` describes what does
not exist yet, and a PRD is deleted when its feature ships rather than archived
— the code and `docs/` describe what was built, and git history keeps the
reasoning. Test counts, line counts, and status tables do not belong in either;
they go stale on every commit, and a stale number is worse than no number.
