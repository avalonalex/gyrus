# Benchmarks

Recorded outputs for the programs `scripts/benchmark.sh` runs. Each file in
`expected/` is the exact stdout of the corresponding BrainFuck program, so a
regression shows up as a diff rather than a mismatched hash — you can see
*which* line changed.

```bash
scripts/benchmark.sh                 # time every program, verify output
scripts/benchmark.sh --full          # also run --debug on hanoi and mandelbrot
scripts/benchmark.sh --record        # regenerate these files
scripts/benchmark.sh --profile PROG  # execution profile via --trace
```

## What is measured

End-to-end wall clock for the `gyrus` binary: parse, optimize, execute, write
output. For per-phase numbers with warmup and outlier rejection, use the
criterion benches instead:

```bash
cargo bench
```

## Why the outputs are checked

A benchmark that reports a faster time while producing different bytes is
measuring a bug, not an optimization. Every run is diffed against `expected/`,
and for the programs fast enough to run twice, the optimized interpreter's
output is also compared against `--debug`'s. That second check makes this a
differential test of the optimizer on real programs, not only a stopwatch.

Regenerate the golden files with `--record` **only** after confirming the new
output is correct — the whole point is that they do not move silently.

## Choosing programs

They must terminate without input and be deterministic. `mandelbrot.bf` is the
best of them: it runs long enough to measure (~20s), always renders the same
48x129 fractal, and exercises arithmetic, nested loops, and output together.
`rot13.bf` is deliberately absent — it never terminates on EOF.

`hanoi.bf` and `mandelbrot.bf` skip their `--debug` run unless `--full` is
given: the debug interpreter is roughly 40x slower, which turns hanoi alone
into three minutes.

## Attribution

The programs under `programs/third-party/` were written by other people and
carry their own licenses — see
[`../programs/third-party/CREDITS.md`](../programs/third-party/CREDITS.md).
The recorded outputs here are derived from running them.
