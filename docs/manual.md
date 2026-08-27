# gyrus User Manual

A path through the tools, organised by what you are trying to do rather than by
what each flag is called. Every section is short and points at the reference
page that has the detail.

Back to the [README](../README.md).

## What you have

Four binaries, from `cargo build --release`:

| | For |
|---|---|
| `gyrus` | Running a program |
| `gyrus-debug` | Running a program one instruction at a time, with the tape in view |
| `gyrus-tool` | Working on the source: validate, minify, view, inspect, generate |
| `gyrus-tutorial` | Learning BrainFuck |

`gyrus` and `gyrus-debug` take the same runtime settings, so anything you learn
about one applies to the other.

## Run a program

```bash
gyrus programs/basic/hello_world.bf
```

That is the whole interface for the common case: the program's output goes to
stdout, its input comes from stdin, and nothing else is printed. Add `--verbose`
for statistics and runtime warnings afterwards, or `--quiet` to suppress the
warnings you would otherwise get from a program that wraps cells (most of them).

## Run a macro program

```bash
gyrus programs/macros/records.bfm
```

A `.bfm` is macro source: named cells, named record fields, constants, macros,
conditionals and `@include`, which the expander turns into ordinary BrainFuck
before running it. It
behaves like any other program, with one difference that matters — a runtime
error reports the line and column of the `.bfm`, not of the expansion nobody
wrote.

To see the BrainFuck rather than run it:

```bash
gyrus-tool expand programs/macros/records.bfm
gyrus-tool expand prog.bfm -o prog.bf     # and then treat it as any other .bf
```

Everything else in `gyrus-tool` takes BrainFuck, and refuses a `.bfm` rather
than reading it as one — which would not fail, it would quietly be a different
program.

[The macro preprocessor](architecture.md#the-macro-expander-cratesgyrus-macro)
describes what the language is; `programs/macros/` has seventeen programs
written in it, and four libraries in `lib/`. Two are worth reading: `99bottles.bfm` is
199 lines that print 11,354 bytes, byte for byte what the hand-written
`programs/third-party/advanced/99beer.bf` prints; and `factor.bfm` finds the
prime factors of a number too big for a cell, using arithmetic that is a
library rather than a language feature.

## Pick an execution mode

There are four, and the default is right until it is not:

| Mode | Use it when |
|---|---|
| *(default)* | Always, unless one of the rows below applies |
| `--jit` | The program runs for more than a few hundred milliseconds. Compiling costs tens of milliseconds, so it loses on short programs and wins big on long ones — 3.5× on mandelbrot |
| `--debug` | You want a line and a column on a runtime error. Much slower: it keeps the AST and a source location for every instruction |
| `--trace` | You want to know which loop the time is going into. Implies `--debug`, and prints a heatmap at the end |

All four produce the same output and the same errors. That is held by a
differential test suite rather than by intention — see
[Testing](testing.md#differential-testing-is-the-backbone).

**A `.bfm` defaults to `--debug` instead**, because it exists so that errors
point back at macro source and the optimized interpreter is the one engine that
cannot name a source position. [Usage](usage.md#running-a-macro-program) has
the rest.

[Execution models](execution-models.md#the-jit) has the JIT's details.

## When it will not parse

Brackets. It is essentially always brackets, and gyrus reports **all** of them
in one pass with a line, a column, and the source:

```bash
gyrus programs/errors/unmatched_bracket.bf
```

You do not need `--debug` for this — parse errors always carry their location.
[Errors and diagnostics](errors.md) covers the rest.

## When it fails at run time

Run it again with `--debug` and you get the line and column of the instruction
that failed, plus the loop it was inside:

```bash
gyrus --debug program.bf
```

`--jit` also reports source locations, at no run-time cost, so on a long program
`--jit` is often the better way to find a runtime failure than `--debug`.

## When it runs, but the answer is wrong

This is the hard case, because nothing has failed. Two tools, in order:

**First, make the failure loud.** Most silent wrongness in BrainFuck is
arithmetic leaving the range you assumed. `--cell-model checked` turns that from
a wrap into an error with a location:

```bash
gyrus --debug --cell-model checked program.bf
```

If your program is *supposed* to wrap, this will fire on the wrap and you have
learned nothing; if it is not, you have just found the bug.

**Then, watch it happen.** `gyrus-debug` puts the source, the tape, the output,
and any cells you are watching on screen together:

```bash
gyrus-debug program.bf
gyrus-debug program.bf --break 12:5      # stop at line 12, column 5
gyrus-debug program.bf --cell-model checked
```

Breakpoints are characters, not lines, because a BrainFuck program is often one
line of a hundred instructions. You can also put them in the source, where they
survive being committed — a `@` is a breakpoint, and it means nothing to any
other BrainFuck implementation:

```
+++@[->+<]        stops at the [
```

**When you know the symptom but not the place**, say what has to happen instead
of where to stop:

```bash
gyrus-debug program.bf --break-output any    # before anything is printed
gyrus-debug program.bf --break-output X      # before an X is printed
gyrus-debug program.bf --break-output '\n'   # before each line ends
```

That is the one a positional breakpoint cannot express, and it is usually the
question you actually have: the output is wrong at some character, and you want
the tape as it was just before that character was produced. Execution stops
*before* the `.`, so the cell holding it is still there to look at.

**Once you are inside**, seven keys cover most of it:

| | |
|---|---|
| `space` | execute one instruction |
| `s` | run in slow motion — `+` and `-` change the speed |
| `c` | run to the next breakpoint, at full speed |
| `n` / `o` | step over a loop / step out of one |
| `b` | breakpoint at the cursor |
| `w` | watch something |
| `?` | everything else |
| `q` | leave |

`w` is the one worth knowing properly, because it takes both kinds of watch, and
you spell them the way the panel displays them back:

```
3          watch cell 3 — shown at every stop, never stops anything
out        stop before anything is printed
out W      stop before a W is printed
out \n     stop before each line ends
```

A bare number is a cell; `out` and whatever follows it is a condition on output.
So `5` watches cell 5 and `out 5` stops on the digit. Everything you set with
`w` lasts as long as the session, and a `●` beside a row marks the ones that
stop rather than only being shown. If a watch never fires, the debugger says so
when the program ends — "never printed" is the answer when you are chasing a
character missing from the output, and it is also how you find out the shell ate
your backslash.

The same conditions are available before you start, as `--break-output`, which
is how you put one in a script or a bug report.

[The debugger](debugger.md) has the full key list and the rest.

## When it is slow

```bash
gyrus --trace program.bf
```

`--trace` attributes execution to loops and prints the hot ones. Almost always,
one loop is most of the runtime. Once you know which, `--jit` is the cheap fix
and rewriting that loop is the real one.

[Performance](performance.md) records what has already been tried and measured
here, including the optimizations that did not pay — worth reading before
attempting one.

## When it does not stop

You cannot tell in general whether it ever will, so put a bound on it:

```bash
gyrus --max-steps 10000000 program.bf
gyrus --timeout 5000 program.bf
```

Both report where the program was when the limit hit. Before running it at all,
`gyrus-tool validate` catches the loops whose non-termination is visible
statically:

```bash
gyrus-tool validate program.bf
gyrus-tool validate program.bf --strict     # non-zero exit for CI
```

## Giving it input

`gyrus` reads stdin, so pipes and here-strings work as usual:

```bash
echo "hello" | gyrus programs/third-party/utilities/cat.bf
```

What happens when the input runs out is a choice, not a standard, and programs
disagree about it. If a program loops forever after consuming its input, this is
the first thing to try:

```bash
gyrus --eof-behavior neg-one program.bf     # zero, neg-one, no-change, error
```

Under the debugger the keyboard belongs to the interface, so input is queued
instead — `--input TEXT`, `--input-file FILE`, or `i` while it is running.
`--input` adds the trailing newline `echo` would have, which most programs
reading a number need; `--input-file` is byte-exact for when they must not have
one. When a `,` is reached with nothing queued the debugger stops and says
`needs input` rather than quietly taking the EOF branch.

## Matching another interpreter

Three settings, independent of each other, cover most dialect differences:

```bash
gyrus --memory-model unbounded --cell-model wrapping --eof-behavior zero program.bf
```

- **Memory model**: `fixed` (bounds-checked, 30,000 cells) or `unbounded` (grows
  to a limit).
- **Cell model**: `wrapping` (standard) or `checked` (errors instead).
- **EOF behavior**: what `,` does with no input left.

Getting a borrowed program to work is usually one of these. [Memory, cells, and
EOF](execution-models.md) explains what each one changes.

## Working on the source

```bash
gyrus-tool view program.bf --line-numbers    # syntax-highlighted, with nesting
gyrus-tool validate program.bf               # static warnings
gyrus-tool minify program.bf                 # strip comments and whitespace
gyrus-tool optimize program.bf               # what the optimizer did, visually
gyrus-tool debug-info program.bf             # instruction-to-source mapping
gyrus-tool compile "Hello"                   # a program that prints that string
gyrus-tool generate --length 200             # a random program, for fuzzing
```

[Development tools](tooling.md) covers each in full.

## Learning the language

```bash
gyrus-tutorial
gyrus-tutorial --list
gyrus-tutorial --lesson 3
```

Thirteen lessons from `+` to the halting problem. Every run of your program is
recorded, so you can step *backwards* through a loop — which is the thing that
makes `[->+<]` legible. [The tutorial](tutorial.md) has the key list.

It is a reasonable place to start even if you know BrainFuck, because lessons 4,
5 and 11 name the shapes gyrus's optimizer recognises, which is what
[performance](performance.md) is about.

## Using gyrus from Rust

The library is not on crates.io by decision; take it as a path or git
dependency. The examples are the fastest way in:

```bash
cargo run --example basic_usage
cargo run --example memory_models
cargo run --example custom_io
```

`crates/gyrus/examples/` has the rest. [Architecture](architecture.md) explains
how the pieces fit together, including the hook system that both terminal tools
are built on.

## Everything else

| | |
|---|---|
| [Usage](usage.md) | Every flag, in full |
| [Errors and diagnostics](errors.md) | What gyrus reports when a program is wrong |
| [Memory, cells, and EOF](execution-models.md) | The three execution knobs, and the JIT |
| [Development tools](tooling.md) | `gyrus-tool`, subcommand by subcommand |
| [The debugger](debugger.md) | `gyrus-debug` in detail |
| [The tutorial](tutorial.md) | `gyrus-tutorial` in detail |
| [Performance](performance.md) | How it got fast, and what did not work |
| [Architecture](architecture.md) | How the pieces fit together |
| [Development](development.md) | Building, testing, and the gates |
