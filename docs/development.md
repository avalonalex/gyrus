# Development

Building, testing, benchmarking, and the checks that keep this repository's
claims honest.

Back to the [README](../README.md).

## Development

### Checks

```bash
cargo test --workspace                # the full suite
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check

scripts/check-msrv.sh                 # builds on the MSRV declared in Cargo.toml
scripts/check-readme-commands.py      # every flag documented here really exists

scripts/benchmark.sh                  # time the interpreter, verify output
scripts/benchmark.sh --profile PROG   # execution profile via --trace
cargo bench                           # criterion micro-benchmarks
```

The last two guard claims that rot quietly: the declared MSRV, and this file's
own command lines. Both were wrong at some point, which is why they are scripts
now. `check-readme-commands.py` needs `cargo build --release --workspace` first.

Dependencies are declared at breaking-change granularity (`"2"`, not
`"2.0.17"`), so `cargo update` picks up compatible upgrades on its own and
`Cargo.lock` records exactly what is in use.

### Running Tests

gyrus has a comprehensive testing infrastructure: unit tests, integration tests
over a corpus of real programs, property-based tests, and benchmarks.

#### Run All Tests

```bash
# Run all unit tests
cargo test

# Run with output
cargo test -- --nocapture
```

#### Run Property-Based Tests

Property-based tests use [proptest](https://github.com/proptest-rs/proptest) to verify properties hold across thousands of randomly generated inputs:

```bash
# Run only property tests
cargo test proptest

# Run property tests with more cases (default is 100)
PROPTEST_CASES=1000 cargo test proptest
```

**What property tests verify:**
- Parsing never panics on any input
- Valid BrainFuck programs always parse successfully
- Parsing is deterministic (same input = same output)
- Balanced brackets always parse correctly
- Comments don't affect program validity

#### Run Benchmarks

Benchmarks use [criterion](https://github.com/bheisler/criterion.rs) to measure performance with statistical analysis:

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark suite
cargo bench --bench interpreter
cargo bench --bench parser

# Run specific benchmark
cargo bench -- simple_arithmetic
```

**Benchmark suites:**
- **Interpreter benchmarks**: Arithmetic, loops, pointer movement, I/O, Hello World
- **Parser benchmarks**: Simple programs, nested loops, long programs, comments

Benchmark results are saved in `target/criterion/` with detailed HTML reports including:
- Performance graphs
- Regression analysis
- Statistical comparisons

**View HTML reports:**
```bash
# After running benchmarks
open target/criterion/report/index.html
```

#### Test Organization

```
crates/gyrus/
├── src/
│   ├── test_utils.rs        # Test helper functions
│   ├── parser.rs            # Unit tests + property tests
│   ├── interpreter.rs       # Unit tests
│   └── ...
└── benches/
    ├── interpreter.rs       # Interpreter benchmarks
    └── parser.rs            # Parser benchmarks
```

### Development Build

```bash
cargo build
```

### Running with Cargo

```bash
cargo run -- path/to/your/program.bf
```

## Testing strategy

See [Testing](testing.md) for coverage goals, property-based testing, and the
program corpus.

## Checks that guard claims

Some facts about this repository are claims nobody exercises day to day, so
they rot silently. Each of these has been wrong at least once, which is why it
is now a script rather than a habit:

```bash
scripts/check-msrv.sh              # the workspace builds on its declared MSRV
scripts/check-readme-commands.py   # every flag the docs use really exists
scripts/check-doc-links.py         # every relative link resolves
scripts/benchmark.sh               # output still matches benchmarks/expected/
```

When adding a claim to the docs, ask whether a script could check it. If it
could, write the script: an unexecuted claim is one that will eventually be
false.
