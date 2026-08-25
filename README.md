# gyrus

[![CI](https://github.com/avalonalex/gyrus/actions/workflows/ci.yml/badge.svg)](https://github.com/avalonalex/gyrus/actions/workflows/ci.yml)

**Production-grade tooling for BrainFuck — interpreter, optimizer, and
Cranelift JIT in Rust.**

When a bracket does not close, you get the line and the column, not a program
that quietly does the wrong thing forever — and you get *every* unbalanced
bracket in one pass, not the first one. When something is slow, `--trace` shows
you which loop is burning 99% of your runtime. When arithmetic leaves the
range you expected, `--cell-model checked` halts and reports that cell 0
overflowed at line 412, column 7, instead of rolling to zero and poisoning
everything downstream. A runaway program dies on a step limit or a wall-clock
timeout rather than hanging the pipeline behind it. Memory is bounds-checked or
grows on demand, and the three semantic knobs — memory model, cell model, EOF
behaviour — are fully independent, so you can match whatever dialect your
program was written against instead of arguing with the interpreter.

The language deserves none of this. That is the fun.

What it is being asked to support: BrainFuck has eight instructions, no
variables, no functions, no types, and no error messages. It was built in 1993
to see how small a compiler could get, and it succeeded so completely that
writing anything in it is closer to a dare than to programming.

A *gyrus* is a fold of the cerebral cortex. The word also reads as *gyre* — a
loop, a spiral — which is most of what a BrainFuck program is.

> **Note**: written with AI assistance (Claude), as a project for learning how
> interpreters, optimizers, and debuggers fit together. Built in earnest, which
> is not the same as battle-tested.

## What a real error message looks like

Here is what BrainFuck traditionally tells you when your brackets do not
balance: nothing. The program runs until it does something unforgivable, and
you find out by watching the memory fill with garbage.

Here is what gyrus says instead — with syntax highlighting, in a real
terminal:

```
$ gyrus programs/errors/unmatched_bracket.bf
Error: Unmatched '[' at line 12, column 1
   10 | >+          * Move and increment
   11 |
   12 | [>+++       * Opening bracket without matching close
      | ^
   13 |             * This will cause a parse error
```

Every bracket error is reported in one pass, not one per run, because finding
your mistakes one at a time is a punishment the language already inflicts
enough of. Runtime errors carry the same context under `--debug`: the parser
keeps a source location for every single instruction, so "cell overflow at
instruction 5042" becomes a line, a column, and a caret.

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
own. Building against another toolchain needs Rust **1.95** or newer — that is
Cranelift's floor, inherited through the JIT. The library on its own needs
1.88, where let-chains stabilized.

## Features

The unreasonable part is that all of this actually works.

**Execution**
- Four execution modes: an optimized interpreter (default), a Cranelift JIT
  (`--jit`) that compiles the same optimized program to native code, a debug
  interpreter that tracks source locations, and a tracing interpreter
  (`--trace`) that profiles execution and prints a heatmap of hot code
- An optimizer that fuses instruction runs and recognizes clear, scan, and
  multiply loops — Hello World compresses 103 instructions to 45
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
| [Changelog](CHANGELOG.md) | What changed, and why, from 0.1 to now |

## Status

**Working**: the parser with full source locations, four execution modes
(optimized, JIT, debug, tracing), the optimizer, the hook system, static
validation, minification, syntax highlighting, and the `gyrus-tool`
subcommands. The JIT is `gyrus --jit program.bf`: the same bytes and the same
errors as the interpreters, with source locations, three times faster on
mandelbrot -- and slower on programs that finish before it has finished
compiling. See [execution models](docs/execution-models.md#the-jit).

**Planned**: a TUI debugger with breakpoints and memory visualization, a REPL,
and an AOT build on the JIT's translator — because if you are going to
over-engineer a BrainFuck implementation, you may as well. Designs live in
[`PRD/`](PRD/).

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

This is a personal learning project, so there is no roadmap to sign up for. Bug
reports and corrections are welcome all the same — particularly if you wrote
one of the programs under `programs/third-party/` and want it credited
differently or removed.

If you found a genuine bug in a BrainFuck interpreter, you have my sincere
respect for how you spent your afternoon.
