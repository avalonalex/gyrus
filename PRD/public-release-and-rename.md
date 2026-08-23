# PRD: Public Release — Rename to `gyrus` and Repository Hygiene

**Status**: In Progress — Phase 1 complete, Phase 2 complete, Phase 3 partial
**Last Updated**: 2026-08-23
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
- **R8** — Publishing to crates.io remains possible but is *not* required by
  this PRD; the rename must not make it harder.

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

**S2 — No declared MSRV.** Edition 2024 needs Rust ≥ 1.85. Without
`rust-version`, users on older toolchains get a manifest parse error instead of
a clear message.

**S3 — No CI.** Baseline is already green: `cargo fmt --check` clean, 302 tests
passing, exactly one clippy warning (useless `vec!` in the library). A GitHub
Actions workflow running fmt / clippy `-D warnings` / test on stable locks that
in cheaply.

**S4 — Documentation drift.** `CLAUDE.md` claims 166 tests (actual: 302) and
lists 10 modules, but `crates/ferrous-cortex/src/` also contains `optimizer.rs`,
`codegen.rs`, `io.rs`, `types.rs`, `debug.rs`, plus `interpreter/` and `hooks/`
subdirectories. `README.md:986-987` still lists "Performance optimizations" and
"JIT/AOT compiler backend" as unchecked roadmap items while an optimizer and an
optimized interpreter exist. Newcomers judge the project by exactly these files.

**S5 — Overclaiming.** "An industry-strength BrainFuck interpreter/compiler"
(`README.md:3`) and "production-grade" (library crate description) invite
eye-rolls. The `azores` README is the model to copy: state plainly what it is,
that it's a learning-driven project, and that it was written with AI assistance.

**S6 — Doc directory triage.** `internal/` (15 files) and `PRD/` (10 active +
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
- Commit email and crates.io publishing: still open.

### Phase 0 (original) — Decisions (blocking, human)

- Confirm `gyrus` and reserve nothing (crates.io names are claimed by publishing,
  not reserving).
- Decide `factor.bf`: drop, or keep fenced under `programs/third-party/` with a
  GPL notice.
- Decide git-history rewrite for `benchmarks/mandelbrot` (yes/no).
- Decide commit email: 118 commits use `yuhanhao@gmail.com`; switch to GitHub's
  noreply address going forward, rewrite, or accept it as public.
- Decide whether v0.3.0 publishes to crates.io or is GitHub-only for now.

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

### Phase 3 — Hygiene ⏳ PARTIAL (c58a12f: binary untracked, license/repository
consolidated into [workspace.package]; remaining: version unification, authors
field, rust-version, CI, the one clippy warning)

1. `git rm --cached benchmarks/mandelbrot`, gitignore `benchmarks/mandelbrot`.
2. Consolidate manifest metadata into `[workspace.package]`; add `rust-version`;
   unify on `0.3.0`.
3. Add `.github/workflows/ci.yml`: fmt check, clippy `-D warnings`, test, on
   stable, plus a `cargo build --release` smoke run.
4. Fix the one clippy warning so `-D warnings` passes.

### Phase 4 — Docs (S4, S5, S6)

1. Rewrite the README opening: honest tagline, AI-assistance disclosure, a
   30-second quickstart, one screenshot or code block of the highlighted error
   output (the project's most distinctive feature).
2. Sync the roadmap with what exists (optimizer, codegen, hooks, tool CLI).
3. Update `CLAUDE.md`: module list, test count, current status.
4. Triage `internal/` and `PRD/`; update `PRD/README.md`.

### Phase 5 — Release

1. Tag `v0.3.0`, write a short `CHANGELOG.md` covering 0.1 → 0.3.
2. Flip the repository to public.
3. Optionally `cargo publish` the three crates in dependency order:
   `gyrus`, then `gyrus-cli`, `gyrus-tool`.

## Success Criteria

- [x] `git grep -iE 'ferrous[-_ ]?cortex'` returns hits only in `PRD/archived/`
      and in this document's record of the before-state.
- [x] `cargo test --workspace` green (302 tests) after the rename.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [x] `LICENSE` (MIT) present; README license section filled. Dual-licensing was
      dropped: MIT alone, matching the other projects in this collection.
- [x] Every file in `programs/` is either original or credited in
      `programs/third-party/CREDITS.md` with author, source, and license.
- [x] No tracked binary artifacts (verified: 0).
- [ ] All three crates share one version and inherit workspace metadata.
- [ ] CI green on a pushed branch.
- [ ] README describes the project as it is, with no unearned superlatives.
- [ ] `cargo package -p gyrus` succeeds (whether or not it is published).

## Dependencies

- None technical. Phase 1 is mechanical; Phases 2–4 are independent of each
  other and can land in any order after the rename.
- Phase 0 decisions block Phases 2, 3, and 5.

## Risks and Open Questions

- **Publishing blocker: bench include paths** — `cargo package -p gyrus` ships
  `benches/` but not `programs/`, so the `include_str!("../../../programs/...")`
  calls in `benches/interpreter.rs` cannot resolve from a published crate. Fix
  by excluding benches from the package, or by reading the programs at runtime.
- **Publishing blocker: license text** — `LICENSE` sits at the repo root,
  outside `crates/gyrus/`, so cargo will not include it in the published crate.
  Copy it into each published crate directory before `cargo publish`.

- **History rewrite** — Dropping `benchmarks/mandelbrot` from history means
  rewriting all 118 commits. Free now (private, single-author, no forks),
  impossible later. The artifact is only 460 KB, so leaving it is defensible;
  the real question is whether the commit email should also be rewritten.
- **GPL exposure** — Shipping `factor.bf` does not affect the interpreter's own
  license (no linking, no derivation), but it does mean the repository as a
  whole is not purely MIT/Apache. Cleanest resolution is removal.
- **Cristofani's collection** — the terms on `brainfuck.org` need to be read and
  quoted in `CREDITS.md` rather than assumed permissive.
- **crates.io squatting** — `gyrus` is free today; it will not be reserved until
  a publish happens. If the name matters, publishing a 0.3.0 early is the only
  way to hold it.
- **Name collision check** — crates.io and GitHub are clear; a quick search for
  an existing well-known "gyrus" in adjacent tooling is worth five minutes
  before committing.
