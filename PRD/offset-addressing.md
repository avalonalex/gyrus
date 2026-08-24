# PRD: Offset Addressing (Lazy Pointer Motion)

**Status**: Not Started
**Last Updated**: 2026-08-24
**Priority**: High — the largest interpreter win left before a compiler backend

## Summary

Give cell operations an offset from the pointer, so that a straight-line run of
instructions stops moving the pointer at all and does its work in place. On the
benchmark corpus this deletes 17–30% of the optimized instruction stream, and
the savings sit at the nesting depths where execution concentrates.

The transform is standard. What it depends on is a change to the tape contract:
today the pointer may not leave the tape even momentarily, and that rule is
enforced by the very moves this transform deletes. The contract becomes
*outside-bounds **access** is an error; moving the cursor outside the bounds,
with no effect, is fine*. That is simpler to state, cheaper to enforce, fixes a
silent corruption in the existing `allow_negative_pointer` option, and is a
prerequisite rather than a side effect — it lands first, in both interpreters,
on its own.

## Motivation

After the fused-run work and the limit-check work, `mandelbrot`'s profile is
~100% the dispatch loop with no hot callee: ~62% the instruction match, ~15%
step counting, ~15% loop overhead. There is nothing left to make faster inside
an instruction.

Pointer moves are 35–51% of the optimized instruction stream across the corpus,
and 67% of *executed* instructions on mandelbrot. They are already as cheap as
an instruction can be — one add, one compare, both inlined. The remaining cost
is the dispatch itself, so the win is not making moves faster. It is not
dispatching them.

Measured statically over the corpus (`optimize()` output, moves that the
transform can delete outright):

| program | instrs | moves | of those, foldable | as % of all instrs |
|---|---:|---:|---:|---:|
| mandelbrot | 2373 | 51% | 33% | **17%** |
| hanoi | 7797 | 46% | 50% | **23%** |
| 99beer | 709 | 42% | 72% | **30%** |
| squares | 102 | 41% | 57% | **24%** |
| bf2c | 472 | 35% | 64% | **22%** |

Savings by loop nesting depth, mandelbrot (`saved/total` instructions):

```
d0: 5/29   d1: 8/64   d2: 25/190  d3: 71/503
d4: 115/650  d5: 67/411  d6: 68/330  d7: 32/156  d8: 8/40
```

The mass is at depth 3–6, which is where execution time is, so the dynamic
reduction should be at least the static one. Note this is a *lower* number than
the 25–40% figure quoted when this idea was first raised — that estimate was not
evidence-based, and the table above supersedes it.

## Requirements

**Functional**

- Output byte-identical to today on the whole corpus.
- The optimized and `--debug` interpreters must still agree, under both cell
  models, as `benchmark.sh` enforces.
- Every read or write outside the tape still errors, in both interpreters, with
  the message it has today. Cursor movement outside the tape stops erroring,
  deliberately and by contract.

**Non-functional**

- ≥15% wall clock on mandelbrot and hanoi.
- No regression for programs with few pointer moves.
- No new dependency; no `unsafe`.

## Design

### The transform

Within a straight-line run, carry a running offset. Each cell operation is
rewritten to act at `ptr + offset`; pointer moves are deleted; one net move is
emitted at the end of the run if and only if the run leaves the pointer
somewhere new.

```
Right(3) Add(1) Left(3) Right(5) Sub(2) Left(5)    6 instructions, 4 moves
    ->  Add(1)@+3  Sub(2)@+5                       2 instructions, 0 moves
```

### Run boundaries

A run ends at anything that needs a materialized pointer:

- `Loop` entry and exit — `[` and `]` read `memory[ptr]` to decide.
- `SeekRight` / `SeekLeft` — they search from the real pointer and move it by an
  amount not known until run time.
- End of block.

`MultiplyAdd` is already offset-relative and leaves the pointer where it found
it, so it could carry a base offset rather than ending a run. Treat it as a
boundary in v1 and relax it once the rest is measured.

### IR change

Add an offset to the variants that touch a cell, rather than adding new
variants — offset `0` is exactly today's behaviour, so the pass is optional and
the interpreter keeps one code path per operation:

```rust
Add(u8, i32, SourceRange)
Sub(u8, i32, SourceRange)
Zero(i32, SourceRange)
Output(i32, SourceRange)
Input(i32, SourceRange)
```

`OptimizedInstruction` is 48 bytes today, sized by
`MultiplyAdd(Vec<(isize, i32)>, SourceRange)`. The smaller variants have padding
to spare, so this should not grow the enum — assert it with `size_of` so a later
variant cannot quietly regress the instruction stream's cache footprint.

### The tape contract becomes about access, not position

Today the pointer move performs the bounds check, so the rule in force is *the
pointer may never point outside the tape*. The new contract:

> **Reading or writing a cell outside the tape is an error. Moving the cursor
> outside the tape is not — a cursor that points nowhere valid, and is never
> used, has no effect.**

The rule is uniform: only access counts. There is no separate rule for where a
run comes to rest, no rule about how far the cursor may travel, and nothing for
the optimizer to check when it deletes a move. That is the whole reason this is
worth stating as a language contract rather than handling as an optimizer
detail: the alternative is machinery in the optimizer to reproduce a rule that
was never worth having.

What changes:

```
$ gyrus --memory-size 5   '>>>>>>>>>><<<<<<<<<<+.'
Error: Memory pointer out of bounds at instruction 0     # today
                                                          # under the contract: prints \x01
```

What does not change: every read and write outside the tape still errors, at the
access, with the message it has today. For unbounded memory, the tape grows to
cover cells that are actually accessed — moving far to the right no longer
allocates a tape the program never touches, which is a small correctness
improvement in its own right.

**It also fixes a silent corruption.** The existing `allow_negative_pointer`
option does not let the cursor go negative; it makes
`<` at cell 0 *stay* at cell 0. So an excursion off the left end aliases cell
-1 onto cell 0, and a write lands on real data:

```
'<+.'  with allow_negative_pointer(true)  ->  output [1]
'+.'   with no excursion at all            ->  output [1]     # identical
```

Verified against the current binary. The program believes it wrote cell -1 and
has in fact overwritten cell 0, with no diagnostic. Under the contract, `<`
moves the cursor to -1, which is fine, and the `+` is a write outside the tape,
which errors. The option becomes redundant and should be removed with it.

**Consequences to work through:**

- `MemoryAddress` is `pub struct MemoryAddress(pub usize)` and cannot represent
  a position left of the tape. It has to become signed. That is a public type,
  so this is a breaking change — acceptable while the crate is unpublished, and
  cheaper to take now than after release.
- `allow_negative_pointer` and `with_negative_pointer` are subsumed and should
  go. Their only current effect is the aliasing above.
- `peak_memory_used` derives from the peak *pointer*. It should derive from the
  peak cell actually accessed, or a program that walks far away without touching
  anything will report memory it never used.
- The seeks read every cell they test, so they are unaffected in substance: a
  seek that runs off the tape still errors, now at the read rather than the move.
- Error text should say the cell that was accessed rather than the move that was
  attempted, since the move is no longer what failed.

**Both interpreters must adopt it.** `--debug` is the reference the optimized
path is differentially tested against, by `benchmark.sh` and by the
generated-program harness. A contract adopted by one path only is not a contract,
and it would make the differential meaningless. The tree-walker checks each
single-cell move, so it changes in the same way: the cursor becomes signed and
the check moves to the access sites.

This is a language change that happens to enable an optimization, so it lands
first, on its own, with its own tests — not folded into the optimizer commit.
Sequencing it that way also means the optimizer work needs no guard design at
all, because by the time it starts there is nothing to guard.

Scale of the re-baselining: of 400 generated programs on a 32-cell tape, 181
hit `MemoryOutOfBounds` today, though most are genuine out-of-range accesses
that still error under the contract. None of the seven corpus programs hits it
at the default size.

### Step counting

Folding removes instructions, so `total_steps` falls and `--max-steps` bounds
shift — the same way folding `[-]` into `Zero` already collapses a whole loop
into one step. Consistent with the documented model; the `# Limitations` list on
`interpret_optimized` needs a line saying offsets fold moves away.

### Interactions

- **Cell models**: none. The transform changes *where* an operation acts, not
  what it computes, so `optimize_with_cell_model`'s multiply-fold gate is
  unaffected.
- **Hooks**: see `optimizer-hook-integration.md`. Offsets make the
  instruction→source mapping coarser — a folded operation keeps its own
  `SourceRange`, but the moves it absorbed no longer have instructions of their
  own. Read the two documents together before either lands.
- **Seeks**: unaffected; they are run boundaries.

## Implementation plan

0. **The tape contract, on its own.** Signed `MemoryAddress`; checks moved to
   the access sites in both interpreters; `allow_negative_pointer` removed;
   `peak_memory_used` derived from accesses; error text updated; docs updated.
   Re-baseline the differential. This is a prerequisite, not a phase of the
   optimizer work.
1. **Spike.** Implement the pass and the interpreter side crudely and measure
   mandelbrot and hanoi. The estimate is 17–30% fewer
   instructions; confirm that converts to wall clock before building it
   properly. If it does not, stop here and record why.
2. IR change, with the `size_of` assertion.
3. `fold_offsets` pass, applied recursively to loop bodies, after the existing
   fusion and pattern recognition.
4. Differential: the 1600-run generated-program harness across all four
   configurations, plus the boundary programs named here.
5. `benchmark.sh` golden outputs must not move, and both cell-model
   differentials must still pass.

## Success criteria

- ≥15% wall clock on mandelbrot and hanoi; no regression on move-light
  programs.
- Byte-identical corpus output; out-of-range *accesses* still error, in both
  interpreters, with the same message they do today.
- Optimized and `--debug` agree under both cell models.
- `OptimizedInstruction` no larger than 48 bytes.

## Risks

- **The contract change is larger than the optimization it enables.** It
  touches the tree-walker, both memory models, a public type, and a public
  config option, and it changes observable behaviour for a class of programs.
  It is worth doing on its own merits — it is a simpler rule and it fixes a
  silent corruption — but if it were rejected, the optimizer work would need a
  guard design instead: carry the run's extreme offsets on its first folded
  instruction, which costs no extra dispatch but puts a field on the IR that is
  meaningful only sometimes. Decide before starting, not halfway.
- **Short runs.** Most runs relocate the pointer (816 of mandelbrot's 874), so
  they keep one move and save only what they had beyond it. The win comes from
  runs with several moves — chiefly balanced loop bodies, which fold to no
  motion at all. A corpus of mostly short runs would see little.
- **Programs relying on the old rule.** Anything that used out-of-bounds
  movement as a deliberate trap stops erroring. Nothing in the corpus does, and
  the replacement rule catches strictly more real faults — the aliasing above
  was undetectable before — but it is a behaviour change and belongs in the
  release notes, not just the docs.

## Dependencies

The tape contract (step 0) is a hard prerequisite, and it reaches well outside
the optimizer: `types.rs` (`MemoryAddress` becomes signed), `config/memory_model.rs`
and `config/execution_config.rs` (checks move to access sites,
`allow_negative_pointer` removed), `interpreter/execution.rs` and
`interpreter/optimized.rs` (both adopt it), plus `docs/execution-models.md` and
the differential baselines.

The optimizer work itself (steps 1-5) is then confined to `optimizer.rs` and
`interpreter/optimized.rs`.

Independent of the compiler-backend PRD. Should be read alongside
`optimizer-hook-integration.md`, which touches the same instruction-to-source
mapping.
