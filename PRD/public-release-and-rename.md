# PRD: Public Release — Rename to `gyrus` and Repository Hygiene

**Status**: In Progress — Phases 0-4 complete; Phase 5 (release) remaining:
CHANGELOG, tag, flip to public, then CI
**Last Updated**: 2026-08-25
**Priority**: High (blocks first public push)

## Summary

Prepare the project for its first public release on GitHub. Two workstreams:
(1) rename `FerrousCortex` → **`gyrus`** across crates, binaries, docs, and the
repository itself; (2) close the legal, metadata, and documentation gaps that
currently make the repo unsafe or embarrassing to publish — no license file, a
GPL-licensed third-party program shipped without notice, placeholder URLs, a
committed build artifact, and docs that describe a much smaller codebase than
the one that exists.

## Motivation

### Why rename

`FerrousCortex` is the odd one out among this author's projects. The established
pattern is a single lowercase word, 4–9 letters, where a slightly obscure real
word carries a second meaning that names the domain:

| Project | Word | Domain link |
|---|---|---|
| `patina` | oxidation layer | a Rust Scheme interpreter |
| `hypo` | photographic fixer | a RAW developer |
| `irongall` | manuscript ink | a TUI framework, nodding at Ink |
| `organon` | Aristotle's logic corpus | org-mode task/knowledge manager |
| `oxidb` | oxide + db | LSM key-value store in Rust |
| `azores` | islands | a Delos shared-log implementation |
| `linecraft`, `lowres`, `wirewhirl` | compounds | plotters, tixy, dataflow |

`FerrousCortex` breaks it three ways: CamelCase, two words, and it states the
two facts the reader already has (it's Rust, it's BrainFuck).

**`gyrus`** — a fold of the cerebral cortex; also reads as *gyre*, a loop or
spiral. Brain plus loops is precisely what BrainFuck is, and it preserves the
"cortex" idea that the old name was reaching for without announcing it.

**Availability verified 2026-08-22**: `gyrus`, `gyrus-cli`, `gyrus-tool`,
`gyrus-core` are all unclaimed on crates.io (HTTP 404 on the crates API);
`github.com/avalonalex/gyrus` is free.

Names considered and rejected: `pallium` (anatomically exact but stiff),
`sulcus`, `striatum`, `brainpan` (playful, too jokey for a library), `corpuscle`
(clunky). `engram`, `myelin`, `cortex`, `axon`, `dendrite`, `ferrite`,
`medulla`, `pons`, `vellum`, `noggin` are all taken on crates.io.

### Why now

Renaming after a public push means broken links, stale clones, and a crates.io
name that can never be reclaimed. Every item below is cheaper to fix while the
repository is still private and single-author (118 commits, one identity).

## Requirements

### Functional

- **R1** — All crates, binaries, import paths, and docs use `gyrus` naming; no
  occurrence of `FerrousCortex` / `ferrous-cortex` / `ferrous_cortex` survives
  outside `PRD/archived/` (historical record, may keep the old name in prose).
- **R2** — The repository carries the licenses its manifests already claim.
- **R3** — Every bundled third-party BrainFuck program has attribution and a
  known license, and no bundled file's license conflicts with the repo's.
- **R4** — No build artifacts or generated binaries tracked in git.
- **R5** — `README.md` and `CLAUDE.md` accurately describe the code as shipped.
- **R6** — CI enforces `fmt`, `clippy`, and the test suite on every push.

### Non-functional

- **R7** — The rename is a single mechanical commit that leaves the test suite
  green (currently 302 passing: 222 lib + 25 + 6 integration + 49 doctests).
- **R8** — ~~Publishing to crates.io remains possible.~~ Superseded: the project
  is GitHub-only and the manifests set `publish = false`.

## Design

### Name mapping

| Old | New |
|---|---|
| Repo `avalonalex/FerrousCortex` | `avalonalex/gyrus` |
| Crate `ferrous-cortex` (lib) | `gyrus` |
| Crate `ferrous-cortex-cli` | `gyrus-cli` |
| Crate `ferrous-cortex-tool` | `gyrus-tool` |
| Binary `ferrous-cortex` | `gyrus` |
| Binary `ferrous-cortex-tool` | `gyrus-tool` |
| `use ferrous_cortex::…` | `use gyrus::…` |
| Directory `crates/ferrous-cortex*` | `crates/gyrus*` |

Tagline for the new README: *a BrainFuck interpreter, optimizer, and debugger in
Rust* — no "industry-strength", no "production-grade" (see S5).

### Rename scope (measured 2026-08-22)

| Pattern | Files | Occurrences |
|---|---|---|
| `FerrousCortex` | 24 | 82 |
| `ferrous-cortex` | 44 | 472 |
| `ferrous_cortex` | 45 | 157 |

41 of those files are `.rs` / `.toml`; the rest are markdown and `programs/`
comments. Concentrations: `crates/ferrous-cortex/src` (14 files),
`examples/` (13), `internal/` (10), `programs/utilities/` (8), `PRD/` (16
including `archived/`).

### Release blockers

**B1 — No license file.** Both manifests declare `license = "MIT OR Apache-2.0"`
(`crates/ferrous-cortex/Cargo.toml:6`, `crates/ferrous-cortex-tool/Cargo.toml:7`)
but no `LICENSE-MIT` or `LICENSE-APACHE` exists, and `README.md:996` reads
`[Add your license here]`, with `[Add contribution guidelines here]` below it.
Fix: add both license texts, a `license-file`-free dual-license note in the
README, and fill the contributing section (or delete it).

**B2 — Third-party programs shipped without provenance.**
`programs/advanced/factor.bf` is **GPL**: "Copyright (C) 1999 by Brian Raiter /
under the GNU General Public License". A GPL file inside an MIT/Apache
repository is a licensing conflict for anyone redistributing the tree. Also
uncredited or under-credited: `life.bf` (© 2021 Daniel B. Cristofani),
`oobrain.bf` (© 2003 Chris Rathman), `mandelbrot.bf` (Erik Bosman),
`99beer.bf` (jim crawford), `char.bf` (Jeffry Johnston), `hanoi.bf` (unknown).
`programs/README.md` credits only Cristofani's collection (lines 43-63, 156-158).
Fix: move third-party programs to `programs/third-party/` with a `CREDITS.md`
listing author, source URL, and license per file; drop `factor.bf` unless its
GPL status is acceptable and clearly fenced; state in the root README that the
repo license covers the Rust code, not the bundled BF programs.

**B3 — Placeholder repository URLs.** `repository =
"https://github.com/yourusername/ferrous-cortex"` in
`crates/ferrous-cortex/Cargo.toml:7` and
`crates/ferrous-cortex-tool/Cargo.toml:8`; also
`internal/architecture-status.md:657`. Fix as part of the rename.

**B8 — `benchmarks/` held a broken native reference.** ✅ Resolved 2026-08-23.
The directory contained `mandelbrot.rs`, a Rust program whose header claimed it
"generates the same ASCII art as mandelbrot.bf". It does not. The BF program
emits a clean 48x129 grid of `A`-`Z`, `[`, and space; the Rust program emits
variable-length lines (129-132 bytes) containing non-ASCII bytes, because
`b'A' + iteration` runs past `Z` and wraps beyond 127. Beyond the bug, the
coordinates do not match either: a parameter sweep reproduces at most 37% of
the reference cells in f64, or 52% with fixed-point arithmetic, because
Bosman's program uses a fixed-point scheme that would have to be reverse
engineered out of 11 KB of dense BrainFuck to port faithfully.

It was deleted rather than fixed: the thing worth benchmarking is BrainFuck
execution, not a native baseline. `benchmarks/` now holds golden outputs for
`scripts/benchmark.sh`.

**B4 — Committed build artifact.** `benchmarks/mandelbrot` is a 460 KB Mach-O
arm64 executable tracked in git. Delete it, add it to `.gitignore`, and — while
the repo is still private — decide whether to strip it from history (see Risks).

### Should-fix before release

**S1 — Manifest inconsistency.** `ferrous-cortex-tool` pins its own
`version = "0.3.0"`, `edition`, `license`, and `authors = ["FerrousCortex
Contributors"]` while the workspace is `0.2.0` and the other crates inherit.
Fix: move `license`, `repository`, `authors`, and `rust-version` into
`[workspace.package]`; inherit everywhere; drop the fictional "Contributors"
attribution; settle on one version for the 0.3.0 release.

**S2 — No declared MSRV.** ✅ **Resolved: `rust-version = "1.88"`, verified.**

The obvious guess was wrong. Edition 2024 puts the floor at 1.85, but 1.85 does
not build this workspace: `src/interpreter/mod.rs`, `src/error.rs`,
`src/hooks/builtin.rs`, and `src/interpreter/dispatch.rs` use **let-chains** in
12 places, and those stabilized in **1.88**. Building on a 1.85 toolchain fails
with 12 x E0658; on 1.88 everything compiles — lib, binaries, tests, benches,
examples — and all 302 tests pass.

This is why the number is measured rather than reasoned about. The payoff is
the error an old toolchain now gets:

```
error: rustc 1.85.1 is not supported by the following packages:
  gyrus@0.3.0 requires rustc 1.88
```

instead of a wall of unstable-feature errors.

Note that `rust-version` (the minimum a consumer needs) and
`rust-toolchain.toml` (the compiler this repo develops and gates against, 1.97.1)
are different facts and are both declared.

**Automated**: `scripts/check-msrv.sh` reads the version out of `Cargo.toml`,
installs that toolchain if absent, and builds `--workspace --all-targets`
against it. The version is read from the manifest rather than restated, so the
check cannot drift from the thing it checks; bumping the MSRV is a one-line
manifest edit. Verified in both directions — it passes on 1.88 and fails with a
clear message when the manifest is set back to 1.85.

**S3 — No CI.** ⏳ **Written and verified locally, but deferred to Phase 5.**
The account has no Actions minutes for private repositories, so a committed
workflow would either not run or fail on billing until the repo goes public.
The workflow is parked in Phase 5 below and lands with the flip to public. Every
gate it runs passes locally today under the pinned toolchain: 302 tests, fmt
clean, `clippy --all-targets --all-features -- -D warnings` exits 0, release
build plus CLI smoke test.

**S4 — Documentation drift.** ✅ Resolved, and the fix was structural: test and
line counts are now *removed* from the docs rather than corrected, because they
go stale on every commit — the stale figures found here (166 tests when there
were 302; "1,502 lines" for a 12,968-line library) are what that policy is meant
to prevent. Original finding follows. `CLAUDE.md` claims 166 tests (actual: 302) and
lists 10 modules, but `crates/ferrous-cortex/src/` also contains `optimizer.rs`,
`codegen.rs`, `io.rs`, `types.rs`, `debug.rs`, plus `interpreter/` and `hooks/`
subdirectories. `README.md:986-987` still lists "Performance optimizations" and
"JIT/AOT compiler backend" as unchecked roadmap items while an optimizer and an
optimized interpreter exist. Newcomers judge the project by exactly these files.

**S5 — Overclaiming.** ✅ Resolved in the README, both crate descriptions, and
the programs docs. The GitHub repo description was listed as done here but was
not — it still read "production strength tooling for the BrainFuck programming
language" on 2026-08-25, which is the README's joke with the punchline missing.
Fixed then, to the full line. A claim recorded as resolved in a status document
is not a resolved claim; this is the third instance in this file alone. Original finding follows. "An industry-strength BrainFuck interpreter/compiler"
(`README.md:3`) and "production-grade" (library crate description) invite
eye-rolls. The `azores` README is the model to copy: state plainly what it is,
that it's a learning-driven project, and that it was written with AI assistance.

**S7 — Facts that rot.** ✅ **Addressed generally, not case by case.** Two
claims in this repo went stale silently and for the same reason: nothing
executed them. The MSRV was wrong (1.85 declared by inference, 1.88 in fact),
and the README documented `gyrus --validate` / `gyrus --minify` for however long
it had been since those moved to `gyrus-tool`. Both are now scripts that fail
loudly, wired into the parked CI workflow:

- `scripts/check-msrv.sh` — builds the workspace on the declared MSRV
- `scripts/check-readme-commands.py` — extracts every documented `gyrus` /
  `gyrus-tool` invocation and checks its flags against clap's `--help`

Both were tested against the actual historical bugs and do catch them. The
principle for anything added later: a claim in the docs that a script can check
should be checked by a script, because a claim nobody executes is a claim that
will eventually be false.

**S6 — Doc directory triage.** ✅ **Done 2026-08-23, aggressively.** The
repository carried ~22,800 lines of Markdown against ~13,000 lines of Rust.
Now ~8,500:

- `internal/` (6,043 lines) — **deleted**. Milestone records
  (`hook-system-complete.md`), progress logs, and status docs that contradicted
  the code. Two files earned a place in `docs/` first: the debug-tools usage
  guide and the optimizer design notes.
- `PRD/archived/` (6,333 lines) — **deleted**. Completed features are described
  by the code; the reasoning is in git history.
- `README.md` — **1,081 lines to 143**. The reference material moved to
  `docs/`, split by topic; the landing page keeps the pitch, one error message,
  a quick start, and links.
- `ARCHITECTURE.md` — moved to `docs/architecture.md`, absorbing the README's
  project-structure section and the salvaged optimizer design.
- `PRD/optimization-and-advanced-features.md` (2,544 lines) — **deleted** after
  extracting the one part unique to it. Its four categories each restated a
  focused PRD; the hook-integration design is now
  `PRD/optimizer-hook-integration.md`.
- `PRD/debug-symbols-and-runtime-diagnostics.md` — **deleted**, shipped.
- `PRD/TESTING.md` — moved to `docs/testing.md`; it describes what exists, so it
  is not a PRD.

The new rule, recorded in `PRD/README.md` and `CLAUDE.md`: PRDs describe what
does not exist yet and are deleted on completion rather than archived. The move
broke five Markdown links (two already broken beforehand), which is why
`scripts/check-doc-links.py` now exists.

**S6 (original finding).** `internal/` (15 files) and `PRD/` (10 active +
11 archived) include session artifacts — `hook-system-complete.md`,
`phase2-debug-test-results.md`, `cli_refactoring_complete.md`. Per `CLAUDE.md`,
outdated PRDs should be purged aggressively. Decide per file: keep as design
documentation, archive, or delete.

## Implementation Plan

### Phase 0 — Decisions ✅ RESOLVED 2026-08-23

- Name: **gyrus**.
- Third-party programs: **attribute, do not purge**. Mere aggregation (GPL §5)
  means a GPL program can sit in an MIT repo without affecting the Rust code's
  license; and the files carrying explicit grants (GPL, CC BY-SA) are the
  *strongest* ones to redistribute, while the unlicensed bylined programs are
  the weakest. Purging would have cost the benchmark corpus for no legal gain.
- `benchmarks/mandelbrot`: **untracked going forward**, blob left in history.
- **Commit email: rewritten.** Resolved 2026-08-25. All 160 commits moved from
  `yuhanhao@gmail.com` to `1143520+avalonalex@users.noreply.github.com` with
  `git filter-repo`, trees verified byte-identical, force-pushed to `main`.
  Known limit, accepted deliberately: GitHub keeps every PR's commits at
  `refs/pull/N/head` regardless of what `main` says, so PRs #1-#19 still carry
  the old address for anyone who fetches those refs. Scrubbing that too would
  mean deleting and recreating the repository, which costs all 19 PRs and their
  discussion — where a good deal of this project's reasoning lives, the
  negative results especially. Not worth it.
- **crates.io: not publishing.** Decided 2026-08-23. This is a learning
  project, not a dependency anyone should take on, and a registry name is a
  commitment to maintain. `publish = false` in `[workspace.package]` makes
  `cargo publish` fail rather than relying on nobody running it. This closes
  the two packaging blockers below, which only ever mattered for publishing.

### Phase 0 (original) — Decisions (blocking, human)

- Confirm `gyrus` and reserve nothing (crates.io names are claimed by publishing,
  not reserving).
- Decide `factor.bf`: drop, or keep fenced under `programs/third-party/` with a
  GPL notice.
- Decide git-history rewrite for `benchmarks/mandelbrot` (yes/no).
- Decide commit email: 118 commits use `yuhanhao@gmail.com`; switch to GitHub's
  noreply address going forward, rewrite, or accept it as public.
- Decide whether v0.3.0 publishes to crates.io or is GitHub-only for now.
  → Resolved: GitHub-only, permanently.

### Phase 1 — Rename ✅ COMPLETE (eec7c76)

1. `git mv crates/ferrous-cortex crates/gyrus` (and `-cli`, `-tool`).
2. Update the three `Cargo.toml` `name`/`[[bin]]` fields plus the workspace
   `members` list and `[workspace.dependencies]` path entry.
3. Sweep `ferrous_cortex` → `gyrus` in `.rs`, `ferrous-cortex` → `gyrus` in
   `.toml`/docs, `FerrousCortex` → `gyrus` in prose (leave `PRD/archived/`).
4. Fix `repository` URLs to `https://github.com/avalonalex/gyrus` (closes B3).
5. Verify: `cargo fmt --check && cargo clippy --workspace --all-targets &&
   cargo test --workspace` — expect 302 passing, and grep to zero outside
   `PRD/archived/`.
6. Rename the GitHub repository; GitHub keeps the redirect from the old name.

### Phase 2 — Legal ✅ COMPLETE (0b91b16, 6d67b85)

1. Add `LICENSE-MIT` and `LICENSE-APACHE` at the repo root.
2. Create `programs/third-party/` + `CREDITS.md`; move and annotate the borrowed
   programs; resolve `factor.bf` per Phase 0.
3. README license section: dual license for the Rust code, separate note for
   `programs/third-party/`.

### Phase 3 — Hygiene ✅ COMPLETE except CI (c58a12f, c860cd7)

1. `git rm --cached benchmarks/mandelbrot`, gitignore `benchmarks/mandelbrot`.
2. Consolidate manifest metadata into `[workspace.package]`; add `rust-version`;
   unify on `0.3.0`.
3. ~~Add `.github/workflows/ci.yml`~~ — moved to Phase 5: no Actions minutes on
   a private repo.
4. Fix the one clippy warning so `-D warnings` passes.

### Phase 4 — Docs ✅ COMPLETE except doc-directory triage (S6)

Beyond the planned work, the pass turned up documentation that was actively
wrong rather than merely stale: the README documented `gyrus --validate` and
`gyrus --minify`, which moved to `gyrus-tool` subcommands and now fail outright.
Every command line in the README was checked against the binaries' `--help`
output; all documented flags now exist. A blanket "95%+ size reduction" claim
for minification was also false for dense programs (49.6% on `life.bf`) and now
states what it depends on.

1. Rewrite the README opening: honest tagline, AI-assistance disclosure, a
   30-second quickstart, one screenshot or code block of the highlighted error
   output (the project's most distinctive feature).
2. Sync the roadmap with what exists (optimizer, codegen, hooks, tool CLI).
3. Update `CLAUDE.md`: module list, test count, current status.
4. Triage `internal/` and `PRD/`; update `PRD/README.md`.

### Phase 5 — Release

1. Tag `v0.3.0`, write a short `CHANGELOG.md` covering 0.1 → 0.3.
2. Flip the repository to public.
3. **Add CI** once public (Actions is free for public repositories). Commit the
   workflow below verbatim as `.github/workflows/ci.yml`.

   Refreshed 2026-08-25, because the version parked here in August had already
   rotted: it predated the `gyrus-jit` crate and three of the five check
   scripts, and its smoke test never exercised the JIT. Every gate below was
   re-verified locally on that date under the pinned toolchain — 383 tests
   passing, fmt clean, `clippy --all-targets --all-features -D warnings` exit 0,
   `check-msrv.sh` green on 1.95 including `gyrus-jit`, and the three Python
   checks passing.

```yaml
name: CI

on:
  push:
    branches: [ main ]
  pull_request:
    branches: [ main ]

env:
  CARGO_TERM_COLOR: always

# Every job compiles with the version in rust-toolchain.toml — rustup honors
# that file for each cargo invocation, so CI and local development cannot
# drift apart. `rustup show` materializes it; there is deliberately no
# toolchain version anywhere in this file, because a second copy is a second
# thing to forget when bumping.

jobs:
  test:
    name: Test Suite
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest]

    steps:
    - uses: actions/checkout@v4
    - name: Install Rust
      run: rustup show
    - uses: Swatinem/rust-cache@v2
    - name: Run tests
      run: cargo test --workspace
    # The integration corpus reads programs/ from the workspace root, so a
    # release build plus one real program catches path and packaging breakage
    # that the unit tests cannot.
    - name: Build release
      run: cargo build --release --workspace
    - name: Smoke test the CLI
      run: |
        test "$(./target/release/gyrus programs/basic/hello_world.bf)" = "Hello World!"
        # The JIT is a separate engine reached only through this flag; a unit
        # test cannot catch the CLI failing to wire it up.
        test "$(./target/release/gyrus --jit programs/basic/hello_world.bf)" = "Hello World!"
        ./target/release/gyrus-tool minify programs/basic/hello_world.bf > /dev/null

  # Reads rust-version from Cargo.toml and installs that toolchain itself, so
  # there is no version restated here to fall out of sync with the manifest.
  msrv:
    name: MSRV (read from Cargo.toml)
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v4
    - name: Install Rust
      run: rustup show
    - uses: Swatinem/rust-cache@v2
    - run: scripts/check-msrv.sh

  # The README documented `gyrus --validate` and `gyrus --minify` for months
  # after both moved to gyrus-tool subcommands. These jobs make that class of
  # rot fail the build instead of waiting for a reader to hit it. Each script
  # exists because the claim it checks was wrong at least once.
  docs:
    name: Documented claims hold
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v4
    - name: Install Rust
      run: rustup show
    - uses: Swatinem/rust-cache@v2
    - run: cargo build --release --workspace
    - run: scripts/check-readme-commands.py
    - run: scripts/check-doc-links.py
    - run: scripts/check-tape-access.py
    # Runs each example rather than only building it: clippy already builds
    # them, and building was not enough when MemoryAddress became signed.
    - run: scripts/check-examples.sh

  fmt:
    name: Rustfmt
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v4
    - name: Install Rust
      run: rustup show
    - run: cargo fmt --all -- --check

  clippy:
    name: Clippy
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v4
    - name: Install Rust
      run: rustup show
    - uses: Swatinem/rust-cache@v2
    - run: cargo clippy --workspace --all-targets --all-features -- -D warnings
```



## Success Criteria

- [x] `git grep -iE 'ferrous[-_ ]?cortex'` returns hits only in `PRD/archived/`
      and in this document's record of the before-state.
- [x] `cargo test --workspace` green (302 tests) after the rename.
- [x] `cargo clippy --workspace --all-targets -- -D warnings` clean (four
      warnings fixed; the lint surface differs by toolchain, hence the pin).
- [x] `LICENSE` (MIT) present; README license section filled. Dual-licensing was
      dropped: MIT alone, matching the other projects in this collection.
- [x] Every file in `programs/` is either original or credited in
      `programs/third-party/CREDITS.md` with author, source, and license.
- [x] No tracked binary artifacts (verified: 0).
- [x] All three crates share one version (0.3.0) and inherit workspace metadata.
- [ ] CI green on a pushed branch — deferred to Phase 5, when the repo is
      public and Actions minutes are free. Gates verified locally in the
      meantime.
- [x] README describes the project as it is, with no unearned superlatives.

## Dependencies

- None technical. Phase 1 is mechanical; Phases 2–4 are independent of each
  other and can land in any order after the rename.
- Phase 0 decisions block Phases 2, 3, and 5.

## Risks and Open Questions

- ~~Publishing blockers~~ — two problems (`benches/` packaged without
  `programs/`, so its `include_str!` paths cannot resolve; `LICENSE` outside the
  crate directory) applied only to `cargo publish`. Moot now that the project is
  GitHub-only. They would need fixing if that decision ever reverses.

- **History rewrite** — Dropping `benchmarks/mandelbrot` from history means
  rewriting all 118 commits. Free now (private, single-author, no forks),
  impossible later. The artifact is only 460 KB, so leaving it is defensible;
  the real question is whether the commit email should also be rewritten.
- **GPL exposure** — Shipping `factor.bf` does not affect the interpreter's own
  license (no linking, no derivation), but it does mean the repository as a
  whole is not purely MIT/Apache. Cleanest resolution is removal.
- **Cristofani's collection** — the terms on `brainfuck.org` need to be read and
  quoted in `CREDITS.md` rather than assumed permissive.
- **crates.io name** — `gyrus` is unclaimed and will stay that way; someone else
  may take it. Accepted: the GitHub repository is the project's identity.
- **Name collision check** — crates.io and GitHub are clear; a quick search for
  an existing well-known "gyrus" in adjacent tooling is worth five minutes
  before committing.
