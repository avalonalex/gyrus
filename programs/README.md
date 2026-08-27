# BrainFuck Programs

This directory contains BrainFuck programs for testing and demonstrating gyrus functionality.

Programs written for this project live in `basic/`, `tests/`, `errors/`,
`warnings/`, `debug/`, and `macros/`, and are MIT licensed along with the rest
of gyrus.
Programs written by **other authors** live under `third-party/` and keep their
own licenses — see [`third-party/CREDITS.md`](third-party/CREDITS.md) for
per-file attribution.

## Directory Structure

### `basic/` - Introductory Programs

Simple programs demonstrating core BrainFuck features and gyrus capabilities.

- **`hello_world.bf`** - Classic "Hello World!" program
- **`simple.bf`** - Minimal program that prints 'H'
- **`line_comments.bf`** - Demonstrates line comment syntax using `*`
- **`comments_demo.bf`** - Comment usage examples
- **`comments_test.bf`** - Testing comment handling

**Run examples:**
```bash
cargo run -- programs/basic/hello_world.bf
cargo run -- programs/basic/simple.bf
```

### `macros/` - Macro Source (`.bfm`)

Source for the macro preprocessor (`gyrus-macro`), which expands to ordinary
BrainFuck. The extension is `.bfm`, and nothing in this directory is a `.bf`
file.

- **`hello_world.bfm`** - `basic/hello_world.bf` with its counted runs written
  as named constants. It expands to that file character for character, which is
  what makes it a test as well as an example.
- **`variables.bfm`** - named cells and `@to`, around the multiply idiom. Its
  own header makes the case.
- **`macros.bfm`** - the same idiom as a `@macro` with parameters, used three
  times.
- **`arithmetic.bfm`** - `clear`, `set`, `add_to` and `multiply` composed, with
  answers the source does not contain: it says 8 times 9, not 72.
- **`control.bfm`** - a loop opened by one macro and closed by another, which
  is how the design's standard library proposes to write control flow.
- **`scan.bfm`** - `[.>]`, which loses the cursor, and `@here`, which is the
  only way back. Also cells the expander chose and letters written as letters.

- **`records.bfm`** - an array of records walked by a scan, with `@stride` and
  `@field` naming the parts. Only the array's head is a cell; everything inside
  the loop is a field, so the record size is one edit.

- **`compare.bfm`** - the equality test from the
  [esolangs idiom catalogue](https://esolangs.org/wiki/Brainfuck_algorithms),
  transcribed line for line: its notation names cells and means movement by
  juxtaposition, which is what `@var` and `@to` are.

- **`conditional.bfm`** - tracing compiled in or out with `@ifdef`. Removing
  one line takes the marks out of the BrainFuck rather than skipping them at
  run time: 322 instructions become 222. Both numbers are asserted by
  `the_conditional_example_compiles_its_tracing_in_and_out`.

- **`include.bfm`** - a program that is only the program: its vocabulary comes
  from **`lib/idioms.bfm`**, which includes **`lib/ascii.bfm`** in turn. An
  included file cannot emit, which is what keeps every byte's origin inside the
  file being expanded.
- **`99bottles.bfm`** - the one that is a program rather than a demonstration.
  199 lines produce 11,354 bytes of output, byte for byte the same as
  `benchmarks/expected/99beer.txt` -- which is what
  `third-party/advanced/99beer.bf` prints. Two-digit counting with a borrow,
  a number printed without a leading zero, and "bottle" against "bottles":
  three branches in a language that has none, each written once as a named
  macro. It costs 11,556 instructions against that program's 1,762, and the
  gap is honest -- `@say` sets a cell from empty for every character, where a
  hand-written program walks from each character to the next.

- **`factor.bfm`** - the prime factors of 13911, which is 3 times 4637. The
  number does not fit in a cell, so every value here is a pair of cells and
  the arithmetic is **`lib/wide.bfm`** -- a library, not a language feature,
  which is the interesting part: working in numbers wider than a cell was
  written down as something the expander would have to grow. 332 lines expand
  to 30,629 instructions. Run it with `--jit`: it is the first program here
  that does enough work to care which engine runs it, at 0.05 seconds
  optimized against 40 on the tree-walker.

- **`divide.bfm`** - division by a snippet from
  [the esolangs catalogue](https://esolangs.org/wiki/Brainfuck_algorithms),
  pasted in verbatim. **`lib/fast.bfm`** pins the cells beside each other and
  tells the expander where the cursor lands, because that algorithm is a
  pointer walking a fixed workspace and naming cells is exactly what it does
  not do. About a third the cost of the same division built from named idioms
  -- and it cannot yet be used inside a loop, which that file explains.

- **`library.bfm`** - the four idioms nothing else calls: multiply, equal,
  less, swap. An idiom with no caller is a claim about BrainFuck that nothing
  checks, so this is their caller.

- **`primes.bfm`** - the primes below a hundred, by trial division, with the
  pasted-in `@divmod` *inside the loop*. That it can be is the point: a scan
  leaves the movement a loop emits meaningless, so until a body could be
  measured by where it began and ended, an idiom like that one could only
  appear in a straight line.

- **`bignum.bfm`** - add, subtract, multiply and compare on numbers held in
  two cells, from **`lib/wide.bfm`**. The multiply is the naive one, adding
  `b` to nothing `a` times: it costs the *value* rather than the number of
  digits, which is fine for checking an answer and far too slow for a program
  that does it in a loop. It is there to be what a faster one is checked
  against.

- **`blocks.bfm`** - `@while`, `@when` and `@unless` as ordinary macros, and
  `@repeat` around a body. Every other program here writes its loops out,
  because until a macro could take a *body* there was nothing else to do.

- **`signed.bfm`** - numbers below zero, from **`lib/signed.bfm`**: a sign
  cell and a size cell, because a cell counts upwards and has nowhere to put a
  minus. Sign-and-size rather than the wrapping arithmetic a machine uses,
  because multiplying two wrapped numbers means pulling the sign back out of
  each first, where multiplying these is multiplying the sizes and comparing
  the signs — a trade the library states plainly and now collects.

- **`mandelbrot.bfm`** - the set, at a sixteenth. 167 lines of named cells and
  signed fixed point, expanding to 30,637 instructions, checked against a model
  of the same arithmetic written in the test rather than a recorded picture. Not the byte-for-byte equal of
  `third-party/advanced/mandelbrot.bf` and not trying to be: that one is 128 by
  48 at sixteen bits from a different representation. This one is what the
  macro language can *say*.

**Every one of them with a loop in it says what the loop is for**, as a block
of pseudocode in its comments. Naming cells makes a *line* legible without
making the *program* legible -- `@to ones` `[` `-` says what is happening and
not why -- so each file carries what it would be in a language that has
numbers and an `if`. `scripts/check-bfm-pseudocode.py` checks that a program
with logic in it has some; only a reader can check that it is right.

All nineteen are load-bearing rather than illustrative:
`crates/gyrus-macro/tests/round_trip.rs` reads them. To run one:

```bash
gyrus programs/macros/scan.bfm              # expand and run
gyrus-tool expand programs/macros/scan.bfm  # just the BrainFuck
```

### `third-party/advanced/` - Complex Programs

Sophisticated BrainFuck programs by other authors, used as a correctness and
performance corpus. See [`third-party/CREDITS.md`](third-party/CREDITS.md).

- **`quine.bf`** - Self-replicating program (prints its own source code)
- **`factor.bf`** - Integer factorization program
- **`rot13.bf`** - ROT13 Caesar cipher (interactive/infinite loop)
- **`fibonacci.bf`** - Fibonacci sequence generator with sophisticated multi-digit arithmetic (see fibonacci_README.md)

**Run examples:**
```bash
cargo run -- programs/third-party/advanced/quine.bf
cargo run -- programs/third-party/advanced/factor.bf
echo "Hello World" | cargo run -- programs/third-party/advanced/rot13.bf  # Ctrl-C to stop
cargo run -- programs/third-party/advanced/fibonacci.bf  # Ctrl-C to stop
```

### `third-party/utilities/` - Practical Utilities

Small, useful programs from D.B. Cristofani's collection (CC BY-SA 4.0),
demonstrating that BrainFuck can be practical.

- **`cat.bf`** - Copy input to output (`,[ .[-],]`)
- **`reverse.bf`** - Reverse input text
- **`strip_tabs_lf.bf`** - Remove tabs and linefeeds
- **`ascii_unary.bf`** - Show ASCII values in unary (as '!' characters)
- **`clearscreen.bf`** - Output 100 newlines to clear screen
- **`beep.bf`** - Output bell character (ASCII 7)
- **`true.bf`** - Do nothing, exit successfully (shortest quine!)
- **`brainfuck_print.bf`** - Print "brainfuck\n"
- **`text_to_bf.bf`** - Convert text to BrainFuck code

**Run examples:**
```bash
echo "hello" | cargo run -- programs/third-party/utilities/cat.bf
echo "hello" | cargo run -- programs/third-party/utilities/reverse.bf  # outputs: olleh
cargo run -- programs/third-party/utilities/beep.bf  # ring terminal bell
cargo run -- programs/third-party/utilities/brainfuck_print.bf
```

**Source**: <http://brainfuck.org/> — see [`third-party/CREDITS.md`](third-party/CREDITS.md)

### `warnings/` - Runtime Warning Demonstrations

Programs that demonstrate the runtime warning system for memory expansion in unbounded mode.

All warnings include **syntax-highlighted source code** with line numbers, color-coded commands, and a red caret pointing at the exact instruction that triggered the warning!

- **`memory_expansion.bf`** - Demonstrates memory expansion in unbounded mode
- **`cell_overflow.bf`** - Tests cell wrapping behavior (no warnings - wrapping is standard BF)
- **`cell_underflow.bf`** - Tests cell wrapping behavior (no warnings - wrapping is standard BF)
- **`mixed_warnings.bf`** - Historical test file (retained for testing purposes)

See `warnings/README.md` for detailed explanations and usage with `--verbose` flag.

**Run examples:**
```bash
cargo run -- programs/warnings/cell_overflow.bf
cargo run -- programs/warnings/memory_expansion.bf --memory-model unbounded --unbounded-initial 5 --unbounded-max 20
cargo run -- programs/warnings/mixed_warnings.bf --quiet  # Suppress warnings
```

### `tests/` - Feature Testing Programs

Programs designed to test specific gyrus features.

- **`test_eof.bf`** - Tests EOF behavior (default: set to zero)
- **`test_eof_nochange.bf`** - Tests EOF with no-change behavior
- **`warnings_test.bf`** - Triggers validation warnings
- **`warnings_only.bf`** - Contains only warning-triggering patterns
- **`infinite_loop.bf`** - Infinite loop for testing step limits
- **`infinite_loop2.bf`** - Alternative infinite loop pattern
- **`deep_nesting.bf`** - Deeply nested loops (12 levels) for testing parser and validator

**Run examples:**
```bash
cargo run -- programs/tests/test_eof.bf --eof-behavior zero
cargo run -- programs/tests/warnings_test.bf --validate
cargo run -- programs/tests/infinite_loop.bf --max-steps 1000
```

### `errors/` - Error Demonstration Programs

Programs that intentionally trigger errors to demonstrate error handling.

- **`README.md`** - Detailed error handling documentation
- **`unmatched_bracket.bf`** - Parse error: unmatched `[`
- **`memory_overflow.bf`** - Runtime error: memory out of bounds
- **`infinite_loop.bf`** - Step limit exceeded error
- **`validation_warnings.bf`** - Programs with validation warnings
- **`error_test.bf`** - General error testing
- **`unclosed_brackets.bf`** - Multiple bracket errors
- **`multiple_bracket_errors.bf`** - Shows multiple error reporting

**Run examples:**
```bash
# Parse errors with rich context
cargo run -- programs/errors/unmatched_bracket.bf

# Runtime errors
cargo run -- programs/errors/memory_overflow.bf --memory-size 100

# Validation warnings
cargo run -- programs/errors/validation_warnings.bf --validate
cargo run -- programs/errors/validation_warnings.bf --strict  # Exit on warnings
```

## Running Programs

### Basic Execution
```bash
cargo run -- programs/basic/hello_world.bf
```

### With Options
```bash
# Verbose mode with statistics
cargo run -- programs/basic/hello_world.bf --verbose

# Limit execution
cargo run -- programs/tests/infinite_loop.bf --max-steps 10000

# Different memory models
cargo run -- programs/third-party/advanced/factor.bf --memory-model unbounded

# Validate before running
cargo run -- programs/tests/warnings_test.bf --validate
```

## Statistics

- **Total programs**: 52
- **Written for gyrus (MIT)**: 24 — basic (5), tests (7), errors (7), warnings (4), debug (1)
- **Third-party**: 28 — `third-party/advanced` (19), `third-party/utilities` (9)
- **Coverage**: Basic syntax, advanced algorithms, practical utilities, error cases, edge cases
- **Attribution**: see [`third-party/CREDITS.md`](third-party/CREDITS.md)

## Contributing Programs

When adding new BrainFuck programs:

1. **Basic programs**: Simple, educational examples
2. **Advanced programs**: Complex algorithms and interesting patterns
3. **Utilities**: Small, practical tools
4. **Test programs**: Programs that test specific features
5. **Error programs**: Programs that demonstrate error handling

Include comments using `*` for better documentation:
```brainfuck
* This is a line comment
+++    * Increment cell 0 by 3
[      * Start loop
  >.   * Output cell 1
]
```

## See Also

- [Error Handling Documentation](errors/README.md)
- [Main README](../README.md)
- [Library Examples](../crates/gyrus/examples/) - Rust code showing how to use gyrus as a library
