# PRD: Cranelift JIT

**Status**: Not started — design current as of the optimizer work in #8–#11
**Last Updated**: 2026-08-24
**Priority**: High — the largest remaining win, and the interpreter is close to done

## Summary

Compile the optimized IR to native code with Cranelift and run it, as a
fourth execution mode behind `--jit`. The IR it consumes is the one the
optimizer already produces -- fused runs, `Set`, `MultiplyAdd`, strided seeks
-- and every rule the optimized interpreter enforces (the tape contract, the
cell models, the memory models, EOF behaviour, limits) is kept, with the same
error messages, because the tree-walker stays the reference every mode is
differentially tested against.

This replaces two earlier documents. Their case for Cranelift over LLVM
stands; their code was written against Cranelift 0.109 (current: 0.135), their
bounds checks were on the wrong thing, their error mechanism would kill the
process, and their performance claims were off by two orders of magnitude.
The parts that survived are in here.

## Motivation

Where things are, measured on 2026-08-24:

| | interpreter (`main` after #11) | native ceiling |
|---|---:|---:|
| mandelbrot | 2.8 s | **0.57 s** |
| hanoi | 0.19 s | ~0 ms |

The ceiling is a naive BF→C translation at `clang -O3`, output byte-identical.
The interpreter's own profile says what is left in it: the per-iteration
recursive block call (~20%), the step-count memory chain (~30%) and the
jump-table dispatch (~25%) -- a flat IR with registers would buy perhaps 2×
and that is the end of the road. A JIT removes all three at once, and the
work that fed the interpreter (#8–#11) is exactly the input it wants:
Cranelift does not fuse loads and stores across BrainFuck's one mutable
variable, so folding has to happen before translation, and it already does.

Expected: **mandelbrot 0.7–1.2 s** (2.5–4× over today), hanoi tens of
milliseconds. Not the 100–1000× the previous document promised; most of that
distance has been covered by the optimizer since it was written.

## Requirements

**Functional**

- `gyrus --jit program.bf` produces the bytes the other modes produce, on the
  whole corpus, and `benchmark.sh` gains a JIT column and a JIT differential.
- Every runtime error the interpreter reports -- out-of-tape access, cell
  overflow/underflow under checked cells, I/O errors, EOF-as-error, step and
  time limits -- is reported under `--jit` as the same `BfError`, with the
  same cursor, instruction index, memory dump and hint. Rich errors are the
  reason this project exists; a fast mode that dies with SIGILL is not a
  mode.
- Both cell models and both memory models work. The multiply fold's cell-model
  gate (`OptimizedProgram::cell_model`) is honoured.
- `ExecutionStats` is returned, with the divergences named below documented.

**Non-functional**

- ≥ 2.5× over the optimized interpreter on mandelbrot; no program slower than
  it, compile time included.
- The `gyrus` library crate stays dependency-free and keeps its MSRV.
- No `unsafe` beyond what calling JIT-generated code and its callbacks
  requires, each site with a comment saying what invariant makes it sound.

## Design

### Decisions

**One new crate, `gyrus-jit`, with its own MSRV.** Cranelift's MSRV is
"stable minus two", 1.95 today, moving every six weeks; the workspace promises
1.88 for the library. So `gyrus-jit` declares Cranelift's floor, the pinned
toolchain (1.97.1) already satisfies it, and `scripts/check-msrv.sh` excludes
it (`--workspace --exclude gyrus-jit`, and `gyrus-cli --no-default-features`)
so the library's promise is still checked. The CLI grows a `jit` cargo feature,
on by default, that adds the dependency and the `--jit` flag. Not two crates:
the translation and the runtime are one thing, and AOT, if it comes, shares
the translator by being a second entry point in the same crate.

**Checks are on access, not movement.** The tape contract (#4): moving the
cursor anywhere is legal; reading or writing outside the tape is the error.
Every load and store the JIT emits is preceded by one unsigned compare of the
cursor against the tape length -- `MemoryAddress::index` in one instruction --
and a `brif` to an exit block. The earlier design checked pointer *moves* with
`trapz`, which is both the wrong rule and, see next, the wrong mechanism.
Fused runs and `MultiplyAdd` know all their offsets statically, so a later
pass can hoist one min/max guard per straight-line run; in a JIT a guard is
two compares, which is what the interpreter could not afford.

**Failure is a branch to an exit block, never a trap.** `cranelift-jit`
installs no signal handler; a Cranelift trap is a SIGILL. Each site that can
fail gets an exit block that stores the cursor into the runtime struct and
returns a small integer identifying the site. The translator keeps a side
table `site -> (what failed, instruction index)`, and the runtime turns the
return value into the exact `BfError` the interpreter would have built --
including the memory dump, which it takes from the tape -- through the same
constructors. Source location comes from the instruction index via
`DebugInfo`, as it does today. No DWARF, no debugger protocol: the interactive
debugger has the tree-walker.

**Unbounded memory is allocated at its maximum up front.** JIT code holds the
tape base in a register, so the tape cannot move underneath it. A zeroed
allocation of `--unbounded-max` costs nothing until pages are touched, and the
bound is then a fixed tape of that size. Divergences, documented: under
`--jit`, `memory_allocated` reports the maximum and `MemoryExpanded` warnings
are not produced. Growth-on-access is an interpreter behaviour.

**Limits are checked at loop back-edges.** A step counter in the interpreter's
sense would cost a register write per instruction; back-edges are where time
goes. Each `]` decrements a per-run counter held in a Cranelift `Variable`; on
reaching zero it calls the runtime, which charges the interval to the step
count, reads the clock if `--timeout` is set, and returns whether to stop.
Under `--jit`, `total_steps` counts loop iterations, and `--max-steps` bounds
that; the interpreter's step model was already approximate and documented as
such. The sampling interval is the interpreter's (1024). Limits off means the
counter is never emitted, as `CHECK_LIMITS = false` deletes the check today.

**Statistics.** `loop_iterations` and `total_steps` from the back-edge counter;
`bytes_read`/`bytes_written` from the I/O callbacks; `peak_memory_used` from a
`umax` into a register on each access (the interpreter found this free as a
conditional move); `cells_modified` by scanning the tape at exit;
`memory_allocated` the tape length. `warnings` empty.

### Translation

One Cranelift function per program, signature `fn(*mut Runtime) -> i32`. The
runtime struct holds the tape base and length, the cursor slot written on
exit, the I/O objects, the limit state, and the counters. Entry loads base
and length into SSA values; the cursor is a `Variable` (`declare_var(I64)`),
which is how a mutable BrainFuck pointer lives in SSA.

| IR | code |
|---|---|
| `Add(n)` / `Sub(n)` | check; load i8; `iadd_imm`; store. Under checked cells: widen, add, compare against 0..=255, `brif` to an overflow/underflow exit. |
| `Set(v)` | check; store immediate. |
| `Right(n)` / `Left(n)` | `iadd_imm` on the cursor variable. Nothing else -- this is the contract. |
| `Output` | check; load; call `bf_write(rt, byte)`; `brif` on its status to an I/O exit. |
| `Input` | call `bf_read(rt)`; it returns the byte or a code for "no change" / "EOF is an error" after applying `EofBehavior`; check; store if a byte. |
| `Zero` | check; store 0. |
| `MultiplyAdd` | check source; load; for each `(offset, m)`: check target; load; `imul_imm`; `iadd`; store; then store 0 to source. Skipped entirely when the source is zero, as the interpreter does. |
| `SeekRight(s)` / `SeekLeft(s)` | header block: check; load; `brif` zero → exit; body: cursor `+= ±s`; jump header. The check inside the loop is what keeps "a seek off the tape fails at the read". |
| `Loop` | header: check; load; `brif` zero → after; body; back-edge: limit counter if armed; jump header. Three blocks, sealed in order. |

`MemFlags::trusted()` on every access: the compare before it is the proof.
`set_srcloc(SourceRange.start)` on every instruction is free and makes
Cranelift's own diagnostics readable; it is not a debugging feature.

### What the JIT does not do

- **Hooks.** The optimized interpreter has none either; the tree-walker is the
  hook and debugger path (`optimizer-hook-integration.md` is about the
  interpreter, and unaffected).
- **DWARF or in-process debugging.** See above.
- **AOT.** `cranelift-object` can emit an object from the same translator; a
  standalone binary also needs a runtime library and a linker step. Worth a
  section in this document when someone wants it, not before.
- **Growing memory.** See the allocation decision.

### Cranelift, as of 0.135

Held here so the next person does not learn them from a 2022 blog post:

- `declare_var(ty) -> Variable`, `def_var`, `use_var`; blocks via
  `create_block` / `switch_to_block` / `seal_block`, and a block is sealed once
  all its predecessors are known -- loop headers last.
- Conditional branches are `brif(cond, then, &[], else, &[])`; `brz`/`brnz`
  are gone.
- External functions: `module.declare_function(name, Linkage::Import, &sig)`,
  the symbol supplied to `JITBuilder::symbol`; `declare_func_in_func` inside
  the function; `ins().call`.
- `JITBuilder::new(cranelift_module::default_libcall_names())`, `JITModule`,
  `define_function`, `finalize_definitions()`, `get_finalized_function`;
  `free_memory` is `unsafe` because it invalidates the pointer.
- `settings::builder()` with `opt_level = speed`; measure `none` too -- for a
  program this shape the difference may be small and the compile time is not.

## Implementation plan

1. **Spike.** Wrapping cells, fixed memory, no limits, errors as exit codes
   without messages. Run mandelbrot and hanoi through `benchmark.sh`'s
   differential. The number to beat is 2.8 s; if the spike is not under
   ~1.5 s, stop and profile before building anything else -- the interpreter
   work taught that an estimate is not a measurement.
2. Errors: the side table and `BfError` reconstruction, checked against the
   interpreter's messages on `programs/errors/` and the boundary tests in
   `optimized.rs`.
3. Cell and memory models, EOF behaviours, limits, statistics.
4. `gyrus --jit`; `benchmark.sh` column and differential; `check-msrv.sh`
   exclusion; `check-readme-commands.py` will demand the flag be documented.
5. Corpus tests under `--jit`: a test in `gyrus-jit/tests` driven by
   `programs/test_manifest.toml`, same expectations as `program_corpus.rs`.
6. Docs: `docs/execution-models.md` gains the mode and its named divergences;
   `docs/architecture.md` the crate. Then delete this document.

## Success criteria

- Corpus output byte-identical across `--jit`, the optimized interpreter and
  `--debug`; both cell-model differentials agree.
- mandelbrot ≤ 1.2 s; nothing slower than the interpreter.
- Every runtime error the interpreter can raise is raised under `--jit` with
  the same message, checked by test.
- `check-msrv.sh` still passes on 1.88 for everything but `gyrus-jit`.

## Risks

- **Cranelift's moving MSRV** will one day exceed the pinned toolchain; the
  fix is a toolchain bump, which the pin's comment already describes.
- **`unsafe`.** Calling generated code and handing it a `*mut Runtime` is
  inherent. Keep it to the entry call and the two callbacks, with the
  invariants written down; everything the generated code touches is inside
  the runtime struct.
- **Error-path fidelity is the long tail.** The memory dump, the hint text,
  the loop call stack in an error (the interpreter reports one) -- each is a
  test against the interpreter's output, and step 2 is not done until they
  all pass.
- **The spike disappoints.** Cranelift is not LLVM; if 0.7–1.2 s does not
  materialise, the first suspects are the per-access bounds compare (hoist per
  run) and load/store forwarding across a run (keep the cell in an SSA value
  within a run). Both are translator changes, not architecture.

## Dependencies

`cranelift-codegen`, `cranelift-frontend`, `cranelift-module`,
`cranelift-jit` at 0.135, in `gyrus-jit` only. Nothing in `gyrus` changes
except that the JIT reads `OptimizedProgram` and `DebugInfo` through the
public API. Independent of the hook and TUI documents.
