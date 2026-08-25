# Optimizer Improvements - Missed Optimization Opportunities

## Overview

This document tracks optimization patterns that are currently missed by the gyrus optimizer. Each pattern includes examples, current behavior, desired behavior, and implementation notes.

## Current Optimizer Capabilities

**Instruction Fusion:**
- ✅ `+++` → `Add(3)` - Combine repeated increments
- ✅ `---` → `Sub(3)` - Combine repeated decrements
- ✅ `>>>` → `Right(3)` - Combine pointer movements
- ✅ `<<<` → `Left(3)` - Combine pointer movements

**Loop Pattern Recognition:**
- ✅ `[-]` → `Zero` - Clear current cell (not `[+]`: it reaches zero only by
  wrapping, which checked cells reject)
- ✅ `[>]` → `SeekRight` - Find next zero cell
- ✅ `[<]` → `SeekLeft` - Find previous zero cell
- ✅ `[->+<]` → `MultiplyAdd([(1, 1)])` - Move value to next cell
- ✅ `[->++<]` → `MultiplyAdd([(1, 2)])` - Multiply by 2 and move
- ✅ `[->+++>+<<]` → `MultiplyAdd([(1, 3), (2, 1)])` - Multi-target multiply

## Missed Optimization Opportunities

### 1. Loop Rotation/Normalization for MultiplyAdd — ✅ shipped

**Priority:** High
**Complexity:** Medium
**Impact:** Common pattern in real BrainFuck programs

**Shipped 2026-08-24**, as option B: `recognize_multiply_loop` accepts the
source's single `-`/`+` anywhere in the body. Measured share of executed
instructions inside such loops: squares 56%, triangle 32%, 99beer 22%,
bf2c 11%, mandelbrot 1%. Optimized steps: squares 959K → 267K, 99beer
339K → 137K, triangle 44K → 20K. Wall clock on those is process startup,
so the step counts are the honest number; hanoi and mandelbrot unchanged.

**Problem:**
The MultiplyAdd pattern matcher requires the loop to start with `-` or `+`, but BrainFuck loops are circular - the starting position doesn't matter semantically.

**Examples:**

```brainfuck
[>+++++<-]    ❌ NOT recognized (decrement at end)
[->+++++<]    ✅ Recognized as MultiplyAdd([(1, 5)])
```

Both patterns are **mathematically equivalent**:
- Decrement cell[0] by 1
- Add 5 to cell[1]
- Loop until cell[0] = 0
- Result: cell[1] += cell[0] * 5; cell[0] = 0

**Current Behavior:**
```
[>+++++<-] → Loop([Right(1), Add(5), Left(1), Sub(1)])
9 instructions → 5 optimized (1.80× compression)
```

**Desired Behavior:**
```
[>+++++<-] → MultiplyAdd([(1, 5)])
9 instructions → 1 optimized (9.00× compression)
```

**Implementation Notes:**
- Option A: **Loop rotation** - Try all rotations of loop body to find one that starts with +/-
- Option B: **Flexible pattern matching** - Modify `recognize_multiply_loop()` to find the +/- anywhere in the loop
- Option C: **Canonical form** - Normalize all loops to start with +/- before pattern matching

**Recommendation:** Option A (loop rotation) is cleanest - try each rotation and pick the one that matches a known pattern.

**Code Location:** `crates/gyrus/src/optimizer.rs:343` (`recognize_loop_pattern`)

---

### 2. Scan by N Cells — ✅ shipped

**Priority:** Medium
**Complexity:** Low
**Impact:** Useful for data structures like arrays

**Shipped 2026-08-24:** `SeekRight(stride)` / `SeekLeft(stride)`. Mandelbrot
4.82 s → 2.89 s (**+40%**), its loop iterations 754M → 266M; hanoi unchanged.
Kept here because the evidence below is what put it first.

**Problem:**
Currently we only recognize `[>]` and `[<]` for scanning by 1 cell. Many programs scan by N cells (e.g., for packed data structures).

**Evidence (2026-08-24):** this is the one item with a measured target behind
it. `mandelbrot.bf` works in 9-cell records, and 124 of its 478 innermost
loops are nothing but `[>>>>>>>>>]` or `[<<<<<<<<<]`. Each iteration of one
is a recursive `execute_block` call for a single fused `Right(9)` -- and the
profile in the "Tried and failed" section below puts that call's
prologue/epilogue at ~20% of run time. Mandelbrot's loops average 3.3
instructions per iteration; these are the shortest of them. A strided
`SeekRight(9)` makes each such loop one instruction, the same way `[>]`
already is.

**Examples:**

```brainfuck
[>>]      → SeekRight(2)  - Find next zero cell, stepping by 2
[>>>]     → SeekRight(3)  - Find next zero cell, stepping by 3
[<<]      → SeekLeft(2)   - Find previous zero cell, stepping by 2
[<<<<]    → SeekLeft(4)   - Find previous zero cell, stepping by 4
```

**Current Behavior:**
```
[>>] → Loop([Right(2)])
```

**Desired Behavior:**
```
[>>] → SeekRight(2)
```

**Implementation Notes:**
- Extend `SeekRight` and `SeekLeft` to include step size parameter
- Modify pattern matcher to recognize `[>+]`, `[>>+]`, etc.
- Update optimized interpreter to handle variable step sizes

**Code Location:** `crates/gyrus/src/optimizer.rs:366-378` (SeekRight/SeekLeft pattern recognition)

---

### 3. Set Value Pattern — ✅ shipped

**Priority:** Medium
**Complexity:** Medium
**Impact:** Common in initialization code

**Shipped 2026-08-24**, as the post-optimization pass: `Zero Add(n)` →
`Set(n)` under any cell model, `Zero Sub(n)` → `Set(256 − n)` under wrapping
only, and a `Set` absorbs further arithmetic. It was not premature: hanoi
executes **46%** of its instructions inside `[-]+++` patterns (324 sites),
and fusing them is hanoi 226 ms → 192 ms (**+15%**), steps 154M → 133M.
squares 267K → 255K steps; mandelbrot unchanged.

**Problem:**
Setting a cell to a specific value currently requires separate Zero + Add instructions. We could fuse these into a single Set instruction.

**Examples:**

```brainfuck
[-]++++    → Set(4)    - Clear cell then set to 4
[+]---     → Set(-3)   - Clear cell then set to -3 (wrapping: 253)
[-]+++++++ → Set(8)    - Clear cell then set to 8
```

**Current Behavior:**
```
[-]++++    → Zero, Add(4)
6 instructions → 2 optimized
```

**Desired Behavior:**
```
[-]++++    → Set(4)
6 instructions → 1 optimized
```

**Implementation Notes:**
- Add `Set(value)` optimized instruction
- Requires lookahead in optimizer: when we see Zero, check if next instruction is Add/Sub
- Must handle both `[-]` and `[+]` zero patterns
- Alternative: Post-optimization pass to fuse Zero + Add → Set

**Code Location:** `crates/gyrus/src/optimizer.rs:211-340` (optimize_block function)

**Consideration:** This might be premature - modern JIT compilers can easily fuse these at a lower level. Benefit may be minimal.

---

### 4. Addition/Subtraction Cancellation — measured, not worth doing

**Priority:** Low
**Complexity:** Low
**Impact:** Mostly benefits obfuscated code

**Measured 2026-08-24:** 0.0% of executed instructions on every benchmark
program (hanoi has 10 static sites, the rest none). The only program in
`programs/` with any number of them is oobrain (328, generated code), which
is not benchmarked. Not done; revisit only with a program that has them.

**Problem:**
Consecutive Add/Sub operations can partially or fully cancel out.

**Examples:**

```brainfuck
+++-      → Add(2)          - 3 increments + 1 decrement = net +2
-----+++  → Sub(2)          - 5 decrements + 3 increments = net -2
+++---    → (nothing)       - Complete cancellation
>><<      → (nothing)       - Pointer movement cancellation
```

**Current Behavior:**
```
+++- → Add(3), Sub(1)
```

**Desired Behavior:**
```
+++- → Add(2)
```

**Implementation Notes:**
- Post-optimization pass to merge adjacent Add/Sub operations
- Also applies to Right/Left pointer movements
- Edge case: `Add(3), Sub(5)` → `Sub(2)`

**Consideration:** This is rarely seen in hand-written BrainFuck. Only benefits generated or obfuscated code.

---

### 5. Dead Code Elimination — measured, not worth doing

**Priority:** Medium
**Complexity:** Medium
**Impact:** Helps with generated code and initialization

**Measured 2026-08-24:** the `+++[-]` shape occurs 1 time in hanoi, 2 in
mandelbrot, 1 in life, and nowhere else in `programs/`. Not done.

**Problem:**
Operations at the start of a program that are overwritten before being read are unnecessary.

**Examples:**

```brainfuck
+++[-]+++.    → [-]+++.     - Initial +++ is overwritten by [-]
>><<+.        → +.          - Pointer movement that returns to start
```

**Current Behavior:**
```
+++[-]+++. → Add(3), Zero, Add(3), Output
```

**Desired Behavior:**
```
+++[-]+++. → Zero, Add(3), Output
```

**Implementation Notes:**
- Requires dataflow analysis to track which values are live
- `Zero` instruction makes all previous operations on that cell dead
- Pointer movements that cancel out can be eliminated
- This is complex - may want to defer to JIT/AOT compiler stage

**Consideration:** Medium complexity, medium benefit. Good candidate for JIT optimization pass.

---

### 6. Copy Pattern Recognition

**Priority:** Low
**Complexity:** Medium
**Impact:** Common pattern, but already optimized via MultiplyAdd

**Problem:**
Cell copy with preservation of source is currently a general loop.

**Examples:**

```brainfuck
[->+>+<<]     → Copy cell[0] to cell[1] and cell[2], then zero cell[0]
              → Already recognized as MultiplyAdd([(1, 1), (2, 1)])
              → Could add explicit Copy instruction for clarity

[->>+>+<<<]   → Copy to non-adjacent cells
              → Already recognized as MultiplyAdd([(2, 1), (3, 1)])
```

**Current Behavior:**
```
[->+>+<<] → MultiplyAdd([(1, 1), (2, 1)])
```

**Alternative Representation:**
```
[->+>+<<] → Copy([1, 2])  - Copy cell[0] to offsets 1 and 2, then zero
```

**Implementation Notes:**
- This is purely a semantic distinction - MultiplyAdd with all multipliers = 1
- Could add Copy as an alias/wrapper around MultiplyAdd for debugging clarity
- Minimal performance benefit (already optimized)

**Consideration:** Low priority - MultiplyAdd already handles this efficiently.

---

## Future Advanced Optimizations

These optimizations are complex and likely better suited for a JIT/AOT compiler stage:

### 7. Constant Folding

Track cell values at compile time when deterministic:

```brainfuck
+++>>---<<    → Cell[0]=3, Cell[2]=-3 (known at compile time)
```

### 8. Loop Unrolling

For loops with known iteration counts:

```brainfuck
+++[>+<-]     → Iterations known (3), could unroll to: >+<>+<>+<
```

### 9. Common Subexpression Elimination

Detect repeated computation patterns:

```brainfuck
>+++<>+++<    → Both add 3 to cell[1], could cache
```

### 10. Inlining

Inline simple loop bodies when beneficial.

---

## Implementation Priority

**Phase 1 - High Value, Low Complexity:**
1. ✅ Scan by N cells (SeekRight/Left with step size) — shipped, +40% on
   mandelbrot. 124 such loops there, each iteration a block call (see
   "Evidence" under item 2)

**Phase 2 - Medium Value:**
2. ✅ Loop rotation for MultiplyAdd recognition — shipped. Little for
   mandelbrot (334 of its 345 balanced loops already folded), but the
   dominant pattern in squares, triangle and 99beer; see item 1

3. ✅ Set value pattern (Zero + Add fusion) — shipped, +15% on hanoi; see
   item 3

**Phase 3 - measured and declined (2026-08-24):**
4. Addition/subtraction cancellation — 0% of executed instructions on the
   benchmark set; see item 4
5. Dead code elimination — four static sites in the whole corpus; see item 5

**Defer to JIT/AOT:**
- Constant folding
- Loop unrolling
- Common subexpression elimination
- Inlining

---

## The JIT's remaining performance work

The JIT shipped (`gyrus --jit`; its PRD was deleted when it did, as the
directory's rule says). What is still open is performance, and it belongs in
this catalogue because every item is an optimization of what the translator
emits.

**The first round (2026-08-25)**, Measured, interleaved min-of-N on a quiet machine, against the step-2 JIT
(#15), which had neither statistics nor limits:

| | step-2 | after #16 | statistics opt-in | + nest guards |
|---|---:|---:|---:|---:|
| mandelbrot | 939 ms | 1032 ms | 954 ms | 944 ms |
| hanoi | 160 ms | 206 ms | 163 ms | **135 ms** |

- **Statistics are opt-in** (`Statistics::Cheap`/`Full`; `--verbose` is the
  only reader). The peak notes and the iteration counter were 5% + 2.5% of
  mandelbrot and, through the value each loop kept live across itself, 20%
  of hanoi.
- **The bounds-check ceiling was measured before building guards**: with
  every check removed, mandelbrot gains 4.5% and hanoi 28%, the latter
  almost all compile time from the check blocks. So checks are not where
  mandelbrot's remaining 1.6× to native lies; that is codegen.
- **Guards** cover a straight-line run touching three or more cells, and,
  where it pays, a whole balanced loop nest: the cells a nest touches are the
  same every iteration, so one two-compare guard before the header replaces
  every check inside, and the nest compiles check-free. A failed guard calls
  `bf_slow`, a small interpreter of the same IR in the runtime, which
  reproduces every effect up to the failing access and then the
  interpreter's own error -- or finishes the region when the guard was
  merely pessimistic. Per-run guards alone were neutral (hot accesses live
  in one- and two-cell runs and loop headers); nest guards are the 18% on
  hanoi, mostly compile time.

What is left for mandelbrot is the quality of the generated code, not its
checks: the cell is loaded and stored at every access rather than kept in a
register across a run, the byte arithmetic goes through extends, and the
cursor is threaded as a variable through every block. Next: keep the cell
value in SSA within a run, and read Cranelift's output for a hot loop.

---

## Tried and failed

Kept so that nobody measures these again. Each has a branch or PR with the
code; the numbers are what decided it.

### The seek loop is latency-bound — 2026-08-25, the useful negative

**How it was measured.** `GYRUS_JIT_DUMP=path` writes the emitted bytes and
prints the address they were mapped at, and every emitted instruction now
carries the AST index it came from (`set_srcloc`), so Cranelift's offset table
turns a profiler's hex into BrainFuck. `samply` at 20 kHz on mandelbrot, leaf
frames mapped through that table:

| construct | share of time |
|---|---|
| `SeekRight` | 30.3% |
| `Loop` (the header test) | 24.6% |
| `SeekLeft` | 18.4% |
| `MultiplyAdd` | 9.5% |
| `Right` / `Left` | 9.5% |
| `Add` / `Sub` | 7.2% |

95.9% of samples are inside the generated code; the runtime callbacks and I/O
do not appear. **Seeks are 49% of the run**, which confirms by time what was
previously known by instruction count.

**The experiment.** The seek loop's whole body is a bounds check, a byte load
and a test. The test was costing two instructions rather than one:
Cranelift cannot assume an `I8` has clear high bits, so `brif` on one lowers to
`ands wzr, w, #255` and a flag-testing branch. Widening afterwards does not
help -- the egraph rewrites `brif(uextend(v))` back to `brif(v)`. Loading the
byte straight into an `I32` with `uload8` leaves no `I8` in the IR, and the
test becomes a single `cbnz`:

```
                                  subs xzr, x0, x20     ; bounds check
subs xzr, x0, x20                 b.lo body
b.lo body                         ldrb w5, [x19, x0]
ldrb w5, [x19, x0]        -->     cbnz w5, step
ands wzr, w5, #255
b.ne step
```

Six instructions an iteration became five, in 49% of the runtime.

**Result: 0.0%.** mandelbrot 938.9 ms against 939.0 ms; hanoi 131.7 against
131.7. Interleaved min-of-7 against a `main` worktree binary. Only 99beer
moved, -3.7%, and that is a 10 ms program where the win is compile time.

**What that means, and it is the point.** Removing a sixth of the instructions
from half the runtime changed nothing, so the seek loop is not
instruction-throughput-bound. Its branch depends on its load, and there is
exactly one load in flight at a time: the loop runs at load-to-branch latency
no matter how few instructions surround it. That also explained the then-current
4.5% ceiling for removing every bounds check. (Both numbers have since moved --
see the re-measurement below.)

**So the next thing to try is memory-level parallelism, not fewer
instructions.** — and it was, and it worked. See below.

### Re-profiled after the unrolling, and what it says to do — 2026-08-25

Every number above was measured against a program the unrolling then made 17.6%
faster, so the profile and the bounds-check ceiling were both re-taken.

**Where the time goes now:**

| construct | before | now |
|---|---|---|
| `Loop` (the header test) | 24.6% | **29.6%** |
| `SeekRight` | 30.3% | 25.4% |
| `SeekLeft` | 18.4% | 12.1% |
| `MultiplyAdd` | 9.5% | 12.3% |

Seeks fell from 48.7% to 37.5%; `Loop` is now the largest construct. Read as
absolute time rather than share -- the run is 17% shorter -- every untouched
construct held its cost exactly: `Loop` was about 234 ms before and 234 ms
after. The shares moved only where work was removed, which is the check that
the profile measures what it claims to.

**The bounds-check ceiling is 7.5%, not 4.5%** -- mandelbrot 7.7% and 7.5% over
two rounds, hanoi 9.0%, measured with a same-binary toggle compiling every
access without its check, output byte-identical to the golden file in both
modes. The checks did not get more expensive; the run got shorter around them.
The toggle was not merged: it compiles out the tape contract, and an
environment variable that silently makes the JIT unsound is a hazard the
disassembly flag is not.

**That 7.5% is declined.** Bounds-checked memory is what gyrus is for -- the
README leads with it, and the tape contract is the one invariant a script
exists to enforce. A `--unsafe` flag may earn a place later; it has none now.

What remains, and it is a different thing the ceiling number blurs together:
*removing* a check is unsound, but *not repeating* one is not. A loop header
that re-reads the cell a seek just stopped on is re-establishing a fact it
already holds. Eliminating that is ordinary bounds-check elimination inside the
contract. A loop containing a seek can never be guarded -- `span()` returns
`None` on a seek, because the reach depends on the data -- so those loops pay a
check every iteration, and every hot loop in mandelbrot contains a seek. Two
things the proof must survive: the redundancy holds on the back edge but not on
loop entry, and under the unbounded model the tape can move mid-loop, which is
where a soundness bug would live. Worth a fraction of 7.5%, and only with that
argument made.

### Removing instructions from hot code, three times — 2026-08-25, all neutral

Worth stating together, because the conclusion is stronger than any one of
them:

| change | mandelbrot |
|---|---|
| `loop_test`: drop the `I8` mask, 6 instructions to 5 in 49% of the run | 0.0% |
| Share one exit epilogue, -33% emitted code | +1.5% |
| `MultiplyAdd`: drop the same mask from its source test | +0.2% |

Against the one structural change, unrolling the seek: **-17.6%**.

Three separate attempts to make hot code shorter changed nothing measurable,
while one attempt to change its *shape* -- independent loads in flight instead
of a dependent load-test-branch per cell -- paid. This target is not
instruction-throughput-limited anywhere that has been looked at. Before
proposing an optimization here, the question worth asking is not "how many
instructions is this" but "what is waiting on what".

### Unrolling the seek — 2026-08-25, **-17.6%**

Acting on the finding above: the seek now reads `SEEK_UNROLL` cells per step
instead of one. The loads have no dependency on each other and issue together;
`umin` folds them, which is zero exactly when one of the cells is zero; and a
single branch decides whether to take the whole step.

Reading ahead is only sound where the cells are on the tape, so the unrolled
step sits behind a range guard over the whole span, and the one-at-a-time loop
still runs at the tape's ends and for the last few cells of a seek whose zero
falls inside a span. That is what keeps it exact: a seek that runs off the tape
fails at the read that does it, never at a speculative one, and all three
engines report the same cell.

**The first version of this was measured wrong, and the write-up said the wrong
thing about why.** The `umin`s were folded left to right, so the reduction was a
dependent chain -- seven `subs`/`csel` pairs, about fourteen cycles of serial
ALU latency, sitting between the loads and the branch in the loop the whole
change exists to de-latency. Because the chain grows with the width, it
penalised the wider settings, and the tuning curve that produced looked like a
guard-width effect:

| unroll | chain reduction | balanced tree |
|---|---|---|
| 2 | -10.2% | — |
| 4 | -13.2% | **-15.7%** |
| 8 | -14.7% | -14.8% |
| 16 | -8.1% | -9.8% |

With the tree, 4 and 8 are the same speed: +0.1% between them over nine
interleaved rounds, which is inside the run-to-run spread. Four is chosen for
being that speed in less code, with a narrower guarded span, so more seeks take
the wide path near the tape's ends. 16 is still clearly worse, and *now* the
guard-width explanation is the one left standing.

Two other things the review of the first version found, both in the hot path:
the range guard combined its two comparisons with `band` and branched once,
which costs the `ands wzr, w, #255` mask that `loop_test` had been changed to
avoid a commit earlier; branching twice instead is shorter and mask-free. And
the guard is now one shared helper rather than a copy in `guard()` and another
in `seek()`.

Final, interleaved min-of-7 against a `main` worktree binary: mandelbrot
-17.6% and -17.4%; hanoi -1.2%; 99beer -2.1%. The JIT is 3.5x the optimized
interpreter on mandelbrot, from 3x.

Which mechanism dominates -- load latency now overlapped, or four times fewer
branches -- is not separated here; that wants CPU performance counters rather
than a stopwatch.

**Not done, and the next thing to try here.** The range check is re-derived on
every wide step. For a given cursor, tape length and stride, the number of
steps that stay on the tape is computable once at seek entry, which would make
the inner loop a counted loop with no bounds comparison at all and leave the
tail to the one-at-a-time path exactly as now.

**A note on what the tests can and cannot pin.** A guard that is one stride too
generous is invisible to a black-box test: a speculative read never decides the
answer by itself, because the one-at-a-time path re-checks every cell the seek
actually stops on, so the over-read only changes behaviour if the byte past the
tape happens to be non-zero. Verified: loosening the guard by one stride fails
no test. The reads and the guarded reach are therefore tied together by a
`debug_assert!` in the translator, which does catch it.

### One shared exit epilogue in the JIT — 2026-08-25, negative

**The observation.** `GYRUS_JIT_DISASM=1` prints what Cranelift emitted. For
mandelbrot that is 19,968 instructions, and 1,088 of them are `retabsp`. Each
failure site ends in its own `return_`, and a return carries the frame teardown
with it: six register-pair restores plus the authenticated return, nine
instructions a site. **Half the emitted function is frame teardown**, spread
from instruction 159 to 19,967, in between the loops that do the work.

**The idea:** one shared cold block taking the cursor and the site id as block
parameters, so each site is a two-instruction trampoline. mandelbrot drops to
13,288 instructions (-33.5%) and two epilogues, byte-identical output, JIT
suite green.

**Code:** PR #26 (branch `experiment/jit-shared-exit-epilogue`), not merged.

**Result.** Interleaved min-of-7 against a `main` binary in a separate
worktree:

| program | `main` | shared exit | vs `main` |
|---|---|---|---|
| mandelbrot | 947 ms | 961 ms | **+1.5%** |
| mandelbrot, second round | 1090 ms | 1101 ms | **+1.0%** |
| hanoi | 133 ms | 136 ms | **+2.4%** |
| hello_world | 5.3 ms | 5.0 ms | -6.5% |

Slower on both real programs, on both rounds, including the compile-bound one.
Only hello_world improves, and at 5 ms that is noise. The absolute times moved
between rounds because the machine was loaded; the ratios did not.

**Why.** The epilogues were cold and never executed, so their only cost was
instruction-cache footprint -- and a line that is never fetched costs nothing.
Against that, the shared block's parameters pin the cursor into a fixed
register at every branch out of hot code, which constrains the register
allocator where it does matter. Code size is not the thing; the executed path
is.

**What it rules out.** Shrinking cold code. It also gives the hot loop's exact
cost: mandelbrot's strided seek, 47% of executed instructions, is six
instructions an iteration --

```
subs xzr, x2, x20      ; bounds check
b.lo body
ldrb w7, [x19, x2]     ; load the cell
ands wzr, w7, #255     ; test zero
b.ne step
add  x2, x2, #4        ; cursor += stride
b    header
```

-- of which two are the bounds check, consistent with the 4.5% ceiling measured
for removing checks altogether. A native `while (tape[i]) i += 4;` is four
instructions. The remaining 1.6x to native is not in cold code and not in the
guards.

### Offset addressing (lazy pointer motion) — 2026-08-24, negative

**The idea:** give cell operations an offset from the pointer, so a
straight-line run stops moving the pointer and does its work in place.
`Right(3) Add(1) Left(3) Right(5) Sub(2) Left(5)` becomes
`Add(1)@+3 Sub(2)@+5` — six instructions and four moves become two and none.
Runs end at loops, seeks and `MultiplyAdd`. Its prerequisite, the tape
contract (access outside the tape is an error, movement is not), shipped in
#4 and stands on its own at 8–11%.

**Code:** PR #5 (branch `experiment/offset-addressing`), closed without
merging. It carries a `GYRUS_NO_FOLD=1` toggle that disables the pass at
optimize time, which is how the fold's effect was separated from the
interpreter-side cost.

**Result.** Interleaved min-of-N runs, alternating binaries. A control build
of `main` with two match arms swapped — a layout-only change — measured 0.0%
from `main`, so these differences are real:

| | `main` | fold off | fold on | vs `main` | fold's own effect |
|---|---:|---:|---:|---:|---:|
| hanoi | 223 ms | 250 ms | 172 ms | **+23%** | +31% |
| mandelbrot | 4.69 s | 5.33 s | 5.16 s | **−10%** | +3% |

"Fold off" runs `main`'s exact instruction stream through the changed
interpreter, so that column is the interpreter-side cost alone. The transform
does what it says where runs are long — hanoi dispatches 28% fewer
instructions (154.5M → 111.1M) — and fails the PRD's own criterion of ≥15% on
*both* programs.

**Why it fails on mandelbrot.** Its 754M loop iterations execute 2.5G
instructions: 3.3 per iteration. Its moves are 67% of what it executes, but
they are the whole bodies of `[>>>>>>>>>]` scans, not moves between
operations, so there is no run to fold. The pass removes 5.6% of its dynamic
instructions against a 17% static estimate. The static count weighted moves by
nesting depth; what mattered was whether a hot loop body contained a run with
more than one move in it.

**Why the interpreter side costs 9–18% whatever you do.** A `samply` profile
of `execute_block` on hanoi, on `main`:

| share | what |
|---:|---|
| ~32% | the `step_count` load/add/store — a store-to-load hop per instruction |
| ~26% | the jump-table dispatch |
| ~20% | `execute_block`'s prologue/epilogue, once per loop *iteration* |
| ~5% | the cursor load; the move arms themselves are noise |

The loop runs at the latency of one store-to-load forward, ~5 cycles per
instruction, so anything added per instruction is a large fraction of that.
Three variants were measured, and the cause was different each time: an
offset field on every operation puts one dependent add on the cursor chain
(−13%); separate `AddAt`-style twins keep the plain arms' code but LLVM
tail-merges each twin with its plain arm, which costs the plain arm a taken
branch (−18%); hoisting cursor and step count into block-locals removes both
chains from the profile and replaces them with a store/reload pair around
every block call, which on 3-instruction blocks is no better (−12%).

**What it says to do instead.** The recursion is the cost, not the moves: a
loop iteration is a call that saves twelve registers, and blocks are 3–5
instructions long. The structural fix is a flat instruction array with jump
targets — one loop, registers live for the whole run — which is the IR the
compilation-backend PRD wants anyway. Short of that, the item on this list
with a measured target is scan by N (item 2 above). And any future change to
the `execute_instruction` match should be measured with the pass it enables
switched *off*, against `main`, before the pass is judged.

---

## How the priorities were measured

Static counts of a pattern say little: hanoi's 324 `[-]+++` sites could have
been cold. What decided the order above was the share of *executed*
instructions each pattern covers, from a throwaway tool: a hook that counts
`before_instruction` per instruction index on the debug interpreter, dumped
per source offset of the minified program, then summed over each pattern's
span. It is ~40 lines against the library and takes seconds on everything
but mandelbrot (minutes) and hanoi (~10 minutes). Worth rebuilding before
picking the next item.

## Testing Strategy

For each new optimization:

1. **Unit tests** - Add to `crates/gyrus/src/optimizer.rs` tests
2. **Integration tests** - Test with real BrainFuck programs
3. **Regression tests** - Ensure existing optimizations still work
4. **Compression ratio benchmarks** - Measure improvement on standard programs:
   - `programs/third-party/advanced/hanoi.bf`
   - `programs/third-party/advanced/pi.bf`
   - `programs/basic/hello_world.bf`

---

## Related Files

- **Optimizer implementation:** `crates/gyrus/src/optimizer.rs`
- **Optimized interpreter:** `crates/gyrus/src/interpreter/optimized.rs`
- **Optimizer tests:** the `tests` module at the end of `crates/gyrus/src/optimizer.rs`
- **Optimizer example:** `crates/gyrus/examples/optimizer.rs`
- **CLI integration:** `crates/gyrus-cli/src/main.rs`
- **Tool visualization:** `crates/gyrus-tool/src/main.rs` (optimize subcommand)

---

## Notes

- All optimizations must preserve source range tracking for debugging
- Performance improvements should be measured, not assumed
- Complex optimizations may be better suited for JIT/AOT compilation stage
- Focus on patterns that appear in real-world BrainFuck programs, not theoretical cases

---

## Revision History

- 2025-11-02: Initial document with 6 optimization opportunities identified
- 2026-08-24: Offset addressing tried and recorded as a negative result; scan by N promoted on its evidence
- 2026-08-25: JIT shared exit epilogue tried and recorded as a negative result; `GYRUS_JIT_DISASM=1` added to make emitted code inspectable
- 2026-08-25: profiled the JIT by construct (`GYRUS_JIT_DUMP` + srcloc); seeks are 49% of mandelbrot and the loop is latency-bound, so instruction count is the wrong target
- 2026-08-25: seek unrolled on that evidence, -17.6% on mandelbrot; the first tuning curve was an artifact of a chained `umin` reduction and was redone
- 2026-08-25: re-profiled after it -- `Loop` is now the largest construct at 29.6%, the bounds-check ceiling is 7.5% rather than 4.5%, and that 7.5% is declined on purpose
