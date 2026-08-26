#!/usr/bin/env bash
# Run every example in every crate and fail if any does not exit 0.
#
# `cargo build --examples` and clippy both compile these, so a type error is
# caught already. A *runtime* failure is not: when MemoryAddress became signed,
# hooks_execution_tracer kept compiling and panicked on its first instruction
# (`ptr - 2` at cell 0 is negative, and indexing with it is a very large usize).
# Nothing noticed, because nothing ran them.
#
# Examples are documentation that executes. This is the script that checks it.
set -uo pipefail

cd "$(dirname "$0")/.."

shopt -s nullglob
# Every crate, not only `gyrus`: this globbed one directory, so
# `gyrus-macro`'s example -- the only way to run a .bfm until the CLI accepts
# one -- was compiled by clippy and run by nothing, which is the exact gap
# this script exists to close.
EXAMPLES=(crates/*/examples/*.rs)
if [ "${#EXAMPLES[@]}" -eq 0 ]; then
    echo "error: no examples found under crates/*/examples" >&2
    exit 1
fi

echo "Running ${#EXAMPLES[@]} example(s)..."
failures=0
for file in "${EXAMPLES[@]}"; do
    name=$(basename "$file" .rs)
    crate=$(basename "$(dirname "$(dirname "$file")")")
    if out=$(cargo run --quiet --release -p "$crate" --example "$name" 2>&1); then
        printf '  %-28s ok\n' "$name"
    else
        printf '  %-28s FAILED\n' "$name"
        printf '%s\n' "$out" | tail -5 | sed 's/^/      /'
        failures=$((failures + 1))
    fi
done

echo
if [ "$failures" -gt 0 ]; then
    echo "FAIL: $failures example(s) did not run cleanly." >&2
    exit 1
fi
echo "OK: every example runs."
