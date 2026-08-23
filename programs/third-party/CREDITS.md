# Third-Party BrainFuck Programs

Everything in this directory was **written by other people**. It is bundled with
gyrus as a test and benchmark corpus, redistributed under each program's own
terms. The MIT license at the root of this repository covers the interpreter —
it does **not** cover these files.

The interpreter does not incorporate any of these programs: it reads them at
runtime the way `grep` reads a text file. Under GPL §5 and the equivalent CC
BY-SA terms this is *mere aggregation*, which is why a GPL-licensed program can
sit in an MIT-licensed repository without affecting the license of the Rust
code.

One exception worth naming: the benchmark harness
(`crates/gyrus/benches/interpreter.rs`) embeds `advanced/hanoi.bf` and
`advanced/mandelbrot.bf` at compile time via `include_str!`, so a compiled
benchmark binary carries copies of those two programs. The benchmark binaries
are not distributed.

## Provenance

Attribution below was verified by comparing each file's BrainFuck instruction
stream (comments and whitespace stripped) against the author's published
original. "Verbatim" means the instruction streams are byte-identical.

### Daniel B. Cristofani — CC BY-SA 4.0

Source: <http://brainfuck.org/> (formerly hevanet.com/cristofd/brainfuck/)

His index page states: *"I'm licensing all of these under a Creative Commons
Attribution-ShareAlike 4.0 International License. Haven't got around to putting
that into all the individual files."* Only `life.bf` carries the notice inline.

| File | Upstream | Match |
|---|---|---|
| `advanced/bf2c.bf` | `dbf2c.b` | verbatim |
| `advanced/collatz.bf` | `collatz.b` | verbatim (upstream adds commentary) |
| `advanced/fibonacci.bf` | `fib.b` | verbatim (upstream adds commentary) |
| `advanced/life.bf` | `life.b` | verbatim |
| `advanced/random.bf` | `random.b` | verbatim |
| `advanced/squares.bf` | `squares.b` | verbatim |
| `advanced/wc.bf` | `wc.b` | verbatim |
| `utilities/ascii_unary.bf` | `short.b` | verbatim |
| `utilities/beep.bf` | `short.b` | verbatim |
| `utilities/brainfuck_print.bf` | `short.b` | verbatim |
| `utilities/cat.bf` | `short.b` | verbatim |
| `utilities/clearscreen.bf` | `short.b` | verbatim |
| `utilities/reverse.bf` | `short.b` | verbatim |
| `utilities/strip_tabs_lf.bf` | `short.b` | verbatim |
| `utilities/text_to_bf.bf` | `short.b` | verbatim |
| `utilities/true.bf` | `short.b` | the empty program |

The `utilities/` files carry gyrus-written header comments (usage notes); the
program bodies are unmodified. Their descriptions are adapted from `short.b`.

### Brian Raiter — GNU General Public License

| File | Notice | Source |
|---|---|---|
| `advanced/factor.bf` | "Copyright (C) 1999 by Brian Raiter, under the GNU General Public License" | <https://www.muppetlabs.com/~breadbox/bf/> |

The file states no GPL version. It is redistributed verbatim with its notice
intact, as the GPL requires.

### Named authors, no license stated

These carry a copyright notice or byline but no grant of terms. They are
long-circulated community programs, redistributed here in the same spirit in
which they were published. If you are one of these authors and want a file
removed or relicensed, open an issue and it will be handled.

| File | Author | Notes |
|---|---|---|
| `advanced/99beer.bf` | jim crawford | "99 bottles in 1752 brainfuck instructions", goombas.org |
| `advanced/calc.bf` | Antosser | built with <https://github.com/Antosser/brainfuck-compiler> |
| `advanced/char.bf` | Jeffry Johnston | 2001; prints the ASCII character set |
| `advanced/mandelbrot.bf` | Erik Bosman | the standard interpreter benchmark |
| `advanced/oobrain.bf` | Chris Rathman | © 2003 |
| `advanced/pi.bf` | Felix Nawothnig | successor to `pi16.b` |

### Unattributed

Widely circulated programs carrying no byline. Provenance was not established.

| File | Notes |
|---|---|
| `advanced/hanoi.bf` | Towers of Hanoi; commonly attributed to Clifford Wolf — unverified |
| `advanced/numwarp.bf` | shares a name with Cristofani's `numwarp.b`, but the code differs |
| `advanced/quine.bf` | a self-reproducing program |
| `advanced/rot13.bf` | shares a name with Cristofani's `rot13.b`, but the code differs |
| `advanced/triangle.bf` | Sierpiński triangle; differs from Cristofani's `sierpinski.b` |

### Project-authored files in this directory

| File | Notes |
|---|---|
| `advanced/fibonacci_README.md` | gyrus documentation *about* Cristofani's `fib.b`, MIT like the rest of the project |

## Why the files are not individually annotated

Program bodies are left byte-exact so they stay verifiable against upstream, and
because `advanced/quine.bf` would stop being a quine if its source changed. The
directory name and this file carry the attribution instead.
