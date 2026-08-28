#!/usr/bin/env bash
# Verify that the MSRV declared in Cargo.toml is the truth.
#
# `rust-version` is a claim about a compiler nobody here runs day to day, so it
# rots silently: the moment someone uses a newer language feature, the manifest
# is lying and nothing notices. This repo has already been bitten once — the
# obvious value (1.85, the edition-2024 floor) was wrong, because the code uses
# let-chains, which did not stabilize until 1.88.
#
# The version is read FROM the manifest rather than restated here, so this
# script cannot drift from what it checks. Bumping the MSRV means editing
# Cargo.toml and running this, which is also what tells you the README badge
# needs the same number -- the badge is the one place the version is restated,
# so it is the one place that can disagree.
#
# Usage: scripts/check-msrv.sh
set -euo pipefail

cd "$(dirname "$0")/.."

MSRV="$(sed -n 's/^rust-version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
if [ -z "$MSRV" ]; then
    echo "error: no rust-version found in [workspace.package] of Cargo.toml" >&2
    exit 1
fi
echo "Declared MSRV: $MSRV"

# Checked before the toolchain is installed, because a badge that disagrees is
# worth knowing about in a second rather than after a download and a build.
BADGE_URL="$(sed -n 's|.*img\.shields\.io/badge/rust-\([0-9.]*\)%2B.*|\1|p' README.md | head -1)"
BADGE_ALT="$(sed -n 's|.*!\[Rust \([0-9.]*\)+\].*|\1|p' README.md | head -1)"
if [ -z "$BADGE_URL" ] || [ -z "$BADGE_ALT" ]; then
    echo "FAIL: README.md has no Rust version badge to check." >&2
    echo "Expected a line like:" >&2
    echo '  [![Rust '"$MSRV"'+](https://img.shields.io/badge/rust-'"$MSRV"'%2B-orange.svg)](Cargo.toml)' >&2
    exit 1
fi
if [ "$BADGE_URL" != "$MSRV" ] || [ "$BADGE_ALT" != "$MSRV" ]; then
    echo "FAIL: the README badge says $BADGE_ALT/$BADGE_URL, and Cargo.toml says $MSRV." >&2
    echo "The badge is a promise to anyone who clones this; make it the same number." >&2
    exit 1
fi
echo "OK: the README badge agrees with the manifest."

if ! rustup toolchain list | grep -q "^$MSRV"; then
    echo "Installing Rust $MSRV (not present locally)..."
    rustup toolchain install "$MSRV" --profile minimal
fi

# --all-targets so tests, benches, and examples are held to the same floor as
# the library. A dev-dependency that needs a newer compiler is still a broken
# promise to anyone who clones the repo and runs the suite.
echo "Checking the workspace with Rust $MSRV..."
if cargo "+$MSRV" check --workspace --all-targets; then
    echo "OK: the workspace builds on its declared MSRV ($MSRV)."
else
    echo >&2
    echo "FAIL: the workspace does NOT build on its declared MSRV ($MSRV)." >&2
    echo "Either revert whatever needs a newer compiler, or raise" >&2
    echo "rust-version in Cargo.toml to a version that does build." >&2
    exit 1
fi
