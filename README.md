# gyrus

A BrainFuck interpreter, optimizer, and debugger written in Rust.

A *gyrus* is one of the folds of the cerebral cortex. The word also reads as
*gyre* — a loop, a spiral — which is most of what a BrainFuck program is.

> **Note**: this codebase is written with AI assistance (Claude). It is a
> project for learning how interpreters, optimizers, and debuggers fit
> together, not production software.

## Why

BrainFuck gives you eight instructions and no diagnostics: no line numbers, no
variable names, no stack traces. When a program misbehaves, the language offers
nothing to help. gyrus is an attempt to give it everything a real toolchain has.

An unmatched bracket, for instance, is reported where it actually is — with
context, and in colour in a real terminal:

```
$ gyrus programs/errors/unmatched_bracket.bf
Error: Unmatched '[' at line 12, column 1
   10 | >+          * Move and increment
   11 |
   12 | [>+++       * Opening bracket without matching close
      | ^
   13 |             * This will cause a parse error
```

All bracket errors are reported in one pass rather than one per run. Runtime
errors carry the same context when built with `--debug`, because the parser
keeps a source location for every instruction.

## Quick start

```bash
git clone https://github.com/avalonalex/gyrus.git
cd gyrus
cargo build --release

./target/release/gyrus programs/basic/hello_world.bf     # Hello World!
./target/release/gyrus --verbose programs/basic/hello_world.bf
./target/release/gyrus-tool view programs/basic/simple.bf --line-numbers
```

`rust-toolchain.toml` pins the compiler, so `cargo` picks the right one on its
own. Building against another toolchain needs Rust **1.88** or newer (the code
uses let-chains, which stabilized there).

## Features

**Execution**
- Three execution modes: an optimized interpreter (default), a debug
  interpreter that tracks source locations, and a tracing interpreter
  (`--trace`) that profiles execution and prints a heatmap of hot code
- An optimizer that fuses instruction runs and recognizes clear, scan, and
  multiply loops — Hello World compresses 103 instructions to 55
- Memory models: fixed (bounds-checked) or unbounded (grows to a limit)
- Cell models: `wrapping` (standard BrainFuck) or `checked` (errors on
  overflow, for finding arithmetic bugs) — orthogonal to the memory model
- Configurable EOF behavior: zero, -1, no change, or error
- Execution limits by step count and by wall-clock timeout
- A hook system (`ExecutionHook`) with five hook points, the foundation for
  breakpoints, profilers, and tracers — and zero cost when unused

**Diagnostics**
- Every error carries a line, a column, and a syntax-highlighted excerpt
- All bracket mismatches reported in a single pass
- Static validation for empty loops, extreme nesting, and inefficient idioms,
  aware of which cell model you are running
- Execution statistics: steps, loop iterations, peak memory, I/O counts

**Tooling** (`gyrus-tool`)
- `minify` — strip comments and whitespace (94.9% on the bundled
  `line_comments.bf`: 514 bytes to 26)
- `validate` — static analysis warnings
- `view` — syntax-highlighted source with line numbers and nesting depth
- `debug-info` — inspect the instruction-to-source mapping
- `optimize` — show what the optimizer did, visually
- `compile` — turn a string into a BrainFuck program that prints it
- `generate` — random program generation for fuzzing

**Language**
- All eight commands, arbitrarily nested
- `*` line comments, so programs can be documented without accidental
  instructions

## Documentation

| | |
|---|---|
| [Usage](docs/usage.md) | CLI options, execution modes, statistics, the language itself |
| [Errors and diagnostics](docs/errors.md) | What gyrus reports when a program is wrong |
| [Memory, cells, and EOF](docs/execution-models.md) | The three orthogonal execution knobs |
| [Development tools](docs/tooling.md) | `gyrus-tool`: validate, minify, view, inspect |
| [Development](docs/development.md) | Building, testing, benchmarking |
| [Architecture](docs/architecture.md) | How the pieces fit together |

## Status

Working: the parser with full source locations, three execution modes
(optimized, debug, tracing), an optimizer, the hook system, static validation,
minification, syntax highlighting, and the `gyrus-tool` subcommands.

Planned: a TUI debugger with breakpoints and memory visualization, a REPL, and
a Cranelift JIT/AOT backend. Designs for those live in [`PRD/`](PRD/).

## Development

```bash
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check

scripts/check-msrv.sh                 # builds on the MSRV declared in Cargo.toml
scripts/check-readme-commands.py      # every documented flag really exists
scripts/benchmark.sh                  # time the interpreter, verify its output
```

The last three guard claims that rot quietly. See
[Development](docs/development.md) for the rest.

## References

- [The BrainFuck Programming Language](https://www.muppetlabs.com/~breadbox/bf/) - Comprehensive guide and reference by Brian Raiter

## License

The Rust code in this repository is licensed under the [MIT License](LICENSE).

The BrainFuck programs under `programs/third-party/` were written by other
authors and keep their own licenses — including CC BY-SA 4.0 and, in one case,
the GPL. They are aggregated here as a test and benchmark corpus; the MIT
license above covers the interpreter, not those programs. Per-file attribution
is in [`programs/third-party/CREDITS.md`](programs/third-party/CREDITS.md).

## Contributing

This is a personal learning project, so there is no roadmap you can sign up
for — but bug reports and corrections are welcome, especially about the
BrainFuck programs under `programs/third-party/` if you are one of their
authors.
