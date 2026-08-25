# PRD: Proving the Parts Tests Cannot Reach

**Status**: Not started — recorded for later, deliberately not scheduled
**Last Updated**: 2026-08-25
**Priority**: Low — after the TUI debugger, and only the narrow half of it

## Summary

Add bounded verification to the handful of places in gyrus where a bug is a
*universally quantified arithmetic fact* rather than something a test can
sample: the JIT's bounds guards, and the optimizer's fold conditions. Use
[Kani](https://model-checking.github.io/kani/), which is a cargo subcommand and
needs no second language.

This is the narrow half of "provably correct". The grand half — proving the
optimizer semantics-preserving in a proof assistant — is discussed under
[Not proposed](#not-proposed-and-why), because it is a different project and
should be started as one if it is ever started.

## Motivation

gyrus has had exactly two real correctness bugs. Both are the same shape, and
neither was caught by the test suite that exists to catch them.

**1. `fuse_sets` folded past 255 under checked cells.** Folding `[-]` and a
following run of `+` into one `Set` is valid only while the sum stays under
256; past that, under `CellModel::U8Checked`, the overflow is the thing the
program existed to report. The guard was wrong. It was found by a code review.
The 400-seed, three-engine differential missed it, because no generator could
produce a value near a cell boundary — every idiomatic fragment wrote
single-digit values. The generator was fixed afterwards, and now catches it in
under two seconds, which is the right outcome; but the general problem remains
that a fuzzer only finds what its grammar can express.

**2. A bounds guard one stride too generous.** Demonstrated during the seek
unrolling work: loosening the guard by one stride fails **no test at all**. The
reason is structural rather than a gap in coverage — a speculative read never
decides the answer on its own, because the one-at-a-time path re-checks every
cell the seek actually stops on, so an over-read only changes behaviour if the
byte past the tape happens to be non-zero. With `MemFlags::trusted()` the loads
are `notrap`, so the failure mode is silent memory corruption rather than a
crash. The fallback was a `debug_assert!` in the translator tying the guarded
reach to the reads emitted.

Both are small, arithmetic, and quantified over every cursor, tape length and
stride. That is precisely what a model checker settles and what sampling
cannot: the first needed a value at a boundary the generator could not reach,
and the second is invisible to observation entirely.

The `debug_assert!` is the honest marker of where this belongs. It is a
statement of a property, checked on the paths that happen to run, in a
language that cannot express "for all".

## Requirements

- **R1** — The seek's guard is proved sufficient for the reads it protects,
  for every cursor, tape length, stride and unroll width.
- **R2** — Each optimizer fold is proved to preserve behaviour under *both*
  cell models, or proved to be declined under the model where it would not.
  `fuse_sets` is the worked example; the others follow the same shape.
- **R3** — The newtype arithmetic in `types.rs` (`MemoryAddress(isize)`,
  `MemorySize(usize)`) is proved not to overflow, wrap, or produce an index
  that looks valid and is not.
- **R4** — Verification runs in CI, or is explicitly excluded with a reason.
  A proof nobody runs decays exactly like an unexecuted claim in the docs, and
  this repository already has five scripts that exist because of that.
- **R5** — Nothing in the shipped crates depends on the verifier. Harnesses are
  `#[cfg(kani)]` and the normal build does not know they exist.

## Design

### Why bounded model checking rather than a proof assistant

The properties above are arithmetic over machine integers with small bounds.
That is the shape bounded model checking settles automatically, and the shape
proof assistants make you work for. Kani compiles the Rust itself, so there is
no second model of the code to keep in step — which matters here more than
usual, because the last two PRDs in this directory were deleted for resting on
premises the code had moved past.

### Candidate harnesses, in the order they are worth writing

1. **The seek guard.** The property the `debug_assert!` stands in for: given a
   cursor, a tape length, a stride and `SEEK_UNROLL`, if the guard passes then
   every offset the unrolled step reads is on the tape. Small, self-contained,
   and it is the one place a mistake corrupts memory silently.
2. **`fuse_sets` and its siblings.** For all `v`, `n` and both cell models: the
   folded program and the unfused program agree, or the fold does not fire.
   This is the bug that got through, stated as a theorem.
3. **The tape newtypes.** `MemoryAddress::index` against a `MemorySize`, for
   every input including negative cursors and sizes near `usize::MAX`.
4. **Cell arithmetic.** `try_increment`/`try_decrement` under each model:
   wrapping wraps, checked reports, and neither ever silently does the other.

### The friction to expect

Kani pins its own Rust toolchain. This repository pins 1.97.1 in
`rust-toolchain.toml` deliberately, because clippy's lint surface moves and an
unpinned toolchain turns unrelated changes red. Those two pins will not be the
same version. The likely shape is a separate CI job that installs Kani's
toolchain and runs only the harnesses, leaving the main gates on the pinned
compiler — the same separation `scripts/check-msrv.sh` already uses. **Confirm
this is tolerable on a small harness before writing more than one**, because if
it is not, that is the whole feature's viability and it is cheap to find out
first.

## Not proposed, and why

**Proving the optimizer semantics-preserving in Lean or Rocq.** State a formal
semantics for BrainFuck and for `OptimizedProgram`, then prove
`execute(optimize(p)) == execute(p)` for all `p`.

BrainFuck is an unusually good target for this, and that is worth saying
plainly rather than dismissing it: the entire semantics is a tape, a cursor and
eight instructions, formalisable in an afternoon, where the equivalent for C
took CompCert years. The optimizer is about six folds, each a small theorem. If
the goal is *learning verified compilation*, it is hard to name a better
vehicle, and learning is what this project is for.

It is not proposed here because it is a different project with a different
shape: the proofs live outside Rust, so there is a refinement gap between the
verified model and the shipped code unless the code is extracted from the proof
or re-derived against it; and it does not compose with the work above, which is
in-language and incremental. If it is ever wanted, it should be started
deliberately as its own thing, not grown out of a Kani harness.

**Verifying the JIT's machine code.** Proving Cranelift's output implements the
IR is CompCert-scale, and Cranelift is not ours. Out of proportion to
everything else here.

## What this would not have helped with

Worth recording, because the honest case for verification is narrower than the
enthusiasm for it.

Of everything that went wrong during the JIT optimization work, verification
addresses the two bugs in Motivation and none of the rest: a change that made
the program 1.5% slower, a profile that described a program that no longer
existed, four unchecked copies of a speedup figure that drifted apart, a
comment whose premise had quietly become false, and a tuning curve that was an
artifact of the thing being tuned. Those were caught by measurement, review,
and scripts.

Verification is narrow and deep. This codebase's actual failure mode has mostly
been broad and shallow. Both are worth spending on; only one of them is
fashionable.

## Success Criteria

- The seek-guard harness exists, passes, and **fails when the guard is
  loosened by one stride** — the mutation that no test catches today. Until
  that has been demonstrated, the harness proves nothing about the harness.
- The `fuse_sets` property holds under both cell models, and fails if the
  boundary condition is removed.
- Verification runs somewhere automatic, or `docs/development.md` says why not.
- The `debug_assert!` in `seek` either becomes redundant and is removed with a
  note pointing at the proof, or stays with a comment saying what the proof
  does and does not cover.

Deliberately not criteria: a count of harnesses, a percentage of the codebase
"verified", or verifying anything whose failure a test can already observe.

## Dependencies

- The TUI debugger comes first; this is not competing for that slot.
- No code dependency. The properties above are all statements about code that
  exists today and is unlikely to move: the guard, the folds, and the newtypes.
