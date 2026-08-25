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

## Tried and failed

Kept so that nobody measures these again. Each has a branch or PR with the
code; the numbers are what decided it.

### Offset addressing (lazy pointer motion) — 2026-08-24, negative

**The idea:** give cell operations an offset from the pointer, so a
straight-line run stops moving the pointer and does its work in place.
`Right(3) Add(1) Left(3) Right(5) Sub(2) Left(5)` becomes
`Add(1)@+3 Sub(2)@+5` — six instructions and four moves become two and none.
Runs end at loops, seeks and `MultiplyAdd`. Its prerequisite, the tape
contract (access outside the tape is an error, movement is not), shipped in
#4 and stands on its own at 8–11%.

**Code:** PR #5 (branch `experiment/offset-addressing`), kept open and not
merged. It carries a `GYRUS_NO_FOLD=1` toggle that disables the pass at
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
