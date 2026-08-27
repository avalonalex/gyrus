# Making it fast, and what didn't work

How gyrus got to the speed it runs at, written for someone who has not spent
time on compilers. There is a [glossary](#glossary) at the end; anything in
*italics* on first use is in it.

This is a closed record rather than a plan. The optimization work it describes
ran from October 2025 to August 2026 and reached a natural stopping point: the
remaining ideas are either hard, declined on principle, or measured and found
not to pay. Nothing here is a promise about future work.

Back to the [README](../README.md). For how the execution modes differ, see
[execution models](execution-models.md).

## Double-and-add multiplication in `.bfm` (August 2026) — did not pay

`lib/signed.bfm` multiplies by adding `b` to nothing `a` times, which costs the
*value* of `a`. Its own comment named the alternative: eight rounds of double
and add, costing the number of digits instead. Built and measured, it is
**2.5× worse** for the program that wanted it.

| | naive | double-and-add |
|---|---|---|
| 40 × 20 | 27,000 | 17,041 |
| 0 × 20 | ~600 | 9,527 |
| `mandelbrot.bfm` | 91,513,283 | 227,244,885 |

Eight rounds happen whatever the numbers are, and each round doubles a two-cell
number — which in BrainFuck costs the value being doubled, because adding is
counting. So the fixed cost is thousands of steps, and `mandelbrot.bfm` spends
most of its multiplies on small operands and zeros, where counting `a` times is
nearly free.

The crossover is around 60, and `lib/signed.bfm` refuses operands whose product
passes 4096 — so no legal input reaches it. A faster multiply for this library
would have to make *doubling* cheap, which is the same wall
`@signed_halve` hit: arithmetic on a cell costs its value unless the loop
structure carries it.

## Where it stands

| | mandelbrot | hanoi |
|---|---|---|
| Tree-walking interpreter (`--debug`) | minutes | slow |
| Optimized interpreter (default) | 2.9 s | 234 ms |
| *JIT* (`--jit`) | 811 ms | 170 ms |

The JIT is about **3.5x** the optimized interpreter on mandelbrot and **1.4x**
on hanoi, and it *loses* on programs that finish in a few milliseconds, because
compiling them takes longer than interpreting them. Those figures are from one
laptop; the ratios travel better than the times.

## The two things that worked

### Recognising what the program is actually doing

BrainFuck has eight instructions and no way to say "add 5" — you write `+`
five times. Executing that literally means five trips through the interpreter's
dispatch loop for what a processor does in one instruction.

So gyrus does not execute the program as written. The *optimizer* rewrites it
first:

- A run of `+++++` becomes one `Add(5)`.
- `[-]`, the idiom for "set this cell to zero", becomes one `Zero` — instead of
  a loop that decrements up to 255 times.
- `[-]+++` becomes `Set(3)`: clear, then store a constant, in one operation.
  hanoi contains 324 of these.
- `[>>>>]`, which walks rightwards four cells at a time until it finds a zero,
  becomes one `SeekRight(4)`. mandelbrot spends a large share of its time in
  124 such loops.
- `[->+++<]`, which is how BrainFuck writes "add three times this cell to the
  next one", becomes one `MultiplyAdd`.

Hello World goes from 103 instructions to 45. This is the single largest source
of gyrus's speed, and it is worth more than the JIT: by the time compilation
shipped, most of the win the original plan attributed to it had already been
taken here.

### Getting more than one thing in flight at a time

This is the interesting one, and it took three failures to find.

A *seek* — `[>>>>]` above — was 49% of mandelbrot's runtime. Compiled, its
inner loop was six machine instructions:

```
check the cursor is still on the tape
load the cell
is it zero? if so stop
move the cursor
go round again
```

The obvious idea is to make that shorter. We did, twice, and it changed
nothing at all (see below). The reason is that the loop was not limited by how
*many* instructions it ran. It was limited by *waiting*: the decision to
continue depends on the cell just loaded, and a load takes several cycles to
arrive. One load was ever outstanding, so the loop ran at the speed of one
load-then-branch, no matter what surrounded it.

The fix was to change the shape rather than the size. The loop now reads
**four cells at once**:

```
check all four cells are on the tape
load cell, cell+4, cell+8, cell+12      ← four loads, none waiting on another
combine them (zero if any of them is zero)
if none was zero, jump four cells and go round again
```

The four loads have nothing to do with each other, so the processor issues them
together and their waiting overlaps. That is *memory-level parallelism*, and it
was worth **-17.6%** on mandelbrot — the largest single win of the whole effort.

Two details it needed:

- **Reading ahead is only safe on the tape.** The unrolled step sits behind one
  *bounds check* covering all four cells; where that does not hold — at the
  tape's ends — the original one-cell-at-a-time loop runs instead. A seek that
  runs off the tape still fails at exactly the cell that does it, never at a
  cell it only peeked at.
- **How you combine the four matters.** Folding them one after another built a
  chain where each step waited for the last — about fourteen cycles of waiting,
  in the loop the whole change existed to stop waiting in. Combining them in
  pairs, as a tree, is the same number of operations at a third of the depth.
  Getting this wrong cost about a fifth of the win, and made the tuning look
  like it favoured a different setting than it really did.

## What didn't work

Every one of these was implemented, measured, and reverted. They are recorded
because the next person to have the same idea should be able to find out it was
tried, and what it cost.

| Idea | Result on mandelbrot |
|---|---|
| Drop a redundant instruction from the seek's test (6 instructions to 5, in 49% of the run) | **0.0%** |
| Drop the same redundant instruction from the multiply's test | **+0.2%** |
| Share one function exit instead of 1,088 copies, cutting emitted code by a third | **+1.5%** |
| Consolidate the hot loop's three bounds checks into one | **+9.2%** |
| Let multiplies join a guarded run, threshold unchanged | **+2.2%** |
| Offset addressing: stop moving the cursor, address cells relative to it | +23% on hanoi, -10% on mandelbrot |

Some of these deserve a sentence.

**Cutting emitted code by a third made it slower.** Every place the compiled
program could fail had its own copy of the code that tidies up and returns —
1,088 copies, half of everything emitted. Sharing one copy was an obvious win
and was not one, because those copies were *cold*: they never ran. Code that
never runs costs nothing to have, and sharing it forced a value into a fixed
register at every branch out of the hot path, which made the hot path worse.

**Consolidating three bounds checks made it much slower.** The hottest loop in
mandelbrot pays three separate checks per iteration, each one gating the next
so the loads cannot overlap — which is exactly the problem unrolling solved for
seeks. But the mechanism for consolidating them is not free: it carries a
fallback path that hands the region to the runtime, and that plumbing costs
more than the checks it removes unless it is amortised over at least three
accesses. The existing threshold turns out to be right.

**Offset addressing failed its own acceptance criterion.** It made hanoi 23%
faster and mandelbrot 10% *slower*. The reason is a good illustration of why
static counts mislead: mandelbrot's cursor movement is 67% of what it executes,
which looked like an enormous opportunity, but nearly all of it is the *body*
of scan loops rather than movement between operations — so there was nothing to
fold. The transform removed 5.6% of its instructions against a 17% estimate.

Two more were measured before being built at all, and then not built:
**cancelling out `+++-` into `Add(2)`** covers 0.0% of executed instructions on
every benchmark program, and **dead code elimination** would fire once in
hanoi, twice in mandelbrot, and nowhere else.

## The finding

Four separate attempts to make hot code *shorter* changed nothing or made
things worse. One attempt to change its *shape* — so that several loads are
waiting at the same time instead of one after another — was worth 17.6%.

This target is not limited by how many instructions it executes. When
considering an optimization here, the question worth asking is not "how many
instructions is this?" but **"what is waiting on what?"**

That is not a general law about computers. It is what is true of this program
on this kind of processor, and it was only discovered by measuring, after three
plausible ideas produced nothing.

## What was deliberately not done

Every read and write checks that the cell is on the tape. Removing all of those
checks would be worth **7.5%** on mandelbrot and 9% on hanoi — measured, not
estimated.

That is declined. Bounds-checked memory is most of what gyrus is *for*: the
tape contract is the invariant the project is built around, and an
implementation that abandons it to go faster is one of the many BrainFuck
implementations this deliberately is not. A `--unsafe` flag might earn a place
some day; it has not.

Worth keeping straight, because the 7.5% figure blurs two different things:
*removing* a check is unsound, but *not repeating* a check is not. If a seek
has just read a cell successfully, code that immediately re-reads the same cell
is re-establishing something already known, and eliminating that would change
no behaviour at all. That version stays legitimate — it just has to survive the
case where the tape is reallocated part-way through a loop, which is where a
subtle and silent memory bug would live.

## How any of this was measured

Everything above is a measurement, and the method matters more than it sounds,
because most of these effects are small enough to be invented by a careless one.

- **Compare two binaries run-for-run, alternating, and take the fastest of
  seven.** The laptop this was measured on drifts 10–15% between rounds
  depending on what else is running. Absolute times moved by 15% mid-session;
  the *ratios* between alternating runs did not.
- **Keep the baseline in a separate git worktree** so both binaries exist at
  once and neither has to be rebuilt between rounds.
- **Check the output, not just the clock.** `scripts/benchmark.sh` diffs every
  run against a known-good output. A program that got faster and stopped
  printing the right thing is not a faster program, and this is easier to do
  by accident than it sounds.
- **A control tells you what "no change" looks like.** A build of the baseline
  with two unrelated lines swapped — a change that cannot matter — measured
  0.0%, which is what makes a 1% result believable.
- **Profile before choosing, and again after changing.** The profile was taken
  twice; the second time, seeks had fallen from 49% of the run to 37% and a
  different construct was the largest. Picking the next target from the old
  profile would have been picking from fiction.

The tools for the last one are in the repo: `GYRUS_JIT_DISASM=1` prints the
machine code, and `GYRUS_JIT_DUMP=path` writes it out alongside a table mapping
each machine instruction back to the BrainFuck it came from — which is what lets
a *profiler*'s output be read as BrainFuck rather than as hexadecimal. See
[Development](development.md).

## Glossary

**Bounds check** — code inserted before a memory access to confirm the address
is inside the region it is allowed to touch. The cost is a comparison and a
branch; the benefit is that a bug is an error message instead of corrupted
memory or a crash.

**Branch** — a jump in the program, often conditional ("if this is zero, go
there"). Processors guess which way a branch will go and run ahead
speculatively; a wrong guess costs the work done in the meantime.

**Cold path / hot path** — code that almost never runs versus code that runs
constantly. Making cold code smaller or faster is usually worthless, which is
one of the lessons above.

**Cursor** — in BrainFuck, the pointer into the tape that `>` and `<` move.

**Dependency chain** — a sequence where each step needs the previous step's
result, so none can start early. The length of the chain, not the number of
steps, sets how long it takes.

**Guard** — one bounds check covering a whole region, replacing a check at
every individual access inside it.

**Instruction-level / memory-level parallelism** — a processor works on many
instructions at once when they do not depend on each other. Memory-level
parallelism specifically means having several loads outstanding at the same
time, so their waiting overlaps rather than adding up.

**IR (intermediate representation)** — a program in a form that is neither the
original source nor machine code: easier to analyse and rewrite than either.
gyrus has one (`OptimizedProgram`), and the JIT compiles it.

**JIT (just-in-time compiler)** — translates the program into machine code
right before running it, so compilation time is part of the run. This is why
gyrus's JIT loses on short programs and wins on long ones.

**Latency vs throughput** — how long one operation takes to finish, versus how
many can be in progress at once. A load has a latency of several cycles but the
processor can have many in flight; a loop that waits for each load in turn is
paying latency for work that could have been overlapped.

**Optimizer** — here, the pass that rewrites the parsed program into fewer,
larger operations before anything executes it.

**Profiler** — a tool that samples what the program is doing thousands of times
a second and reports where the time went. `samply` was used here.

**Register** — the handful of very fast storage slots inside the processor.
"Register allocation" is the compiler's job of deciding what lives in them;
constraining that choice can slow down code even when the code is shorter.

**Seek** — in gyrus, a folded `[>]`-style loop that walks the tape until it
finds a zero cell.

**Tape** — BrainFuck's memory: a linear array of byte cells with a cursor.

**Unrolling** — doing several iterations' worth of work in one pass round a
loop, so per-iteration overhead is paid less often and independent work can
overlap.
